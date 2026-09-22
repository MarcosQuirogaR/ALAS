// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The off-design propulsion deck every candidate mission is priced with.
//!
//! One constructor, [`product_orchestrator`], builds the selected catalogue
//! engine's typed off-design model (the Bartel-Young/OpenAP turbofan deck
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

    /// The rating a full-command normalized-force demand resolves to.
    ///
    /// Each deck normalizes its force command against one of its own ratings:
    /// the turbofan against maximum climb, the turboprop against the
    /// takeoff-rated force its governor inverse solves for. Stating it here
    /// lets the inverse thrust solve read that reference from the rating map
    /// instead of running the normalized-force demand for it.
    fn normalizing_rating(&self) -> PropulsionRating {
        match self.kind {
            DeckKind::Turbofan => PropulsionRating::MaximumClimb,
            DeckKind::Turboprop => PropulsionRating::TakeoffGoAround,
        }
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
        self.at_thrust_between(flight, required_n, cap, None, None)
    }

    /// As [`Self::at_thrust`], reusing bounds the caller already solved.
    ///
    /// The mission integrator needs the idle floor and the rating at every
    /// step to decide whether the step is bounded at all, and then asks for
    /// the point between them. Re-deriving both inside this call repeated two
    /// deck evaluations per integration step - on the turboprop, whose deck
    /// evaluations are the expensive ones, that is pure duplication. `None`
    /// keeps the self-contained behaviour; a supplied bound must be the same
    /// deck's point at the same flight condition, which is what makes this
    /// exactly result-preserving rather than an approximation.
    ///
    /// # Errors
    ///
    /// As [`Self::at_thrust`].
    pub fn at_thrust_between(
        &self,
        flight: FlightCondition,
        required_n: f64,
        cap: PropulsionRating,
        known_idle: Option<OperatingPoint>,
        known_ceiling: Option<OperatingPoint>,
    ) -> Result<OperatingPoint, DeckError> {
        if !required_n.is_finite() {
            return Err(DeckError::Model(format!(
                "required thrust {required_n} N is not finite"
            )));
        }
        let idle = match known_idle {
            Some(point) => point,
            None => self.idle_point(flight)?,
        };
        if required_n <= idle.thrust_n {
            return Ok(idle);
        }
        let ceiling = match known_ceiling {
            Some(point) => point,
            None => self.rated_point(flight, cap)?,
        };
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
        //
        // **This is the turboprop's dominant evaluation cost and it is not
        // safe to route around. Measured, so it is not re-attempted blind:**
        // on the PW127M at 4 000 m and 120 m/s one `at_thrust` costs 186 us
        // against 3.2 us for `rated_point`, because the turboprop's
        // normalized-force demand is itself a governor inverse and the
        // shortcut runs two of them. Answering through `solve_fraction`
        // instead costs 18 us and agrees with this path to twelve significant
        // figures at 10 000 N and 14 000 N, but *not* at low power, where
        // the PW127M's rating map folds (3 450 N at fraction 0, 1 643 N at
        // fraction 0.082, 38 962 N at fraction 1) and the two inverses land
        // on different operating points: at 6 000 N, 0.1414 kg/s through the
        // governor against 0.0925 kg/s through the rating fraction, a factor
        // of 1.53. Mixing them therefore makes delivered fuel flow
        // *non-monotone in the request* (1 557 N at 0.09526 kg/s beside
        // 3 115 N at 0.03900 kg/s), which
        // `the_turboprop_inverse_closes_on_the_request_across_the_whole_thrust_band`
        // now catches. Which of the two inverses is right at low power is a
        // **deck question owned by propulsion**; until it is answered the
        // mission must read one of them, and it reads the governor's.
        //
        // What *is* safe is not paying a governor inverse merely to read the
        // reference force. A full-command normalized-force demand returns the
        // deck's own normalizing rating, so `rated_point` answers the same
        // question directly - measured bit-identical in thrust on both decks
        // and pinned by
        // `a_full_command_normalized_force_is_the_deck_normalizing_rating`.
        // The request itself still goes through the governor below, so the
        // solved operating point is unchanged; only the reference read moves
        // from ~85 us to ~3 us on the PW127M.
        let reference = if self.normalizing_rating() == cap {
            ceiling
        } else {
            self.rated_point(flight, self.normalizing_rating())?
        };
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
    /// and monotone, so regula falsi with a stagnation safeguard converges in
    /// a few evaluations.
    ///
    /// The safeguard is the Illinois down-weighting, and it is load-bearing.
    /// Plain regula falsi retains one endpoint indefinitely on a convex map,
    /// so the bracket shrinks only through the moving side; the earlier form
    /// of this solver additionally clamped each iterate 5 % inside the
    /// bracket, capping the per-iteration contraction at 0.95. On the PW127M
    /// the two together meant every off-rating request ran the full
    /// [`THRUST_SOLVE_MAX_ITERATIONS`] and left through the terminal linear
    /// interpolation rather than converging: measured at **171 us per
    /// `at_thrust` call against 3.2 us per `rated_point`**, about 54 deck
    /// evaluations each time, which made one coupled ATR leg 167 ms against
    /// the A320-200's 0.6 ms. Halving the retained endpoint's residual after
    /// it is retained twice restores superlinear convergence, so the solve
    /// now reaches the same 1e-6 relative thrust tolerance (more often
    /// *inside* it than before, since the old path usually fell out of the
    /// loop) in a handful of evaluations. Nothing in the deck, the rating
    /// map or the tolerance changes.
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
        // Residuals carried separately from the bracket points: the Illinois
        // safeguard down-weights the *retained* side's residual to move the
        // next iterate off it, while the monotonicity check below must still
        // compare against the deck's real thrust and fuel flow.
        let mut low_residual = low.thrust_n - required_n;
        let mut high_residual = high.thrust_n - required_n;
        let mut low_retained = 0_u32;
        let mut high_retained = 0_u32;
        for _ in 0..THRUST_SOLVE_MAX_ITERATIONS {
            let span = high_residual - low_residual;
            let secant = if span > 0.0 {
                low_fraction - low_residual / span * (high_fraction - low_fraction)
            } else {
                0.5 * (low_fraction + high_fraction)
            };
            // Keep the iterate strictly inside the bracket so a flat map
            // cannot pin it to an endpoint. The margin is the smallest that
            // does that, not the 5 % that used to throttle the contraction.
            let width = high_fraction - low_fraction;
            let fraction = secant
                .max(low_fraction + 1.0e-6 * width)
                .min(high_fraction - 1.0e-6 * width);
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
                low_residual = point.thrust_n - required_n;
                low = point;
                low_retained = 0;
                high_retained += 1;
                if high_retained >= 2 {
                    high_residual *= 0.5;
                }
            } else {
                high_fraction = fraction;
                high_residual = point.thrust_n - required_n;
                high = point;
                high_retained = 0;
                low_retained += 1;
                if low_retained >= 2 {
                    low_residual *= 0.5;
                }
            }
            if high_fraction - low_fraction < 1.0e-12 {
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
            let model =
                Atr72TurbopropSystem::new(turboprop_unit_model(payload), Vec::new(), installation)
                    .map_err(|error| format!("mission propulsion construction failed: {error}"))?;
            Ok((PropulsionOrchestrator::new(model), DeckKind::Turboprop))
        }
    }
}

