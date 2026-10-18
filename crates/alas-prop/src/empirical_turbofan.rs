// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Empirical off-design thrust deck for transport turbofans.
//!
//! One engine model, Jenkinson's charts, serves take-off ([takeoff_chart]),
//! climb ([max_climb]) and cruise (0.93 of climb, Torenbeek 2013 and
//! Jenkinson); the Bartel-Young/OpenAP take-off lapse it replaced is the
//! second source of the take-off band. The deck
//! is deliberately separate from the conceptual fixed-area cycle: a mission
//! deck must reproduce an installed engine's certified static rating. The
//! catalogue's part-power cruise thrust sets the throttle basis of the
//! cruise TSFC anchor only.

use crate::mission_turbofan::components::part_power_fuel_fraction;
use crate::system::*;

mod correlation;
mod flat_rating;
mod installation;
pub mod max_climb;
mod ratings;
pub mod takeoff_chart;
use correlation::*;
use flat_rating::*;
pub use installation::InstallationLosses;
use installation::{tsfc_shape, CoreFlowBasis};
use max_climb::*;

/// Data required by the empirical thrust and fuel deck.
#[derive(Debug, Clone, PartialEq)]
pub struct EmpiricalTurbofanDeck {
    /// Installed all-engine sea-level-static takeoff thrust, N.
    pub takeoff_thrust_n: f64,
    /// Installed all-engine thrust at which `cruise_reference_tsfc_kg_kgf_h`
    /// is quoted, at the reference altitude and Mach, N: the catalogue's
    /// part-power cruise thrust/SFC pair. It sets the throttle basis of the
    /// cruise TSFC anchor only and must not exceed the maximum-climb thrust
    /// there. The maximum-climb rating itself is derived from
    /// `takeoff_thrust_n` and `bypass_ratio` ([max_climb]). A catalogue
    /// figure up to 15 % above that rating is accepted (see
    /// `EmpiricalTurbofanModel::catalogue_cruise_over_maximum_climb`). The
    /// name predates that change, when this thrust was (wrongly) the climb
    /// anchor.
    pub max_climb_reference_thrust_n: f64,
    /// Reference pressure altitude of the cruise TSFC anchor and of the
    /// catalogue cruise thrust, m: 9,144 m < h <= 11,000 m.
    pub max_climb_reference_altitude_m: f64,
    /// Reference Mach number of the cruise TSFC anchor and of the catalogue
    /// cruise thrust: 0.4 <= M <= 0.9.
    pub max_climb_reference_mach: f64,
    /// Engine bypass ratio used by the takeoff correlation and to blend the
    /// Jenkinson maximum-climb charts.
    pub bypass_ratio: f64,
    /// Installed all-engine ICAO LTO takeoff fuel flow, kg/s.
    pub takeoff_fuel_flow_kg_s: f64,
    /// Installed cruise-reference thrust-specific fuel consumption at
    /// `max_climb_reference_thrust_n`, kg/(kgf h). This anchors high-altitude
    /// fuel flow independently of the sea-level-static ICAO LTO schedule.
    pub cruise_reference_tsfc_kg_kgf_h: f64,
    /// Normalized ICAO LTO fuel flow at 7, 30, 85 and 100% static thrust.
    pub part_power_fuel_flow_ratios: [f64; 4],
    /// Optional representative maximum-climb rate used by the empirical
    /// schedule, ft/min. `None` means that no source rate is available; the
    /// correlation's rate-dependent terms then use their zero-rate baseline.
    /// It is intentionally optional so construction cannot invent a universal
    /// aircraft-independent rate.
    pub max_climb_rate_ft_min: Option<f64>,
    /// Flight-idle fraction of the take-off rating (which is never below the
    /// maximum-climb rating at the same condition).
    pub flight_idle_fraction: f64,
    /// Ambient temperature at sea-level static up to which the rating is flat,
    /// K (the engine certificate's flat-rating temperature, per engine);
    /// `None` when no datum is known, in which case no thrust is lost to hot
    /// ambient temperature. See `flat_rating::flat_rating_thrust_factor`.
    pub flat_rating_temperature_k: Option<f64>,
    /// Whether `max_climb_reference_thrust_n` is a published datum of this
    /// engine (a manufacturer or ICAO-class cruise point of this engine or
    /// its declared family) rather than a class statistic computed from the
    /// rating. If so, the chart maximum-climb table is scaled by one factor so
    /// that its maximum-cruise rating at the reference point equals that
    /// datum; otherwise the chart is used unscaled.
    pub cruise_reference_is_engine_datum: bool,
    /// Whether that datum is a maximum-climb thrust (scale `F / MCL`, not `F / (0.93 MCL)`).
    pub cruise_reference_is_maximum_climb: bool,
    /// Standing service losses charged to the TSFC anchors.
    pub installation: InstallationLosses,
}

/// Mission model coupling a sourced thrust deck to absolute ICAO LTO fuel
/// anchors. Fuel remains explicitly extrapolated until an off-design fuel
/// deck is calibrated; the retained legacy model supplies installation and
/// mass inventory only.
pub struct EmpiricalTurbofanModel {
    deck: EmpiricalTurbofanDeck,
    fuel_model: LegacyTurbofanModel,
    provenance: ModelProvenance,
    tsfc: TsfcBasis,
    climb_reference: ClimbReference,
    core_flow: CoreFlowBasis,
}

/// Standard gravity defining the kilogram-force, m/s^2 (CGPM 1901).
const STANDARD_GRAVITY_M_S2: f64 = 9.806_65;

/// Highest Mach number accepted by [validate_flight].
const MAXIMUM_DECK_MACH: f64 = 0.9;

/// Full-rating thrust-specific fuel consumption closed on the deck's two
/// declared anchors.
///
/// At every altitude the TSFC follows the static-anchored linear-in-Mach
/// form with a square-root temperature correction,
///
/// `c(M, theta) = c_SLS * theta^0.5 * (1 + k M)`,
///
/// from Mair & Birdsall, "Aircraft Performance" (1992) and ESDU 73019, as
/// transcribed by O. Schulz, "Assessment of Numerical Models for Thrust and
/// Specific Fuel Consumption for Turbofan Engines", HAW Hamburg (2007),
/// Eq. 2.21 (linear Mach term, anchored on the static sea-level TSFC) and
/// Eqs. 2.15/2.17 (theta^0.5; Martinez-Val & Perez, J. Aircraft 29(4), 1992).
/// The power law `c ~ M^n` of Eq. 2.15/2.17 is not used because it vanishes
/// at M = 0 and cannot hold the sea-level-static ICAO anchor. Schulz gives no
/// value for the engine-dependent slope `k`; it is closed here on the engine's
/// own anchors, `c(M_ref, theta_ref) = c_cruise`, so no aircraft- or
/// preset-specific factor enters.
///
/// Because `k` is closed on the cruise anchor, the form is identical to the
/// cruise-referenced form of the same source,
/// `c = c_cruise * (theta/theta_ref)^0.5 * (1 + k M)/(1 + k M_ref)`, so one
/// expression holds both anchors exactly and keeps the temperature and Mach
/// trends above the reference altitude and on non-standard days. The previous
/// altitude smoothstep onto a constant cruise TSFC dropped both trends at and
/// above the reference altitude.
///
/// Throttle basis of the cruise anchor: the catalogue pairs the cruise TSFC
/// with `off_design.cruise_reference_thrust_n` at the same altitude and Mach
/// (the OpenAP `engines.csv` cruise thrust/SFC pair, both quoted at the
/// cruise altitude and Mach, or Svoboda's class cruise thrust where no pair
/// is published). That is a part-power cruise setting, at utilization
/// `u_c = F_cruise / F_MCL` of the maximum-climb rating there. The fuel flow
/// is the full-rating TSFC times the rated thrust times the ICAO part-power
/// fraction `f(u)`, so the full-rating TSFC at the reference is closed as
/// `c_cruise * u_c / f(u_c)`: the catalogue TSFC is delivered exactly at the
/// catalogue cruise thrust, and the part-power shape is counted once.
#[derive(Debug, Clone, Copy, PartialEq)]
struct TsfcBasis {
    /// ICAO take-off fuel flow over take-off thrust at sea-level static,
    /// kg/(N s).
    static_tsfc_kg_n_s: f64,
    /// US 1976 sea-level static temperature defining theta, K.
    sea_level_temperature_k: f64,
    /// Linear Mach coefficient `k` closed on the two anchors, 1/Mach.
    mach_slope: f64,
}

