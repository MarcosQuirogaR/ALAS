// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Empirical off-design thrust deck for transport turbofans.
//!
//! This implements the three-region Bartel-Young maximum-climb correlation
//! as published by OpenAP. It is deliberately separate from the conceptual
//! fixed-area cycle: a mission deck must reproduce an installed engine's
//! certified static rating and an explicitly sourced or estimated
//! maximum-climb thrust anchor.

use crate::mission_turbofan::components::part_power_fuel_fraction;
use crate::system::*;

/// Data required by the Bartel-Young/OpenAP lapse correlation.
#[derive(Debug, Clone, PartialEq)]
pub struct EmpiricalTurbofanDeck {
    /// Installed all-engine sea-level-static takeoff thrust, N.
    pub takeoff_thrust_n: f64,
    /// Installed all-engine maximum climb thrust at the reference point, N.
    pub max_climb_reference_thrust_n: f64,
    /// Maximum-climb reference pressure altitude, m.
    pub max_climb_reference_altitude_m: f64,
    /// Maximum-climb reference Mach number.
    pub max_climb_reference_mach: f64,
    /// Engine bypass ratio used by the low-altitude takeoff correlation.
    pub bypass_ratio: f64,
    /// Installed all-engine ICAO LTO takeoff fuel flow, kg/s.
    pub takeoff_fuel_flow_kg_s: f64,
    /// Installed cruise-reference thrust-specific fuel consumption,
    /// kg/(kgf h). This anchors high-altitude fuel flow independently of the
    /// sea-level-static ICAO LTO schedule.
    pub cruise_reference_tsfc_kg_kgf_h: f64,
    /// Normalized ICAO LTO fuel flow at 7, 30, 85 and 100% static thrust.
    pub part_power_fuel_flow_ratios: [f64; 4],
    /// Optional representative maximum-climb rate used by the empirical
    /// schedule, ft/min. `None` means that no source rate is available; the
    /// correlation's rate-dependent terms then use their zero-rate baseline.
    /// It is intentionally optional so construction cannot invent a universal
    /// aircraft-independent rate.
    pub max_climb_rate_ft_min: Option<f64>,
    /// Flight-idle fraction of the takeoff-lapse deck.
    pub flight_idle_fraction: f64,
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
/// cruise altitude and Mach). The deck uses that thrust as its full rating
/// there, so the catalogue value is the TSFC at rating utilization 1 at the
/// reference condition, where the ICAO part-power factor is exactly 1. The
/// part-power shape therefore applies relative to the reference thrust and
/// is not counted twice; a mission cruising below that thrust delivers the
/// ICAO-shaped part-power TSFC, not the catalogue figure.
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
            || !(0.0..1.0).contains(&deck.max_climb_reference_mach)
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
        {
            return Err(PropulsionError::InvalidInput {
                field: "empirical turbofan deck",
                value: f64::NAN,
            });
        }
        let static_tsfc_kg_n_s = deck.takeoff_fuel_flow_kg_s / deck.takeoff_thrust_n;
        let sea_level_temperature_k = alas_atmo::us1976_compute_values(0.0, 0.0).temperature_k;
        let reference_temperature_k =
            alas_atmo::us1976_compute_values(deck.max_climb_reference_altitude_m, 0.0)
                .temperature_k;
        let cruise_tsfc_kg_n_s =
            deck.cruise_reference_tsfc_kg_kgf_h / (STANDARD_GRAVITY_M_S2 * 3_600.0);
        let mach_slope = (cruise_tsfc_kg_n_s
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
        Ok(Self {
            deck,
            fuel_model,
            provenance,
            tsfc: TsfcBasis {
                static_tsfc_kg_n_s,
                sea_level_temperature_k,
                mach_slope,
            },
        })
    }

