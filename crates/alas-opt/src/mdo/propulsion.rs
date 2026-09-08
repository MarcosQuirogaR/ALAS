// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The off-design propulsion deck every candidate mission is priced with.
//!
//! One constructor, [`product_orchestrator`], builds the selected catalogue
//! engine's typed off-design model (the Bartel--Young/OpenAP turbofan deck
//! anchored on ICAO LTO fuel flow and the cruise reference point, or the
//! PW127M/568F turboprop surrogate) and is shared with the native mission
//! stage in `alas-pipeline`, so the optimizer's segment model, the dispatch
//! fuel policy and the finalist mission read thrust and fuel flow from the
//! same physics. The deck answers three questions the mission integrator
//! asks at every step: what thrust a rating provides at this flight
//! condition, what the flight-idle floor is, and what fuel flow delivers a
//! required thrust between those limits.

use std::fmt;
use std::sync::Arc;

use alas_atmo::us1976_try_compute_values;
use alas_config::{ActiveEngineModel, EngineConfig};
use alas_prop::empirical_turbofan::{EmpiricalTurbofanDeck, EmpiricalTurbofanModel};
use alas_prop::mission_turbofan::{
    freestream_from_atmosphere, size_turbofan_to_static_rating, PartPowerModel, TurbofanInputs,
    VehicleBuilderParams,
};
use alas_prop::system::{
    FailureState, FlightCondition, LegacyTurbofanModel, ModelIdentity, ModelProvenance,
    OperatingMode, PropulsionDemand, PropulsionInstallation, PropulsionLoads,
    PropulsionOrchestrator, PropulsionRating, PropulsionRequest, PropulsionResult, PropulsionState,
    ResourceKind,
};
use alas_prop::turboprop::{Atr72TurbopropSystem, Pw127m568fModel};

/// Metres per second to feet per minute, for the empirical climb schedule.
const METRES_PER_SECOND_TO_FEET_PER_MINUTE: f64 = 196.850_393_700_787_4;

/// Ratio tolerance on the inverse thrust solve.
const THRUST_SOLVE_RELATIVE_TOLERANCE: f64 = 1.0e-6;
/// Iteration budget for the bracketed inverse thrust solve.
const THRUST_SOLVE_MAX_ITERATIONS: usize = 60;

/// Which technology family the selected engine binding resolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckKind {
    /// A turbofan priced with the empirical off-design deck.
    Turbofan,
    /// A turboprop priced with the shaft-power/propeller surrogate.
    Turboprop,
}

/// How a solved operating point was bounded by the deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThrustLimit {
    /// The requested thrust was delivered between idle and the rating.
    Unlimited,
    /// The requested thrust was below the flight-idle floor; the point is
    /// the idle point and the excess thrust is reported by the caller.
    IdleFloor,
}

/// One solved propulsion operating point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OperatingPoint {
    /// Installed net thrust along the body x axis, N.
    pub thrust_n: f64,
    /// Installed all-engine Jet-A fuel flow, kg/s.
    pub fuel_flow_kg_s: f64,
    /// Fraction of the capping rating that was commanded.
    pub rating_fraction: f64,
    /// The rating the fraction refers to.
    pub rating: PropulsionRating,
    /// Whether the deck bounded the request.
    pub limit: ThrustLimit,
}

/// Why the deck could not deliver a request.
#[derive(Debug, Clone, PartialEq)]
pub enum DeckError {
    /// The propulsion model rejected the request or returned an unusable
    /// value.
    Model(String),
    /// The required thrust exceeds what the capping rating delivers here.
    ThrustDeficit {
        /// Required installed thrust, N.
        required_n: f64,
        /// Available installed thrust at the rating, N.
        available_n: f64,
        /// The rating that was capped.
        rating: PropulsionRating,
        /// Altitude of the request, m.
        altitude_m: f64,
    },
}

impl fmt::Display for DeckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Model(reason) => write!(formatter, "propulsion deck: {reason}"),
            Self::ThrustDeficit {
                required_n,
                available_n,
                rating,
                altitude_m,
            } => write!(
                formatter,
                "thrust deficit at {altitude_m:.0} m: {required_n:.0} N required, {available_n:.0} N available at {rating:?}"
            ),
        }
    }
}

