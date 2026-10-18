// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Fixture parsing and failed assertions identify broken source evidence in this
// test target; production calculation paths return typed errors instead.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Electric-propulsion solver and source-boundary tests on SYNTHETIC propeller tables.

use alas_uav::full_catalog;
use alas_uav::propulsion_electric::{
    apc_12x6e_performance_map, apc_performance_map, apc_performance_maps,
    assess_acc2026_electrical, native_propulsion_map, simulate_catalogue_mission,
    solve_catalogue_powertrain, solve_powertrain, Acc2026ElectricalFinding,
    CataloguePowertrainSelection, ElectricFlightCondition, ElectricMissionPhase,
    ElectricMissionPlan, ElectricMotorModel, ElectricPowertrainModel, EscElectricalModel,
    FixedVoltageBattery,
};
#[path = "support/prop_tables.rs"]
mod prop_tables;
use prop_tables::use_synthetic_tables;

/// The tables are SYNTHETIC (made-up numbers), so this checks the solver's
/// physics against the table itself, not against any manufacturer value: at the
/// returned point the motor voltage balance and the Ct/Cp scaling must hold.
#[test]
fn the_bracketed_solver_satisfies_the_torque_balance_on_the_table() {
    use_synthetic_tables();
    let propeller = apc_12x6e_performance_map()
        .expect("synthetic table parses")
        .clone();
    let (voltage, kv, resistance, rho) = (11.4_f64, 900.0_f64, 0.082_f64, 1.225_f64);

    for speed in [0.0, 10.0, 20.0] {
        let result = solve_powertrain(
            &ElectricPowertrainModel {
                battery: FixedVoltageBattery {
                    open_circuit_voltage_v: voltage,
                    internal_resistance_ohm: 0.0,
                },
                motor: ElectricMotorModel {
                    kv_rpm_per_v: kv,
                    no_load_current_a: 0.0,
                    resistance_ohm: resistance,
                },
                esc: EscElectricalModel {
                    series_resistance_ohm: 0.0,
                },
                propeller: propeller.clone(),
                motor_count: 1,
            },
            ElectricFlightCondition::full_power(speed, rho),
        )
        .unwrap_or_else(|error| panic!("{speed} m/s: {error}"));
        let point = result.per_motor;
        let coefficients = propeller
            .coefficients_at(point.rpm, speed)
            .expect("the solution lies inside the table");
        let n = point.rpm / 60.0;
        let d = propeller.diameter_m();
        let thrust = rho * coefficients.thrust_coefficient * n * n * d.powi(4);
        let torque =
            rho * coefficients.power_coefficient * n * n * d.powi(5) / (2.0 * std::f64::consts::PI);
        let current = torque * 2.0 * std::f64::consts::PI * kv / 60.0;
        assert_close(thrust, point.thrust_n, "thrust from Ct");
        assert_close(current, point.motor_current_a, "current from Cp");
        let residual = voltage - point.rpm / kv - current * resistance;
        assert!(
            residual.abs() < 1.0e-6,
            "{speed} m/s: voltage residual {residual}"
        );
        assert_close(voltage * current, result.battery_power_w, "battery power");
    }
}

#[test]
fn density_scales_the_table_load_without_reusing_static_thrust() {
    use_synthetic_tables();
    let propeller = apc_12x6e_performance_map()
        .expect("APC source table parses")
        .clone();
    let model = ElectricPowertrainModel {
        battery: FixedVoltageBattery {
            open_circuit_voltage_v: 11.4,
            internal_resistance_ohm: 0.0,
        },
        motor: ElectricMotorModel {
            kv_rpm_per_v: 900.0,
            no_load_current_a: 0.0,
            resistance_ohm: 0.082,
        },
        esc: EscElectricalModel {
            series_resistance_ohm: 0.0,
        },
        propeller,
        motor_count: 1,
    };
    let sea_level = solve_powertrain(&model, ElectricFlightCondition::full_power(20.0, 1.225))
        .expect("sea-level point solves");
    let lower_density = solve_powertrain(&model, ElectricFlightCondition::full_power(20.0, 1.0))
        .expect("lower-density point solves");

    assert!(lower_density.per_motor.thrust_n < sea_level.per_motor.thrust_n);
    assert!(lower_density.per_motor.rpm > sea_level.per_motor.rpm);
}

