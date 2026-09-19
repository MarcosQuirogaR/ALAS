// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Preliminary, stand-alone turboprop operating-point model.
//!
//! This module deliberately contains no aircraft or mission wiring.  It separates
//! gas-turbine shaft power, accessory and gearbox losses, propeller force, residual
//! jet thrust, and fuel flow.  The PW127M ratings are certified installation data;
//! the fuel and propeller models are explicitly labelled family-level surrogates.

include!("turboprop_parts/part_01.rs");
include!("turboprop_parts/part_02.rs");
include!("turboprop_parts/part_03.rs");
include!("turboprop_parts/part_04.rs");

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
        assert_eq!(
            fl170_power_w,
            model.maximum_climb_power_w * (0.70_f64 / 1.225).powf(0.75)
        );
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

    #[test]
    fn cruise_fuel_anchor_reproduces_the_published_aircraft_flow() {
        let model = Pw127m568fModel::default();
        let command = TurbopropCommand {
            rating: Pw127mRating::MaximumCruise,
            power_fraction: 1.0,
            mode: TurbopropMode::Governed,
            propeller_speed_rpm: model.governed_propeller_speed_rpm,
        };
        let reference_output = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: model.fuel_reference_density_kg_m3,
                    true_airspeed_m_s: 140.0,
                },
                command,
            )
            .unwrap();
        let sea_level_output = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: model.power_lapse_reference_density_kg_m3,
                    true_airspeed_m_s: 140.0,
                },
                command,
            )
            .unwrap();

        assert!((2.0 * reference_output.fuel_flow_kg_s * 3_600.0 - 762.0).abs() < 1.0e-9);
        assert!(sea_level_output.fuel_flow_kg_s > reference_output.fuel_flow_kg_s);
    }

    /// A propeller that reaches the ideal actuator-disk bound has no profile
    /// loss at all, and this model used to report exactly that: eta_p = 0.98
    /// at the ATR's FL170 / 275 kt cruise. The blade-efficiency factor puts
    /// the cruise point inside the band three independent sources agree on,
    /// and leaves the static thrust, which was never on the ideal bound,
    /// exactly where it was.
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

        // The loss-free bound at this condition, which is what the model must
        // no longer sit on.
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
        let expected_fuel = output.engine_shaft_power_w * model.reference_psfc_kg_kwh
            / model.power_lapse_fraction(model.fuel_reference_density_kg_m3)
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
            * model.power_lapse_fraction(density_kg_m3);
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
                && field.mean_ground_roll_thrust_per_engine_n
                    > field.lift_off_thrust_per_engine_n
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

    /// The declared bands must have the sign the evidence gives them: the
    /// static thrust can only be lower than modelled because its figure of
    /// merit sits at the optimistic end of its band, and the forward-flight
    /// thrust can only be higher because its blade efficiency sits at the
    /// conservative end of a different one. A band that pointed both ways
    /// would be a guess, not a reading of the evidence.
    #[test]
    fn the_declared_thrust_bands_carry_the_sign_their_evidence_gives_them() {
        let model = Pw127m568fModel::default();
        let field = model
            .field_performance(1.225, Pw127mRating::NormalTakeoff, 59.2)
            .unwrap();

        let static_band = field.static_thrust_uncertainty;
        assert_eq!(static_band.relative_high, 0.0);
        // (0.50 / 0.72)^(2/3) - 1.
        assert!((static_band.relative_low - (-0.215_803_3)).abs() < 1e-6);

        let forward_band = field.forward_flight_thrust_uncertainty;
        assert_eq!(forward_band.relative_low, 0.0);
        // 0.91 / 0.86 - 1.
        assert!((forward_band.relative_high - 0.058_139_5).abs() < 1e-6);

        // A model already at the optimistic end of both bands must be given
        // no band at all rather than a fabricated symmetric one.
        let mut bounded = model;
        bounded.static_figure_of_merit = 0.50;
        bounded.blade_efficiency_cruise = 0.91;
        let bounded = bounded
            .field_performance(1.225, Pw127mRating::NormalTakeoff, 59.2)
            .unwrap();
        assert_eq!(bounded.static_thrust_uncertainty.relative_low, 0.0);
        assert_eq!(bounded.forward_flight_thrust_uncertainty.relative_high, 0.0);
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
        assert!(envelope
            .supported_modes
            .contains(&TurbopropMode::Governed));
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

    /// The one aircraft-level datum the fuel model has must be reproduced
    /// exactly, and nothing in this lane's propeller work may disturb it: the
    /// blade-efficiency correction changed thrust, not shaft power, so the
    /// published 762 kg/h must still come back to the gram.
    #[test]
    fn the_published_cruise_fuel_flow_anchor_is_reproduced_exactly() {
        let model = Pw127m568fModel::default();
        let calibration = model.fuel_calibration();
        let cruise = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: model.fuel_reference_density_kg_m3,
                    true_airspeed_m_s: model.operating_envelope().maximum_cruise_true_airspeed_m_s,
                },
                TurbopropCommand {
                    rating: Pw127mRating::MaximumCruise,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            )
            .unwrap();
        let both_engines_kg_h = 2.0 * cruise.fuel_flow_kg_s * 3_600.0;
        assert!(
            (both_engines_kg_h - 762.0).abs() < 1e-3,
            "published anchor is 762 kg/h for both engines, got {both_engines_kg_h}"
        );
        assert_eq!(calibration.anchor_engine_count, 2);
        assert!(
            (calibration.anchor_total_fuel_flow_kg_s * 3_600.0 - 762.0).abs() < 1e-9,
            "the typed anchor must be the same number the model reproduces"
        );
    }

    /// The fuel model is one constant, and the contract has to say so in a way
    /// a consumer can act on rather than in prose only.
    #[test]
    fn the_implied_fuel_consumption_is_constant_and_its_excess_is_quantified() {
        let model = Pw127m568fModel::default();
        let calibration = model.fuel_calibration();

        // `reference_psfc_kg_kwh` is not a PSFC. The distinction is the whole
        // point of the field's documentation and is worth 52 % of the number.
        assert!(
            (model.implied_psfc_kg_kwh() - 0.364_630).abs() < 1e-6,
            "{}",
            model.implied_psfc_kg_kwh()
        );
        assert!(model.implied_psfc_kg_kwh() > 1.5 * model.reference_psfc_kg_kwh);

        // Flat across the whole envelope: same value at sea-level take-off, at
        // cruise and at flight idle. If a future fuel deck ever varies it,
        // this assertion is the one that has to be rewritten deliberately.
        let at = |density_kg_m3: f64, true_airspeed_m_s: f64, rating: Pw127mRating| {
            model
                .evaluate(
                    TurbopropCondition {
                        density_kg_m3,
                        true_airspeed_m_s,
                    },
                    TurbopropCommand {
                        rating,
                        power_fraction: 1.0,
                        mode: TurbopropMode::Governed,
                        propeller_speed_rpm: model.governed_propeller_speed_rpm,
                    },
                )
                .unwrap()
                .psfc_kg_kwh
        };
        let sea_level_takeoff = at(1.225, 0.0, Pw127mRating::NormalTakeoff);
        let cruise = at(0.70, 141.47, Pw127mRating::MaximumCruise);
        let low_power = at(1.0, 100.0, Pw127mRating::FlightIdleSurrogate);
        assert_eq!(sea_level_takeoff, cruise);
        assert_eq!(cruise, low_power);
        assert!(!calibration.psfc_varies_with_condition);

        // The excess over measurement is computed, not asserted in prose.
        let (low, high) = calibration.relative_excess_over_measured;
        assert!((low - 0.283_9).abs() < 1e-3, "{low}");
        assert!((high - 0.207_4).abs() < 1e-3, "{high}");
        assert!(calibration.measured_psfc_band_kg_kwh.0 < calibration.implied_psfc_kg_kwh);

        // The anchor's altitude is not published, and the model says so rather
        // than presenting the assumed density as a datum.
        assert!(!calibration.anchor_altitude_is_published);
        let spread = calibration.anchor_altitude_sensitivity;
        assert!(spread.len() >= 3);
        let lowest = spread.iter().map(|entry| entry.1).fold(f64::MAX, f64::min);
        let highest = spread.iter().map(|entry| entry.1).fold(f64::MIN, f64::max);
        assert!(
            highest / lowest > 1.2,
            "the undeclared anchor altitude is worth more than a rounding error"
        );
        // And the declared assumption sits inside the range it spans.
        assert!(calibration.implied_psfc_kg_kwh > lowest);
        assert!(calibration.implied_psfc_kg_kwh < highest);
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
}