/// The shared off-design deck: a thread-safe handle on the selected
/// engine's typed model plus the identity the candidate history records.
#[derive(Clone)]
pub struct PropulsionDeck {
    orchestrator: Arc<PropulsionOrchestrator>,
    identity: String,
    kind: DeckKind,
    n_engines: usize,
}

impl fmt::Debug for PropulsionDeck {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PropulsionDeck")
            .field("identity", &self.identity)
            .field("kind", &self.kind)
            .field("n_engines", &self.n_engines)
            .finish()
    }
}

impl PartialEq for PropulsionDeck {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
            && self.kind == other.kind
            && self.n_engines == other.n_engines
    }
}

impl PropulsionDeck {
    /// Build the deck for `engine` at the product installation the mission
    /// stage uses (units at the configured spanwise stations, thrust along
    /// body x).
    ///
    /// # Errors
    ///
    /// The binding or model-construction failure, as a description.
    pub fn from_engine(
        engine: &EngineConfig,
        cruise_mach: f64,
        cruise_altitude_m: f64,
        max_climb_rate_ft_min: Option<f64>,
    ) -> Result<Self, String> {
        let n_engines = engine.spanwise_positions_m.len();
        if n_engines == 0 {
            return Err("engine binding has no installed units".to_owned());
        }
        let installation = PropulsionInstallation {
            unit_positions_m: engine
                .spanwise_positions_m
                .iter()
                .map(|&y_m| [0.0, y_m, engine.z_m])
                .collect(),
            thrust_axes_body: vec![[1.0, 0.0, 0.0]; n_engines],
            nacelle_wetted_area_m2: None,
            frontal_area_m2: None,
        };
        let (orchestrator, kind) = product_orchestrator(
            engine,
            installation,
            cruise_mach,
            cruise_altitude_m,
            max_climb_rate_ft_min,
        )?;
        let provenance = orchestrator.provenance();
        let identity = format!(
            "{}/{}:{}",
            engine.engine_name, provenance.model.family, provenance.model.version
        );
        Ok(Self {
            orchestrator: Arc::new(orchestrator),
            identity,
            kind,
            n_engines,
        })
    }

    /// Technology family of the bound engine.
    pub fn kind(&self) -> DeckKind {
        self.kind
    }

    /// Engine name and model identity, for provenance.
    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// Number of installed units.
    pub fn n_engines(&self) -> usize {
        self.n_engines
    }

    /// Flight condition at `altitude_m` and true airspeed `tas_m_s` on a day
    /// `isa_deviation_c` warmer than standard, built from the same 1976
    /// atmosphere (and the same deviation convention) the native mission uses.
    ///
    /// # Errors
    ///
    /// A non-finite altitude, speed or deviation.
    pub fn flight_condition(
        &self,
        altitude_m: f64,
        tas_m_s: f64,
        gravity_m_s2: f64,
        isa_deviation_c: f64,
    ) -> Result<FlightCondition, DeckError> {
        if !isa_deviation_c.is_finite() {
            return Err(DeckError::Model(format!(
                "ISA deviation {isa_deviation_c} C is not usable"
            )));
        }
        if !tas_m_s.is_finite() || tas_m_s < 0.0 {
            return Err(DeckError::Model(format!(
                "true airspeed {tas_m_s} m/s is not usable"
            )));
        }
        let values = us1976_try_compute_values(altitude_m, isa_deviation_c)
            .map_err(|error| DeckError::Model(format!("atmosphere at {altitude_m} m: {error}")))?;
        let mach = tas_m_s / values.speed_of_sound_m_s;
        let freestream =
            freestream_from_atmosphere(&values, altitude_m, tas_m_s, mach, gravity_m_s2);
        Ok((&freestream).into())
    }

    /// The operating point at a named rating.
    ///
    /// # Errors
    ///
    /// [`DeckError::Model`] when the model rejects the condition.
    pub fn rated_point(
        &self,
        flight: FlightCondition,
        rating: PropulsionRating,
    ) -> Result<OperatingPoint, DeckError> {
        let result = self.evaluate(flight, PropulsionDemand::Rating(rating))?;
        Ok(OperatingPoint {
            thrust_n: result.body_force_n[0],
            fuel_flow_kg_s: jet_a_mass_flow(&result)?,
            rating_fraction: 1.0,
            rating,
            limit: ThrustLimit::Unlimited,
        })
    }