impl EmpiricalTurbofanModel {
    /// Construct and validate an empirical mission deck.
    pub fn new(
        deck: EmpiricalTurbofanDeck,
        fuel_model: LegacyTurbofanModel,
        provenance: ModelProvenance,
    ) -> Result<Self, PropulsionError> {
        let values = [
            deck.takeoff_thrust_n,
            deck.max_climb_reference_thrust_n,
            deck.max_climb_reference_altitude_m,
            deck.max_climb_reference_mach,
            deck.bypass_ratio,
            deck.takeoff_fuel_flow_kg_s,
            deck.cruise_reference_tsfc_kg_kgf_h,
            deck.flight_idle_fraction,
        ];
        if values.iter().any(|value| !value.is_finite())
            || deck.takeoff_thrust_n <= 0.0
            || deck.max_climb_reference_thrust_n <= 0.0
            || deck.max_climb_reference_altitude_m <= 9_144.0
            || deck.max_climb_reference_altitude_m > ANCHOR_MAXIMUM_ALTITUDE_M
            || !(ANCHOR_MINIMUM_MACH..=ANCHOR_MAXIMUM_MACH).contains(&deck.max_climb_reference_mach)
            || deck.bypass_ratio <= 0.0
            || deck.takeoff_fuel_flow_kg_s <= 0.0
            || deck.cruise_reference_tsfc_kg_kgf_h <= 0.0
            || deck
                .part_power_fuel_flow_ratios
                .iter()
                .any(|ratio| !ratio.is_finite() || *ratio < 0.0)
            || deck
                .part_power_fuel_flow_ratios
                .windows(2)
                .any(|pair| pair[1] < pair[0])
            || (deck.part_power_fuel_flow_ratios[3] - 1.0).abs() > 1.0e-9
            || deck
                .max_climb_rate_ft_min
                .is_some_and(|rate| !rate.is_finite() || rate < 0.0)
            || !(0.0..1.0).contains(&deck.flight_idle_fraction)
            || !flat_rating_temperature_is_valid(deck.flat_rating_temperature_k)
            || !deck.installation.is_valid()
        {
            return Err(PropulsionError::InvalidInput {
                field: "empirical turbofan deck",
                value: f64::NAN,
            });
        }
        let static_tsfc_kg_n_s = deck.takeoff_fuel_flow_kg_s / deck.takeoff_thrust_n;
        let sea_level = alas_atmo::us1976_compute_values(0.0, 0.0);
        let sea_level_temperature_k = sea_level.temperature_k;
        let reference = alas_atmo::us1976_compute_values(deck.max_climb_reference_altitude_m, 0.0);
        let reference_temperature_k = reference.temperature_k;
        let (climb_table, engine_scale, max_climb_thrust_n) =
            anchored_table(&deck, reference.pressure_pa / sea_level.pressure_pa).map_err(
                |value| PropulsionError::InvalidInput {
                    field: "empirical turbofan engine anchor scale",
                    value,
                },
            )?;
        // The catalogue cruise thrust is a part-power setting of that rating.
        // A catalogue figure up to 15 % above the chart rating is a
        // full-rating point of the TSFC anchor (utilization held at 1, raw
        // ratio kept); further above it contradicts the inputs: rejected.
        let catalogue_cruise_over_maximum_climb =
            deck.max_climb_reference_thrust_n / max_climb_thrust_n;
        if !catalogue_cruise_over_maximum_climb.is_finite()
            || !(0.07..=MAXIMUM_CATALOGUE_CRUISE_OVER_MAXIMUM_CLIMB)
                .contains(&catalogue_cruise_over_maximum_climb)
        {
            return Err(PropulsionError::InvalidInput {
                field: "empirical turbofan cruise thrust over maximum-climb thrust",
                value: catalogue_cruise_over_maximum_climb,
            });
        }
        let reference_full_rating_tsfc_kg_n_s = deck.cruise_reference_tsfc_kg_kgf_h
            / (STANDARD_GRAVITY_M_S2 * 3_600.0)
            / tsfc_shape(
                catalogue_cruise_over_maximum_climb,
                deck.max_climb_reference_altitude_m,
                deck.max_climb_reference_altitude_m,
                deck.part_power_fuel_flow_ratios,
            );
        let mach_slope = (reference_full_rating_tsfc_kg_n_s
            / (static_tsfc_kg_n_s * (reference_temperature_k / sea_level_temperature_k).sqrt())
            - 1.0)
            / deck.max_climb_reference_mach;
        // The closure needs a non-zero reference Mach, and the resulting TSFC
        // must stay positive over the whole accepted Mach domain.
        if !mach_slope.is_finite() || 1.0 + mach_slope * MAXIMUM_DECK_MACH <= 0.0 {
            return Err(PropulsionError::InvalidInput {
                field: "empirical turbofan TSFC Mach slope",
                value: mach_slope,
            });
        }
        let climb_reference = ClimbReference {
            table: climb_table,
            engine_scale,
            max_climb_thrust_n,
            catalogue_cruise_over_maximum_climb,
        };
        Ok(Self {
            core_flow: CoreFlowBasis::new(&fuel_model)?,
            deck,
            fuel_model,
            provenance,
            climb_reference,
            tsfc: TsfcBasis {
                static_tsfc_kg_n_s,
                sea_level_temperature_k,
                mach_slope,
            },
        })
    }

    /// Full-rating TSFC at `flight`, kg/(N s), from the [TsfcBasis] form.
    ///
    /// Sea-level static reproduces the ICAO take-off TSFC; at the US 1976
    /// standard-day reference altitude and Mach the declared cruise TSFC is
    /// reproduced at the declared cruise thrust. `theta` uses the flight's
    /// ambient temperature, so non-standard days and off-reference Mach
    /// numbers move TSFC at every altitude.
    fn full_rating_tsfc_kg_n_s(&self, flight: FlightCondition) -> f64 {
        let theta = flight.temperature_k / self.tsfc.sea_level_temperature_k;
        self.tsfc.static_tsfc_kg_n_s * theta.sqrt() * (1.0 + self.tsfc.mach_slope * flight.mach)
    }

    fn rating_fraction_and_thrust(
        &self,
        flight: FlightCondition,
        demand: PropulsionDemand,
    ) -> Result<(f64, f64), PropulsionError> {
        // Each rating is evaluated only when the demand reads it: the three
        // correlations cost two `powf`, a `ln` and two CAS conversions.
        let maximum_climb = || self.climb_available_n(flight, self.deck.max_climb_rate_ft_min);
        match demand {
            // A valid normalized demand is resolved in `evaluate`, which also
            // needs the maximum-climb thrust for its idle floor.
            PropulsionDemand::NormalizedForce(value) => Err(PropulsionError::InvalidInput {
                field: "normalized force demand",
                value,
            }),
            PropulsionDemand::Rating(PropulsionRating::TakeoffGoAround) => {
                Ok((1.0, self.takeoff_available_n(flight)))
            }
            PropulsionDemand::Rating(PropulsionRating::MaximumClimb) => Ok((1.0, maximum_climb())),
            // No public engine-specific MCT deck is available. Keep MCT on
            // the climb envelope and report the model as extrapolated.
            PropulsionDemand::Rating(PropulsionRating::MaximumContinuous) => {
                Ok((1.0, maximum_climb()))
            }
            // Maximum cruise: the zero-rate climb correlation at the
            // chart ratio of maximum cruise to maximum climb. Its
            // utilization of the maximum-climb rating (the basis of the
            // part-power fuel schedule) is that ratio.
            PropulsionDemand::Rating(PropulsionRating::Cruise) => Ok((
                MAXIMUM_CRUISE_OVER_MAXIMUM_CLIMB,
                MAXIMUM_CRUISE_OVER_MAXIMUM_CLIMB * self.climb_available_n(flight, Some(0.0)),
            )),
            PropulsionDemand::Rating(PropulsionRating::FlightIdle) => Ok((
                self.deck.flight_idle_fraction,
                self.deck.flight_idle_fraction * self.takeoff_available_n(flight),
            )),
            PropulsionDemand::RatedFraction { rating, fraction }
                if fraction.is_finite() && (0.0..=1.0).contains(&fraction) =>
            {
                let (fuel_fraction, rated_thrust) =
                    self.rating_fraction_and_thrust(flight, PropulsionDemand::Rating(rating))?;
                Ok((fuel_fraction * fraction, rated_thrust * fraction))
            }
            PropulsionDemand::RatedFraction { fraction, .. } => {
                Err(PropulsionError::InvalidInput {
                    field: "rated force fraction",
                    value: fraction,
                })
            }
            PropulsionDemand::RequiredBodyForceN(_) => Err(PropulsionError::UnsupportedDemand(
                "empirical turbofan deck uses the mission's bounded scalar inverse solve",
            )),
        }
    }
}

