// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Fixture parsing and failed assertions identify broken source evidence in this
// test target; production calculation paths return typed errors instead.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Legacy electric-propulsion parity and source-boundary tests.

use alas_uav::full_catalog;
use alas_uav::propulsion_electric::{
    apc_12x6e_performance_map, apc_performance_map, apc_performance_maps,
    assess_acc2026_electrical, native_propulsion_map, simulate_catalogue_mission,
    solve_catalogue_powertrain, solve_powertrain, Acc2026ElectricalFinding,
    CataloguePowertrainSelection, ElectricFlightCondition, ElectricMissionPhase,
    ElectricMissionPlan, ElectricMotorModel, ElectricPowertrainModel, EscElectricalModel,
    FixedVoltageBattery,
};
use serde::Deserialize;

const FIXTURE: &str = include_str!("../../../golden/prop_elec/apc_12x6e_fixture.json");
const UIUC_STATIC_BENCHMARK: &str =
    include_str!("../../../golden/prop_elec/uiuc_apc_12x6_static_benchmark.json");

#[derive(Debug, Deserialize)]
struct LegacyFixture {
    legacy_ideal_cases: Vec<LegacyCase>,
}

#[derive(Debug, Deserialize)]
struct LegacyCase {
    name: String,
    speed_m_s: f64,
    battery_voltage_v: f64,
    throttle: f64,
    motor_kv_rpm_per_v: f64,
    motor_resistance_ohm: f64,
    motor_no_load_current_a: f64,
    expected: ExpectedPoint,
}

#[derive(Debug, Deserialize)]
struct ExpectedPoint {
    rpm: f64,
    motor_current_a: f64,
    thrust_n: f64,
    shaft_power_w: f64,
    battery_power_w: f64,
}

#[derive(Debug, Deserialize)]
struct UiucStaticBenchmark {
    comparison: BenchmarkComparison,
    samples: Vec<BenchmarkSample>,
}

#[derive(Debug, Deserialize)]
struct BenchmarkComparison {
    maximum_relative_ct_error: f64,
    maximum_relative_cp_error: f64,
}

#[derive(Debug, Deserialize)]
struct BenchmarkSample {
    rpm: f64,
    thrust_coefficient: f64,
    power_coefficient: f64,
}

#[test]
fn the_bracketed_solver_matches_the_legacy_ideal_torque_balance() {
    let fixture: LegacyFixture = serde_json::from_str(FIXTURE).expect("fixture parses");
    let propeller = apc_12x6e_performance_map()
        .expect("APC source table parses")
        .clone();

    for case in fixture.legacy_ideal_cases {
        let result = solve_powertrain(
            &ElectricPowertrainModel {
                battery: FixedVoltageBattery {
                    open_circuit_voltage_v: case.battery_voltage_v,
                    internal_resistance_ohm: 0.0,
                },
                motor: ElectricMotorModel {
                    kv_rpm_per_v: case.motor_kv_rpm_per_v,
                    no_load_current_a: case.motor_no_load_current_a,
                    resistance_ohm: case.motor_resistance_ohm,
                },
                esc: EscElectricalModel {
                    series_resistance_ohm: 0.0,
                },
                propeller: propeller.clone(),
                motor_count: 1,
            },
            ElectricFlightCondition {
                speed_m_s: case.speed_m_s,
                air_density_kg_m3: 1.225,
                throttle: case.throttle,
            },
        )
        .unwrap_or_else(|error| panic!("{}: {error}", case.name));

        assert_close(case.expected.rpm, result.per_motor.rpm, &case.name);
        assert_close(
            case.expected.motor_current_a,
            result.per_motor.motor_current_a,
            &case.name,
        );
        assert_close(
            case.expected.thrust_n,
            result.per_motor.thrust_n,
            &case.name,
        );
        assert_close(
            case.expected.shaft_power_w,
            result.per_motor.shaft_power_w,
            &case.name,
        );
        assert_close(
            case.expected.battery_power_w,
            result.battery_power_w,
            &case.name,
        );
    }
}

#[test]
fn density_scales_the_table_load_without_reusing_static_thrust() {
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
fn apc_source_table_stays_bounded_against_independent_uiuc_static_characterization() {
    let benchmark: UiucStaticBenchmark =
        serde_json::from_str(UIUC_STATIC_BENCHMARK).expect("UIUC benchmark fixture parses");
    let table = apc_12x6e_performance_map().expect("APC source table parses");
    for sample in benchmark.samples {
        let predicted = table
            .coefficients_at(sample.rpm, 0.0)
            .unwrap_or_else(|| panic!("source table covers {} RPM static", sample.rpm));
        assert_relative_error(
            sample.thrust_coefficient,
            predicted.thrust_coefficient,
            benchmark.comparison.maximum_relative_ct_error,
            "UIUC static Ct",
        );
        assert_relative_error(
            sample.power_coefficient,
            predicted.power_coefficient,
            benchmark.comparison.maximum_relative_cp_error,
            "UIUC static Cp",
        );
    }
}

#[test]
fn zero_throttle_is_a_nonpropulsive_state_without_a_table_extrapolation() {
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
fn every_supplied_apc_table_is_addressable_without_curve_fitting() {
    let tables = apc_performance_maps().expect("the compact APC bundle parses");
    assert_eq!(tables.len(), 436);
    let catalog = full_catalog().expect("full catalogue parses");
    assert!(tables
        .iter()
        .all(|table| catalog.get(table.propeller_id()).is_some()));
    let table = apc_performance_map("apc-13x4-5ep-f2b")
        .expect("the F2B source file has a stable catalogue key");
    assert_eq!(table.source_file(), "PER3_13x45EP(F2B).dat");
    assert_eq!(table.model(), "13x4.5EP");
    assert!(table.rpm_bounds_at_speed(10.0).is_some());
    assert!(table.coefficients_at(5_000.0, 10.0).is_some());
}

#[test]
fn multi_motor_map_and_mission_sum_battery_demand_but_keep_motor_ratings_per_unit() {
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
}

#[test]
fn acc_screening_keeps_unverified_hardware_details_visible() {
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

fn assert_relative_error(expected: f64, actual: f64, maximum: f64, label: &str) {
    let relative_error = (expected - actual).abs() / expected.abs();
    assert!(
        relative_error <= maximum,
        "{label}: expected {expected:.6}, got {actual:.6}, relative error {relative_error:.3} exceeds {maximum:.3}"
    );
}