    /// The flight-idle floor at this condition.
    ///
    /// # Errors
    ///
    /// As [`Self::rated_point`].
    pub fn idle_point(&self, flight: FlightCondition) -> Result<OperatingPoint, DeckError> {
        let mut point = self.rated_point(flight, PropulsionRating::FlightIdle)?;
        point.limit = ThrustLimit::IdleFloor;
        point.rating_fraction = 0.0;
        Ok(point)
    }

    /// Solve the throttle that delivers `required_n` under `cap`, and read
    /// the fuel flow there.
    ///
    /// Requests below the flight-idle thrust return the idle point flagged
    /// [`ThrustLimit::IdleFloor`]; requests above the rating are a typed
    /// [`DeckError::ThrustDeficit`] so the caller can adapt the flight path
    /// or reject the profile with the numbers.
    ///
    /// # Errors
    ///
    /// [`DeckError::ThrustDeficit`] or the model's own rejection.
    pub fn at_thrust(
        &self,
        flight: FlightCondition,
        required_n: f64,
        cap: PropulsionRating,
    ) -> Result<OperatingPoint, DeckError> {
        if !required_n.is_finite() {
            return Err(DeckError::Model(format!(
                "required thrust {required_n} N is not finite"
            )));
        }
        let idle = self.idle_point(flight)?;
        if required_n <= idle.thrust_n {
            return Ok(idle);
        }
        let ceiling = self.rated_point(flight, cap)?;
        if required_n > ceiling.thrust_n * (1.0 + THRUST_SOLVE_RELATIVE_TOLERANCE) {
            return Err(DeckError::ThrustDeficit {
                required_n,
                available_n: ceiling.thrust_n,
                rating: cap,
                altitude_m: flight.altitude_m,
            });
        }
        if required_n >= ceiling.thrust_n {
            return Ok(ceiling);
        }
        // Both decks define the normalized-force demand as a fraction of a
        // reference force that is linear in the command (the turbofan's
        // maximum-climb thrust; the turboprop's takeoff-rated force, solved
        // by its own governor inverse), so the first guess is exact unless
        // the deck floors the request at its lowest running point.
        let reference = self.normalized_point(flight, 1.0)?;
        if reference.thrust_n > 0.0 {
            let fraction = (required_n / reference.thrust_n).clamp(0.0, 1.0);
            let point = self.normalized_point(flight, fraction)?;
            let tolerance_n = THRUST_SOLVE_RELATIVE_TOLERANCE * required_n.abs().max(1.0);
            if (point.thrust_n - required_n).abs() <= tolerance_n {
                return Ok(OperatingPoint {
                    rating: cap,
                    rating_fraction: (point.thrust_n / ceiling.thrust_n).clamp(0.0, 1.0),
                    ..point
                });
            }
            if point.limit == ThrustLimit::IdleFloor && point.thrust_n > required_n {
                // The deck's lowest running point sits above the flight-idle
                // surrogate: report it as the floor the caller must respect.
                return Ok(OperatingPoint {
                    rating: cap,
                    rating_fraction: (point.thrust_n / ceiling.thrust_n).clamp(0.0, 1.0),
                    ..point
                });
            }
        }
        self.solve_fraction(flight, required_n, cap, ceiling)
    }

    /// The point at a normalized-force demand, flagged when the deck floored
    /// the request at its lowest running point.
    fn normalized_point(
        &self,
        flight: FlightCondition,
        fraction: f64,
    ) -> Result<OperatingPoint, DeckError> {
        let result = self.evaluate(flight, PropulsionDemand::NormalizedForce(fraction))?;
        let floored = result
            .active_limits
            .iter()
            .any(|limit| limit.name == "flight-idle-thrust");
        Ok(OperatingPoint {
            thrust_n: result.body_force_n[0],
            fuel_flow_kg_s: jet_a_mass_flow(&result)?,
            rating_fraction: fraction,
            rating: PropulsionRating::MaximumClimb,
            limit: if floored {
                ThrustLimit::IdleFloor
            } else {
                ThrustLimit::Unlimited
            },
        })
    }