    /// Full-rating TSFC at `flight`, kg/(N s), from the [TsfcBasis] form.
    ///
    /// Sea-level static reproduces the ICAO take-off TSFC; the US 1976
    /// standard-day reference altitude and Mach reproduce the declared cruise
    /// TSFC. `theta` uses the flight's ambient temperature, so non-standard
    /// days and off-reference Mach numbers move TSFC at every altitude.
    fn full_rating_tsfc_kg_n_s(&self, flight: FlightCondition) -> f64 {
        let theta = flight.temperature_k / self.tsfc.sea_level_temperature_k;
        self.tsfc.static_tsfc_kg_n_s * theta.sqrt() * (1.0 + self.tsfc.mach_slope * flight.mach)
    }

    fn takeoff_available_n(&self, flight: FlightCondition) -> f64 {
        let delta = flight.pressure_pa / 101_325.0;
        let bpr = self.deck.bypass_ratio;
        let g0 = 0.0606 * bpr + 0.6337;
        let a = -0.4327 * delta.powi(2) + 1.3855 * delta + 0.0472;
        let z = 0.9106 * delta.powi(3) - 1.7736 * delta.powi(2) + 1.8697 * delta;
        let x = 0.1377 * delta.powi(3) - 0.4374 * delta.powi(2) + 1.3003 * delta;
        let ratio = a - 0.377 * (1.0 + bpr) / ((1.0 + 0.82 * bpr) * g0).sqrt() * z * flight.mach
            + (0.23 + 0.19 * bpr.sqrt()) * x * flight.mach.powi(2);
        (ratio * self.deck.takeoff_thrust_n).max(0.0)
    }

    fn climb_available_n(&self, flight: FlightCondition, climb_rate_ft_min: Option<f64>) -> f64 {
        let p = flight.pressure_pa;
        let p10 = standard_pressure_pa(3_048.0);
        let pcr = standard_pressure_pa(self.deck.max_climb_reference_altitude_m);
        let reference_cas = cas_m_s(self.deck.max_climb_reference_mach, pcr, 1.4);
        let cas = cas_m_s(flight.mach, p, flight.gamma);
        let speed_ratio = (cas / reference_cas).max(1.0e-6);
        let mach_ratio = (flight.mach / self.deck.max_climb_reference_mach).max(1.0e-6);
        let roc = climb_rate_ft_min.unwrap_or(0.0).abs();
        let a = speed_ratio.powf(-0.1);
        let n = 2.667e-5 * roc + 0.8633;
        let ratio = if flight.altitude_m > 9_144.0 {
            let d = -0.4204 * mach_ratio + 1.0824;
            d * (p / pcr).ln() + mach_ratio.powf(-0.11)
        } else if flight.altitude_m > 3_048.0 {
            a * (p / pcr).powf(-0.355 * speed_ratio + n)
        } else {
            let f10_ratio = a * (p10 / pcr).powf(-0.355 * speed_ratio + n);
            let m = -0.12043 * speed_ratio - 8.8889e-9 * roc.powi(2) + 2.4444e-5 * roc + 0.47379;
            m * (p / pcr) + (f10_ratio - m * (p10 / pcr))
        };
        (ratio * self.deck.max_climb_reference_thrust_n).max(0.0)
    }

