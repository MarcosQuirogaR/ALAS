// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Empirical off-design thrust deck for transport turbofans.
//!
//! This implements the three-region Bartel-Young maximum-climb correlation
//! as published by OpenAP. It is deliberately separate from the conceptual
//! fixed-area cycle: a mission deck must reproduce an installed engine's
//! certified static rating. The correlation is a ratio to the maximum-climb
//! thrust at its reference point; that anchor is Howe's class-level
//! maximum thrust of the static rating at the reference altitude and Mach
//! ([max_climb]), not the catalogue's part-power cruise thrust.

use crate::mission_turbofan::components::part_power_fuel_fraction;
use crate::system::*;

mod correlation;
mod max_climb;
use correlation::*;
use max_climb::*;

/// Data required by the Bartel-Young/OpenAP lapse correlation.
#[derive(Debug, Clone, PartialEq)]
pub struct EmpiricalTurbofanDeck {
    /// Installed all-engine sea-level-static takeoff thrust, N.
    pub takeoff_thrust_n: f64,
    /// Installed all-engine thrust at which `cruise_reference_tsfc_kg_kgf_h`
    /// is quoted, at the reference altitude and Mach, N: the catalogue's
    /// part-power cruise thrust/SFC pair. It sets the throttle basis of the
    /// cruise TSFC anchor only and must not exceed the maximum-climb thrust
    /// there. The maximum-climb rating itself is derived from
    /// `takeoff_thrust_n` and `bypass_ratio` ([max_climb]). The name predates
    /// that change, when this thrust was (wrongly) the climb anchor.
    pub max_climb_reference_thrust_n: f64,
    /// Reference pressure altitude of the climb correlation and of the
    /// cruise TSFC anchor, m. Howe's anchor needs 9,144 m < h <= 11,000 m.
    pub max_climb_reference_altitude_m: f64,
    /// Reference Mach number of the climb correlation and of the cruise
    /// TSFC anchor; Howe's anchor needs 0.4 <= M <= 0.9.
    pub max_climb_reference_mach: f64,
    /// Engine bypass ratio used by the takeoff correlation and by Howe's
    /// maximum-climb anchor.
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
    climb_reference: ClimbReference,
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
            || deck.max_climb_reference_altitude_m > HOWE_MAXIMUM_ALTITUDE_M
            || !(HOWE_MINIMUM_MACH..=HOWE_MAXIMUM_MACH).contains(&deck.max_climb_reference_mach)
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
        let sea_level = alas_atmo::us1976_compute_values(0.0, 0.0);
        let sea_level_temperature_k = sea_level.temperature_k;
        let reference = alas_atmo::us1976_compute_values(deck.max_climb_reference_altitude_m, 0.0);
        let reference_temperature_k = reference.temperature_k;
        let max_climb_thrust_n = deck.takeoff_thrust_n
            * howe_maximum_thrust_ratio(
                deck.bypass_ratio,
                deck.max_climb_reference_mach,
                reference.density_kg_m3 / sea_level.density_kg_m3,
            );
        // The catalogue cruise thrust is a part-power setting of that rating.
        let cruise_utilization = deck.max_climb_reference_thrust_n / max_climb_thrust_n;
        if !cruise_utilization.is_finite() || !(0.07..=1.0).contains(&cruise_utilization) {
            return Err(PropulsionError::InvalidInput {
                field: "empirical turbofan cruise thrust over maximum-climb thrust",
                value: cruise_utilization,
            });
        }
        let reference_full_rating_tsfc_kg_n_s = deck.cruise_reference_tsfc_kg_kgf_h
            / (STANDARD_GRAVITY_M_S2 * 3_600.0)
            * cruise_utilization
            / part_power_fuel_fraction(cruise_utilization, deck.part_power_fuel_flow_ratios);
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
        let reference_pressure_pa = standard_pressure_pa(deck.max_climb_reference_altitude_m);
        let climb_reference = ClimbReference {
            p10_pa: standard_pressure_pa(3_048.0),
            pcr_pa: reference_pressure_pa,
            cas_m_s: cas_m_s(deck.max_climb_reference_mach, reference_pressure_pa, 1.4),
            max_climb_thrust_n,
        };
        Ok(Self {
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
        let ClimbReference {
            p10_pa: p10,
            pcr_pa: pcr,
            cas_m_s: reference_cas,
            max_climb_thrust_n,
        } = self.climb_reference;
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
        (ratio * max_climb_thrust_n).max(0.0)
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
            PropulsionDemand::Rating(PropulsionRating::Cruise) => {
                Ok((1.0, self.climb_available_n(flight, Some(0.0))))
            }
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
                reason: "thrust uses the Bartel-Young/OpenAP climb lapse anchored on Howe's class maximum thrust at the reference point (Schulz 2007: within 20 % to FL350); fuel is TSFC times lapsed thrust, with TSFC joining the ICAO takeoff and cruise-TSFC anchors through a Mach/temperature form and remaining uncalibrated between anchors".to_owned(),
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
                message: "Three-region Bartel-Young thrust lapse anchored on Howe's maximum thrust of the static rating at the reference point; fuel uses active-rating utilization and a TSFC joining the ICAO takeoff and cruise-TSFC anchors.".to_owned(),
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

    /// US 1976 density ratio at `altitude_m`.
    fn sigma(altitude_m: f64) -> f64 {
        alas_atmo::us1976_compute_values(altitude_m, 0.0).density_kg_m3
            / alas_atmo::us1976_compute_values(0.0, 0.0).density_kg_m3
    }

    #[test]
    fn maximum_climb_anchor_is_howes_maximum_thrust_at_the_reference_point() {
        // Howe (2000) p. 67 via Schulz (2007) Table 2.1, BPR 3-6 row for
        // 0.4 <= M <= 0.9: F/F_SLS = (0.88 - 0.016 BPR - 0.3 M) sigma^0.7.
        let model = model();
        let flight = standard_flight(10_668.0, 0.8);
        let (_, thrust) = model
            .rating_fraction_and_thrust(
                flight,
                PropulsionDemand::Rating(PropulsionRating::MaximumClimb),
            )
            .unwrap_or_else(|error| panic!("maximum-climb anchor: {error}"));
        let howe = (0.88 - 0.016 * 5.9 - 0.3 * 0.8) * sigma(10_668.0).powf(0.7);
        assert!((thrust - 235_800.0 * howe).abs() < 1.0e-6, "{thrust}");
        // Schulz's own spot value (Sec. 3.1.3 trend, Fig. 3.7) and the
        // phase-2 source note: BPR 5, M 0.78, 10.7 km gives about a quarter
        // of the static thrust.
        let quarter = howe_maximum_thrust_ratio(5.0, 0.78, sigma(10_700.0));
        assert!((0.24..0.26).contains(&quarter), "{quarter}");
    }

    #[test]
    fn howe_rows_hold_inside_their_bypass_bands_and_blend_continuously_between() {
        let s = sigma(10_668.0);
        let row = |k: [f64; 5], bpr: f64, mach: f64| {
            (k[0] + k[1] * bpr + (k[2] + k[3] * bpr) * mach) * s.powf(k[4])
        };
        let bpr_1 = [0.856, 0.062, 0.16, -0.23, 0.8];
        let bpr_3_6 = [0.88, -0.016, -0.3, 0.0, 0.7];
        let bpr_8 = [0.89, -0.014, -0.3, 0.005, 0.7];
        for mach in [0.4, 0.78, 0.85, 0.9] {
            let at = |bpr| howe_maximum_thrust_ratio(bpr, mach, s);
            assert!((at(1.0) - row(bpr_1, 1.0, mach)).abs() < 1.0e-15);
            for bpr in [3.0, 4.5, 6.0] {
                assert!((at(bpr) - row(bpr_3_6, bpr, mach)).abs() < 1.0e-15);
            }
            assert!((at(8.0) - row(bpr_8, 8.0, mach)).abs() < 1.0e-15);
            let blend_7 = 0.5 * (row(bpr_3_6, 7.0, mach) + row(bpr_8, 7.0, mach));
            assert!((at(7.0) - blend_7).abs() < 1.0e-15);
            let blend_2 = 0.5 * (row(bpr_1, 2.0, mach) + row(bpr_3_6, 2.0, mach));
            assert!((at(2.0) - blend_2).abs() < 1.0e-15);
            for edge in [1.0, 3.0, 6.0, 8.0] {
                assert!((at(edge + 1.0e-9) - at(edge)).abs() < 1.0e-8, "BPR {edge}");
            }
        }
    }

    #[test]
    fn maximum_climb_at_the_reference_exceeds_the_class_cruise_thrusts() {
        // A climb rating above the cruise setting is what leaves the residual
        // climb a cruise level needs. Class cruise thrusts at 10.668 km:
        // Scholz 2007b, F_CR/F_TO = (0.0013 BPR - 0.0397) h_km - 0.0248 BPR
        // + 0.7125, and Svoboda 2000, F_CR = 200 lbf + 0.2 F_TO (Schulz 2007
        // Eqs. 2.12-2.13).
        let takeoff_n = 120_000.0;
        let h_km = 10.668;
        for bpr in [4.3, 5.7, 6.6, 8.45, 9.6, 11.4] {
            let deck = EmpiricalTurbofanDeck {
                takeoff_thrust_n: takeoff_n,
                max_climb_reference_thrust_n: 0.2 * takeoff_n + 200.0 * 4.448_221_615_260_5,
                bypass_ratio: bpr,
                ..a320_class_model().deck
            };
            let model = model_from_deck(deck);
            let maximum_climb = model.climb_reference.max_climb_thrust_n;
            let scholz = ((0.0013 * bpr - 0.0397) * h_km - 0.0248 * bpr + 0.7125) * takeoff_n;
            assert!(
                maximum_climb > scholz,
                "BPR {bpr}: {maximum_climb} vs {scholz}"
            );
            assert!(
                maximum_climb > model.deck.max_climb_reference_thrust_n,
                "BPR {bpr}"
            );
        }
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
    fn a_cruise_thrust_above_the_maximum_climb_rating_is_rejected() {
        let model = a320_class_model();
        let mut deck = model.deck.clone();
        deck.max_climb_reference_thrust_n = 1.01 * model.climb_reference.max_climb_thrust_n;
        assert!(matches!(
            EmpiricalTurbofanModel::new(deck, model.fuel_model, model.provenance),
            Err(PropulsionError::InvalidInput { .. })
        ));
    }

    /// The correlation as originally written, with the reference pressures and
    /// CAS recomputed on every call.
    fn climb_available_recomputing_anchors_n(
        model: &EmpiricalTurbofanModel,
        flight: FlightCondition,
        climb_rate_ft_min: Option<f64>,
    ) -> f64 {
        let p = flight.pressure_pa;
        let p10 = standard_pressure_pa(3_048.0);
        let pcr = standard_pressure_pa(model.deck.max_climb_reference_altitude_m);
        let reference_cas = cas_m_s(model.deck.max_climb_reference_mach, pcr, 1.4);
        let cas = cas_m_s(flight.mach, p, flight.gamma);
        let speed_ratio = (cas / reference_cas).max(1.0e-6);
        let mach_ratio = (flight.mach / model.deck.max_climb_reference_mach).max(1.0e-6);
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
        (ratio * model.climb_reference.max_climb_thrust_n).max(0.0)
    }

    #[test]
    fn hoisted_anchors_and_lazy_ratings_are_bitwise_the_original_evaluation() {
        for model in [model_with_climb_rate(Some(2_500.0)), a320_class_model()] {
            for altitude_m in [
                0.0, 1_500.0, 3_048.0, 3_049.0, 6_000.0, 9_144.0, 9_145.0, 11_000.0,
            ] {
                for mach in [0.0, 0.3, 0.6, 0.8] {
                    let mut flight = sea_level_static();
                    flight.altitude_m = altitude_m;
                    flight.pressure_pa = standard_pressure_pa(altitude_m);
                    flight.mach = mach;
                    flight.velocity_m_s = mach * flight.speed_of_sound_m_s;
                    let climb = |rate| climb_available_recomputing_anchors_n(&model, flight, rate);
                    for rate in [None, Some(0.0), model.deck.max_climb_rate_ft_min] {
                        assert_eq!(
                            model.climb_available_n(flight, rate).to_bits(),
                            climb(rate).to_bits()
                        );
                    }
                    let rated = |rating| {
                        model
                            .evaluate(&request_at(flight, PropulsionDemand::Rating(rating)))
                            .unwrap_or_else(|error| panic!("rated point: {error}"))
                            .body_force_n[0]
                    };
                    assert_eq!(
                        rated(PropulsionRating::MaximumClimb).to_bits(),
                        climb(model.deck.max_climb_rate_ft_min).to_bits()
                    );
                    assert_eq!(
                        rated(PropulsionRating::Cruise).to_bits(),
                        climb(Some(0.0)).to_bits()
                    );
                    assert_eq!(
                        rated(PropulsionRating::TakeoffGoAround).to_bits(),
                        model.takeoff_available_n(flight).to_bits()
                    );
                    // The idle floor is a fraction of takeoff thrust, and a
                    // normalized demand above it is that fraction of the
                    // maximum-climb thrust.
                    let normalized = model
                        .evaluate(&request_at(flight, PropulsionDemand::NormalizedForce(0.6)))
                        .unwrap_or_else(|error| panic!("normalized point: {error}"))
                        .body_force_n[0];
                    let idle = model.deck.flight_idle_fraction * model.takeoff_available_n(flight);
                    let expected = (0.6 * climb(model.deck.max_climb_rate_ft_min)).max(idle);
                    assert_eq!(normalized.to_bits(), expected.to_bits());
                }
            }
        }
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
            // Elsewhere the ICAO shape applies relative to the rating: the
            // delivered TSFC is the declared value times the ratio of f(u)/u
            // to its value at the cruise utilization.
            let shape =
                |u: f64| part_power_fuel_fraction(u, model.deck.part_power_fuel_flow_ratios) / u;
            for utilization in [0.5, 0.95, 1.0] {
                let part = evaluated_tsfc_kg_kgf_h(
                    &model,
                    flight,
                    PropulsionDemand::NormalizedForce(utilization),
                );
                let expected = model.deck.cruise_reference_tsfc_kg_kgf_h * shape(utilization)
                    / shape(cruise_utilization);
                assert!((part - expected).abs() < 1.0e-12, "u {utilization}");
            }
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