    /// Bracketed inverse of the rating-fraction thrust map.
    ///
    /// The turbofan deck is linear in the fraction, so the first secant
    /// guess is exact there; the turboprop's propeller surrogate is smooth
    /// and monotone, so regula falsi with a bisection safeguard converges
    /// in a few evaluations.
    fn solve_fraction(
        &self,
        flight: FlightCondition,
        required_n: f64,
        cap: PropulsionRating,
        ceiling: OperatingPoint,
    ) -> Result<OperatingPoint, DeckError> {
        let point_at = |fraction: f64| -> Result<OperatingPoint, DeckError> {
            let result = self.evaluate(
                flight,
                PropulsionDemand::RatedFraction {
                    rating: cap,
                    fraction,
                },
            )?;
            Ok(OperatingPoint {
                thrust_n: result.body_force_n[0],
                fuel_flow_kg_s: jet_a_mass_flow(&result)?,
                rating_fraction: fraction,
                rating: cap,
                limit: ThrustLimit::Unlimited,
            })
        };
        let tolerance_n = THRUST_SOLVE_RELATIVE_TOLERANCE * required_n.abs().max(1.0);
        let mut low_fraction = 0.0;
        let mut low = point_at(0.0)?;
        let mut high_fraction = 1.0;
        let mut high = ceiling;
        if !(low.thrust_n <= required_n && required_n <= high.thrust_n) {
            return Err(DeckError::Model(format!(
                "rating map does not bracket {required_n:.1} N ({:.1}..{:.1} N)",
                low.thrust_n, high.thrust_n
            )));
        }
        for _ in 0..THRUST_SOLVE_MAX_ITERATIONS {
            let span = high.thrust_n - low.thrust_n;
            let secant = if span > 0.0 {
                low_fraction + (required_n - low.thrust_n) / span * (high_fraction - low_fraction)
            } else {
                0.5 * (low_fraction + high_fraction)
            };
            // Keep the iterate strictly inside the bracket so a flat map
            // cannot pin it to an endpoint.
            let fraction = secant
                .max(low_fraction + 0.05 * (high_fraction - low_fraction))
                .min(high_fraction - 0.05 * (high_fraction - low_fraction));
            let point = point_at(fraction)?;
            // The rating map must be monotone in both thrust and fuel flow
            // inside the bracket; a deck that is not is an invalid off-design
            // evaluation, not a root to close on numerically.
            let thrust_tolerance = 1.0e-9 * ceiling.thrust_n.abs().max(1.0);
            let flow_tolerance = 1.0e-9 * ceiling.fuel_flow_kg_s.abs().max(1.0e-6);
            if point.thrust_n < low.thrust_n - thrust_tolerance
                || point.thrust_n > high.thrust_n + thrust_tolerance
                || point.fuel_flow_kg_s < low.fuel_flow_kg_s - flow_tolerance
                || point.fuel_flow_kg_s > high.fuel_flow_kg_s + flow_tolerance
            {
                return Err(DeckError::Model(format!(
                    "rating map is not monotone at fraction {fraction:.4}: {:.1} N / {:.5} kg/s between ({:.1} N / {:.5} kg/s) and ({:.1} N / {:.5} kg/s)",
                    point.thrust_n,
                    point.fuel_flow_kg_s,
                    low.thrust_n,
                    low.fuel_flow_kg_s,
                    high.thrust_n,
                    high.fuel_flow_kg_s
                )));
            }
            if (point.thrust_n - required_n).abs() <= tolerance_n {
                return Ok(point);
            }
            if point.thrust_n < required_n {
                low_fraction = fraction;
                low = point;
            } else {
                high_fraction = fraction;
                high = point;
            }
            if high_fraction - low_fraction < 1.0e-9 {
                break;
            }
        }
        // Linear interpolation across the final bracket: within tolerance of
        // both bracket ends for any monotone map this narrow.
        let span = high.thrust_n - low.thrust_n;
        let weight = if span > 0.0 {
            ((required_n - low.thrust_n) / span).clamp(0.0, 1.0)
        } else {
            0.5
        };
        Ok(OperatingPoint {
            thrust_n: required_n,
            fuel_flow_kg_s: low.fuel_flow_kg_s
                + weight * (high.fuel_flow_kg_s - low.fuel_flow_kg_s),
            rating_fraction: low_fraction + weight * (high_fraction - low_fraction),
            rating: cap,
            limit: ThrustLimit::Unlimited,
        })
    }