    fn rating_fraction_and_thrust(
        &self,
        flight: FlightCondition,
        demand: PropulsionDemand,
    ) -> Result<(f64, f64), PropulsionError> {
        let maximum_climb = self.climb_available_n(flight, self.deck.max_climb_rate_ft_min);
        let cruise = self.climb_available_n(flight, Some(0.0));
        let takeoff = self.takeoff_available_n(flight);
        match demand {
            PropulsionDemand::NormalizedForce(value)
                if value.is_finite() && (0.0..=1.0).contains(&value) =>
            {
                Ok((value, value * maximum_climb))
            }
            PropulsionDemand::NormalizedForce(value) => Err(PropulsionError::InvalidInput {
                field: "normalized force demand",
                value,
            }),
            PropulsionDemand::Rating(PropulsionRating::TakeoffGoAround) => Ok((1.0, takeoff)),
            PropulsionDemand::Rating(PropulsionRating::MaximumClimb) => Ok((1.0, maximum_climb)),
            // No public engine-specific MCT deck is available. Keep MCT on
            // the climb envelope and report the model as extrapolated.
            PropulsionDemand::Rating(PropulsionRating::MaximumContinuous) => {
                Ok((1.0, maximum_climb))
            }
            PropulsionDemand::Rating(PropulsionRating::Cruise) => Ok((1.0, cruise)),
            PropulsionDemand::Rating(PropulsionRating::FlightIdle) => Ok((
                self.deck.flight_idle_fraction,
                self.deck.flight_idle_fraction * takeoff,
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
        validate_flight(request.flight)?;
        let (rating_utilization, requested_thrust_n) =
            self.rating_fraction_and_thrust(request.flight, request.demand)?;
        let (thrust_n, achieved_demand, active_limits) = match request.demand {
            PropulsionDemand::NormalizedForce(requested_fraction) => {
                let maximum_climb =
                    self.climb_available_n(request.flight, self.deck.max_climb_rate_ft_min);
                let idle_thrust =
                    self.deck.flight_idle_fraction * self.takeoff_available_n(request.flight);
                if requested_thrust_n < idle_thrust {
                    let achieved_fraction = if maximum_climb > 0.0 {
                        idle_thrust / maximum_climb
                    } else {
                        requested_fraction
                    };
                    (
                        idle_thrust,
                        PropulsionDemand::NormalizedForce(achieved_fraction),
                        vec![ActiveLimit {
                            name: "flight-idle-thrust".to_owned(),
                            utilization: 1.0,
                        }],
                    )
                } else {
                    (requested_thrust_n, request.demand, Vec::new())
                }
            }
            _ => (requested_thrust_n, request.demand, Vec::new()),
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
        let fuel_fraction =
            part_power_fuel_fraction(rating_utilization, self.deck.part_power_fuel_flow_ratios);
        let rated_thrust_n = if rating_utilization > 0.0 {
            thrust_n / rating_utilization
        } else {
            thrust_n
        };
        // Interpolate TSFC, not absolute fuel flow: blending the constant
        // static take-off flow into the lapsed rating charged low-altitude
        // flight with sea-level-static fuel flow for a fraction of the thrust.
        // The full-rating flow is TSFC times the lapsed rated thrust; the ICAO
        // part-power shape then scales it with rating utilization.
        let full_rating_fuel_flow_kg_s =
            self.full_rating_tsfc_kg_n_s(request.flight) * rated_thrust_n;
        let fuel_flow = full_rating_fuel_flow_kg_s * fuel_fraction;
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
                reason: "thrust uses the Bartel-Young/OpenAP correlation; fuel is TSFC times lapsed thrust, with TSFC joining the ICAO takeoff and cruise-TSFC anchors through a Mach/temperature form and remaining uncalibrated between anchors".to_owned(),
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
                reason: "Bartel-Young/OpenAP empirical mission thrust deck".to_owned(),
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
                message: "Three-region thrust lapse with an engine-specific maximum-climb anchor; fuel uses active-rating utilization and a TSFC joining the ICAO takeoff and cruise-TSFC anchors.".to_owned(),
            }],
            provenance: self.provenance.clone(),
        }
    }
}

fn standard_pressure_pa(altitude_m: f64) -> f64 {
    // Use the same atmosphere implementation that supplies mission flight
    // conditions. A separate rounded troposphere fit moves the nominal
    // OpenAP anchor away from ratio 1 and breaks exact calibration closure.
    alas_atmo::us1976_compute_values(altitude_m, 0.0).pressure_pa
}

fn cas_m_s(mach: f64, pressure_pa: f64, gamma: f64) -> f64 {
    let qc = pressure_pa
        * ((1.0 + 0.5 * (gamma - 1.0) * mach.powi(2)).powf(gamma / (gamma - 1.0)) - 1.0);
    let sea_level_mach = ((2.0 / 0.4) * ((qc / 101_325.0 + 1.0).powf(0.4 / 1.4) - 1.0))
        .max(0.0)
        .sqrt();
    340.294 * sea_level_mach
}

fn validate_flight(flight: FlightCondition) -> Result<(), PropulsionError> {
    for (field, value) in [
        ("empirical turbofan altitude_m", flight.altitude_m),
        ("empirical turbofan mach", flight.mach),
        ("empirical turbofan pressure_pa", flight.pressure_pa),
        ("empirical turbofan temperature_k", flight.temperature_k),
        ("empirical turbofan gamma", flight.gamma),
        ("empirical turbofan velocity_m_s", flight.velocity_m_s),
        ("empirical turbofan gravity_m_s2", flight.gravity_m_s2),
    ] {
        if !value.is_finite() {
            return Err(PropulsionError::InvalidInput { field, value });
        }
    }
    if flight.altitude_m < 0.0
        // The transport presets include certified/observed cruise through
        // FL410 and ceilings near FL431. A hidden FL400 numerical cutoff made
        // otherwise valid step climbs fail as a propulsion NaN. Keep a
        // bounded 45,000-ft correlation domain and retain Extrapolated
        // validity outside the declared engine reference point.
        || flight.altitude_m > 13_716.0
        || !(0.0..=MAXIMUM_DECK_MACH).contains(&flight.mach)
        || flight.pressure_pa <= 0.0
        || flight.temperature_k <= 0.0
        || flight.gamma <= 1.0
        || flight.velocity_m_s < 0.0
        || flight.gravity_m_s2 <= 0.0
    {
        return Err(PropulsionError::InvalidInput {
            field: "empirical turbofan flight condition",
            value: flight.altitude_m,
        });
    }
    Ok(())
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission_turbofan::{size_turbofan, TurbofanInputs, VehicleBuilderParams};

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
        let sized = size_turbofan(&inputs, &params);
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

    #[test]
    fn maximum_climb_reference_is_exactly_calibrated_at_its_anchor() {
        let model = model();
        let mut flight = sea_level_static();
        flight.altitude_m = 10_668.0;
        flight.pressure_pa = standard_pressure_pa(flight.altitude_m);
        flight.mach = 0.8;
        flight.velocity_m_s = flight.mach * flight.speed_of_sound_m_s;
        let (_, thrust) = model
            .rating_fraction_and_thrust(
                flight,
                PropulsionDemand::Rating(PropulsionRating::MaximumClimb),
            )
            .unwrap_or_else(|error| panic!("maximum-climb anchor: {error}"));
        assert!((thrust - 44_482.0).abs() < 1.0e-9);
    }

    #[test]
    fn missing_climb_rate_uses_zero_rate_baseline_not_a_hidden_universal_default() {
        let no_rate = model_with_climb_rate(None);
        let zero_rate = model_with_climb_rate(Some(0.0));
        let explicit_rate = model_with_climb_rate(Some(2_500.0));
        let mut flight = sea_level_static();
        flight.altitude_m = 1_500.0;
        flight.pressure_pa = standard_pressure_pa(flight.altitude_m);
        flight.mach = 0.4;
        flight.velocity_m_s = flight.mach * flight.speed_of_sound_m_s;

        let no_rate_thrust = no_rate.climb_available_n(flight, no_rate.deck.max_climb_rate_ft_min);
        let zero_rate_thrust =
            zero_rate.climb_available_n(flight, zero_rate.deck.max_climb_rate_ft_min);
        let explicit_rate_thrust =
            explicit_rate.climb_available_n(flight, explicit_rate.deck.max_climb_rate_ft_min);
        assert!((no_rate_thrust - zero_rate_thrust).abs() < 1.0e-9);
        assert!((explicit_rate_thrust - zero_rate_thrust).abs() > 1.0e-3);
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
        assert!((climb - cruise).abs() < 1.0e-9);
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
    fn cruise_reference_tsfc_anchor_is_reproduced() {
        let model = model();
        let flight = standard_flight(
            model.deck.max_climb_reference_altitude_m,
            model.deck.max_climb_reference_mach,
        );
        let result = model
            .evaluate(&PropulsionRequest {
                flight,
                demand: PropulsionDemand::Rating(PropulsionRating::Cruise),
                mode: OperatingMode::Normal,
                failure: FailureState::None,
                loads: PropulsionLoads::default(),
                state: PropulsionState::default(),
                time_step_s: None,
            })
            .unwrap_or_else(|error| panic!("cruise anchor: {error}"));
        let thrust_n = result.body_force_n[0];
        let fuel_flow_kg_s = result.resource_flows[0]
            .mass_flow_kg_s
            .unwrap_or_else(|| panic!("cruise Jet-A flow"));
        let recovered_tsfc = fuel_flow_kg_s * flight.gravity_m_s2 * 3_600.0 / thrust_n;
        assert!((recovered_tsfc - model.deck.cruise_reference_tsfc_kg_kgf_h).abs() < 1.0e-12);
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
        let cruise_tsfc = model.deck.cruise_reference_tsfc_kg_kgf_h;
        let static_tsfc = evaluated_tsfc_kg_kgf_h(&model, sea_level_static(), full);
        assert!((static_tsfc - takeoff_tsfc).abs() < 1.0e-12);

        // Climb at 300 kt CAS, then at the reference Mach, from sea level to
        // the reference altitude. TSFC rises strictly while it is below the
        // cruise anchor. Once the climb reaches the reference Mach below the
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
        // thrust at the reference altitude and Mach. The mission commands the
        // deck with NormalizedForce; at the reference condition a unit demand
        // is exactly that reference thrust, where the ICAO part-power factor
        // is 1, so the declared TSFC is delivered without a second part-power
        // correction.
        for model in [model(), a320_class_model()] {
            let flight = standard_flight(
                model.deck.max_climb_reference_altitude_m,
                model.deck.max_climb_reference_mach,
            );
            let result = model
                .evaluate(&request_at(flight, PropulsionDemand::NormalizedForce(1.0)))
                .unwrap();
            assert!(
                (result.body_force_n[0] - model.deck.max_climb_reference_thrust_n).abs() < 1.0e-9
            );
            let tsfc =
                evaluated_tsfc_kg_kgf_h(&model, flight, PropulsionDemand::NormalizedForce(1.0));
            assert!(
                (tsfc - model.deck.cruise_reference_tsfc_kg_kgf_h).abs() < 1.0e-12,
                "TSFC {tsfc}"
            );
            // Below the reference thrust the ICAO shape applies relative to
            // it: the delivered TSFC is the declared value times f(u)/u.
            let utilization = 0.8;
            let part = evaluated_tsfc_kg_kgf_h(
                &model,
                flight,
                PropulsionDemand::NormalizedForce(utilization),
            );
            let shape =
                part_power_fuel_fraction(utilization, model.deck.part_power_fuel_flow_ratios)
                    / utilization;
            assert!((part - model.deck.cruise_reference_tsfc_kg_kgf_h * shape).abs() < 1.0e-12);
        }
    }

    #[test]
    fn tsfc_keeps_temperature_and_mach_trends_above_the_reference_altitude() {
        let model = model();
        let cruise = PropulsionDemand::Rating(PropulsionRating::Cruise);
        let reference_altitude_m = model.deck.max_climb_reference_altitude_m;
        let reference_mach = model.deck.max_climb_reference_mach;
        let reference = standard_flight(reference_altitude_m, reference_mach);
        let c_ref = model.deck.cruise_reference_tsfc_kg_kgf_h;
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

        // The superseded schedule blended absolute take-off fuel flow into the
        // lapsed rating by altitude.
        let deck = &model.deck;
        let x = flight.altitude_m / deck.max_climb_reference_altitude_m;
        let blend = x * x * (3.0 - 2.0 * x);
        let rated_thrust_n = thrust_n / utilization;
        let cruise_flow_kg_s =
            deck.cruise_reference_tsfc_kg_kgf_h * rated_thrust_n / flight.gravity_m_s2 / 3_600.0;
        let old_flow_kg_s = (deck.takeoff_fuel_flow_kg_s
            + blend * (cruise_flow_kg_s - deck.takeoff_fuel_flow_kg_s))
            * part_power_fuel_fraction(utilization, deck.part_power_fuel_flow_ratios);
        let old_tsfc = old_flow_kg_s * flight.gravity_m_s2 * 3_600.0 / thrust_n;

        assert!((0.80..0.87).contains(&old_tsfc), "old TSFC {old_tsfc}");
        assert!((0.55..0.65).contains(&tsfc), "TSFC {tsfc}");
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