/// The single-engine propeller model the mission deck is built from.
///
/// Exposed so a consumer that needs a *per-engine* quantity the orchestrator
/// does not aggregate - the field-performance contract's ground-roll mean
/// thrust, its uncertainty bands and its operating envelope - reads it from
/// the same model the mission flies, instead of rebuilding the catalogue
/// mapping and drifting from it.
#[must_use]
pub fn turboprop_unit_model(payload: &alas_config::TurbopropEngineSpec) -> Pw127m568fModel {
    Pw127m568fModel {
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

    /// The turboprop inverse now answers through the rating-fraction solve
    /// wherever that map brackets the request, and through the governor's own
    /// normalized-force inverse everywhere else. The two must remain one
    /// operating point, not two, so this pins the contract the routing rests
    /// on: the delivered thrust closes on the request to the solver's own
    /// tolerance, the point stays between the idle floor and the rating, and
    /// fuel flow rises monotonically with the request across the whole band -
    /// which is exactly what would break if the fast path started answering
    /// off the PW127M's folded low-power region instead of falling back.
    #[test]
    fn the_turboprop_inverse_closes_on_the_request_across_the_whole_thrust_band() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": "ATR72-600"}))
            .unwrap_or_else(|error| panic!("preset: {error}"));
        let deck = PropulsionDeck::from_engine(
            &config.geometry.engine,
            config.requirements.cruise_mach,
            config.requirements.cruise_altitude_m,
            max_climb_rate_ft_min(config.mission.profile.initial_climb_rate_m_s),
        )
        .unwrap_or_else(|error| panic!("ATR deck: {error}"));
        for (altitude_m, tas_m_s) in [(1_000.0, 90.0), (4_000.0, 120.0), (5_180.0, 136.8)] {
            let flight = deck
                .flight_condition(altitude_m, tas_m_s, 9.80665, 0.0)
                .unwrap_or_else(|error| panic!("{altitude_m} m: {error}"));
            let idle = deck.idle_point(flight).unwrap();
            let rated = deck
                .rated_point(flight, PropulsionRating::MaximumClimb)
                .unwrap();
            let mut previous: Option<(f64, f64)> = None;
            for step in 1..=16 {
                let required_n =
                    idle.thrust_n + f64::from(step) / 17.0 * (rated.thrust_n - idle.thrust_n);
                let point = deck
                    .at_thrust(flight, required_n, PropulsionRating::MaximumClimb)
                    .unwrap_or_else(|error| panic!("{altitude_m} m, {required_n:.0} N: {error}"));
                // A floored answer is the deck refusing to run that low, and
                // it reports the floor rather than the request; every other
                // answer must be the request.
                if point.limit == ThrustLimit::Unlimited {
                    assert!(
                        (point.thrust_n - required_n).abs()
                            <= THRUST_SOLVE_RELATIVE_TOLERANCE * required_n.max(1.0),
                        "{altitude_m} m: asked {required_n:.3} N, got {:.3} N",
                        point.thrust_n
                    );
                }
                assert!(
                    point.fuel_flow_kg_s > 0.0 && point.fuel_flow_kg_s <= rated.fuel_flow_kg_s,
                    "{altitude_m} m at {required_n:.0} N: {} kg/s outside the deck's own band",
                    point.fuel_flow_kg_s
                );
                if let Some((previous_thrust_n, previous_flow_kg_s)) = previous {
                    assert!(
                        point.thrust_n >= previous_thrust_n - 1.0e-6 * rated.thrust_n
                            && point.fuel_flow_kg_s
                                >= previous_flow_kg_s - 1.0e-6 * rated.fuel_flow_kg_s,
                        "{altitude_m} m: {previous_thrust_n:.1} N / {previous_flow_kg_s:.5} kg/s \
                         then {:.1} N / {:.5} kg/s",
                        point.thrust_n,
                        point.fuel_flow_kg_s
                    );
                }
                previous = Some((point.thrust_n, point.fuel_flow_kg_s));
            }
        }
    }
    /// The inverse thrust solve reads its reference force from the rating map
    /// rather than from a full-command normalized-force demand. That is only
    /// legitimate if the two are the same number, so pin it on both decks and
    /// at conditions spanning the flown envelope: a deck that changed which
    /// rating normalizes its force command would otherwise silently move the
    /// first guess, and on the turboprop the first guess *is* the answer.
    #[test]
    fn a_full_command_normalized_force_is_the_deck_normalizing_rating() {
        let turbofan = default_deck();
        let atr_config = AlasConfig::from_value(&serde_json::json!({"preset": "ATR72-600"}))
            .unwrap_or_else(|error| panic!("preset: {error}"));
        let turboprop = PropulsionDeck::from_engine(
            &atr_config.geometry.engine,
            atr_config.requirements.cruise_mach,
            atr_config.requirements.cruise_altitude_m,
            max_climb_rate_ft_min(atr_config.mission.profile.initial_climb_rate_m_s),
        )
        .unwrap_or_else(|error| panic!("ATR deck: {error}"));
        for deck in [&turbofan, &turboprop] {
            for (altitude_m, tas_m_s) in [(0.0, 80.0), (4_000.0, 120.0), (9_000.0, 200.0)] {
                let flight = deck
                    .flight_condition(altitude_m, tas_m_s, 9.80665, 0.0)
                    .unwrap_or_else(|error| panic!("{altitude_m} m: {error}"));
                let normalized = deck.normalized_point(flight, 1.0).unwrap();
                let rated = deck.rated_point(flight, deck.normalizing_rating()).unwrap();
                // The two arrive by different routes through the same model,
                // so they agree to rounding (measured within one ulp), not
                // necessarily to the bit. A tolerance this tight still fails
                // on any real change of which rating normalizes the command.
                assert!(
                    (normalized.thrust_n - rated.thrust_n).abs()
                        <= 1.0e-12 * rated.thrust_n.abs().max(1.0),
                    "{:?} at {altitude_m} m: full command {} N against {:?} {} N",
                    deck.kind(),
                    normalized.thrust_n,
                    deck.normalizing_rating(),
                    rated.thrust_n
                );
            }
        }
    }
}
