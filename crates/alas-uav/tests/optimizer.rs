// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Failed unwraps in this test target report a broken optimizer contract; they
// cannot escape from library code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Mixed UAV optimizer physics, evidence, and determinism tests.

use alas_uav::catalog::{
    BatterySpec, ComponentKind, ComponentRecord, Dimensions, ElectronicsSpec, EscSpec,
    LandingGearSpec, MaterialStockSpec, MotorSpec, PropellerSpec, Provenance, ReceiverSpec,
    ServoOperatingPoint, ServoSpec,
};
use alas_uav::optimizer::{
    optimize, optimize_with_control, DesignObjectives, GeometrySearchBounds, MissionPhase,
    MissionProfile, OptimizationError, OptimizationProblem, PreliminaryModel, PropulsionMap,
    PropulsionOperatingPoint, SystemsDefinition, VariableBounds,
};
use alas_uav::{seed_catalog, Catalog, FindingKind};

#[test]
fn a_complete_mixed_search_returns_a_reproducible_verified_aircraft() {
    let catalog = complete_catalog();
    let maps = propulsion_maps(50.0);
    let problem = complete_problem(&catalog, &maps, 1_024);
    let first = optimize(&problem).expect("complete problem has a feasible design");
    let second = optimize(&problem).expect("fixed seed reproduces a feasible design");

    assert!(
        first.report.verified_feasible(),
        "{:#?}",
        first.report.findings
    );
    assert_eq!(first, second);
    assert_eq!(first.seed, 0x5eed_cafe);
    assert!(first.geometry.wing.span_m > 2.5);
    assert!(first.geometry.fuselage.length_m >= 1.8);
    assert!(first.geometry.empennage.horizontal_area_m2 > 0.0);
    assert!(first.geometry.landing_gear.minimum_leg_length_m > 0.2);
    assert!(first.metrics.mission_duration_s >= 900.0);
    assert!(first.metrics.mission_energy_wh > 0.0);
    assert!(first.metrics.propulsive_efficiency >= 0.15);
    assert_eq!(first.components.receiver_id, "receiver-reviewed");
    assert_eq!(first.components.electronics_id, "gps-reviewed");
    assert_eq!(first.components.landing_gear_id, "gear-reviewed");
    assert_eq!(first.design.servos.len(), 3);
    assert_eq!(first.design.other_items.len(), 4);
}

#[test]
fn a_multi_motor_multi_phase_problem_keeps_per_unit_ratings_and_system_totals_consistent() {
    let catalog = complete_catalog();
    let mut maps = propulsion_maps(50.0);
    maps[0].motor_count = 2;
    let profile = MissionProfile {
        evidence: "three reviewed electrical mission phases".to_owned(),
        phases: vec![
            mission_phase(
                "takeoff", 45.0, 10.0, 1.0, 100.0, 25.0, 500.0, 50.0, 1_000.0,
            ),
            mission_phase("climb", 135.0, 15.0, 0.75, 80.0, 20.0, 400.0, 40.0, 800.0),
            mission_phase("cruise", 720.0, 20.0, 0.55, 60.0, 15.0, 300.0, 30.0, 600.0),
        ],
    };
    let mut problem = complete_problem(&catalog, &maps, 256);
    problem.mission_profile = Some(&profile);
    let optimized = optimize(&problem).expect("complete twin-motor mission remains feasible");

    assert_eq!(optimized.components.motor_count, 2);
    assert_eq!(optimized.design.propulsor_count(), 2);
    assert_eq!(optimized.design.additional_propulsors.len(), 1);
    assert_eq!(optimized.metrics.mission_duration_s, 900.0);
    assert!(optimized.metrics.mission_energy_wh > 0.0);
    assert!(optimized
        .report
        .flight_conditions
        .iter()
        .any(|condition| condition.name == "takeoff"));
    assert!(
        optimized.design.propulsion_demand.battery_current_a
            >= 2.0 * optimized.design.propulsion_demand.motor_current_a
    );
}

