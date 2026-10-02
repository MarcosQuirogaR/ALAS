// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Preliminary, stand-alone turboprop operating-point model.
//!
//! This module deliberately contains no aircraft or mission wiring.  It separates
//! gas-turbine shaft power, accessory and gearbox losses, propeller force, residual
//! jet thrust, and fuel flow.  The PW127M ratings are certified installation data;
//! the fuel and propeller models are explicitly labelled family-level surrogates.

use crate::system::{
    ActiveLimit, DiagnosticItem, DiagnosticsQuery, FailureState, FlightCondition, ModelIdentity,
    ModelProvenance, OperatingMode, PropulsionCapability, PropulsionDemand, PropulsionDiagnostics,
    PropulsionError, PropulsionInstallation, PropulsionMassItem, PropulsionRating,
    PropulsionRequest, PropulsionResult, PropulsionSystemModel, Residual, ResourceFlow,
    ResourceKind, StateDerivative, ValidityStatus,
};
use std::f64::consts::PI;

mod types;
pub use types::*;
mod model;
pub use model::actuator_disk_thrust_bound_n;
mod evidence;
mod inverse;
mod ratings;
mod system;
pub use evidence::*;
use ratings::isa_temperature_from_density_k;
pub use system::*;

#[cfg(test)]
mod solver_tests;