impl PropulsionSystemModel for EmpiricalTurbofanModel {
    fn evaluate(&self, request: &PropulsionRequest) -> Result<PropulsionResult, PropulsionError> {
        if request.mode != OperatingMode::Normal {
            return Err(PropulsionError::UnsupportedMode(request.mode));
        }
        if request.failure != FailureState::None {
            return Err(PropulsionError::UnsupportedFailureState);
        }
        let loads = installation::checked_loads(request.loads)?;
        validate_flight(request.flight)?;
        let (rating_utilization, thrust_n, achieved_demand, active_limits) = match request.demand {
            PropulsionDemand::NormalizedForce(requested_fraction)
                if requested_fraction.is_finite() && (0.0..=1.0).contains(&requested_fraction) =>
            {
                let maximum_climb =
                    self.climb_available_n(request.flight, self.deck.max_climb_rate_ft_min);
                let requested_thrust_n = requested_fraction * maximum_climb;
                let idle_thrust =
                    self.deck.flight_idle_fraction * self.takeoff_available_n(request.flight);
                if requested_thrust_n < idle_thrust {
                    let achieved_fraction = if maximum_climb > 0.0 {
                        idle_thrust / maximum_climb
                    } else {
                        requested_fraction
                    };
                    // Below flight idle the engine delivers idle thrust, so
                    // fuel is scheduled from the achieved idle utilization of
                    // the maximum-climb rating, not from the request:
                    // every saturated request burns the same idle fuel.
                    (
                        achieved_fraction,
                        idle_thrust,
                        PropulsionDemand::NormalizedForce(achieved_fraction),
                        vec![ActiveLimit {
                            name: "flight-idle-thrust".to_owned(),
                            utilization: 1.0,
                        }],
                    )
                } else {
                    (
                        requested_fraction,
                        requested_thrust_n,
                        request.demand,
                        Vec::new(),
                    )
                }
            }
            demand => {
                let (utilization, thrust) =
                    self.rating_fraction_and_thrust(request.flight, demand)?;
                (utilization, thrust, demand, Vec::new())
            }
        };
        if !thrust_n.is_finite() {
            return Err(PropulsionError::InvalidInput {
                field: "empirical turbofan thrust",
                value: thrust_n,
            });
        }
        // Fuel scheduling must use utilization of the active flight rating,
        // not thrust divided by sea-level-static takeoff thrust. The latter
        // mistakes a fully commanded high-altitude engine for part power and
        // has no thermodynamic meaning.
        let rating_utilization = rating_utilization.clamp(0.07, 1.0);
        // Fuel flow is TSFC (interpolated, not absolute flow) times the thrust,
        // the part-power shape and the installed-to-uninstalled load factor.
        let shape = tsfc_shape(
            rating_utilization,
            request.flight.altitude_m,
            self.deck.max_climb_reference_altitude_m,
            self.deck.part_power_fuel_flow_ratios,
        );
        let fuel_flow = self.full_rating_tsfc_kg_n_s(request.flight)
            * thrust_n
            * shape
            * self.installation_sfc_factor(request.flight, loads);
        if !fuel_flow.is_finite() || fuel_flow < 0.0 {
            return Err(PropulsionError::NonFiniteOutput(
                "ICAO-LTO fuel interpolation",
            ));
        }
        Ok(PropulsionResult {
            body_force_n: [thrust_n, 0.0, 0.0],
            body_moment_nm: [0.0; 3],
            resource_flows: vec![ResourceFlow {
                resource: ResourceKind::JetA,
                mass_flow_kg_s: Some(fuel_flow),
                power_w: None,
            }],
            state_derivatives: vec![StateDerivative {
                state: "jet_a_mass_kg".to_owned(),
                rate_per_s: -fuel_flow,
                unit: "kg/s".to_owned(),
            }],
            shaft_power_w: None,
            electrical_power_w: None,
            heat_rejection_w: None,
            torque_nm: None,
            rotational_speed_rpm: None,
            achieved_demand,
            active_limits,
            residuals: Vec::new(),
            validity: ValidityStatus::Extrapolated {
                reason: "maximum-climb thrust is read from Jenkinson's engine charts (Civil Jet Aircraft Design, Figs. 9.18(b)-9.21(b), BPR 3-13, reading uncertainty +-0.01-0.02 of the static thrust), maximum cruise is 0.93 of it (Torenbeek 2013 Sec. 9.2, Jenkinson Figs. 9.19(c), 9.21(c)), take-off is read from the take-off charts of the same book (Figs. 9.18(a)-9.21(a); the Bartel-Young/OpenAP correlation is the comparison source of the take-off band); fuel is TSFC times lapsed thrust, with TSFC joining the ICAO takeoff and cruise-TSFC anchors through a Mach/temperature form and remaining uncalibrated between anchors".to_owned(),
            },
            provenance: self.provenance.clone(),
            trace: None,
        })
    }

    fn capability(
        &self,
        flight: FlightCondition,
        failure: FailureState,
    ) -> Result<PropulsionCapability, PropulsionError> {
        if failure != FailureState::None {
            return Err(PropulsionError::UnsupportedFailureState);
        }
        validate_flight(flight)?;
        Ok(PropulsionCapability {
            maximum_body_force_n: [
                self.climb_available_n(flight, self.deck.max_climb_rate_ft_min),
                0.0,
                0.0,
            ],
            minimum_body_force_n: [
                self.deck.flight_idle_fraction * self.takeoff_available_n(flight),
                0.0,
                0.0,
            ],
            maximum_shaft_power_w: None,
            active_limits: Vec::new(),
            validity: ValidityStatus::Extrapolated {
                reason: "Jenkinson-chart empirical mission thrust deck".to_owned(),
            },
            provenance: self.provenance.clone(),
        })
    }