#[test]
fn zero_throttle_is_a_nonpropulsive_state_without_a_table_extrapolation() {
    use_synthetic_tables();
    let result = solve_powertrain(
        &ElectricPowertrainModel {
            battery: FixedVoltageBattery {
                open_circuit_voltage_v: 11.1,
                internal_resistance_ohm: 0.0,
            },
            motor: ElectricMotorModel {
                kv_rpm_per_v: 900.0,
                no_load_current_a: 1.2,
                resistance_ohm: 0.082,
            },
            esc: EscElectricalModel {
                series_resistance_ohm: 0.0,
            },
            propeller: apc_12x6e_performance_map()
                .expect("APC source table parses")
                .clone(),
            motor_count: 1,
        },
        ElectricFlightCondition {
            speed_m_s: 500.0,
            air_density_kg_m3: 1.225,
            throttle: 0.0,
        },
    )
    .expect("idle does not query an unavailable performance point");

    assert_eq!(result.per_motor.rpm, 0.0);
    assert_eq!(result.total_thrust_n, 0.0);
    assert_eq!(result.battery_current_a, 0.0);
}

#[test]
fn a_reviewed_selection_produces_the_optimizer_map_without_manual_operating_points() {
    use_synthetic_tables();
    let catalog = full_catalog().expect("full catalogue parses");
    let selection = selected_powertrain();
    let map = native_propulsion_map(&catalog, &selection, &[20.0, 10.0], 1.225)
        .expect("reviewed selection produces a native map");

    assert_eq!(map.motor_id, "tmotor-at2814-900kv");
    assert_eq!(map.propeller_id, "apc-12x6e");
    assert_eq!(map.series_cells, 3);
    assert_eq!(map.motor_count, 1);
    assert_eq!(map.points.len(), 2);
    assert!(map.points[0].speed_m_s < map.points[1].speed_m_s);
    assert!(map.points.iter().all(|point| {
        point.thrust_n > 0.0 && point.motor_current_a > 0.0 && point.motor_power_w > 0.0
    }));
}

#[test]
fn every_installed_table_is_addressable_without_curve_fitting() {
    use_synthetic_tables();
    let tables = apc_performance_maps().expect("the synthetic tables parse");
    assert_eq!(tables.len(), 6);
    let catalog = full_catalog().expect("full catalogue parses");
    assert!(tables
        .iter()
        .all(|table| catalog.get(table.propeller_id()).is_some()));
    let table = apc_performance_map("apc-9x4-5ep-f2b")
        .expect("the F2B source file has a stable catalogue key");
    assert_eq!(table.source_file(), "PER3_9x45EP(F2B).dat");
    assert_eq!(table.model(), "9x4.5EP");
    assert!((table.diameter_m() - 9.0 * 0.0254).abs() < 1.0e-12);
    assert!((table.pitch_m() - 4.5 * 0.0254).abs() < 1.0e-12);
    assert!(table.rpm_bounds_at_speed(10.0).is_some());
    assert!(table.coefficients_at(5_000.0, 10.0).is_some());
    // Outside the sampled envelope nothing is extrapolated.
    assert!(table.coefficients_at(9_000.0, 10.0).is_none());
    assert!(table.coefficients_at(5_000.0, 25.0).is_none());
}

#[test]
fn multi_motor_map_and_mission_sum_battery_demand_but_keep_motor_ratings_per_unit() {
    use_synthetic_tables();
    let catalog = full_catalog().expect("full catalogue parses");
    let mut selection = selected_powertrain();
    selection.motor_count = 2;
    let map = native_propulsion_map(&catalog, &selection, &[10.0, 20.0], 1.225)
        .expect("two identical propulsors produce one per-unit optimizer map");
    assert_eq!(map.motor_count, 2);
    let single = solve_catalogue_powertrain(
        &catalog,
        &selected_powertrain(),
        ElectricFlightCondition::full_power(10.0, 1.225),
    )
    .expect("single powertrain solves");
    let twin = solve_catalogue_powertrain(
        &catalog,
        &selection,
        ElectricFlightCondition::full_power(10.0, 1.225),
    )
    .expect("twin powertrain solves");
    assert_close(
        2.0 * single.total_thrust_n,
        twin.total_thrust_n,
        "twin thrust",
    );
    assert_close(
        2.0 * single.battery_current_a,
        twin.battery_current_a,
        "twin battery current",
    );
    assert_close(
        single.per_motor.motor_current_a,
        twin.per_motor.motor_current_a,
        "per motor current",
    );

    let mission = simulate_catalogue_mission(
        &catalog,
        &selection,
        &ElectricMissionPlan {
            evidence: "native three-phase test mission".to_owned(),
            phases: vec![
                mission_phase("takeoff", 30.0, 10.0, 1.0),
                mission_phase("climb", 90.0, 15.0, 0.95),
                mission_phase("cruise", 780.0, 20.0, 0.95),
            ],
        },
    )
    .expect("multi-phase mission solves");
    assert_eq!(mission.phases.len(), 3);
    assert_eq!(mission.total_duration_s, 900.0);
    assert!(mission.propulsion_energy_wh > 0.0);
    assert!(mission.maximum_battery_current_a >= mission.phases[0].propulsion.battery_current_a);
    let profile = mission.optimizer_profile();
    assert_eq!(profile.phases.len(), 3);
    assert_eq!(
        profile.phases[0].total_thrust_n,
        mission.phases[0].propulsion.total_thrust_n
    );
}