#[cfg(test)]
// Test fixtures assert successful construction through unwrap and expect.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn atr_system() -> Atr72TurbopropSystem {
        Atr72TurbopropSystem::new(
            Pw127m568fModel::default(),
            Vec::new(),
            PropulsionInstallation {
                unit_positions_m: vec![[2.0, -6.0, 0.0], [2.0, 6.0, 0.0]],
                thrust_axes_body: vec![[1.0, 0.0, 0.0]; 2],
                nacelle_wetted_area_m2: None,
                frontal_area_m2: None,
            },
        )
        .unwrap()
    }

    fn static_flight() -> FlightCondition {
        FlightCondition {
            altitude_m: 0.0,
            mach: 0.0,
            pressure_pa: 101_325.0,
            temperature_k: 288.15,
            density_kg_m3: 1.225,
            dynamic_viscosity_pa_s: 1.789e-5,
            gravity_m_s2: 9.806_65,
            gamma: 1.4,
            cp_j_kgk: 1_004.5,
            gas_constant_j_kgk: 287.05,
            speed_of_sound_m_s: 340.3,
            velocity_m_s: 0.0,
            stagnation_temperature_k: 288.15,
            stagnation_pressure_pa: 101_325.0,
        }
    }

    fn system_request(failure: FailureState) -> PropulsionRequest {
        PropulsionRequest {
            flight: static_flight(),
            demand: PropulsionDemand::Rating(PropulsionRating::TakeoffGoAround),
            mode: OperatingMode::Normal,
            failure,
            loads: Default::default(),
            state: Default::default(),
            time_step_s: None,
        }
    }

    fn nominal_command() -> TurbopropCommand {
        TurbopropCommand {
            rating: Pw127mRating::NormalTakeoff,
            power_fraction: 0.70,
            mode: TurbopropMode::Governed,
            propeller_speed_rpm: 1_200.0,
        }
    }

    #[test]
    fn certified_ratings_remain_distinct_and_in_si() {
        let normal = Pw127mRating::NormalTakeoff.shaft_power_w();
        let reserve = Pw127mRating::MaximumTakeoffReserve.shaft_power_w();
        let continuous = Pw127mRating::MaximumContinuous.shaft_power_w();
        let climb = Pw127mRating::MaximumClimb.shaft_power_w();
        let cruise = Pw127mRating::MaximumCruise.shaft_power_w();
        assert!((normal - 1_845_607.183_2).abs() < 1.0);
        assert!(reserve > continuous && continuous > normal);
        assert!(normal > climb && climb > cruise);
        assert!((climb / WATTS_PER_SHP - 2_192.0).abs() < 1.0e-12);
        assert!((cruise / WATTS_PER_SHP - 2_132.0).abs() < 1.0e-12);
    }

    #[test]
    fn sea_level_density_preserves_exact_typed_shaft_rating() {
        let model = Pw127m568fModel::default();
        let output = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: model.power_lapse_reference_density_kg_m3,
                    true_airspeed_m_s: 85.0,
                },
                TurbopropCommand {
                    rating: Pw127mRating::MaximumClimb,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            )
            .unwrap();

        assert_eq!(output.engine_shaft_power_w, model.maximum_climb_power_w);
    }

    #[test]
    fn available_shaft_power_decreases_monotonically_with_density() {
        let model = Pw127m568fModel::default();
        let evaluate_power = |density_kg_m3| {
            model
                .evaluate(
                    TurbopropCondition {
                        density_kg_m3,
                        true_airspeed_m_s: 100.0,
                    },
                    TurbopropCommand {
                        rating: Pw127mRating::MaximumClimb,
                        power_fraction: 1.0,
                        mode: TurbopropMode::Governed,
                        propeller_speed_rpm: model.governed_propeller_speed_rpm,
                    },
                )
                .unwrap()
                .engine_shaft_power_w
        };
        let sea_level_power_w = evaluate_power(1.225);
        let intermediate_power_w = evaluate_power(0.90);
        let fl170_power_w = evaluate_power(0.70);

        assert!(sea_level_power_w > intermediate_power_w);
        assert!(intermediate_power_w > fl170_power_w);
        // Was `(0.70 / 1.225)^0.75`, a lapse from sea-level ISA with an
        // aircraft-calibrated exponent. The climb rating now shares the
        // maximum-continuous flat-rating corner (sea-level pressure at 48 C,
        // TCDS IM.E.041) and lapses with Nita's PW120 exponent 0.728 below it.
        let corner_kg_m3 = 1.225 * 288.15 / 321.15;
        assert!(
            (model.flat_rating_corner_density_kg_m3(Pw127mRating::MaximumClimb) - corner_kg_m3)
                .abs()
                < 1e-12
        );
        assert_eq!(
            fl170_power_w,
            model.maximum_climb_power_w * (0.70_f64 / corner_kg_m3).powf(0.728)
        );
    }

    /// The TCDS flat rating: take-off power is available unchanged at sea
    /// level up to 39 C and maximum-continuous power up to 48 C; above that
    /// corner the density lapse takes over, and it is continuous there.
    #[test]
    fn ratings_are_flat_to_the_tcds_temperatures_and_lapse_beyond_them() {
        let model = Pw127m568fModel::default();
        // Sea-level pressure at an ambient temperature, as a density.
        let sea_level_density = |temperature_k: f64| 1.225 * 288.15 / temperature_k;
        let takeoff =
            |density: f64| model.available_shaft_power_w(Pw127mRating::NormalTakeoff, density);
        let continuous =
            |density: f64| model.available_shaft_power_w(Pw127mRating::MaximumContinuous, density);
        // ISA, ISA+15 and exactly 39 C: the whole take-off rating.
        for temperature_k in [288.15, 303.15, 312.15] {
            assert_eq!(
                takeoff(sea_level_density(temperature_k)),
                model.normal_takeoff_power_w
            );
        }
        // 44 C is past the take-off corner but inside the continuous one.
        let hot = sea_level_density(317.15);
        assert!(takeoff(hot) < model.normal_takeoff_power_w);
        assert_eq!(continuous(hot), model.maximum_continuous_power_w);
        assert!(continuous(sea_level_density(325.15)) < model.maximum_continuous_power_w);
        // Continuous across the corner.
        let corner = model.flat_rating_corner_density_kg_m3(Pw127mRating::NormalTakeoff);
        let just_below = takeoff(corner * (1.0 - 1e-9));
        assert!((just_below / model.normal_takeoff_power_w - 1.0).abs() < 1e-8);
        // The corner as an ISA altitude is about 810 m for take-off and
        // 1,110 m for maximum continuous: flat rating that ends low, as a
        // sea-level temperature limit implies.
        let corner_temperature_k = isa_temperature_from_density_k(corner, 1.225);
        let corner_altitude_m = (288.15 - corner_temperature_k) / 0.0065;
        assert!(
            (700.0..900.0).contains(&corner_altitude_m),
            "{corner_altitude_m}"
        );
    }

    /// PSFC is flat in power setting and scales as `sqrt(T / T_ref)`
    /// (Majeed 2009, SRS-TSD-002 Tab. 4.1-4.3), from a reference inside
    /// the source's 0.28-0.31 kg/kWh band.
    #[test]
    fn psfc_is_flat_in_power_and_scales_with_the_root_of_ambient_temperature() {
        let model = Pw127m568fModel::default();
        assert!((0.28..=0.31).contains(&model.psfc_reference_kg_kwh));
        let at = |temperature_k: f64, rating: Pw127mRating, power_fraction: f64| {
            model
                .evaluate_at_temperature(
                    TurbopropCondition {
                        density_kg_m3: 0.72,
                        true_airspeed_m_s: 130.0,
                    },
                    temperature_k,
                    TurbopropCommand {
                        rating,
                        power_fraction,
                        mode: TurbopropMode::Governed,
                        propeller_speed_rpm: model.governed_propeller_speed_rpm,
                    },
                )
                .unwrap()
        };
        let reference = at(254.4, Pw127mRating::MaximumCruise, 1.0);
        for (rating, fraction) in [
            (Pw127mRating::MaximumCruise, 0.46),
            (Pw127mRating::MaximumClimb, 0.8),
            (Pw127mRating::NormalTakeoff, 1.0),
        ] {
            let output = at(254.4, rating, fraction);
            assert_eq!(output.psfc_kg_kwh, reference.psfc_kg_kwh);
            assert!(
                (output.fuel_flow_kg_s
                    - output.engine_shaft_power_w * output.psfc_kg_kwh / JOULES_PER_KWH)
                    .abs()
                    < 1e-15
            );
        }
        let cold = at(230.0, Pw127mRating::MaximumCruise, 1.0).psfc_kg_kwh;
        let warm = at(270.0, Pw127mRating::MaximumCruise, 1.0).psfc_kg_kwh;
        assert!(cold < reference.psfc_kg_kwh && reference.psfc_kg_kwh < warm);
        assert!((warm / cold - (270.0_f64 / 230.0).sqrt()).abs() < 1e-12);
        // Independent check at sea level: Majeed Tab. 3.2 measures 0.322-0.328
        // kg/kWh at normal take-off power, which this relation reaches from a
        // cruise reference to within 3 %.
        let sea_level = model.psfc_kg_kwh(288.15);
        assert!((0.312..=0.338).contains(&sea_level), "{sea_level}");
        // Outside the relation's temperature basis the model refuses.
        assert!(model
            .evaluate_at_temperature(
                TurbopropCondition {
                    density_kg_m3: 0.72,
                    true_airspeed_m_s: 130.0,
                },
                f64::NAN,
                nominal_command(),
            )
            .is_err());
    }

    /// Momentum-theory static thrust grows with power and density, and the
    /// thrust at constant power falls monotonically as the propeller
    /// accelerates, from the figure-of-merit static value into `eta_p P / V`.
    #[test]
    fn static_thrust_follows_momentum_theory_and_falls_monotonically_with_speed() {
        let model = Pw127m568fModel::default();
        let at = |density_kg_m3: f64, true_airspeed_m_s: f64, power_fraction: f64| {
            model
                .evaluate(
                    TurbopropCondition {
                        density_kg_m3,
                        true_airspeed_m_s,
                    },
                    TurbopropCommand {
                        rating: Pw127mRating::NormalTakeoff,
                        power_fraction,
                        mode: TurbopropMode::Governed,
                        propeller_speed_rpm: model.governed_propeller_speed_rpm,
                    },
                )
                .unwrap()
        };
        let mut previous = 0.0;
        for fraction in [0.4, 0.6, 0.8, 1.0] {
            let thrust = at(1.225, 0.0, fraction).total_thrust_n;
            assert!(thrust > previous);
            previous = thrust;
        }
        assert!(at(1.1, 0.0, 1.0).total_thrust_n < at(1.225, 0.0, 1.0).total_thrust_n);
        // FM 0.70 on the ATR's 3.93 m disc at 2,475 shp: about 37 kN per
        // engine (46.7 kN ideal).
        let static_n = at(1.225, 0.0, 1.0).total_thrust_n;
        assert!((34_000.0..40_000.0).contains(&static_n), "{static_n}");
        let mut previous = f64::INFINITY;
        for step in 0..=60 {
            let speed_m_s = 2.5 * f64::from(step);
            let output = at(1.225, speed_m_s, 1.0);
            assert!(
                output.total_thrust_n <= previous,
                "thrust rises at {speed_m_s} m/s"
            );
            previous = output.total_thrust_n;
            if speed_m_s > 0.0 {
                // Never above the forward-flight blend `eta_p P / V`.
                assert!(
                    output.total_thrust_n * speed_m_s
                        <= model.maximum_propulsive_efficiency * output.propeller_power_w + 1e-6
                );
            }
        }
    }

    #[test]
    fn live_typed_rating_values_drive_the_kernel_instead_of_enum_defaults() {
        let model = Pw127m568fModel {
            maximum_cruise_power_w: 1_400_000.0,
            ..Pw127m568fModel::default()
        };
        let output = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: 1.225,
                    true_airspeed_m_s: 0.0,
                },
                TurbopropCommand {
                    rating: Pw127mRating::MaximumCruise,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            )
            .unwrap();
        assert_eq!(output.engine_shaft_power_w, 1_400_000.0);
    }

    /// Was `cruise_fuel_anchor_reproduces_the_published_aircraft_flow`, which
    /// pinned the constant-PSFC calibration to 762 kg/h to 1e-9. The flow is
    /// now shaft power times the class PSFC at the ISA temperature of the
    /// density, and the published point is not reproduced by construction.
    #[test]
    fn cruise_fuel_flow_is_shaft_power_times_the_class_psfc() {
        let model = Pw127m568fModel::default();
        let command = TurbopropCommand {
            rating: Pw127mRating::MaximumCruise,
            power_fraction: 1.0,
            mode: TurbopropMode::Governed,
            propeller_speed_rpm: model.governed_propeller_speed_rpm,
        };
        let at = |density_kg_m3: f64| {
            model
                .evaluate(
                    TurbopropCondition {
                        density_kg_m3,
                        true_airspeed_m_s: 140.0,
                    },
                    command,
                )
                .unwrap()
        };
        let cruise = at(model.fuel_reference_density_kg_m3);
        let sea_level = at(model.power_lapse_reference_density_kg_m3);
        let cruise_temperature_k = isa_temperature_from_density_k(0.70, 1.225);
        // ISA 0.70 kg/m^3 is about FL180, 252.4 K.
        assert!((cruise_temperature_k - 252.4).abs() < 0.3);
        assert_eq!(cruise.psfc_kg_kwh, model.psfc_kg_kwh(cruise_temperature_k));
        assert!(
            (cruise.fuel_flow_kg_s
                - cruise.engine_shaft_power_w * cruise.psfc_kg_kwh / JOULES_PER_KWH)
                .abs()
                < 1e-15
        );
        // More power and a warmer day at sea level: more fuel, and a higher
        // specific consumption.
        assert!(sea_level.fuel_flow_kg_s > cruise.fuel_flow_kg_s);
        assert!(sea_level.psfc_kg_kwh > cruise.psfc_kg_kwh);
    }

    /// A propeller that reaches the ideal actuator-disk bound has no profile
    /// loss at all (eta_p = 0.98 at the ATR's FL170 / 275 kt cruise). The
    /// blade-efficiency factor puts the cruise point inside the band three
    /// independent sources agree on, and leaves the static thrust, which is
    /// not on the ideal bound, where the static figure of merit puts it.
    #[test]
    fn cruise_efficiency_is_a_real_propeller_rather_than_the_loss_free_bound() {
        let model = Pw127m568fModel::default();
        let density_kg_m3 = 0.728_5;
        let true_airspeed_m_s = 275.0 * 1_852.0 / 3_600.0;
        let output = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3,
                    true_airspeed_m_s,
                },
                TurbopropCommand {
                    rating: Pw127mRating::MaximumCruise,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            )
            .unwrap();

        // The loss-free bound at this condition, which the model must stay
        // clear of.
        let disk_area_m2 = PI * model.propeller_diameter_m.powi(2) / 4.0;
        let ideal_thrust_n = actuator_disk_thrust_bound_n(
            output.propeller_power_w,
            density_kg_m3,
            disk_area_m2,
            true_airspeed_m_s,
        );
        let ideal_efficiency = ideal_thrust_n * true_airspeed_m_s / output.propeller_power_w;
        assert!(
            ideal_efficiency > 0.95,
            "the ideal bound at this condition is {ideal_efficiency}"
        );
        assert!(
            output.propulsive_efficiency < 0.9 * ideal_efficiency,
            "cruise efficiency {} is still on the loss-free bound {ideal_efficiency}",
            output.propulsive_efficiency
        );
        // The corroborated installed band for a 568F-class propeller in
        // cruise: 0.85-0.86 from the Hamilton Standard map and 0.859 from the
        // Scholz chart as read for the ATR 72, against a declared 0.86 blade
        // efficiency on an ideal bound just under 0.98.
        assert!(
            (0.80..=0.88).contains(&output.propulsive_efficiency),
            "cruise efficiency {} is outside the corroborated band",
            output.propulsive_efficiency
        );
        assert!(output.propulsive_efficiency <= model.maximum_propulsive_efficiency);
    }

    /// The static point is the one condition the model already treated with a
    /// figure of merit, so the correction must leave it untouched.
    #[test]
    fn the_static_thrust_is_unchanged_and_still_carries_the_figure_of_merit() {
        let model = Pw127m568fModel::default();
        let output = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: 1.225,
                    true_airspeed_m_s: 0.0,
                },
                nominal_command(),
            )
            .unwrap();
        let disk_area_m2 = PI * model.propeller_diameter_m.powi(2) / 4.0;
        let ideal_static_thrust_n =
            (2.0 * 1.225 * disk_area_m2 * output.propeller_power_w.powi(2)).cbrt();
        let expected = model.static_figure_of_merit.powf(2.0 / 3.0) * ideal_static_thrust_n;
        assert!(
            (output.propeller_thrust_n - expected).abs() / expected < 1.0e-12,
            "{} vs {expected}",
            output.propeller_thrust_n
        );
        // And the blade efficiency at zero advance ratio is that same figure
        // of merit, which is what makes the two branches agree there.
        assert_eq!(model.blade_efficiency(0.0), model.static_figure_of_merit);
    }

    #[test]
    fn static_thrust_is_finite_without_velocity_division() {
        let model = Pw127m568fModel::default();
        let output = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: 1.225,
                    true_airspeed_m_s: 0.0,
                },
                nominal_command(),
            )
            .unwrap();
        assert!(output.propeller_thrust_n.is_finite());
        assert!(output.propeller_thrust_n > 0.0);
        assert_eq!(output.propulsive_efficiency, 0.0);
    }

    #[test]
    fn low_speed_blend_is_continuous_at_static_and_map_boundary() {
        let evaluate_at = |true_airspeed_m_s| {
            Pw127m568fModel::default()
                .evaluate(
                    TurbopropCondition {
                        density_kg_m3: 1.225,
                        true_airspeed_m_s,
                    },
                    nominal_command(),
                )
                .unwrap()
                .propeller_thrust_n
        };
        let static_thrust = evaluate_at(0.0);
        let near_static_thrust = evaluate_at(1.0e-6);
        // The static point is now on the momentum bound rather than under it,
        // because the bound is evaluated at the same figure of merit the
        // static branch uses. The two therefore differ by the bound's own
        // physical slope, `dT/dV ~ -2T/(3 v_i)`, which is about 2e-8 of the
        // thrust at 1 um/s and not a force jump; a discontinuity would be of
        // order one.
        let induced_velocity_m_s =
            (static_thrust / (2.0 * 1.225 * PI * 3.93_f64.powi(2) / 4.0)).sqrt();
        let momentum_slope = 2.0 / (3.0 * induced_velocity_m_s) * 1.0e-6;
        assert!(
            (near_static_thrust - static_thrust).abs() / static_thrust < 10.0 * momentum_slope,
            "{near_static_thrust} vs {static_thrust}"
        );
        let below_transition = evaluate_at(5.0 - 1.0e-6);
        let above_transition = evaluate_at(5.0 + 1.0e-6);
        assert!((above_transition - below_transition).abs() / below_transition.abs() < 1.0e-6);
    }

    #[test]
    fn shaft_power_balance_and_torque_units_close() {
        let model = Pw127m568fModel::default();
        let output = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: 1.0,
                    true_airspeed_m_s: 90.0,
                },
                nominal_command(),
            )
            .unwrap();
        assert!(output.power_balance_residual_w.abs() < 1.0e-8);
        let reconstructed_power = output.propeller_torque_n_m * 2.0 * PI * 20.0;
        assert!((reconstructed_power - output.propeller_power_w).abs() < 1.0e-8);
        // Was the constant calibrated PSFC; now the class PSFC at the ISA
        // temperature of the evaluated density.
        let expected_fuel = output.engine_shaft_power_w
            * model.psfc_kg_kwh(isa_temperature_from_density_k(1.0, 1.225))
            / JOULES_PER_KWH;
        assert!((output.fuel_flow_kg_s - expected_fuel).abs() < 1.0e-12);
    }

    #[test]
    fn governed_efficiency_is_physically_bounded() {
        let output = Pw127m568fModel::default()
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: 1.0,
                    true_airspeed_m_s: 90.0,
                },
                nominal_command(),
            )
            .unwrap();
        assert!((0.0..=1.0).contains(&output.propulsive_efficiency));
        let surrogate = PropellerSurrogate::generic_six_blade();
        assert!(output.blade_angle_deg >= surrogate.minimum_blade_angle_deg);
        assert!(output.blade_angle_deg <= surrogate.maximum_blade_angle_deg);
    }

    #[test]
    fn takeoff_speed_thrust_respects_actuator_disk_power() {
        let model = Pw127m568fModel::default();
        let output = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: 1.225,
                    true_airspeed_m_s: 85.0,
                },
                TurbopropCommand {
                    rating: Pw127mRating::NormalTakeoff,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            )
            .unwrap();
        let disk_area_m2 = PI * model.propeller_diameter_m.powi(2) / 4.0;
        let induced_velocity_m_s = 0.5
            * ((85.0_f64.powi(2) + 2.0 * output.propeller_thrust_n / (1.225 * disk_area_m2))
                .sqrt()
                - 85.0);
        let ideal_power_w = output.propeller_thrust_n * (85.0 + induced_velocity_m_s);

        assert!(ideal_power_w <= output.propeller_power_w * (1.0 + 1.0e-12));
        assert!(output.propulsive_efficiency < 1.0);
        assert!(output.propulsive_efficiency > 0.0);
    }

    #[test]
    fn actuator_disk_bound_is_continuous_near_takeoff_speed() {
        let model = Pw127m568fModel::default();
        let evaluate_at = |true_airspeed_m_s| {
            model
                .evaluate(
                    TurbopropCondition {
                        density_kg_m3: 1.225,
                        true_airspeed_m_s,
                    },
                    TurbopropCommand {
                        rating: Pw127mRating::NormalTakeoff,
                        power_fraction: 1.0,
                        mode: TurbopropMode::Governed,
                        propeller_speed_rpm: model.governed_propeller_speed_rpm,
                    },
                )
                .unwrap()
                .propeller_thrust_n
        };
        let below_takeoff = evaluate_at(85.0 - 1.0e-4);
        let above_takeoff = evaluate_at(85.0 + 1.0e-4);

        assert!((above_takeoff - below_takeoff).abs() / below_takeoff < 1.0e-5);
    }

    #[test]
    fn fl170_climb_extrapolates_when_generic_governor_map_is_exhausted() {
        let model = Pw127m568fModel {
            power_lapse_reference_density_kg_m3: 0.70,
            ..Pw127m568fModel::default()
        };
        let density_kg_m3 = 0.70;
        let true_airspeed_m_s = 120.0;
        let revolutions_s = model.governed_propeller_speed_rpm / 60.0;
        let advance_ratio = true_airspeed_m_s / (revolutions_s * model.propeller_diameter_m);
        let engine_power_w = model.maximum_climb_power_w;
        let propeller_power_w =
            (engine_power_w - model.accessory_power_w) * model.gearbox_efficiency;
        assert!(matches!(
            model.solve_governor(
                density_kg_m3,
                revolutions_s,
                advance_ratio,
                propeller_power_w,
                false,
            ),
            Err(TurbopropError::GovernorNoSolution { .. })
        ));

        let output = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3,
                    true_airspeed_m_s,
                },
                TurbopropCommand {
                    rating: Pw127mRating::MaximumClimb,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            )
            .unwrap();
        let disk_area_m2 = PI * model.propeller_diameter_m.powi(2) / 4.0;
        // The fallback bounds the thrust on the blade-efficiency share of the
        // shaft power, which is the same effective-power loss factor the
        // governed branch now applies, and not on the whole of it.
        let expected_fallback_thrust_n = actuator_disk_thrust_bound_n(
            model.blade_efficiency(advance_ratio) * output.propeller_power_w,
            density_kg_m3,
            disk_area_m2,
            true_airspeed_m_s,
        );

        assert!((output.propeller_thrust_n - expected_fallback_thrust_n).abs() < 1.0e-8);
        assert_eq!(
            output.blade_angle_deg,
            model.surrogate.maximum_blade_angle_deg
        );
        assert!((0.0..1.0).contains(&output.propulsive_efficiency));
        assert_eq!(
            output.propeller_model_uncertainty,
            ModelUncertainty::UnquantifiedSurrogate
        );
    }

    #[test]
    fn governed_high_advance_ratio_rejects_negative_map_thrust_and_extrapolates() {
        let model = Pw127m568fModel::default();
        let density_kg_m3 = 0.70;
        let true_airspeed_m_s = 180.0;
        let power_fraction = 0.20;
        let revolutions_s = model.governed_propeller_speed_rpm / 60.0;
        let advance_ratio = true_airspeed_m_s / (revolutions_s * model.propeller_diameter_m);
        let engine_power_w = model.maximum_continuous_power_w * power_fraction;
        let propeller_power_w =
            (engine_power_w - model.accessory_power_w) * model.gearbox_efficiency;
        assert_eq!(
            model
                .solve_governor(
                    density_kg_m3,
                    revolutions_s,
                    advance_ratio,
                    propeller_power_w,
                    false,
                )
                .unwrap_err(),
            TurbopropError::NonPhysicalResult("negative forward thrust")
        );

        let output = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3,
                    true_airspeed_m_s,
                },
                TurbopropCommand {
                    rating: Pw127mRating::MaximumContinuous,
                    power_fraction,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            )
            .unwrap();
        let disk_area_m2 = PI * model.propeller_diameter_m.powi(2) / 4.0;
        // The fallback bounds the thrust on the blade-efficiency share of the
        // shaft power, which is the same effective-power loss factor the
        // governed branch now applies, and not on the whole of it.
        let expected_fallback_thrust_n = actuator_disk_thrust_bound_n(
            model.blade_efficiency(advance_ratio) * output.propeller_power_w,
            density_kg_m3,
            disk_area_m2,
            true_airspeed_m_s,
        );

        assert!((output.propeller_thrust_n - expected_fallback_thrust_n).abs() < 1.0e-8);
        assert!(output.propeller_thrust_n > 0.0);
        assert!((0.0..1.0).contains(&output.propulsive_efficiency));
    }

    #[test]
    fn invalid_domain_is_rejected_without_clipping() {
        let mut command = nominal_command();
        command.power_fraction = 1.01;
        assert!(matches!(
            Pw127m568fModel::default().evaluate(
                TurbopropCondition {
                    density_kg_m3: 1.225,
                    true_airspeed_m_s: 50.0,
                },
                command
            ),
            Err(TurbopropError::OutsideDomain {
                field: "power_fraction",
                ..
            })
        ));
    }

    #[test]
    fn unavailable_568f_modes_are_explicit_errors() {
        let mut command = nominal_command();
        command.mode = TurbopropMode::Reverse;
        assert_eq!(
            Pw127m568fModel::default()
                .evaluate(
                    TurbopropCondition {
                        density_kg_m3: 1.225,
                        true_airspeed_m_s: 0.0,
                    },
                    command
                )
                .unwrap_err(),
            TurbopropError::UnsupportedMode(TurbopropMode::Reverse)
        );
    }

    #[test]
    fn atr_adapter_aggregates_two_units_once_and_exposes_resources() {
        let system = atr_system();
        let result = system
            .evaluate(&system_request(FailureState::None))
            .unwrap();
        let unit = Pw127m568fModel::default()
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: 1.225,
                    true_airspeed_m_s: 0.0,
                },
                TurbopropCommand {
                    rating: Pw127mRating::NormalTakeoff,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: 1_200.0,
                },
            )
            .unwrap();
        assert!((result.body_force_n[0] - 2.0 * unit.total_thrust_n).abs() < 1.0e-9);
        assert_eq!(result.body_moment_nm, [0.0; 3]);
        assert_eq!(result.shaft_power_w, Some(2.0 * unit.propeller_power_w));
        assert_eq!(result.torque_nm, Some(2.0 * unit.propeller_torque_n_m));
        assert_eq!(result.rotational_speed_rpm, Some(1_200.0));
        assert_eq!(
            result.resource_flows[0].mass_flow_kg_s,
            Some(2.0 * unit.fuel_flow_kg_s)
        );
        assert!(matches!(
            result.validity,
            ValidityStatus::Extrapolated { .. }
        ));
    }

    #[test]
    fn atr_oei_uses_one_reserve_rated_engine_without_double_counting() {
        let system = atr_system();
        let result = system
            .evaluate(&system_request(FailureState::UnitsUnavailable(vec![0])))
            .unwrap();
        let unit = Pw127m568fModel::default()
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: 1.225,
                    true_airspeed_m_s: 0.0,
                },
                TurbopropCommand {
                    rating: Pw127mRating::MaximumTakeoffReserve,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: 1_200.0,
                },
            )
            .unwrap();
        assert!((result.body_force_n[0] - unit.total_thrust_n).abs() < 1.0e-9);
        assert_eq!(result.shaft_power_w, Some(unit.propeller_power_w));
        assert!(result.active_limits[0]
            .name
            .contains("MaximumTakeoffReserve"));
    }

    #[test]
    fn normalized_force_is_inverted_in_force_space_not_power_space() {
        let system = atr_system();
        let maximum = system
            .evaluate(&system_request(FailureState::None))
            .unwrap();
        let mut half_request = system_request(FailureState::None);
        half_request.demand = PropulsionDemand::NormalizedForce(0.5);
        let half = system.evaluate(&half_request).unwrap();
        assert!((half.body_force_n[0] / maximum.body_force_n[0] - 0.5).abs() < 1.0e-10);
        assert!(half.shaft_power_w.unwrap() / maximum.shaft_power_w.unwrap() < 0.5);
    }

    #[test]
    fn rated_fraction_preserves_selected_rating_and_power_fraction() {
        let system = atr_system();
        let mut half_request = system_request(FailureState::None);
        half_request.demand = PropulsionDemand::RatedFraction {
            rating: PropulsionRating::Cruise,
            fraction: 0.5,
        };
        let half = system.evaluate(&half_request).unwrap();
        let model = Pw127m568fModel::default();
        let idle_power_w = model.rated_shaft_power_w(Pw127mRating::FlightIdleSurrogate);
        let interpolated_engine_power_w =
            idle_power_w + 0.5 * (model.maximum_cruise_power_w - idle_power_w);
        let expected_unit_propeller_power_w =
            (interpolated_engine_power_w - model.accessory_power_w) * model.gearbox_efficiency;

        assert_eq!(
            half.shaft_power_w,
            Some(2.0 * expected_unit_propeller_power_w)
        );
        assert_eq!(
            half.achieved_demand,
            PropulsionDemand::RatedFraction {
                rating: PropulsionRating::Cruise,
                fraction: 0.5,
            }
        );
        assert_eq!(half.active_limits[0].utilization, 0.5);
    }

    #[test]
    fn non_idle_rated_fraction_endpoints_span_idle_to_named_rating_power() {
        let system = atr_system();
        let model = Pw127m568fModel::default();
        let evaluate_at = |fraction| {
            let mut request = system_request(FailureState::None);
            request.demand = PropulsionDemand::RatedFraction {
                rating: PropulsionRating::MaximumContinuous,
                fraction,
            };
            system.evaluate(&request).unwrap()
        };
        let at_idle_floor = evaluate_at(0.0);
        let at_named_rating = evaluate_at(1.0);
        let idle_engine_power_w = model.rated_shaft_power_w(Pw127mRating::FlightIdleSurrogate);
        let expected_idle_propeller_power_w =
            (idle_engine_power_w - model.accessory_power_w) * model.gearbox_efficiency;
        let expected_full_propeller_power_w =
            (model.maximum_continuous_power_w - model.accessory_power_w) * model.gearbox_efficiency;

        assert_eq!(
            at_idle_floor.shaft_power_w,
            Some(2.0 * expected_idle_propeller_power_w)
        );
        assert_eq!(
            at_named_rating.shaft_power_w,
            Some(2.0 * expected_full_propeller_power_w)
        );
        assert_eq!(at_idle_floor.active_limits[0].utilization, 0.0);
        assert_eq!(at_named_rating.active_limits[0].utilization, 1.0);
    }

    #[test]
    fn flight_idle_rated_fraction_evaluates_directly_in_idle_mode() {
        let system = atr_system();
        let mut request = system_request(FailureState::None);
        request.demand = PropulsionDemand::RatedFraction {
            rating: PropulsionRating::FlightIdle,
            fraction: 1.0,
        };
        let result = system.evaluate(&request).unwrap();
        let model = Pw127m568fModel::default();
        let unit = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: request.flight.density_kg_m3,
                    true_airspeed_m_s: request.flight.velocity_m_s,
                },
                TurbopropCommand {
                    rating: Pw127mRating::FlightIdleSurrogate,
                    power_fraction: 1.0,
                    mode: TurbopropMode::FlightIdle,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            )
            .unwrap();

        assert_eq!(result.body_force_n[0], 2.0 * unit.total_thrust_n);
        assert_eq!(result.shaft_power_w, Some(2.0 * unit.propeller_power_w));
        assert_eq!(result.active_limits[0].utilization, 1.0);
        assert!(result.active_limits[0].name.contains("FlightIdleSurrogate"));
    }

    #[test]
    fn flight_idle_uses_neutral_force_without_a_windmilling_map() {
        let model = Pw127m568fModel::default();
        let density_kg_m3 = 0.95;
        let revolutions_s = model.governed_propeller_speed_rpm / 60.0;
        let engine_power_w = model.rated_shaft_power_w(Pw127mRating::FlightIdleSurrogate)
            * model.power_lapse_fraction(Pw127mRating::FlightIdleSurrogate, density_kg_m3);
        let propeller_power_w =
            (engine_power_w - model.accessory_power_w) * model.gearbox_efficiency;
        let advance_ratio_at_100 = 100.0 / (revolutions_s * model.propeller_diameter_m);
        let polynomial_solution = model
            .solve_governor(
                density_kg_m3,
                revolutions_s,
                advance_ratio_at_100,
                propeller_power_w,
                true,
            )
            .unwrap();
        assert!(polynomial_solution.1 < 0.0);

        for true_airspeed_m_s in [100.0, 115.0, 130.0] {
            let output = model
                .evaluate(
                    TurbopropCondition {
                        density_kg_m3,
                        true_airspeed_m_s,
                    },
                    TurbopropCommand {
                        rating: Pw127mRating::FlightIdleSurrogate,
                        power_fraction: 1.0,
                        mode: TurbopropMode::FlightIdle,
                        propeller_speed_rpm: model.governed_propeller_speed_rpm,
                    },
                )
                .unwrap();

            assert_eq!(output.propeller_thrust_n, 0.0);
            assert_eq!(output.total_thrust_n, 0.0);
            assert_eq!(output.propulsive_efficiency, 0.0);
        }
    }

    #[test]
    fn atr_adapter_rejects_unavailable_maps_and_nonmechanical_loads() {
        let system = atr_system();
        let mut request = system_request(FailureState::None);
        request.mode = OperatingMode::Reverse;
        assert!(matches!(
            system.evaluate(&request),
            Err(PropulsionError::UnsupportedMode(OperatingMode::Reverse))
        ));
        request.mode = OperatingMode::Normal;
        request.demand = PropulsionDemand::Rating(PropulsionRating::Cruise);
        let cruise = system.evaluate(&request).unwrap();
        assert!(matches!(
            cruise.validity,
            ValidityStatus::Extrapolated { .. }
        ));
        request.demand = PropulsionDemand::NormalizedForce(0.5);
        request.loads.electrical_power_w = 1_000.0;
        assert!(matches!(
            system.evaluate(&request),
            Err(PropulsionError::UnsupportedDemand(_))
        ));
    }

    #[test]
    fn shutdown_requires_zero_demand_and_has_no_active_power_rating() {
        let system = atr_system();
        let mut request = system_request(FailureState::None);
        request.mode = OperatingMode::Shutdown;
        assert!(matches!(
            system.evaluate(&request),
            Err(PropulsionError::UnsupportedDemand(_))
        ));
        request.demand = PropulsionDemand::NormalizedForce(0.0);
        let result = system.evaluate(&request).unwrap();
        assert_eq!(result.body_force_n, [0.0; 3]);
        assert_eq!(result.rotational_speed_rpm, Some(0.0));
        assert_eq!(
            result.achieved_demand,
            PropulsionDemand::NormalizedForce(0.0)
        );
        assert_eq!(
            result.active_limits[0].name,
            "propulsion shutdown commanded"
        );
        assert!(!result.active_limits[0].name.contains("PW127M"));
    }

    #[test]
    fn unavailable_unit_semantics_are_normalized_and_trace_zero_capability() {
        let system = atr_system();
        let aeo = system
            .evaluate(&system_request(FailureState::None))
            .unwrap();
        let empty_selection = system
            .evaluate(&system_request(FailureState::UnitsUnavailable(Vec::new())))
            .unwrap();
        assert_eq!(empty_selection.body_force_n, aeo.body_force_n);
        assert_eq!(empty_selection.shaft_power_w, aeo.shaft_power_w);

        let unavailable = system
            .evaluate(&system_request(FailureState::UnitsUnavailable(vec![0, 1])))
            .unwrap();
        assert_eq!(unavailable.body_force_n, [0.0; 3]);
        assert_eq!(unavailable.shaft_power_w, Some(0.0));
        assert_eq!(
            unavailable.achieved_demand,
            PropulsionDemand::NormalizedForce(0.0)
        );
        assert!(unavailable.active_limits[0]
            .name
            .contains("zero capability"));
    }

    /// The field-performance contract must be a view of the same evaluation a
    /// caller could have assembled itself, not a second model. Every thrust it
    /// publishes has to reproduce `evaluate` at the stated operating point to
    /// the bit, and its three speeds have to be the ones it says they are.
    #[test]
    fn the_field_performance_contract_reproduces_the_evaluated_operating_points() {
        let model = Pw127m568fModel::default();
        // 115 kt lift-off, the take-off point the blade-efficiency knee is
        // declared at.
        let lift_off_m_s = 59.2;
        let field = model
            .field_performance(1.225, Pw127mRating::NormalTakeoff, lift_off_m_s)
            .unwrap();

        let at = |true_airspeed_m_s: f64| {
            model
                .evaluate(
                    TurbopropCondition {
                        density_kg_m3: 1.225,
                        true_airspeed_m_s,
                    },
                    TurbopropCommand {
                        rating: Pw127mRating::NormalTakeoff,
                        power_fraction: 1.0,
                        mode: TurbopropMode::Governed,
                        propeller_speed_rpm: model.governed_propeller_speed_rpm,
                    },
                )
                .unwrap()
        };
        assert_eq!(field.static_thrust_per_engine_n, at(0.0).total_thrust_n);
        assert_eq!(
            field.lift_off_thrust_per_engine_n,
            at(lift_off_m_s).total_thrust_n
        );
        // The roll mean is taken at V_LOF / sqrt(2), which is where a
        // quantity linear in V^2 equals its distance-weighted mean over a
        // ground roll.
        assert!(
            (field.mean_ground_roll_true_airspeed_m_s - lift_off_m_s / std::f64::consts::SQRT_2)
                .abs()
                < 1e-12
        );
        assert_eq!(
            field.mean_ground_roll_thrust_per_engine_n,
            at(field.mean_ground_roll_true_airspeed_m_s).total_thrust_n
        );
        // A propeller at constant shaft power loses thrust as it accelerates.
        assert!(
            field.static_thrust_per_engine_n > field.mean_ground_roll_thrust_per_engine_n
                && field.mean_ground_roll_thrust_per_engine_n > field.lift_off_thrust_per_engine_n
        );
        // Sea-level static: the lapse is unity, so the available power is the
        // certificated rating itself.
        assert!(
            (field.available_shaft_power_per_engine_w
                - Pw127mRating::NormalTakeoff.shaft_power_w())
            .abs()
                < 1e-6
        );
    }

    /// No jet thrust may appear anywhere in the contract, at any speed. This
    /// is the acceptance statement for a shaft-power engine: the propeller
    /// carries the whole force, and the residual core term stays the declared
    /// zero rather than becoming a place to put a converted shaft power.
    #[test]
    fn the_field_performance_contract_manufactures_no_jet_thrust() {
        let model = Pw127m568fModel::default();
        assert_eq!(model.residual_jet_thrust_n, 0.0);
        let field = model
            .field_performance(1.225, Pw127mRating::NormalTakeoff, 59.2)
            .unwrap();
        assert_eq!(field.residual_jet_thrust_per_engine_n, 0.0);
        assert_eq!(
            field.model_uncertainty,
            ModelUncertainty::UnquantifiedSurrogate
        );
        assert!(field.provenance.contains("568F"));
    }

    /// The declared bands must have the sign the evidence gives them. The
    /// static figure of merit is now the class preliminary-design 0.70 inside
    /// a 0.65-0.80 band (it was 0.72 at the optimistic end of an older
    /// 0.50-0.70 band, so the static band was one-sided), so the static thrust
    /// band points both ways; the forward-flight thrust can still only be
    /// higher, because its blade efficiency sits at the conservative end of
    /// its band.
    #[test]
    fn the_declared_thrust_bands_carry_the_sign_their_evidence_gives_them() {
        let model = Pw127m568fModel::default();
        let field = model
            .field_performance(1.225, Pw127mRating::NormalTakeoff, 59.2)
            .unwrap();

        let static_band = field.static_thrust_uncertainty;
        // (0.65 / 0.70)^(2/3) - 1 and (0.80 / 0.70)^(2/3) - 1.
        assert!((static_band.relative_low - (-0.048_205)).abs() < 1e-5);
        assert!((static_band.relative_high - 0.093_103).abs() < 1e-5);

        let forward_band = field.forward_flight_thrust_uncertainty;
        assert_eq!(forward_band.relative_low, 0.0);
        // 0.91 / 0.86 - 1.
        assert!((forward_band.relative_high - 0.058_139_5).abs() < 1e-6);

        // A model already at an end of a band is given nothing beyond it
        // rather than a fabricated symmetric band.
        for (figure_of_merit, low_is_zero) in [(0.65, true), (0.80, false)] {
            let mut bounded = model;
            bounded.static_figure_of_merit = figure_of_merit;
            bounded.blade_efficiency_cruise = 0.91;
            let bounded = bounded
                .field_performance(1.225, Pw127mRating::NormalTakeoff, 59.2)
                .unwrap();
            let band = bounded.static_thrust_uncertainty;
            if low_is_zero {
                assert_eq!(band.relative_low, 0.0);
                assert!(band.relative_high > 0.0);
            } else {
                assert_eq!(band.relative_high, 0.0);
                assert!(band.relative_low < 0.0);
            }
            assert_eq!(bounded.forward_flight_thrust_uncertainty.relative_high, 0.0);
        }
    }

    /// The envelope is only honest if the evaluator actually enforces it: a
    /// mode the envelope lists as unsupported must be refused with a typed
    /// error rather than answered with a plausible number.
    #[test]
    fn every_mode_the_envelope_calls_unsupported_is_refused_by_the_evaluator() {
        let model = Pw127m568fModel::default();
        let envelope = model.operating_envelope();
        let condition = TurbopropCondition {
            density_kg_m3: 1.225,
            true_airspeed_m_s: 100.0,
        };
        for (mode, reason) in envelope.unsupported_modes {
            assert!(!reason.is_empty());
            let outcome = model.evaluate(
                condition,
                TurbopropCommand {
                    rating: Pw127mRating::MaximumContinuous,
                    power_fraction: 1.0,
                    mode: *mode,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            );
            assert!(
                matches!(outcome, Err(TurbopropError::UnsupportedMode(refused)) if refused == *mode),
                "{mode:?} is listed as unsupported but was not refused"
            );
        }
        assert!(envelope.supported_modes.contains(&TurbopropMode::Governed));
        // A surrogate mode is the opposite case and must be separated from the
        // refused ones: it answers, and the answer is unvalidated. Reading it
        // as supported is the failure this split exists to prevent.
        for (mode, reason) in envelope.surrogate_modes {
            assert!(!reason.is_empty());
            assert!(!envelope.supported_modes.contains(mode));
            assert!(model
                .evaluate(
                    condition,
                    TurbopropCommand {
                        rating: Pw127mRating::FlightIdleSurrogate,
                        power_fraction: 1.0,
                        mode: *mode,
                        propeller_speed_rpm: model.governed_propeller_speed_rpm,
                    },
                )
                .is_ok());
        }
        // The lapse floor is a real density, and it is below the tropopause
        // value the aircraft can actually reach.
        assert!(
            envelope.power_lapse_floor_density_kg_m3 > 0.0
                && envelope.power_lapse_floor_density_kg_m3 < 0.4
        );
        assert!(envelope.fuel_validity.contains("762 kg/h"));
    }

    /// Replaces `the_published_cruise_fuel_flow_anchor_is_reproduced_exactly`
    /// and `the_implied_fuel_consumption_is_constant_and_its_excess_is_quantified`,
    /// which pinned the single-point calibration (762 kg/h to the gram, a
    /// constant 0.3646 kg/kWh 21-28 % above measured PW120A data). The
    /// published flow is now a validation point: the model is evaluated
    /// against it, and moving the published datum does not move the model.
    #[test]
    fn the_published_cruise_fuel_flow_is_a_validation_point_not_a_fit() {
        let model = Pw127m568fModel::default();
        let validation = model.fuel_validation().unwrap();
        assert!((validation.published_total_fuel_flow_kg_s * 3_600.0 - 762.0).abs() < 1e-6);
        assert_eq!(validation.published_engine_count, 2);
        assert!(!validation.altitude_is_published);

        // The modelled number is exactly the evaluator's at the stated point.
        let temperature_k =
            isa_temperature_from_density_k(model.fuel_reference_density_kg_m3, 1.225);
        let point = model
            .evaluate_at_temperature(
                TurbopropCondition {
                    density_kg_m3: model.fuel_reference_density_kg_m3,
                    true_airspeed_m_s: model.operating_envelope().maximum_cruise_true_airspeed_m_s,
                },
                temperature_k,
                TurbopropCommand {
                    rating: Pw127mRating::MaximumCruise,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            )
            .unwrap();
        assert!(
            (validation.modelled_total_fuel_flow_kg_s - 2.0 * point.fuel_flow_kg_s).abs() < 1e-12
        );
        // An engine-level comparison inside the lapse relation's own 5-10 %
        // rms scatter plus the unpublished altitude: within 15 %.
        assert!(
            validation.relative_error.abs() < 0.15,
            "modelled {:.0} kg/h against 762 kg/h",
            validation.modelled_total_fuel_flow_kg_s * 3_600.0
        );
        assert!(
            (validation.source_psfc_band_kg_kwh.0..=validation.source_psfc_band_kg_kwh.1)
                .contains(&model.psfc_reference_kg_kwh)
        );
        // Higher is thinner and colder: less power and a lower PSFC, so less
        // fuel, monotonically across the flight levels the aircraft uses.
        let errors: Vec<f64> = validation
            .altitude_sensitivity
            .iter()
            .map(|entry| entry.1)
            .collect();
        assert!(
            errors.windows(2).all(|pair| pair[1] < pair[0]),
            "{errors:?}"
        );

        // Not a fit: halving the published datum changes the comparison and
        // leaves the engine's fuel flow untouched.
        let other = Pw127m568fModel {
            reference_psfc_kg_kwh: 0.5 * model.reference_psfc_kg_kwh,
            ..model
        };
        let other_validation = other.fuel_validation().unwrap();
        assert_eq!(
            other_validation.modelled_total_fuel_flow_kg_s,
            validation.modelled_total_fuel_flow_kg_s
        );
        assert!((other_validation.published_total_fuel_flow_kg_s * 3_600.0 - 381.0).abs() < 1e-6);
    }

    /// A shut-down engine burns nothing; the specific consumption of nothing
    /// is undefined, and reporting it as zero would read as a perfect engine.
    #[test]
    fn a_shut_down_engine_reports_no_fuel_and_no_specific_consumption() {
        let model = Pw127m568fModel::default();
        let output = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: 1.225,
                    true_airspeed_m_s: 100.0,
                },
                TurbopropCommand {
                    rating: Pw127mRating::MaximumContinuous,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Shutdown,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            )
            .unwrap();
        assert_eq!(output.fuel_flow_kg_s, 0.0);
        assert_eq!(output.total_thrust_n, 0.0);
        assert!(output.psfc_kg_kwh.is_nan());
    }

    /// The published maximum cruise speed is the one aircraft-level check the
    /// blade-efficiency correction can be held to: at maximum cruise power at
    /// its cruise altitude, the aircraft must not have a large thrust surplus
    /// at 275 kt. Before the correction it had 14 %.
    #[test]
    fn maximum_cruise_thrust_no_longer_exceeds_the_published_cruise_speed_by_a_surplus() {
        let model = Pw127m568fModel::default();
        let envelope = model.operating_envelope();
        // FL170, the ATR 72-600's declared cruise altitude in this product.
        let density_kg_m3 = 0.7;
        let cruise = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3,
                    true_airspeed_m_s: envelope.maximum_cruise_true_airspeed_m_s,
                },
                TurbopropCommand {
                    rating: Pw127mRating::MaximumCruise,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            )
            .unwrap();
        // The loss-free actuator-disc ideal at this disc loading is 0.976; a
        // model sitting on or above it is a propeller with no profile drag,
        // no tip loss and no inflow non-uniformity.
        assert!(
            cruise.propulsive_efficiency < 0.90,
            "eta_p = {} is above anything a real six-blade propeller reaches",
            cruise.propulsive_efficiency
        );
        assert!(cruise.propulsive_efficiency > 0.80);
    }

    #[test]
    fn a_rejected_model_parameter_is_named_with_its_own_value() {
        let model = Pw127m568fModel {
            governed_propeller_speed_rpm: 50.0,
            ..Pw127m568fModel::default()
        };
        let error = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: 1.0,
                    true_airspeed_m_s: 100.0,
                },
                nominal_command(),
            )
            .unwrap_err();
        assert_eq!(
            error,
            TurbopropError::OutsideDomain {
                field: "governed_propeller_speed_rpm",
                value: 50.0,
            }
        );
        assert_eq!(
            error.to_string(),
            "turboprop input outside domain: governed_propeller_speed_rpm=50"
        );
        let model = Pw127m568fModel {
            maximum_climb_power_w: -1.0,
            ..Pw127m568fModel::default()
        };
        assert_eq!(
            model
                .evaluate(
                    TurbopropCondition {
                        density_kg_m3: 1.0,
                        true_airspeed_m_s: 100.0,
                    },
                    nominal_command(),
                )
                .unwrap_err(),
            TurbopropError::OutsideDomain {
                field: "maximum_climb_power_w",
                value: -1.0,
            }
        );
    }
}