    fn evaluate(
        &self,
        flight: FlightCondition,
        demand: PropulsionDemand,
    ) -> Result<PropulsionResult, DeckError> {
        let request = PropulsionRequest {
            flight,
            demand,
            mode: OperatingMode::Normal,
            failure: FailureState::None,
            loads: PropulsionLoads::default(),
            state: PropulsionState::default(),
            time_step_s: None,
        };
        let result = self
            .orchestrator
            .evaluate(&request)
            .map_err(|error| DeckError::Model(error.to_string()))?;
        let thrust_n = result.body_force_n[0];
        if !thrust_n.is_finite() || thrust_n < 0.0 {
            return Err(DeckError::Model(format!(
                "thrust {thrust_n} N at {:.0} m / M{:.3} is not usable",
                flight.altitude_m, flight.mach
            )));
        }
        Ok(result)
    }
}

fn jet_a_mass_flow(result: &PropulsionResult) -> Result<f64, DeckError> {
    let flow = result
        .resource_flows
        .iter()
        .find(|flow| flow.resource == ResourceKind::JetA)
        .and_then(|flow| flow.mass_flow_kg_s)
        .ok_or_else(|| DeckError::Model("no Jet-A mass-flow result".to_owned()))?;
    if !flow.is_finite() || flow < 0.0 {
        return Err(DeckError::Model(format!(
            "fuel flow {flow} kg/s is not usable"
        )));
    }
    Ok(flow)
}