#[test]
fn a_source_resolved_mission_must_cover_endurance_range_and_cruise_speed() {
    let catalog = complete_catalog();
    let maps = propulsion_maps(50.0);
    let profile = MissionProfile {
        evidence: "reviewed but incomplete mission profile".to_owned(),
        phases: vec![mission_phase(
            "cruise", 900.0, 20.0, 0.6, 60.0, 15.0, 300.0, 30.0, 600.0,
        )],
    };
    let mut problem = complete_problem(&catalog, &maps, 1);
    problem.objectives.range_m = 20_000.0;
    problem.mission_profile = Some(&profile);
    assert!(matches!(
        optimize(&problem),
        Err(OptimizationError::InvalidProblem(message)) if message.contains("distance")
    ));

    let profile = MissionProfile {
        evidence: "reviewed non-cruise mission profile".to_owned(),
        phases: vec![mission_phase(
            "loiter", 900.0, 15.0, 0.6, 60.0, 15.0, 300.0, 30.0, 600.0,
        )],
    };
    let mut problem = complete_problem(&catalog, &maps, 1);
    problem.mission_profile = Some(&profile);
    assert!(matches!(
        optimize(&problem),
        Err(OptimizationError::InvalidProblem(message)) if message.contains("design cruise speed")
    ));
}

#[test]
fn controlled_search_preserves_seeded_result_and_reports_monotonic_progress() {
    let catalog = complete_catalog();
    let maps = propulsion_maps(50.0);
    let problem = complete_problem(&catalog, &maps, 128);
    let expected = optimize(&problem);
    let mut progress = Vec::new();
    let controlled = optimize_with_control(&problem, |value| progress.push(value), || false);

    assert_eq!(controlled, expected);
    assert_eq!(progress.len(), problem.evaluations);
    assert_eq!(progress.last().unwrap().evaluated_candidates, 128);
    assert!(progress.windows(2).all(|window| {
        window[1].evaluated_candidates == window[0].evaluated_candidates + 1
            && window[1].verified_candidates >= window[0].verified_candidates
    }));
}

#[test]
fn cancellation_stops_between_candidates_and_reports_completed_work() {
    let catalog = complete_catalog();
    let maps = propulsion_maps(50.0);
    let problem = complete_problem(&catalog, &maps, 128);
    let mut cancellation_checks = 0;
    let error = optimize_with_control(
        &problem,
        |_| {},
        || {
            cancellation_checks += 1;
            cancellation_checks > 17
        },
    )
    .expect_err("the cooperative stop is observed before candidate 18");

    assert_eq!(
        error,
        OptimizationError::Cancelled {
            evaluated_candidates: 17
        }
    );
}

#[test]
fn the_retail_seed_cannot_pass_when_required_evidence_is_absent() {
    let catalog = seed_catalog().expect("embedded retail seed parses");
    let maps = Vec::new();
    let problem = complete_problem(catalog, &maps, 16);
    let OptimizationError::NoFeasibleDesign(summary) =
        optimize(&problem).expect_err("missing retail evidence must not be inferred")
    else {
        panic!("expected a typed no-feasible-design result");
    };
    assert_eq!(summary.evaluated_candidates, 16);
    assert!(summary
        .rejections
        .iter()
        .any(|row| row.kind == FindingKind::MissingData && row.candidates == 16));
    assert!(summary
        .examples
        .iter()
        .any(|example| example.kind == FindingKind::MissingData));
    assert!(summary.best_evaluated.is_none());
}

#[test]
fn every_required_failure_family_can_close_the_search() {
    assert_rejected(
        |catalog, _, _, _| {
            let battery = battery_mut(catalog);
            battery.discharge_rating_c = Some(0.5);
        },
        FindingKind::BatteryCurrentOverload,
    );
    assert_rejected(
        |_, _, objectives, _| objectives.endurance_s = 20_000.0,
        FindingKind::EnergyShortfall,
    );
    assert_rejected(
        |_, _, objectives, _| objectives.maximum_stall_speed_m_s = 3.0,
        FindingKind::InsufficientLift,
    );
    assert_rejected(
        |_, maps, _, _| {
            for point in &mut maps[0].points {
                point.thrust_n = 0.5;
            }
        },
        FindingKind::InsufficientThrust,
    );
    assert_rejected(
        |_, _, _, bounds| {
            bounds.wing_leading_edge_fraction = range(0.65, 0.70);
        },
        FindingKind::CenterOfGravityViolation,
    );
    assert_rejected(
        |_, _, _, bounds| bounds.fuselage_length_m = range(0.65, 0.70),
        FindingKind::PackagingViolation,
    );
    assert_rejected_with_model(
        |model| model.hinge_moment_coefficient = 10.0,
        FindingKind::ServoTorqueOverload,
    );
    assert_rejected(
        |catalog, _, _, _| material_mut(catalog).allowable_stress_pa = Some(1.0e6),
        FindingKind::StructuralOverload,
    );
}

