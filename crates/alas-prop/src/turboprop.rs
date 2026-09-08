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
        assert!((near_static_thrust - static_thrust).abs() / static_thrust < 1.0e-10);
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
        let expected_fallback_thrust_n = actuator_disk_thrust_bound_n(
            model.static_figure_of_merit * output.propeller_power_w,
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
        let expected_fallback_thrust_n = actuator_disk_thrust_bound_n(
            model.static_figure_of_merit * output.propeller_power_w,
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
}