#[test]
fn kv_only_motor_estimates_remain_explicitly_unverified() {
    use_synthetic_tables();
    let mut catalog = full_catalog().expect("full catalogue parses");
    let record = catalog
        .records
        .iter_mut()
        .find(|record| record.id == "tmotor-at2814-900kv")
        .expect("reviewed motor exists");
    let alas_uav::ComponentKind::Motor(spec) = &mut record.kind else {
        panic!("selected motor must retain its motor type");
    };
    spec.winding_resistance_ohm = None;
    spec.no_load_current_a = None;
    let result = solve_catalogue_powertrain(
        &catalog,
        &selected_powertrain(),
        ElectricFlightCondition::full_power(10.0, 1.225),
    )
    .expect("Kv establishes an explicitly ideal bounded solve");
    assert!(result.checks.iter().any(|check| {
        matches!(
            check,
            alas_uav::propulsion_electric::ElectricalCheck::Unverified {
                field: "winding resistance",
                ..
            }
        )
    }));
    assert!(result
        .assumptions
        .iter()
        .any(|assumption| assumption.contains("ideal-motor")));

    // The optimizer consumes the map, which has no assumption list, so the
    // substitution must reach it through the evidence text.
    let map = native_propulsion_map(&catalog, &selected_powertrain(), &[10.0], 1.225)
        .expect("Kv establishes an explicitly ideal map");
    assert!(map.evidence.contains("ideal-motor"), "{}", map.evidence);
    let reviewed = native_propulsion_map(
        &full_catalog().expect("full catalogue parses"),
        &selected_powertrain(),
        &[10.0],
        1.225,
    )
    .expect("reviewed selection produces a native map");
    assert!(
        !reviewed.evidence.contains("ideal-motor"),
        "{}",
        reviewed.evidence
    );
}

#[test]
fn acc_screening_keeps_unverified_hardware_details_visible() {
    use_synthetic_tables();
    let catalog = full_catalog().expect("full catalogue parses");
    let mut selection = selected_powertrain();
    selection.motor_count = 2;
    let point = solve_catalogue_powertrain(
        &catalog,
        &selection,
        ElectricFlightCondition::full_power(0.0, 1.225),
    )
    .expect("two identical reviewed propulsors solve");
    let assessment = assess_acc2026_electrical(&catalog, &selection, &point)
        .expect("selected battery is a battery record");

    assert!(!assessment.has_failure(), "{:#?}", assessment.findings);
    assert!(!assessment.verified_compliant());
    assert!(assessment
        .findings
        .iter()
        .any(|finding| { matches!(finding, Acc2026ElectricalFinding::CurrentPenalty { .. }) }));
    assert!(assessment.findings.iter().any(|finding| {
        matches!(
            finding,
            Acc2026ElectricalFinding::Unverified {
                requirement: "battery connector",
                ..
            }
        )
    }));
    assert!(assessment.findings.iter().any(|finding| {
        matches!(
            finding,
            Acc2026ElectricalFinding::Unverified {
                requirement: "motor modification",
                ..
            }
        )
    }));
}

fn selected_powertrain() -> CataloguePowertrainSelection {
    CataloguePowertrainSelection::single_motor(
        "unmannedtech-gensace-gtech-2200-3s-45c-xt60",
        "tmotor-at2814-900kv",
        "hobbywing-skywalker-30a-v2-mini",
        "apc-12x6e",
    )
}

fn mission_phase(
    name: &str,
    duration_s: f64,
    speed_m_s: f64,
    throttle: f64,
) -> ElectricMissionPhase {
    ElectricMissionPhase {
        name: name.to_owned(),
        duration_s,
        condition: ElectricFlightCondition {
            speed_m_s,
            air_density_kg_m3: 1.225,
            throttle,
        },
    }
}

fn assert_close(expected: f64, actual: f64, case: &str) {
    let scale = expected.abs().max(1.0);
    assert!(
        (expected - actual).abs() <= 1.0e-11 * scale,
        "{case}: expected {expected:.15}, got {actual:.15}"
    );
}