#[test]
fn malformed_or_out_of_range_propulsion_maps_are_rejected_without_extrapolation() {
    let catalog = complete_catalog();
    let mut maps = propulsion_maps(50.0);
    maps[0].points[1].speed_m_s = maps[0].points[0].speed_m_s;
    let problem = complete_problem(&catalog, &maps, 1);
    assert!(matches!(
        optimize(&problem),
        Err(OptimizationError::InvalidProblem(message)) if message.contains("unordered")
    ));

    let maps = vec![PropulsionMap {
        motor_id: "motor-reviewed".to_owned(),
        propeller_id: "propeller-reviewed".to_owned(),
        series_cells: 6,
        motor_count: 1,
        evidence: "dynamometer report outside objective speed".to_owned(),
        points: vec![
            point(12.0, 20.0, 20.0, 400.0),
            point(18.0, 15.0, 30.0, 650.0),
        ],
    }];
    let problem = complete_problem(&catalog, &maps, 8);
    let OptimizationError::NoFeasibleDesign(summary) = optimize(&problem).unwrap_err() else {
        panic!("map extrapolation must not be accepted");
    };
    assert!(summary
        .rejections
        .iter()
        .any(|row| row.kind == FindingKind::MissingData));
    assert!(summary
        .examples
        .iter()
        .any(|example| example.kind == FindingKind::MissingData));
}

fn assert_rejected(
    mutate: impl FnOnce(
        &mut Catalog,
        &mut Vec<PropulsionMap>,
        &mut DesignObjectives,
        &mut GeometrySearchBounds,
    ),
    expected: FindingKind,
) {
    let mut catalog = complete_catalog();
    let mut maps = propulsion_maps(50.0);
    let base = complete_problem(&catalog, &maps, 64);
    let mut objectives = base.objectives;
    let mut bounds = base.geometry_bounds;
    mutate(&mut catalog, &mut maps, &mut objectives, &mut bounds);
    catalog
        .validate()
        .expect("mutated test catalogue remains valid");
    let mut problem = complete_problem(&catalog, &maps, 64);
    problem.objectives = objectives;
    problem.geometry_bounds = bounds;
    let OptimizationError::NoFeasibleDesign(summary) = optimize(&problem).unwrap_err() else {
        panic!("expected the hard constraint to close the search");
    };
    assert!(
        summary.rejections.iter().any(|row| row.kind == expected),
        "missing {expected:?} in {:#?}",
        summary.rejections
    );
}

fn assert_rejected_with_model(mutate: impl FnOnce(&mut PreliminaryModel), expected: FindingKind) {
    let catalog = complete_catalog();
    let maps = propulsion_maps(50.0);
    let mut problem = complete_problem(&catalog, &maps, 64);
    mutate(&mut problem.model);
    let OptimizationError::NoFeasibleDesign(summary) = optimize(&problem).unwrap_err() else {
        panic!("expected model mutation to close the search");
    };
    assert!(summary.rejections.iter().any(|row| row.kind == expected));
    assert!(summary.best_evaluated.is_some());
}