/// Build the product propulsion orchestrator for `engine` at
/// `installation`.
///
/// This is the single construction path for the product mission: the
/// turbofan branch sizes the legacy cycle to the catalogue static rating for
/// installation/mass bookkeeping and wraps it in the empirical off-design
/// deck; the turboprop branch binds the PW127M/568F surrogate. The native
/// mission stage calls this same function, so candidate ranking and the
/// finalist mission cannot disagree on the engine.
///
/// # Errors
///
/// The binding or construction failure, as a description.
pub fn product_orchestrator(
    engine: &EngineConfig,
    installation: PropulsionInstallation,
    cruise_mach: f64,
    cruise_altitude_m: f64,
    max_climb_rate_ft_min: Option<f64>,
) -> Result<(PropulsionOrchestrator, DeckKind), String> {
    let n_engines = engine.spanwise_positions_m.len();
    if n_engines == 0 {
        return Err("mission aircraft has no engines".to_owned());
    }
    let active_model = engine
        .active_model()
        .map_err(|error| format!("mission engine binding failed: {error}"))?;
    match active_model {
        ActiveEngineModel::Turbofan(payload) => {
            // The legacy scalar evaluator has no per-unit moment model. Keep
            // its equivalent thrust line through the mission reference point;
            // the typed propulsion boundary is still what mission code sees.
            let legacy_installation = PropulsionInstallation {
                unit_positions_m: engine
                    .spanwise_positions_m
                    .iter()
                    .map(|&y_m| [0.0, y_m, 0.0])
                    .collect(),
                ..installation
            };
            let design_thrust_total_n = payload.rated_thrust_kn * n_engines as f64 * 1_000.0;
            if !design_thrust_total_n.is_finite() || design_thrust_total_n <= 0.0 {
                return Err(
                    "mission aircraft has no positive finite rated engine thrust".to_owned(),
                );
            }
            let inputs = TurbofanInputs {
                number_of_engines: n_engines as f64,
                bypass_ratio: payload.bypass_ratio,
                overall_pressure_ratio: payload.overall_pressure_ratio,
                fan_pressure_ratio: payload.fan_pressure_ratio,
                turbine_inlet_temperature_k: payload.turbine_inlet_temp_k,
                cruise_mach,
                cruise_altitude_m,
                design_thrust_total_n,
            };
            let params = VehicleBuilderParams {
                part_power_model: PartPowerModel::IcaoLtoFuelFlow {
                    fuel_flow_ratios: payload.part_power_fuel_flow_ratios,
                },
                ..VehicleBuilderParams::default()
            };
            let sized = size_turbofan_to_static_rating(&inputs, &params);
            let legacy_model = LegacyTurbofanModel::new(
                inputs,
                params,
                sized.compressor_nondimensional_massflow,
                ModelProvenance {
                    model: ModelIdentity {
                        family: "legacy-mission-turbofan".to_owned(),
                        version: "compatibility-v1".to_owned(),
                    },
                    dataset: Some(engine.engine_name.clone()),
                    sources: vec![payload.part_power_source.clone()],
                },
                Vec::new(),
                legacy_installation,
            )
            .map_err(|error| format!("mission propulsion construction failed: {error}"))?;
            let model = EmpiricalTurbofanModel::new(
                EmpiricalTurbofanDeck {
                    takeoff_thrust_n: design_thrust_total_n,
                    max_climb_reference_thrust_n: payload.off_design.cruise_reference_thrust_n
                        * n_engines as f64,
                    max_climb_reference_altitude_m: payload.off_design.cruise_reference_altitude_m,
                    max_climb_reference_mach: payload.off_design.cruise_reference_mach,
                    bypass_ratio: payload.takeoff_bypass_ratio.unwrap_or(payload.bypass_ratio),
                    takeoff_fuel_flow_kg_s: payload.takeoff_fuel_flow_kg_s * n_engines as f64,
                    cruise_reference_tsfc_kg_kgf_h: payload.cruise_tsfc_kg_kgf_hr,
                    part_power_fuel_flow_ratios: payload.part_power_fuel_flow_ratios,
                    max_climb_rate_ft_min,
                    flight_idle_fraction: 0.07,
                },
                legacy_model,
                ModelProvenance {
                    model: ModelIdentity {
                        family: "bartel-young-openap-turbofan".to_owned(),
                        version: "three-region-v2".to_owned(),
                    },
                    dataset: Some(engine.engine_name.clone()),
                    sources: vec![
                        "Battel & Young (2008), Journal of Aircraft 45(4), DOI 10.2514/1.35589"
                            .to_owned(),
                        format!(
                            "{} [{}]",
                            payload.off_design.source, payload.off_design.evidence
                        ),
                        payload.part_power_source.clone(),
                    ],
                },
            )
            .map_err(|error| format!("mission propulsion construction failed: {error}"))?;
            Ok((PropulsionOrchestrator::new(model), DeckKind::Turbofan))
        }
        ActiveEngineModel::Turboprop(payload) => {
            let unit_model = Pw127m568fModel {
                normal_takeoff_power_w: payload.takeoff_shaft_power_kw * 1_000.0,
                maximum_takeoff_reserve_power_w: payload.maximum_reserve_shaft_power_kw * 1_000.0,
                maximum_continuous_power_w: payload.maximum_continuous_shaft_power_kw * 1_000.0,
                maximum_climb_power_w: payload.maximum_climb_shaft_power_kw * 1_000.0,
                maximum_cruise_power_w: payload.maximum_cruise_shaft_power_kw * 1_000.0,
                governed_propeller_speed_rpm: payload.governed_propeller_speed_rpm,
                propeller_diameter_m: payload.propeller_diameter_m,
                // The catalogue's maximum-cruise fuel flow is published for the
                // two-engine installation (`TurbopropEngineSpec` doc comment).
                reference_psfc_kg_kwh: payload.maximum_cruise_fuel_flow_kg_h
                    / (2.0 * payload.maximum_cruise_shaft_power_kw),
                ..Pw127m568fModel::default()
            };
            let model = Atr72TurbopropSystem::new(unit_model, Vec::new(), installation)
                .map_err(|error| format!("mission propulsion construction failed: {error}"))?;
            Ok((PropulsionOrchestrator::new(model), DeckKind::Turboprop))
        }
    }
}