    fn mass_inventory(&self) -> &[PropulsionMassItem] {
        self.fuel_model.mass_inventory()
    }
    fn installation(&self) -> &PropulsionInstallation {
        self.fuel_model.installation()
    }
    fn provenance(&self) -> &ModelProvenance {
        &self.provenance
    }
    fn diagnostics(&self, query: DiagnosticsQuery) -> PropulsionDiagnostics {
        PropulsionDiagnostics {
            query,
            items: vec![DiagnosticItem {
                code: "bartel-young-openap-thrust".to_owned(),
                message: "Maximum-climb thrust from Jenkinson's engine charts over the whole envelope, maximum cruise 0.93 of it, Jenkinson take-off charts (Figs. 9.18(a)-9.21(a)); fuel uses active-rating utilization and a TSFC joining the ICAO takeoff and cruise-TSFC anchors.".to_owned(),
            }],
            provenance: self.provenance.clone(),
        }
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission_turbofan::{
        size_turbofan_to_static_rating, TurbofanInputs, VehicleBuilderParams,
    };

    fn model() -> EmpiricalTurbofanModel {
        model_with_climb_rate(None)
    }

    fn model_with_climb_rate(max_climb_rate_ft_min: Option<f64>) -> EmpiricalTurbofanModel {
        model_from_deck(EmpiricalTurbofanDeck {
            takeoff_thrust_n: 235_800.0,
            max_climb_reference_thrust_n: 44_482.0,
            max_climb_reference_altitude_m: 10_668.0,
            max_climb_reference_mach: 0.8,
            bypass_ratio: 5.9,
            takeoff_fuel_flow_kg_s: 2.284,
            cruise_reference_tsfc_kg_kgf_h: 0.58,
            part_power_fuel_flow_ratios: [0.0893, 0.2767, 0.8222, 1.0],
            max_climb_rate_ft_min,
            flight_idle_fraction: 0.07,
            flat_rating_temperature_k: None,
            cruise_reference_is_engine_datum: false,
            cruise_reference_is_maximum_climb: false,
            installation: InstallationLosses::NONE,
        })
    }

    /// Two CFM56-5B4/3 as the mission builds them from the engine catalogue
    /// (`alas-config/data/engines.json`): catalogue thrust, ICAO EEDB
    /// 01P08CM105 take-off fuel flow, bypass ratio and part-power ratios,
    /// catalogue cruise TSFC and the declared maximum-climb anchor.
    fn a320_class_model() -> EmpiricalTurbofanModel {
        model_from_deck(EmpiricalTurbofanDeck {
            takeoff_thrust_n: 2.0 * 117_770.0,
            max_climb_reference_thrust_n: 2.0 * 22_241.0,
            max_climb_reference_altitude_m: 10_668.0,
            max_climb_reference_mach: 0.8,
            bypass_ratio: 5.7,
            takeoff_fuel_flow_kg_s: 2.0 * 1.142,
            cruise_reference_tsfc_kg_kgf_h: 0.60,
            part_power_fuel_flow_ratios: [0.089_316_99, 0.276_707_53, 0.822_241_68, 1.0],
            max_climb_rate_ft_min: None,
            flight_idle_fraction: 0.07,
            flat_rating_temperature_k: None,
            cruise_reference_is_engine_datum: false,
            cruise_reference_is_maximum_climb: false,
            installation: InstallationLosses::NONE,
        })
    }

    fn model_from_deck(deck: EmpiricalTurbofanDeck) -> EmpiricalTurbofanModel {
        let inputs = TurbofanInputs {
            number_of_engines: 2.0,
            bypass_ratio: deck.bypass_ratio,
            overall_pressure_ratio: 27.1,
            fan_pressure_ratio: 1.6,
            turbine_inlet_temperature_k: 1_600.0,
            cruise_mach: deck.max_climb_reference_mach,
            cruise_altitude_m: deck.max_climb_reference_altitude_m,
            design_thrust_total_n: deck.takeoff_thrust_n,
        };
        let params = VehicleBuilderParams::default();
        let sized = size_turbofan_to_static_rating(&inputs, &params);
        let provenance = ModelProvenance {
            model: ModelIdentity {
                family: "test".to_owned(),
                version: "1".to_owned(),
            },
            dataset: None,
            sources: Vec::new(),
        };
        let legacy = LegacyTurbofanModel::new(
            inputs,
            params,
            sized.compressor_nondimensional_massflow,
            provenance.clone(),
            Vec::new(),
            PropulsionInstallation {
                unit_positions_m: vec![[0.0, -5.0, 0.0], [0.0, 5.0, 0.0]],
                thrust_axes_body: vec![[1.0, 0.0, 0.0]; 2],
                nacelle_wetted_area_m2: None,
                frontal_area_m2: None,
            },
        )
        .unwrap_or_else(|error| panic!("legacy fixture: {error}"));
        EmpiricalTurbofanModel::new(deck, legacy, provenance)
            .unwrap_or_else(|error| panic!("empirical fixture: {error}"))
    }

    /// US 1976 standard-day flight condition at `altitude_m` and `mach`.
    fn standard_flight(altitude_m: f64, mach: f64) -> FlightCondition {
        let air = alas_atmo::us1976_compute_values(altitude_m, 0.0);
        let mut flight = sea_level_static();
        flight.altitude_m = altitude_m;
        flight.mach = mach;
        flight.pressure_pa = air.pressure_pa;
        flight.temperature_k = air.temperature_k;
        flight.density_kg_m3 = air.density_kg_m3;
        flight.speed_of_sound_m_s = air.speed_of_sound_m_s;
        flight.velocity_m_s = mach * air.speed_of_sound_m_s;
        flight
    }

    fn request_at(flight: FlightCondition, demand: PropulsionDemand) -> PropulsionRequest {
        PropulsionRequest {
            flight,
            demand,
            mode: OperatingMode::Normal,
            failure: FailureState::None,
            loads: PropulsionLoads::default(),
            state: PropulsionState::default(),
            time_step_s: None,
        }
    }

    /// Fuel flow over thrust of an evaluated request, kg/(kgf h).
    fn evaluated_tsfc_kg_kgf_h(
        model: &EmpiricalTurbofanModel,
        flight: FlightCondition,
        demand: PropulsionDemand,
    ) -> f64 {
        let result = model
            .evaluate(&request_at(flight, demand))
            .unwrap_or_else(|error| panic!("evaluate: {error}"));
        let fuel_flow_kg_s = result.resource_flows[0].mass_flow_kg_s.unwrap();
        fuel_flow_kg_s * flight.gravity_m_s2 * 3_600.0 / result.body_force_n[0]
    }

    /// Mach number of `cas_m_s` at `flight`'s pressure, by bisection.
    fn mach_for_cas(cas: f64, flight: FlightCondition) -> f64 {
        let (mut low, mut high) = (0.0, 0.9);
        for _ in 0..80 {
            let mid = 0.5 * (low + high);
            if cas_m_s(mid, flight.pressure_pa, flight.gamma) < cas {
                low = mid;
            } else {
                high = mid;
            }
        }
        0.5 * (low + high)
    }

    fn sea_level_static() -> FlightCondition {
        FlightCondition {
            altitude_m: 0.0,
            mach: 0.0,
            pressure_pa: 101_325.0,
            temperature_k: 288.15,
            density_kg_m3: 1.225,
            dynamic_viscosity_pa_s: 1.7894e-5,
            gravity_m_s2: 9.80665,
            gamma: 1.4,
            cp_j_kgk: 1_004.5,
            gas_constant_j_kgk: 287.05287,
            speed_of_sound_m_s: 340.294,
            velocity_m_s: 0.0,
            stagnation_temperature_k: 288.15,
            stagnation_pressure_pa: 101_325.0,
        }
    }

    #[test]
    fn standard_pressure_is_sensible_at_openap_anchor() {
        assert!((23_000.0..25_000.0).contains(&standard_pressure_pa(10_668.0)));
    }

    /// Pressure ratio `delta` at a height in feet.
    fn delta_ft(feet: f64) -> f64 {
        standard_pressure_pa(feet * 0.3048) / 101_325.0
    }

    #[test]
    fn the_table_reproduces_jenkinsons_maximum_climb_charts_at_their_nodes() {
        // Jenkinson, Civil Jet Aircraft Design, Figs. 9.18(b)-9.21(b), read
        // from the printed charts: the chart nodes are returned exactly for
        // an engine of the chart's own bypass ratio.
        for (bpr, feet, mach, expected) in [
            (6.5, 30_000.0, 0.8, 0.237),
            (6.5, 36_000.0, 0.8, 0.193),
            (6.5, 40_000.0, 0.9, 0.163),
            (6.5, 0.0, 0.4, 0.558),
            (6.5, 10_000.0, 0.6, 0.406),
            (8.0, 36_000.0, 0.7, 0.194),
            (13.0, 30_000.0, 0.8, 0.220),
            (13.0, 45_000.0, 0.6, 0.124),
            (3.0, 30_000.0, 0.6, 0.272),
            (3.0, 20_000.0, 0.6, 0.340),
        ] {
            let got = MaximumClimbTable::new(bpr).ratio(mach, delta_ft(feet));
            assert!(
                (got - expected).abs() < 1.0e-12,
                "BPR {bpr} {feet} ft M{mach}: {got} vs {expected}"
            );
        }
    }

    #[test]
    fn the_table_interpolates_linearly_in_mach_pressure_and_bypass_ratio() {
        let table = MaximumClimbTable::new(6.5);
        // Mach: halfway between the 0.7 and 0.8 samples.
        let at = |mach| table.ratio(mach, delta_ft(36_000.0));
        assert!((at(0.75) - 0.5 * (0.185 + 0.193)).abs() < 1.0e-12);
        // Pressure ratio: halfway between 30,000 and 36,000 ft in delta.
        let mid = 0.5 * (delta_ft(30_000.0) + delta_ft(36_000.0));
        let got = table.ratio(0.8, mid);
        assert!((got - 0.5 * (0.237 + 0.193)).abs() < 1.0e-12, "{got}");
        // Bypass ratio: halfway between the 6.5 and 8.0 charts.
        let bpr = MaximumClimbTable::new(7.25).ratio(0.8, delta_ft(36_000.0));
        assert!((bpr - 0.5 * (0.193 + 0.194)).abs() < 1.0e-12, "{bpr}");
        // Outside BPR 3-13 the nearest chart is held; Mach is held at the
        // sampled ends.
        let point = |bpr: f64| MaximumClimbTable::new(bpr).ratio(0.8, delta_ft(30_000.0));
        assert_eq!(point(1.0), point(3.0));
        assert_eq!(point(20.0), point(13.0));
        assert_eq!(table.ratio(0.1, 1.0), table.ratio(0.3, 1.0));
    }

    #[test]
    fn the_charts_lapse_in_proportion_to_pressure_above_the_tropopause() {
        // Torenbeek 2013 Secs. 8.5 and 9.2.1: in the stratosphere T/delta of
        // a fixed rating at fixed Mach is constant. The charts show it
        // (Jenkinson BPR 6.5, M 0.8, 45 over 40 kft: 0.129 / 0.156 = 0.83;
        // the pressure ratio is 0.79), so the table needs no separate
        // stratospheric law.
        for bpr in [3.0, 6.5, 8.0, 13.0] {
            let table = MaximumClimbTable::new(bpr);
            let high = table.ratio(0.8, delta_ft(45_000.0));
            let low = table.ratio(0.8, delta_ft(40_000.0));
            let lapse = delta_ft(45_000.0) / delta_ft(40_000.0);
            assert!(
                ((high / low) / lapse - 1.0).abs() < 0.08,
                "BPR {bpr}: {} vs {lapse}",
                high / low
            );
        }
    }

    #[test]
    fn the_anchor_is_the_chart_ratio_times_the_static_thrust_at_the_reference_point() {
        let model = model();
        let flight = standard_flight(10_668.0, 0.8);
        let (_, thrust) = model
            .rating_fraction_and_thrust(
                flight,
                PropulsionDemand::Rating(PropulsionRating::MaximumClimb),
            )
            .unwrap_or_else(|error| panic!("maximum-climb anchor: {error}"));
        let ratio = MaximumClimbTable::new(5.9).ratio(0.8, flight.pressure_pa / 101_325.0);
        assert!((thrust - 235_800.0 * ratio).abs() < 1.0e-6, "{thrust}");
        assert!((model.climb_reference.max_climb_thrust_n - thrust).abs() < 1.0e-6);
        // Between the BPR 3 and 6.5 charts at 35,000 ft, M 0.8.
        assert!((0.19..0.24).contains(&ratio), "{ratio}");
    }

    #[test]
    fn maximum_cruise_is_the_chart_fraction_of_maximum_climb_and_is_the_cruise_rating() {
        // Torenbeek 2013 Sec. 9.2: a maximum-climb to cruise ratio of 1.08;
        // Jenkinson Figs. 9.19(b)/(c), 9.21(b)/(c): 0.91-0.96, mean 0.93.
        assert!((0.91..0.96).contains(&MAXIMUM_CRUISE_OVER_MAXIMUM_CLIMB));
        assert!((1.0 / MAXIMUM_CRUISE_OVER_MAXIMUM_CLIMB - 1.08).abs() < 0.01);
        let model = model();
        for (altitude_m, mach) in [(10_668.0, 0.8), (11_887.0, 0.84), (9_500.0, 0.7)] {
            let flight = standard_flight(altitude_m, mach);
            let rating = |r| {
                model
                    .rating_fraction_and_thrust(flight, PropulsionDemand::Rating(r))
                    .unwrap_or_else(|error| panic!("rating: {error}"))
            };
            let (climb_utilization, climb) = rating(PropulsionRating::MaximumClimb);
            let (cruise_utilization, cruise) = rating(PropulsionRating::Cruise);
            assert_eq!(climb_utilization, 1.0);
            assert_eq!(cruise_utilization, MAXIMUM_CRUISE_OVER_MAXIMUM_CLIMB);
            assert!(cruise < climb, "{cruise} < {climb}");
            assert!(
                (cruise / climb - MAXIMUM_CRUISE_OVER_MAXIMUM_CLIMB).abs() < 1.0e-12,
                "{altitude_m} m"
            );
        }
    }

    #[test]
    fn an_engine_datum_scales_the_chart_so_maximum_cruise_meets_it_at_the_reference_point() {
        let base = a320_class_model();
        let chart_n = base.climb_reference.max_climb_thrust_n;
        for cruise_n in [0.9 * 0.93 * chart_n, 1.1 * 0.93 * chart_n] {
            let mut deck = base.deck.clone();
            deck.max_climb_reference_thrust_n = cruise_n;
            deck.cruise_reference_is_engine_datum = true;
            let model = EmpiricalTurbofanModel::new(
                deck,
                a320_class_model().fuel_model,
                a320_class_model().provenance,
            )
            .unwrap_or_else(|error| panic!("scaled model: {error}"));
            let factor = model.engine_scale().expect("scaled");
            assert!((factor - cruise_n / (0.93 * chart_n)).abs() < 1.0e-12);
            // Maximum cruise at the reference point is the datum.
            let flight = standard_flight(
                model.deck.max_climb_reference_altitude_m,
                model.deck.max_climb_reference_mach,
            );
            let (_, cruise) = model
                .rating_fraction_and_thrust(
                    flight,
                    PropulsionDemand::Rating(PropulsionRating::Cruise),
                )
                .unwrap_or_else(|error| panic!("cruise: {error}"));
            assert!(
                (cruise / cruise_n - 1.0).abs() < 1.0e-12,
                "{cruise} {cruise_n}"
            );
            // The shape is the chart's: every point scales by the same factor.
            let other = standard_flight(5_000.0, 0.6);
            let scaled = model.climb_available_n(other, None);
            let chart = base.climb_available_n(other, None);
            assert!((scaled / chart - factor).abs() < 1.0e-12);
        }
        // Without an engine datum the chart is used unscaled and flagged.
        assert_eq!(base.engine_scale(), None);
    }

    #[test]
    fn a_maximum_climb_datum_scales_the_chart_by_the_datum_over_maximum_climb() {
        let base = a320_class_model();
        let chart_n = base.climb_reference.max_climb_thrust_n;
        let climb_n = 1.04 * chart_n;
        let mut deck = base.deck.clone();
        deck.max_climb_reference_thrust_n = climb_n;
        deck.cruise_reference_is_engine_datum = true;
        deck.cruise_reference_is_maximum_climb = true;
        let model = EmpiricalTurbofanModel::new(deck, base.fuel_model, base.provenance)
            .unwrap_or_else(|error| panic!("scaled model: {error}"));
        // k = F / MCL, not F / (0.93 MCL).
        let factor = model.engine_scale().expect("scaled");
        assert!((factor - 1.04).abs() < 1.0e-12, "{factor}");
        let flight = standard_flight(
            model.deck.max_climb_reference_altitude_m,
            model.deck.max_climb_reference_mach,
        );
        assert!((model.climb_available_n(flight, None) / climb_n - 1.0).abs() < 1.0e-12);
        assert!((model.catalogue_cruise_over_maximum_climb() - 1.0).abs() < 1.0e-12);
    }

    #[test]
    fn the_chart_scatter_is_the_root_sum_of_the_bypass_and_rating_spreads() {
        // 4.18 % (BPR 6.5/8/13 half-range at FL350, M 0.85) and 2.84 %
        // (maximum-cruise over maximum-climb, 0.9085-0.9615), rounded up.
        let rss = (0.0418_f64.powi(2) + 0.0284_f64.powi(2)).sqrt();
        assert!((rss - 0.0505).abs() < 1.0e-4);
        assert_eq!(CHART_THRUST_SCATTER, 0.051);
        assert!(CHART_THRUST_SCATTER >= rss && CHART_THRUST_SCATTER < rss + 0.001);
    }

    #[test]
    fn an_engine_scale_far_from_the_chart_is_rejected() {
        let base = a320_class_model();
        let mut deck = base.deck.clone();
        deck.max_climb_reference_thrust_n = 2.0 * base.climb_reference.max_climb_thrust_n;
        deck.cruise_reference_is_engine_datum = true;
        assert!(matches!(
            EmpiricalTurbofanModel::new(deck, base.fuel_model, base.provenance),
            Err(PropulsionError::InvalidInput { .. })
        ));
    }

    #[test]
    fn a_catalogue_cruise_thrust_slightly_above_the_chart_rating_is_a_full_rating_anchor() {
        let model = a320_class_model();
        let mut deck = model.deck.clone();
        let maximum_climb = model.climb_reference.max_climb_thrust_n;
        deck.max_climb_reference_thrust_n = 1.08 * maximum_climb;
        let capped = EmpiricalTurbofanModel::new(deck, model.fuel_model, model.provenance)
            .unwrap_or_else(|error| panic!("8 % above the chart rating is accepted: {error}"));
        assert!((capped.catalogue_cruise_over_maximum_climb() - 1.08).abs() < 1.0e-12);
        assert!(a320_class_model().catalogue_cruise_over_maximum_climb() < 1.0);
    }

    #[test]
    fn the_chart_climb_rating_follows_the_published_low_altitude_values() {
        // Jenkinson Fig. 9.19(b), BPR 6.5: sea level M 0.4 0.558, 10,000 ft
        // M 0.6 0.406; a BPR 6.5 deck reproduces them within 2 %. (The
        // earlier anchor-plus-lapse form fell 13-19 % below the chart here.)
        let model = model_from_deck(EmpiricalTurbofanDeck {
            bypass_ratio: 6.5,
            ..a320_class_model().deck
        });
        let ratio = |altitude_m: f64, mach: f64| {
            let flight = standard_flight(altitude_m, mach);
            model.climb_available_n(flight, None) / model.deck.takeoff_thrust_n
        };
        assert!((ratio(0.0, 0.4) / 0.558 - 1.0).abs() < 0.02);
        assert!((ratio(3_048.0, 0.6) / 0.406 - 1.0).abs() < 0.02);
    }

    #[test]
    fn maximum_climb_stays_below_takeoff_thrust_at_low_speed() {
        // Climb is a derate of the takeoff rating. Checked where the
        // Bartel-Young takeoff relation is valid (M < 0.4).
        let model = a320_class_model();
        for altitude_m in [0.0, 500.0, 1_000.0, 2_000.0, 3_000.0] {
            for mach in [0.2, 0.25, 0.3, 0.35, 0.39] {
                let flight = standard_flight(altitude_m, mach);
                let climb = model.climb_available_n(flight, model.deck.max_climb_rate_ft_min);
                let takeoff = model.takeoff_available_n(flight);
                assert!(
                    climb < takeoff,
                    "{altitude_m} m M{mach}: {climb} >= {takeoff}"
                );
            }
        }
    }

    #[test]
    fn a_cruise_thrust_far_above_the_maximum_climb_rating_is_rejected() {
        let model = a320_class_model();
        let mut deck = model.deck.clone();
        deck.max_climb_reference_thrust_n = 1.2 * model.climb_reference.max_climb_thrust_n;
        assert!(matches!(
            EmpiricalTurbofanModel::new(deck, model.fuel_model, model.provenance),
            Err(PropulsionError::InvalidInput { .. })
        ));
    }

    #[test]
    fn named_idle_is_seven_percent_not_seven_percent_squared() {
        let model = model();
        let (_, takeoff) = model
            .rating_fraction_and_thrust(
                sea_level_static(),
                PropulsionDemand::Rating(PropulsionRating::TakeoffGoAround),
            )
            .unwrap_or_else(|error| panic!("takeoff: {error}"));
        let (_, idle) = model
            .rating_fraction_and_thrust(
                sea_level_static(),
                PropulsionDemand::Rating(PropulsionRating::FlightIdle),
            )
            .unwrap_or_else(|error| panic!("idle: {error}"));
        assert!((idle / takeoff - 0.07).abs() < 1.0e-12);
    }

    #[test]
    fn rated_fraction_is_relative_to_the_named_rating_once() {
        let model = model();
        let (_, rated) = model
            .rating_fraction_and_thrust(
                sea_level_static(),
                PropulsionDemand::Rating(PropulsionRating::MaximumClimb),
            )
            .unwrap_or_else(|error| panic!("rated: {error}"));
        let (_, half) = model
            .rating_fraction_and_thrust(
                sea_level_static(),
                PropulsionDemand::RatedFraction {
                    rating: PropulsionRating::MaximumClimb,
                    fraction: 0.5,
                },
            )
            .unwrap_or_else(|error| panic!("half rating: {error}"));
        assert!((half / rated - 0.5).abs() < 1.0e-12);
    }

    #[test]
    fn cruise_uses_zero_rate_schedule_when_no_climb_rate_is_available() {
        let model = model();
        let mut flight = sea_level_static();
        flight.altitude_m = 5_000.0;
        flight.pressure_pa = standard_pressure_pa(flight.altitude_m);
        flight.mach = 0.4;
        flight.velocity_m_s = flight.mach * flight.speed_of_sound_m_s;
        let (_, climb) = model
            .rating_fraction_and_thrust(
                flight,
                PropulsionDemand::Rating(PropulsionRating::MaximumClimb),
            )
            .unwrap_or_else(|error| panic!("maximum climb: {error}"));
        let (_, cruise) = model
            .rating_fraction_and_thrust(flight, PropulsionDemand::Rating(PropulsionRating::Cruise))
            .unwrap_or_else(|error| panic!("cruise: {error}"));
        // The zero-rate correlation times the maximum-cruise fraction.
        assert!(
            (cruise - MAXIMUM_CRUISE_OVER_MAXIMUM_CLIMB * climb).abs() < 1.0e-9,
            "{cruise} {climb}"
        );
    }

    #[test]
    fn invalid_flight_domain_is_rejected() {
        let mut flight = sea_level_static();
        flight.pressure_pa = -1.0;
        assert!(matches!(
            model().capability(flight, FailureState::None),
            Err(PropulsionError::InvalidInput { .. })
        ));
    }

    #[test]
    fn observed_transport_step_cruise_above_fl400_is_inside_the_domain() {
        let mut flight = sea_level_static();
        flight.altitude_m = 41_000.0 * 0.3048;
        flight.pressure_pa = standard_pressure_pa(flight.altitude_m);
        flight.mach = 0.85;
        flight.velocity_m_s = flight.mach * flight.speed_of_sound_m_s;
        assert!(model().capability(flight, FailureState::None).is_ok());
        flight.altitude_m = 45_001.0 * 0.3048;
        flight.pressure_pa = standard_pressure_pa(flight.altitude_m);
        assert!(matches!(
            model().capability(flight, FailureState::None),
            Err(PropulsionError::InvalidInput { .. })
        ));
    }

    #[test]
    fn absolute_icao_takeoff_and_idle_fuel_anchors_are_reproduced() {
        let model = model();
        let request = |demand| PropulsionRequest {
            flight: sea_level_static(),
            demand,
            mode: OperatingMode::Normal,
            failure: FailureState::None,
            loads: PropulsionLoads::default(),
            state: PropulsionState::default(),
            time_step_s: None,
        };
        let takeoff = model
            .evaluate(&request(PropulsionDemand::Rating(
                PropulsionRating::TakeoffGoAround,
            )))
            .unwrap_or_else(|error| panic!("takeoff fuel: {error}"));
        let idle = model
            .evaluate(&request(PropulsionDemand::Rating(
                PropulsionRating::FlightIdle,
            )))
            .unwrap_or_else(|error| panic!("idle fuel: {error}"));
        let takeoff_flow = takeoff.resource_flows[0]
            .mass_flow_kg_s
            .unwrap_or_else(|| panic!("takeoff Jet-A flow"));
        let idle_flow = idle.resource_flows[0]
            .mass_flow_kg_s
            .unwrap_or_else(|| panic!("idle Jet-A flow"));
        assert!((takeoff_flow - 2.284).abs() < 1.0e-12);
        assert!((idle_flow - 2.284 * 0.0893).abs() < 1.0e-12);
        assert_eq!(takeoff.state_derivatives[0].rate_per_s, -takeoff_flow);
        assert_eq!(idle.state_derivatives[0].rate_per_s, -idle_flow);
    }

    #[test]
    fn full_high_altitude_rating_is_not_misclassified_as_static_part_power() {
        let model = model();
        let mut flight = sea_level_static();
        flight.altitude_m = model.deck.max_climb_reference_altitude_m;
        flight.pressure_pa = standard_pressure_pa(flight.altitude_m);
        flight.mach = model.deck.max_climb_reference_mach;
        flight.velocity_m_s = flight.mach * flight.speed_of_sound_m_s;
        let full = model
            .evaluate(&PropulsionRequest {
                flight,
                demand: PropulsionDemand::Rating(PropulsionRating::MaximumClimb),
                mode: OperatingMode::Normal,
                failure: FailureState::None,
                loads: PropulsionLoads::default(),
                state: PropulsionState::default(),
                time_step_s: None,
            })
            .unwrap_or_else(|error| panic!("full climb rating: {error}"));
        let half = model
            .evaluate(&PropulsionRequest {
                flight,
                demand: PropulsionDemand::RatedFraction {
                    rating: PropulsionRating::MaximumClimb,
                    fraction: 0.5,
                },
                mode: OperatingMode::Normal,
                failure: FailureState::None,
                loads: PropulsionLoads::default(),
                state: PropulsionState::default(),
                time_step_s: None,
            })
            .unwrap_or_else(|error| panic!("half climb rating: {error}"));
        let full_flow = full.resource_flows[0].mass_flow_kg_s.unwrap();
        let half_flow = half.resource_flows[0].mass_flow_kg_s.unwrap();
        assert!(full_flow > half_flow);
        assert!((full_flow / half_flow - 1.0).abs() > 0.1);
    }

    #[test]
    fn normalized_force_saturates_at_the_advertised_flight_idle_limit() {
        let model = model();
        let flight = sea_level_static();
        let capability = model
            .capability(flight, FailureState::None)
            .unwrap_or_else(|error| panic!("capability: {error}"));
        let result = model
            .evaluate(&PropulsionRequest {
                flight,
                demand: PropulsionDemand::NormalizedForce(0.0),
                mode: OperatingMode::Normal,
                failure: FailureState::None,
                loads: PropulsionLoads::default(),
                state: PropulsionState::default(),
                time_step_s: None,
            })
            .unwrap_or_else(|error| panic!("normalized idle: {error}"));
        assert!((result.body_force_n[0] - capability.minimum_body_force_n[0]).abs() < 1.0e-9);
        assert!(matches!(
            result.achieved_demand,
            PropulsionDemand::NormalizedForce(value) if value > 0.0
        ));
        assert_eq!(result.active_limits[0].name, "flight-idle-thrust");
        assert_eq!(result.active_limits[0].utilization, 1.0);
    }

    #[test]
    fn fuel_follows_the_achieved_thrust_and_equals_idle_fuel_at_the_floor() {
        let model = model();
        for flight in [
            sea_level_static(),
            standard_flight(3_000.0, 0.45),
            standard_flight(10_668.0, 0.8),
        ] {
            let evaluate = |fraction: f64| {
                let result = model
                    .evaluate(&request_at(
                        flight,
                        PropulsionDemand::NormalizedForce(fraction),
                    ))
                    .unwrap_or_else(|error| panic!("normalized {fraction}: {error}"));
                (
                    result.body_force_n[0],
                    result.resource_flows[0].mass_flow_kg_s.unwrap(),
                    result.achieved_demand,
                )
            };
            let (idle_thrust_n, idle_fuel_kg_s, achieved) = evaluate(0.0);
            let PropulsionDemand::NormalizedForce(idle_fraction) = achieved else {
                panic!("achieved demand is a normalized force");
            };
            assert!(idle_fuel_kg_s > 0.0);
            // Every request at or below the idle floor delivers the same
            // thrust and burns the same fuel as the floor itself.
            for fraction in [
                0.0,
                0.25 * idle_fraction,
                0.5 * idle_fraction,
                idle_fraction,
            ] {
                let (thrust_n, fuel_kg_s, _) = evaluate(fraction);
                assert!((thrust_n - idle_thrust_n).abs() < 1.0e-9 * idle_thrust_n);
                assert!(
                    (fuel_kg_s - idle_fuel_kg_s).abs() < 1.0e-12 * idle_fuel_kg_s,
                    "{fraction}: {fuel_kg_s} vs idle {idle_fuel_kg_s}"
                );
            }
            // Above the floor, fuel never decreases as achieved thrust rises.
            let (mut last_thrust_n, mut last_fuel_kg_s) = (idle_thrust_n, idle_fuel_kg_s);
            for step in 0..=40 {
                let (thrust_n, fuel_kg_s, _) = evaluate(f64::from(step) / 40.0);
                assert!(thrust_n >= last_thrust_n - 1.0e-9);
                assert!(
                    fuel_kg_s >= last_fuel_kg_s * (1.0 - 1.0e-12),
                    "step {step}: {fuel_kg_s} < {last_fuel_kg_s}"
                );
                (last_thrust_n, last_fuel_kg_s) = (thrust_n, fuel_kg_s);
            }
        }
    }

    #[test]
    fn normalized_force_at_one_matches_the_advertised_maximum() {
        let model = model();
        let flight = sea_level_static();
        let capability = model
            .capability(flight, FailureState::None)
            .unwrap_or_else(|error| panic!("capability: {error}"));
        let result = model
            .evaluate(&PropulsionRequest {
                flight,
                demand: PropulsionDemand::NormalizedForce(1.0),
                mode: OperatingMode::Normal,
                failure: FailureState::None,
                loads: PropulsionLoads::default(),
                state: PropulsionState::default(),
                time_step_s: None,
            })
            .unwrap_or_else(|error| panic!("normalized maximum: {error}"));
        assert!((result.body_force_n[0] - capability.maximum_body_force_n[0]).abs() < 1.0e-9);
        assert!(result.active_limits.is_empty());
    }

    #[test]
    fn tsfc_rises_monotonically_between_the_anchors_along_a_constant_cas_climb() {
        let model = model();
        let full = PropulsionDemand::Rating(PropulsionRating::MaximumClimb);
        let takeoff_tsfc = 2.284 * 9.806_65 * 3_600.0 / 235_800.0;
        // Full-rating TSFC at the reference point, closed on the cruise anchor.
        let cruise_tsfc = evaluated_tsfc_kg_kgf_h(
            &model,
            standard_flight(
                model.deck.max_climb_reference_altitude_m,
                model.deck.max_climb_reference_mach,
            ),
            full,
        );
        let static_tsfc = evaluated_tsfc_kg_kgf_h(&model, sea_level_static(), full);
        assert!((static_tsfc - takeoff_tsfc).abs() < 1.0e-12);

        // Climb at 300 kt CAS, then at the reference Mach, from sea level to
        // the reference altitude. TSFC rises strictly while it is below the
        // reference value. Once the climb reaches the reference Mach below the
        // reference altitude, the warmer air's sqrt(theta) term leaves TSFC
        // above the anchor by sqrt(T/T_ref) and it then falls onto the anchor
        // with altitude; that is the physical temperature trend.
        let reference_temperature_k =
            standard_flight(model.deck.max_climb_reference_altitude_m, 0.0).temperature_k;
        let cas = 300.0 * 1_852.0 / 3_600.0;
        let mut previous = static_tsfc;
        let mut altitude_m: f64 = 0.0;
        let mut rising_to_m = 0.0;
        while altitude_m <= model.deck.max_climb_reference_altitude_m {
            let mach = mach_for_cas(cas, standard_flight(altitude_m, 0.0))
                .min(model.deck.max_climb_reference_mach);
            let flight = standard_flight(altitude_m, mach);
            let tsfc = evaluated_tsfc_kg_kgf_h(&model, flight, full);
            if previous < cruise_tsfc {
                assert!(
                    tsfc > previous,
                    "TSFC {tsfc} at {altitude_m} m after {previous}"
                );
                rising_to_m = altitude_m;
            }
            let temperature_bound = (flight.temperature_k / reference_temperature_k).sqrt();
            assert!(
                tsfc <= temperature_bound * cruise_tsfc * (1.0 + 1.0e-12),
                "TSFC {tsfc} at {altitude_m} m"
            );
            previous = tsfc;
            altitude_m += 250.0;
        }
        assert!(rising_to_m > 8_000.0, "rising only to {rising_to_m} m");

        let mach_6_km = mach_for_cas(cas, standard_flight(6_000.0, 0.0));
        let tsfc_6_km = evaluated_tsfc_kg_kgf_h(&model, standard_flight(6_000.0, mach_6_km), full);
        assert!(
            takeoff_tsfc < tsfc_6_km && tsfc_6_km < cruise_tsfc,
            "{takeoff_tsfc} < {tsfc_6_km} < {cruise_tsfc}"
        );
    }

    #[test]
    fn declared_cruise_tsfc_is_delivered_at_the_reference_condition_and_thrust() {
        // The catalogue pairs the cruise TSFC with the cruise reference
        // thrust at the reference altitude and Mach, a part-power setting of
        // the maximum-climb rating. The mission commands the deck with
        // NormalizedForce relative to that rating; at the reference condition
        // a unit demand is the maximum-climb anchor, and the demand that
        // delivers the cruise thrust delivers the declared TSFC.
        for model in [model(), a320_class_model()] {
            let flight = standard_flight(
                model.deck.max_climb_reference_altitude_m,
                model.deck.max_climb_reference_mach,
            );
            let maximum_climb = model.climb_reference.max_climb_thrust_n;
            let result = model
                .evaluate(&request_at(flight, PropulsionDemand::NormalizedForce(1.0)))
                .unwrap();
            assert!((result.body_force_n[0] - maximum_climb).abs() < 1.0e-9);
            let cruise_utilization = model.deck.max_climb_reference_thrust_n / maximum_climb;
            let tsfc = evaluated_tsfc_kg_kgf_h(
                &model,
                flight,
                PropulsionDemand::NormalizedForce(cruise_utilization),
            );
            assert!(
                (tsfc - model.deck.cruise_reference_tsfc_kg_kgf_h).abs() < 1.0e-12,
                "TSFC {tsfc}"
            );
            // At the anchor altitude the sea-level ICAO dip is not carried
            // (it is not measured there): TSFC is flat in throttle above the
            // utilization where f(u)/u crosses one, and keeps the sourced
            // low-power rise below it.
            let ratio =
                |u: f64| part_power_fuel_fraction(u, model.deck.part_power_fuel_flow_ratios) / u;
            for utilization in [0.5, 0.95, 1.0] {
                let part = evaluated_tsfc_kg_kgf_h(
                    &model,
                    flight,
                    PropulsionDemand::NormalizedForce(utilization),
                );
                assert!(
                    (part - model.deck.cruise_reference_tsfc_kg_kgf_h).abs() < 1.0e-12,
                    "u {utilization}"
                );
            }
            let fraction = 0.1;
            let low = evaluated_tsfc_kg_kgf_h(
                &model,
                flight,
                PropulsionDemand::RatedFraction {
                    rating: PropulsionRating::Cruise,
                    fraction,
                },
            );
            let utilization = MAXIMUM_CRUISE_OVER_MAXIMUM_CLIMB * fraction;
            let expected = model.deck.cruise_reference_tsfc_kg_kgf_h * ratio(utilization);
            assert!(ratio(utilization) > 1.0);
            assert!((low - expected).abs() < 1.0e-12, "low {low} vs {expected}");
        }
    }

    /// The sourced sea-level-static ICAO LTO shape is kept at sea level: the
    /// 85 % row of the data bank is delivered there, not a flat TSFC.
    #[test]
    fn the_icao_part_power_dip_is_kept_at_sea_level_static() {
        let model = a320_class_model();
        let flight = standard_flight(0.0, 0.0);
        let takeoff = PropulsionDemand::Rating(PropulsionRating::TakeoffGoAround);
        let full = model
            .evaluate(&request_at(flight, takeoff))
            .unwrap()
            .resource_flows[0]
            .mass_flow_kg_s
            .unwrap();
        assert!((full - model.deck.takeoff_fuel_flow_kg_s).abs() < 1.0e-9 * full);
        for (fraction, ratio) in [
            (0.85, 0.822_241_68),
            (0.30, 0.276_707_53),
            (0.07, 0.089_316_99),
        ] {
            let part = model
                .evaluate(&request_at(
                    flight,
                    PropulsionDemand::RatedFraction {
                        rating: PropulsionRating::TakeoffGoAround,
                        fraction,
                    },
                ))
                .unwrap()
                .resource_flows[0]
                .mass_flow_kg_s
                .unwrap();
            assert!(
                (part / full - ratio).abs() < 1.0e-6,
                "{fraction}: {} vs {ratio}",
                part / full
            );
        }
    }

    #[test]
    fn tsfc_keeps_temperature_and_mach_trends_above_the_reference_altitude() {
        let model = model();
        let cruise = PropulsionDemand::Rating(PropulsionRating::Cruise);
        let reference_altitude_m = model.deck.max_climb_reference_altitude_m;
        let reference_mach = model.deck.max_climb_reference_mach;
        let reference = standard_flight(reference_altitude_m, reference_mach);
        let c_ref = evaluated_tsfc_kg_kgf_h(&model, reference, cruise);
        let slope = model.tsfc.mach_slope;
        let expected = |flight: FlightCondition| {
            c_ref
                * (flight.temperature_k / reference.temperature_k).sqrt()
                * (1.0 + slope * flight.mach)
                / (1.0 + slope * reference_mach)
        };
        for (altitude_m, mach) in [
            (reference_altitude_m, 0.70),
            (reference_altitude_m, 0.85),
            (11_500.0, 0.78),
            (12_500.0, 0.82),
        ] {
            let flight = standard_flight(altitude_m, mach);
            let tsfc = evaluated_tsfc_kg_kgf_h(&model, flight, cruise);
            assert!(
                (tsfc - expected(flight)).abs() < 1.0e-12,
                "{altitude_m} m M{mach}"
            );
        }

        // Mach: faster is costlier at fixed altitude above the reference.
        let slow = evaluated_tsfc_kg_kgf_h(&model, standard_flight(11_500.0, 0.75), cruise);
        let fast = evaluated_tsfc_kg_kgf_h(&model, standard_flight(11_500.0, 0.85), cruise);
        assert!(fast > slow, "{fast} > {slow}");

        // Temperature: an ISA+15 K day above the reference raises TSFC by
        // sqrt(T_hot/T_std); an ISA-15 K day lowers it.
        let standard = standard_flight(11_500.0, 0.8);
        let mut hot = standard;
        hot.temperature_k += 15.0;
        let mut cold = standard;
        cold.temperature_k -= 15.0;
        let tsfc_std = evaluated_tsfc_kg_kgf_h(&model, standard, cruise);
        let tsfc_hot = evaluated_tsfc_kg_kgf_h(&model, hot, cruise);
        let tsfc_cold = evaluated_tsfc_kg_kgf_h(&model, cold, cruise);
        assert!(tsfc_cold < tsfc_std && tsfc_std < tsfc_hot);
        let ratio = (hot.temperature_k / standard.temperature_k).sqrt();
        assert!((tsfc_hot / tsfc_std - ratio).abs() < 1.0e-12);

        // In the isothermal stratosphere at constant Mach TSFC is constant.
        let low = evaluated_tsfc_kg_kgf_h(&model, standard_flight(11_200.0, 0.8), cruise);
        let high = evaluated_tsfc_kg_kgf_h(&model, standard_flight(13_000.0, 0.8), cruise);
        assert!((low - high).abs() < 1.0e-12);
    }

    #[test]
    fn a320_class_fl207_cruise_tsfc_is_not_inflated_by_the_static_fuel_flow() {
        // A320-200 final cruise from the 2026-09 preset census: FL207
        // (6324.45 m), M 0.715, 80.3% of maximum-climb thrust.
        let model = a320_class_model();
        let flight = standard_flight(6_324.45, 0.715);
        let utilization = 0.803;
        let result = model
            .evaluate(&request_at(
                flight,
                PropulsionDemand::NormalizedForce(utilization),
            ))
            .unwrap_or_else(|error| panic!("A320 cruise: {error}"));
        let thrust_n = result.body_force_n[0];
        let fuel_flow_kg_s = result.resource_flows[0].mass_flow_kg_s.unwrap();
        let tsfc = fuel_flow_kg_s * flight.gravity_m_s2 * 3_600.0 / thrust_n;
        // The superseded schedule, which blended absolute take-off fuel flow
        // into the lapsed rating by altitude, gave 0.80-0.87 here.
        assert!((0.55..0.65).contains(&tsfc), "TSFC {tsfc}");
    }

    /// Flight-envelope grid of the rating-order tests: altitude 0-11 km,
    /// Mach 0-0.9.
    fn envelope_grid() -> Vec<(f64, f64)> {
        let mut grid = Vec::new();
        for altitude_m in (0..=11).map(|km| f64::from(km) * 1_000.0) {
            for mach in (0..=18).map(|step| f64::from(step) * 0.05) {
                grid.push((altitude_m, mach));
            }
        }
        grid
    }

    #[test]
    fn rating_order_idle_below_climb_below_takeoff_over_the_whole_envelope() {
        for model in [model(), a320_class_model()] {
            for (altitude_m, mach) in envelope_grid() {
                let flight = standard_flight(altitude_m, mach);
                let takeoff = model.takeoff_available_n(flight);
                let climb = model.climb_available_n(flight, model.deck.max_climb_rate_ft_min);
                let idle = model.deck.flight_idle_fraction * takeoff;
                if mach > TAKEOFF_CORRELATION_MAXIMUM_MACH {
                    assert!(
                        idle < climb && climb <= takeoff,
                        "{altitude_m} m M{mach}: idle {idle} climb {climb} takeoff {takeoff}"
                    );
                } else {
                    // Below the climb correlation's domain only the
                    // take-off/idle order is defined.
                    assert!(idle < takeoff, "{altitude_m} m M{mach}");
                }
            }
        }
    }

    #[test]
    fn the_takeoff_rating_is_the_larger_of_the_take_off_chart_and_maximum_climb() {
        let model = a320_class_model();
        let table = takeoff_chart::TakeoffTable::new(5.7);
        // The take-off chart held at M 0.4 and extended in proportion to
        // pressure above its 20,000 ft node; the rating is `max(chart,
        // climb)` everywhere, so the order climb <= take-off holds.
        for altitude_m in [3_048.0, 6_096.0, 10_668.0, 12_000.0, 13_716.0] {
            let flight = standard_flight(altitude_m, 0.8);
            let chart_n =
                table.ratio(0.4, flight.pressure_pa / 101_325.0) * model.deck.takeoff_thrust_n;
            let climb = model.climb_available_n(flight, None);
            assert_eq!(model.takeoff_available_n(flight), chart_n.max(climb));
        }
        // Low and slow: the Jenkinson chart is the rating, with no floor
        // binding (maximum climb is 0.4-0.6 of static there, the chart 0.8+).
        let low = standard_flight(0.0, 0.2);
        let expected = table.ratio(0.2, low.pressure_pa / 101_325.0) * model.deck.takeoff_thrust_n;
        assert_eq!(model.takeoff_available_n(low), expected);
        assert!(expected > model.climb_available_n(low, None));
        // Static: exactly the installed static take-off thrust.
        assert_eq!(
            model.takeoff_available_n(sea_level_static()),
            model.deck.takeoff_thrust_n
        );
    }

    fn maximum_climb_ratio(model: &EmpiricalTurbofanModel, altitude_m: f64, mach: f64) -> f64 {
        let flight = standard_flight(altitude_m, mach);
        let thrust_n = model.climb_available_n(flight, model.deck.max_climb_rate_ft_min);
        thrust_n / model.deck.takeoff_thrust_n
    }

    /// Standard pressure ratio `delta` at `altitude_m`.
    fn delta(altitude_m: f64) -> f64 {
        standard_pressure_pa(altitude_m) / 101_325.0
    }

    #[test]
    fn the_maximum_climb_level_agrees_with_the_independent_references() {
        // The chart anchor against Mattingly's high-bypass maximum thrust,
        // alpha = delta0 (1 - 0.49 sqrt(M)) (coefficients as recalled, not
        // re-read), at FL350-FL390, M 0.8: within +-15 %. The earlier Howe
        // anchor was 17-25 % above it (audit PROP-06).
        let model = a320_class_model();
        for feet in [35_000.0, 39_000.0] {
            let altitude_m = feet * 0.3048;
            let mach: f64 = 0.8;
            let delta0 = delta(altitude_m) * (1.0 + 0.2 * mach * mach).powf(3.5);
            let mattingly = delta0 * (1.0 - 0.49 * f64::sqrt(mach));
            let deck = maximum_climb_ratio(&model, altitude_m, mach);
            assert!(
                (0.85..1.15).contains(&(deck / mattingly)),
                "{feet} ft: deck {deck} Mattingly {mattingly}"
            );
        }
        // Jenkinson Ch. 11 worked example, text: lapse 0.2 at 35,000 ft for
        // a Trent-class twin (maximum cruise 0.93 x the maximum-climb value).
        let at_fl350 = maximum_climb_ratio(&model, 10_668.0, 0.8);
        assert!((0.19..0.22).contains(&at_fl350), "{at_fl350}");
    }

    #[test]
    fn flat_rating_break_is_continuous_at_tr_and_derates_above_it() {
        let mut deck = a320_class_model().deck;
        deck.flat_rating_temperature_k = Some(303.15);
        let legacy = a320_class_model().fuel_model;
        let provenance = a320_class_model().provenance;
        let hot = EmpiricalTurbofanModel::new(deck, legacy, provenance).unwrap();
        let flat = a320_class_model();
        let at = |model: &EmpiricalTurbofanModel, delta_t_k: f64| {
            let mut flight = standard_flight(0.0, 0.2);
            flight.temperature_k += delta_t_k;
            flight.stagnation_temperature_k =
                flight.temperature_k * (1.0 + 0.2 * flight.mach.powi(2));
            model.takeoff_available_n(flight)
        };
        // ISA+15 static temperature is 303.15 K; at M 0.2 the total
        // temperature is 0.8 percent higher, so the break sits slightly below.
        for delta_t_k in [0.0, 5.0] {
            assert_eq!(at(&hot, delta_t_k), at(&flat, delta_t_k), "ISA+{delta_t_k}");
        }
        let ratio_hot = at(&hot, 35.0) / at(&flat, 35.0);
        let ratio_warm = at(&hot, 20.0) / at(&flat, 20.0);
        assert!(ratio_hot < ratio_warm && ratio_warm < 1.0 + 1.0e-12);
        assert!(ratio_hot < 0.95, "ISA+35 retains {ratio_hot}");
        // Hand case: TR = 303.15/288.15, theta0 at ISA+35 and M 0.2.
        let tr = 303.15 / 288.15;
        let theta0 = (288.15 + 35.0) * (1.0 + 0.2 * 0.04) / 288.15;
        let expected = 1.0 - 3.0 * (theta0 - tr) / (1.7 * (1.0 - 0.49 * 0.2_f64.sqrt()));
        assert!(
            (ratio_hot - expected).abs() < 1.0e-12,
            "{ratio_hot} vs {expected}"
        );
    }

    #[test]
    fn bleed_and_power_extraction_raise_the_fuel_flow_at_the_same_thrust() {
        let model = model();
        let flight = standard_flight(10_668.0, 0.8);
        let cruise = PropulsionDemand::Rating(PropulsionRating::Cruise);
        let base = model.evaluate(&request_at(flight, cruise)).unwrap();
        let fuel = |result: &PropulsionResult| result.resource_flows[0].mass_flow_kg_s.unwrap();
        // The corrected core flow and compressor power of the deck's own cycle.
        let (total_pa, total_k) = installation::total_conditions(flight);
        let corrected = (total_pa / 101_325.0) / (total_k / 288.15).sqrt();
        let core_kg_s = model.core_flow.static_core_flow_kg_s * corrected * model.core_flow.engines;
        let compressor_w =
            core_kg_s * 1_005.0 * total_k * (model.core_flow.compressor_temperature_ratio - 1.0);
        for (loads, expected) in [
            (
                PropulsionLoads {
                    bleed_mass_flow_kg_s: 0.01 * core_kg_s,
                    ..PropulsionLoads::default()
                },
                0.01 * installation::BLEED_SFC_SLOPE_OPTIMUM_PORT,
            ),
            (
                PropulsionLoads {
                    accessory_power_w: 0.01 * compressor_w,
                    ..PropulsionLoads::default()
                },
                0.01,
            ),
            (
                PropulsionLoads {
                    electrical_power_w: 0.02 * compressor_w,
                    ..PropulsionLoads::default()
                },
                0.02,
            ),
        ] {
            let mut request = request_at(flight, cruise);
            request.loads = loads;
            let loaded = model.evaluate(&request).unwrap();
            // Thrust is the rating, unchanged; fuel flow rises by the SFC increment.
            assert_eq!(loaded.body_force_n, base.body_force_n);
            let increase = fuel(&loaded) / fuel(&base) - 1.0;
            assert!(
                (increase - expected).abs() < 1.0e-9,
                "{increase} vs {expected}"
            );
        }
        for bad in [-1.0, f64::NAN, f64::INFINITY] {
            let mut request = request_at(flight, cruise);
            request.loads.bleed_mass_flow_kg_s = bad;
            assert!(matches!(
                model.evaluate(&request),
                Err(PropulsionError::InvalidInput { .. })
            ));
        }
    }

    /// The standing transport losses of an A320-class engine are a few
    /// per cent of TSFC at cruise (bleed 2 % of core flow at the optimum-port
    /// slope plus Raymer's 150 kW per 133 kN class of shaft power).
    #[test]
    fn the_standing_transport_losses_are_a_few_per_cent_at_cruise() {
        let bare = a320_class_model();
        let mut deck = bare.deck.clone();
        deck.installation = InstallationLosses::transport(117_770.0);
        let installed = model_from_deck(deck);
        let flight = standard_flight(10_668.0, 0.78);
        let demand = PropulsionDemand::NormalizedForce(0.8);
        let ratio = evaluated_tsfc_kg_kgf_h(&installed, flight, demand)
            / evaluated_tsfc_kg_kgf_h(&bare, flight, demand);
        assert!(
            (1.02..1.06).contains(&ratio),
            "installed/uninstalled {ratio}"
        );
        let bleed = installed.deck.installation.bleed_core_flow_fraction
            * installed.deck.installation.bleed_sfc_slope;
        assert!(
            ratio > 1.0 + bleed,
            "power extraction adds to bleed: {ratio}"
        );
    }

    #[test]
    fn an_invalid_flat_rating_temperature_is_rejected() {
        for bad in [250.0, 400.0, f64::NAN] {
            let mut deck = a320_class_model().deck;
            deck.flat_rating_temperature_k = Some(bad);
            let legacy = a320_class_model().fuel_model;
            let provenance = a320_class_model().provenance;
            assert!(EmpiricalTurbofanModel::new(deck, legacy, provenance).is_err());
        }
    }

    #[test]
    fn anchors_that_would_make_tsfc_non_positive_are_rejected() {
        let mut deck = a320_class_model().deck;
        deck.cruise_reference_tsfc_kg_kgf_h = 0.01;
        let legacy = a320_class_model().fuel_model;
        let provenance = a320_class_model().provenance;
        assert!(matches!(
            EmpiricalTurbofanModel::new(deck, legacy, provenance),
            Err(PropulsionError::InvalidInput { .. })
        ));
    }
}