fn complete_problem<'a>(
    catalog: &'a Catalog,
    maps: &'a [PropulsionMap],
    evaluations: usize,
) -> OptimizationProblem<'a> {
    OptimizationProblem {
        catalog,
        propulsion_maps: maps,
        mission_profile: None,
        objectives: DesignObjectives {
            endurance_s: 900.0,
            range_m: 10_000.0,
            cruise_speed_m_s: 20.0,
            maximum_stall_speed_m_s: 10.0,
            payload_mass_kg: 0.5,
            payload_dimensions: dimensions(0.25, 0.12, 0.10),
            minimum_propulsive_efficiency: 0.15,
            efficiency_priority: 0.5,
        },
        geometry_bounds: GeometrySearchBounds {
            wing_area_m2: range(0.9, 1.2),
            wing_aspect_ratio: range(8.0, 10.0),
            fuselage_length_m: range(1.8, 2.0),
            wing_leading_edge_fraction: range(0.25, 0.45),
        },
        model: PreliminaryModel {
            air_density_kg_m3: 1.225,
            maximum_lift_coefficient: 1.6,
            zero_lift_drag_coefficient: 0.035,
            oswald_efficiency: 0.8,
            limit_load_factor: 3.0,
            structural_safety_factor: 1.5,
            horizontal_tail_volume_coefficient: 0.5,
            vertical_tail_volume_coefficient: 0.04,
            horizontal_tail_aspect_ratio: 4.0,
            vertical_tail_aspect_ratio: 1.6,
            forward_cg_chord_fraction: 0.15,
            aft_cg_chord_fraction: 0.35,
            equipment_clearance_m: 0.015,
            equipment_gap_m: 0.015,
            nose_length_fraction: 0.10,
            tailcone_length_fraction: 0.30,
            spar_cap_width_fraction: 0.06,
            spar_cap_separation_fraction: 0.14,
            aileron_area_fraction: 0.08,
            aileron_chord_fraction: 0.25,
            elevator_area_fraction: 0.30,
            elevator_chord_fraction: 0.30,
            hinge_moment_coefficient: 0.01,
            landing_gear_track_fraction: 0.18,
            landing_gear_wheelbase_fraction: 0.35,
            propeller_ground_clearance_m: 0.05,
            fixed_systems_mass_kg: 0.15,
        },
        systems: SystemsDefinition {
            avionics_mass_kg: 0.10,
            avionics_dimensions: dimensions(0.08, 0.06, 0.03),
            avionics_power_w: 25.0,
            control_bus_current_a: 0.8,
            control_bus_voltage_v: 6.0,
            minimum_receiver_channels: 6,
            servo_continuous_current_fraction: 0.2,
            maximum_depth_of_discharge: 0.8,
            reserve_fraction: 0.2,
        },
        required_electronics_role: "gps_sensor",
        seed: 0x5eed_cafe,
        evaluations,
    }
}

fn complete_catalog() -> Catalog {
    let records = vec![
        record(
            "battery-reviewed",
            ComponentKind::Battery(BatterySpec {
                chemistry: "LiPo".to_owned(),
                series_cells: Some(6),
                nominal_voltage_v: Some(22.2),
                capacity_ah: Some(20.0),
                discharge_rating_c: Some(30.0),
                mass_kg: Some(1.5),
                dimensions: Some(dimensions(0.20, 0.08, 0.06)),
                connector: Some("AS150".to_owned()),
            }),
        ),
        record(
            "motor-reviewed",
            ComponentKind::Motor(MotorSpec {
                kv_rpm_per_v: Some(500.0),
                winding_resistance_ohm: Some(0.05),
                no_load_current_a: Some(1.0),
                no_load_test_voltage_v: Some(10.0),
                min_series_cells: Some(6),
                max_series_cells: Some(6),
                max_current_a: Some(80.0),
                max_power_w: Some(1_500.0),
                max_static_thrust_n: Some(120.0),
                mass_kg: Some(0.30),
                dimensions: Some(dimensions(0.06, 0.05, 0.05)),
                recommended_propeller_diameter_m: Some(14.0 * 0.0254),
                recommended_propeller_pitch_m: Some(7.0 * 0.0254),
            }),
        ),
        record(
            "esc-reviewed",
            ComponentKind::Esc(EscSpec {
                min_series_cells: Some(6),
                max_series_cells: Some(6),
                continuous_current_a: Some(100.0),
                bec_min_voltage_v: Some(5.0),
                bec_max_voltage_v: Some(8.4),
                bec_continuous_current_a: Some(10.0),
                bec_peak_current_a: Some(20.0),
                mass_kg: Some(0.12),
                dimensions: Some(dimensions(0.08, 0.04, 0.025)),
            }),
        ),
        record(
            "propeller-reviewed",
            ComponentKind::Propeller(PropellerSpec {
                diameter_m: Some(14.0 * 0.0254),
                pitch_m: Some(7.0 * 0.0254),
                blade_count: Some(2),
                mass_kg: Some(0.05),
                bore_diameter_m: Some(0.008),
                electric_compatible: Some(true),
            }),
        ),
        record(
            "servo-reviewed",
            ComponentKind::Servo(ServoSpec {
                mass_kg: Some(0.04),
                dimensions: Some(dimensions(0.04, 0.02, 0.035)),
                operating_points: vec![ServoOperatingPoint {
                    voltage_v: 6.0,
                    stall_torque_nm: 2.0,
                    seconds_per_60_deg: Some(0.12),
                    stall_current_a: Some(1.5),
                }],
            }),
        ),
        record(
            "material-reviewed",
            ComponentKind::MaterialStock(MaterialStockSpec {
                form: "sheet".to_owned(),
                dimensions: Some(dimensions(1.0, 1.0, 0.0004)),
                mass_kg: Some(0.64),
                youngs_modulus_pa: Some(70.0e9),
                allowable_stress_pa: Some(400.0e6),
            }),
        ),
        record(
            "receiver-reviewed",
            ComponentKind::Receiver(ReceiverSpec {
                channel_count: Some(8),
                min_voltage_v: Some(4.5),
                max_voltage_v: Some(8.4),
                mass_kg: Some(0.02),
                dimensions: Some(dimensions(0.04, 0.025, 0.012)),
                protocols: vec!["SBUS".to_owned()],
                telemetry: Some(true),
            }),
        ),
        record(
            "gps-reviewed",
            ComponentKind::Electronics(ElectronicsSpec {
                role: "gps_sensor".to_owned(),
                min_voltage_v: Some(5.0),
                max_voltage_v: Some(8.4),
                max_current_a: Some(0.5),
                mass_kg: Some(0.03),
                dimensions: Some(dimensions(0.035, 0.035, 0.012)),
                protocols: vec!["UART".to_owned()],
            }),
        ),
        record(
            "gear-reviewed",
            ComponentKind::LandingGear(LandingGearSpec {
                form: "fixed_tricycle".to_owned(),
                mass_kg: Some(0.25),
                max_aircraft_mass_kg: Some(20.0),
                dimensions: Some(dimensions(0.35, 0.30, 0.25)),
                nominal_voltage_v: None,
            }),
        ),
    ];
    let catalog = Catalog {
        schema_version: 1,
        records,
    };
    catalog
        .validate()
        .expect("complete test catalogue validates");
    catalog
}