/// The empirical climb schedule's representative rate from the configured
/// initial-climb rate, ft/min; `None` when the configuration is unusable so
/// no aircraft-independent rate is invented.
pub fn max_climb_rate_ft_min(initial_climb_rate_m_s: f64) -> Option<f64> {
    (initial_climb_rate_m_s.is_finite() && initial_climb_rate_m_s >= 0.0)
        .then_some(initial_climb_rate_m_s * METRES_PER_SECOND_TO_FEET_PER_MINUTE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::AlasConfig;

    fn default_deck() -> PropulsionDeck {
        let config = AlasConfig::default();
        PropulsionDeck::from_engine(
            &config.geometry.engine,
            config.requirements.cruise_mach,
            config.requirements.cruise_altitude_m,
            max_climb_rate_ft_min(config.mission.profile.initial_climb_rate_m_s),
        )
        .unwrap_or_else(|error| panic!("deck: {error}"))
    }

    #[test]
    fn the_default_turbofan_deck_lapses_with_altitude_and_orders_its_ratings() {
        let deck = default_deck();
        assert_eq!(deck.kind(), DeckKind::Turbofan);
        let sea_level = deck.flight_condition(0.0, 80.0, 9.80665, 0.0).unwrap();
        let cruise = deck
            .flight_condition(11_000.0, 236.0, 9.80665, 0.0)
            .unwrap();
        let toga_sl = deck
            .rated_point(sea_level, PropulsionRating::TakeoffGoAround)
            .unwrap();
        let climb_cruise = deck
            .rated_point(cruise, PropulsionRating::MaximumClimb)
            .unwrap();
        let idle_cruise = deck.idle_point(cruise).unwrap();
        assert!(toga_sl.thrust_n > climb_cruise.thrust_n);
        assert!(climb_cruise.thrust_n > idle_cruise.thrust_n);
        assert!(idle_cruise.thrust_n > 0.0);
        assert!(toga_sl.fuel_flow_kg_s > climb_cruise.fuel_flow_kg_s);
        assert!(climb_cruise.fuel_flow_kg_s > idle_cruise.fuel_flow_kg_s);
    }

    #[test]
    fn the_inverse_solve_returns_the_requested_thrust_with_monotone_fuel_flow() {
        let deck = default_deck();
        let cruise = deck
            .flight_condition(11_000.0, 236.0, 9.80665, 0.0)
            .unwrap();
        let ceiling = deck
            .rated_point(cruise, PropulsionRating::MaximumClimb)
            .unwrap();
        let mut previous_flow = 0.0;
        for fraction in [0.3, 0.5, 0.7, 0.9] {
            let required = fraction * ceiling.thrust_n;
            let point = deck
                .at_thrust(cruise, required, PropulsionRating::MaximumClimb)
                .unwrap();
            assert!((point.thrust_n - required).abs() <= 1.0e-5 * required);
            assert!(point.fuel_flow_kg_s > previous_flow);
            assert_eq!(point.limit, ThrustLimit::Unlimited);
            previous_flow = point.fuel_flow_kg_s;
        }
        let deficit = deck.at_thrust(
            cruise,
            2.0 * ceiling.thrust_n,
            PropulsionRating::MaximumClimb,
        );
        assert!(matches!(deficit, Err(DeckError::ThrustDeficit { .. })));
        let idle = deck
            .at_thrust(cruise, 1.0, PropulsionRating::MaximumClimb)
            .unwrap();
        assert_eq!(idle.limit, ThrustLimit::IdleFloor);
    }

    #[test]
    fn the_atr_turboprop_binding_yields_a_usable_deck() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": "ATR72-600"}))
            .unwrap_or_else(|error| panic!("preset: {error}"));
        let deck = PropulsionDeck::from_engine(
            &config.geometry.engine,
            config.requirements.cruise_mach,
            config.requirements.cruise_altitude_m,
            max_climb_rate_ft_min(config.mission.profile.initial_climb_rate_m_s),
        )
        .unwrap_or_else(|error| panic!("ATR deck: {error}"));
        assert_eq!(deck.kind(), DeckKind::Turboprop);
        let climb = deck.flight_condition(3_000.0, 90.0, 9.80665, 0.0).unwrap();
        let rated = deck
            .rated_point(climb, PropulsionRating::MaximumClimb)
            .unwrap();
        assert!(rated.thrust_n > 10_000.0 && rated.fuel_flow_kg_s > 0.05);
        let half = deck
            .at_thrust(climb, 0.5 * rated.thrust_n, PropulsionRating::MaximumClimb)
            .unwrap();
        assert!((half.thrust_n - 0.5 * rated.thrust_n).abs() <= 1.0e-4 * rated.thrust_n);
        assert!(half.fuel_flow_kg_s < rated.fuel_flow_kg_s);
    }
}