fn record(id: &str, kind: ComponentKind) -> ComponentRecord {
    ComponentRecord {
        id: id.to_owned(),
        manufacturer: "Reviewed Test Hardware".to_owned(),
        model: id.to_owned(),
        kind,
        provenance: Provenance {
            publisher: "Independent dynamometer and data-sheet review".to_owned(),
            source_url: format!("https://example.invalid/{id}"),
            source_title: format!("Reviewed evidence for {id}"),
            transformations: Vec::new(),
        },
    }
}

fn propulsion_maps(thrust_n: f64) -> Vec<PropulsionMap> {
    vec![PropulsionMap {
        motor_id: "motor-reviewed".to_owned(),
        propeller_id: "propeller-reviewed".to_owned(),
        series_cells: 6,
        motor_count: 1,
        evidence: "independent in-flight dynamometer map R1".to_owned(),
        points: vec![
            point(1.0, thrust_n, 25.0, 500.0),
            point(25.0, 0.5 * thrust_n, 35.0, 750.0),
        ],
    }]
}

fn point(speed: f64, thrust: f64, current: f64, power: f64) -> PropulsionOperatingPoint {
    PropulsionOperatingPoint {
        speed_m_s: speed,
        thrust_n: thrust,
        motor_current_a: current,
        motor_power_w: power,
    }
}

// Keep every independently asserted mission quantity visible at each fixture
// call site instead of hiding test evidence in a positional helper struct.
#[allow(clippy::too_many_arguments)]
fn mission_phase(
    name: &str,
    duration_s: f64,
    speed_m_s: f64,
    throttle: f64,
    total_thrust_n: f64,
    motor_current_a: f64,
    motor_power_w: f64,
    battery_current_a: f64,
    battery_power_w: f64,
) -> MissionPhase {
    MissionPhase {
        name: name.to_owned(),
        duration_s,
        speed_m_s,
        air_density_kg_m3: 1.225,
        throttle,
        total_thrust_n,
        motor_current_a,
        motor_power_w,
        battery_current_a,
        battery_power_w,
    }
}

fn dimensions(length: f64, width: f64, height: f64) -> Dimensions {
    Dimensions {
        length_m: length,
        width_m: width,
        height_m: height,
    }
}

fn range(minimum: f64, maximum: f64) -> VariableBounds {
    VariableBounds { minimum, maximum }
}

fn battery_mut(catalog: &mut Catalog) -> &mut BatterySpec {
    match &mut catalog.records[0].kind {
        ComponentKind::Battery(spec) => spec,
        _ => panic!("first record must remain the battery"),
    }
}

fn material_mut(catalog: &mut Catalog) -> &mut MaterialStockSpec {
    match &mut catalog.records[5].kind {
        ComponentKind::MaterialStock(spec) => spec,
        _ => panic!("sixth record must remain the material"),
    }
}
