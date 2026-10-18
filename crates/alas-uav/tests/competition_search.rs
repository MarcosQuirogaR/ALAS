// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Failed unwraps in this test target report a broken optimizer contract.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

//! Compatibility filter, determinism, and competition-score search tests.

use std::collections::BTreeMap;

use alas_uav::catalog::{
    BatterySpec, ComponentKind, ComponentRecord, Dimensions, ElectronicsSpec, EscSpec,
    LandingGearSpec, MaterialStockSpec, MotorSpec, PropellerSpec, Provenance, ReceiverSpec,
    ServoOperatingPoint, ServoSpec,
};
use alas_uav::optimizer::{
    check_bundle, optimize_competition, optimize_competition_observed, optimize_competition_random,
    optimize_competition_random_compatible, propulsion_bundles, BundleRule, CompetitionProblem,
    DesignObjectives, GeometrySearchBounds, OptimizationError, OptimizationProblem,
    PreliminaryModel, PropulsionMap, PropulsionOperatingPoint, SearchMethod, SystemsDefinition,
    TailVolumeBounds, VariableBounds,
};
use alas_uav::Catalog;

const SEARCH_BUDGET: usize = 240;

fn point(speed: f64, thrust: f64, current: f64, power: f64) -> PropulsionOperatingPoint {
    PropulsionOperatingPoint {
        speed_m_s: speed,
        thrust_n: thrust,
        motor_current_a: current,
        motor_power_w: power,
    }
}

/// Map with static thrust `t0` per propulsor, linearly falling to zero at
/// 45 m/s. Speed 0 is included so that the take-off roll is covered.
fn map(motor: &str, propeller: &str, cells: u16, count: u16, t0: f64) -> PropulsionMap {
    let points = [0.0, 10.0, 20.0, 30.0, 40.0]
        .iter()
        .map(|&v| point(v, t0 * (1.0 - v / 45.0), 20.0 + 0.5 * v, 300.0 + 20.0 * v))
        .collect();
    PropulsionMap {
        motor_id: motor.to_owned(),
        propeller_id: propeller.to_owned(),
        series_cells: cells,
        motor_count: count,
        evidence: format!("test map {motor}/{propeller}"),
        points,
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

fn motor(max_current: Option<f64>, cells: (u16, u16), prop_in: f64, kv: f64) -> MotorSpec {
    MotorSpec {
        kv_rpm_per_v: Some(kv),
        winding_resistance_ohm: Some(0.05),
        no_load_current_a: Some(1.0),
        no_load_test_voltage_v: Some(10.0),
        min_series_cells: Some(cells.0),
        max_series_cells: Some(cells.1),
        max_current_a: max_current,
        max_power_w: Some(1_500.0),
        max_static_thrust_n: Some(120.0),
        mass_kg: Some(0.30),
        dimensions: Some(dimensions(0.06, 0.05, 0.05)),
        recommended_propeller_diameter_m: Some(prop_in * 0.0254),
        recommended_propeller_pitch_m: Some(7.0 * 0.0254),
    }
}

fn esc(continuous: f64, cells: (u16, u16)) -> EscSpec {
    EscSpec {
        min_series_cells: Some(cells.0),
        max_series_cells: Some(cells.1),
        continuous_current_a: Some(continuous),
        bec_min_voltage_v: Some(5.0),
        bec_max_voltage_v: Some(8.4),
        bec_continuous_current_a: Some(10.0),
        bec_peak_current_a: Some(20.0),
        mass_kg: Some(0.12),
        dimensions: Some(dimensions(0.08, 0.04, 0.025)),
    }
}

fn battery(cells: u16, capacity_ah: f64, c_rating: f64) -> BatterySpec {
    BatterySpec {
        chemistry: "LiPo".to_owned(),
        series_cells: Some(cells),
        nominal_voltage_v: Some(3.7 * f64::from(cells)),
        capacity_ah: Some(capacity_ah),
        discharge_rating_c: Some(c_rating),
        mass_kg: Some(1.5 * capacity_ah / 20.0),
        dimensions: Some(dimensions(0.20, 0.08, 0.06)),
        connector: Some("AS150".to_owned()),
    }
}

fn propeller(diameter_in: f64) -> PropellerSpec {
    PropellerSpec {
        diameter_m: Some(diameter_in * 0.0254),
        pitch_m: Some(7.0 * 0.0254),
        blade_count: Some(2),
        mass_kg: Some(0.05),
        bore_diameter_m: Some(0.008),
        electric_compatible: Some(true),
    }
}

/// Complete single-family catalogue used by the search tests: one propulsion
/// bundle is fully compatible, and a second motor, a second pack, and a weak
/// ESC create compatible and incompatible alternatives.
fn search_catalog() -> Catalog {
    let records = vec![
        record(
            "battery-reviewed",
            ComponentKind::Battery(battery(6, 20.0, 30.0)),
        ),
        record(
            "battery-small",
            ComponentKind::Battery(battery(6, 10.0, 20.0)),
        ),
        record(
            "motor-reviewed",
            ComponentKind::Motor(motor(Some(80.0), (6, 6), 14.0, 500.0)),
        ),
        record(
            "motor-b",
            ComponentKind::Motor(motor(Some(60.0), (6, 6), 14.0, 550.0)),
        ),
        record("esc-reviewed", ComponentKind::Esc(esc(100.0, (6, 6)))),
        record("esc-weak", ComponentKind::Esc(esc(30.0, (6, 6)))),
        record(
            "propeller-reviewed",
            ComponentKind::Propeller(propeller(14.0)),
        ),
        // The competition rules allow at most 3S packs; this set is the 3S alternative.
        record("battery-3s", ComponentKind::Battery(battery(3, 9.0, 30.0))),
        record(
            "motor-3s",
            ComponentKind::Motor(motor(Some(60.0), (3, 4), 12.0, 900.0)),
        ),
        record("esc-3s", ComponentKind::Esc(esc(60.0, (3, 4)))),
        record("propeller-12", ComponentKind::Propeller(propeller(12.0))),
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
    catalog.validate().expect("test catalogue validates");
    catalog
}

/// Maps: the reviewed bundle, the second motor on the same pack, and a
/// 24 in propeller that is too large for the 6S pack's speed limit.
fn search_maps() -> Vec<PropulsionMap> {
    vec![
        map("motor-reviewed", "propeller-reviewed", 6, 1, 25.0),
        map("motor-b", "propeller-reviewed", 6, 1, 22.0),
        map("motor-3s", "propeller-12", 3, 1, 30.0),
    ]
}

fn objectives() -> DesignObjectives {
    DesignObjectives {
        endurance_s: 300.0,
        range_m: 3_000.0,
        cruise_speed_m_s: 20.0,
        maximum_stall_speed_m_s: 10.0,
        payload_mass_kg: 0.5,
        payload_dimensions: dimensions(0.25, 0.12, 0.10),
        minimum_propulsive_efficiency: 0.15,
        efficiency_priority: 0.5,
    }
}

fn model() -> PreliminaryModel {
    PreliminaryModel {
        air_density_kg_m3: 1.225,
        maximum_lift_coefficient: 1.8,
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
    }
}

fn systems() -> SystemsDefinition {
    SystemsDefinition {
        avionics_mass_kg: 0.10,
        avionics_dimensions: dimensions(0.08, 0.06, 0.03),
        avionics_power_w: 25.0,
        control_bus_current_a: 0.8,
        control_bus_voltage_v: 6.0,
        minimum_receiver_channels: 6,
        servo_continuous_current_fraction: 0.2,
        maximum_depth_of_discharge: 0.8,
        reserve_fraction: 0.2,
    }
}

fn base_problem<'a>(
    catalog: &'a Catalog,
    maps: &'a [PropulsionMap],
    evaluations: usize,
    seed: u64,
) -> alas_uav::optimizer::OptimizationProblem<'a> {
    OptimizationProblem {
        catalog,
        propulsion_maps: maps,
        mission_profile: None,
        objectives: objectives(),
        geometry_bounds: GeometrySearchBounds {
            wing_area_m2: range(1.2, 2.0),
            wing_aspect_ratio: range(8.0, 12.0),
            fuselage_length_m: range(1.8, 2.0),
            wing_leading_edge_fraction: range(0.25, 0.45),
        },
        model: model(),
        systems: systems(),
        required_electronics_role: "gps_sensor",
        seed,
        evaluations,
    }
}

/// Rule quantities that the simulation cannot know. The payload count is a
/// test value (two cans), not a competition figure.
fn preset_inputs(preset_id: &str) -> BTreeMap<String, f64> {
    let mut inputs = BTreeMap::new();
    if preset_id == "acc-2026" {
        inputs.insert("payload_count".to_owned(), 2.0);
        inputs.insert("declared_payload_count".to_owned(), 2.0);
    }
    if preset_id == "xc-2025-clubs" {
        // Payload bounding box of the test objectives, m^3.
        inputs.insert("payload_volume".to_owned(), 0.25 * 0.12 * 0.10);
    }
    inputs
}

fn competition_problem<'a>(
    catalog: &'a Catalog,
    maps: &'a [PropulsionMap],
    preset_id: &'a str,
    evaluations: usize,
    seed: u64,
) -> CompetitionProblem<'a> {
    CompetitionProblem {
        base: base_problem(catalog, maps, evaluations, seed),
        preset_id,
        preset_variant: None,
        announced_takeoff_m: 60.0,
        extra_inputs: preset_inputs(preset_id),
        tail_volume_bounds: TailVolumeBounds::default(),
        population: None,
        include_landing: true,
    }
}

fn ids(catalog: &Catalog, maps: &[PropulsionMap]) -> Vec<(String, String, String, String)> {
    propulsion_bundles(catalog, maps)
        .compatible
        .into_iter()
        .map(|b| (b.motor_id, b.esc_id, b.battery_id, b.propeller_id))
        .collect()
}

fn find<'a>(catalog: &'a Catalog, id: &str) -> &'a ComponentRecord {
    catalog.records.iter().find(|r| r.id == id).unwrap()
}

fn spec_of<'a, T>(
    catalog: &'a Catalog,
    id: &str,
    pick: impl Fn(&'a ComponentKind) -> Option<&'a T>,
) -> &'a T {
    pick(&find(catalog, id).kind).unwrap()
}

#[test]
fn a_fully_specified_bundle_passes_the_compatibility_rules() {
    let catalog = search_catalog();
    let maps = search_maps();
    let motor = spec_of(&catalog, "motor-reviewed", |k| match k {
        ComponentKind::Motor(s) => Some(s),
        _ => None,
    });
    let esc = spec_of(&catalog, "esc-reviewed", |k| match k {
        ComponentKind::Esc(s) => Some(s),
        _ => None,
    });
    let battery = spec_of(&catalog, "battery-reviewed", |k| match k {
        ComponentKind::Battery(s) => Some(s),
        _ => None,
    });
    let prop = spec_of(&catalog, "propeller-reviewed", |k| match k {
        ComponentKind::Propeller(s) => Some(s),
        _ => None,
    });
    let issues = check_bundle(motor, esc, battery, prop, &maps[0]);
    assert!(issues.is_empty(), "{issues:#?}");
}

#[test]
fn each_physical_rule_rejects_its_own_bundle() {
    let catalog = search_catalog();
    let maps = search_maps();
    let rules = |motor_id: &str, esc_id: &str, battery_id: &str, prop_id: &str| {
        propulsion_bundles(&catalog, &maps)
            .rejected
            .into_iter()
            .find(|r| {
                r.ids.motor_id == motor_id
                    && r.ids.esc_id == esc_id
                    && r.ids.battery_id == battery_id
                    && r.ids.propeller_id == prop_id
            })
            .map(|r| r.issues.iter().map(|i| i.rule).collect::<Vec<_>>())
            .unwrap_or_default()
    };
    // ESC continuous 30 A < motor maximum 80 A.
    assert!(rules(
        "motor-reviewed",
        "esc-weak",
        "battery-reviewed",
        "propeller-reviewed"
    )
    .contains(&BundleRule::EscCurrent));
    // Pack rated 10 Ah x 20 C = 200 A is enough; a 2 Ah x 5 C pack is not.
    let small_pack = catalog_with_battery(&catalog, battery(6, 2.0, 5.0));
    let issues = propulsion_bundles(&small_pack, &maps);
    assert!(issues
        .rejected
        .iter()
        .any(|r| r.ids.battery_id == "battery-small"
            && r.issues
                .iter()
                .any(|i| i.rule == BundleRule::BatteryCurrent)));
}

fn catalog_with_battery(base: &Catalog, spec: BatterySpec) -> Catalog {
    let mut catalog = Catalog {
        schema_version: base.schema_version,
        records: base.records.clone(),
    };
    for record in &mut catalog.records {
        if record.id == "battery-small" {
            record.kind = ComponentKind::Battery(spec.clone());
        }
    }
    catalog
}

#[test]
fn voltage_tip_speed_size_and_missing_evidence_reject_bundles() {
    let mut catalog = search_catalog();
    // A 24 in propeller on the 6S motor: far above the recommended window and
    // above the tip-speed limit at 6S.
    let big = record("propeller-24", ComponentKind::Propeller(propeller(24.0)));
    catalog.records.push(big);
    let mut maps = search_maps();
    maps.push(map("motor-reviewed", "propeller-24", 6, 1, 40.0));
    let issues_for = |motor: &str, prop: &str| {
        propulsion_bundles(&catalog, &maps)
            .rejected
            .into_iter()
            .filter(|r| r.ids.motor_id == motor && r.ids.propeller_id == prop)
            .flat_map(|r| r.issues.into_iter().map(|i| i.rule))
            .collect::<Vec<_>>()
    };
    let size = issues_for("motor-reviewed", "propeller-24");
    assert!(size.contains(&BundleRule::PropellerSize), "{size:?}");
    assert!(size.contains(&BundleRule::TipSpeed), "{size:?}");

    // A 4S-only ESC cannot run the 6S pack of the map.
    let mut catalog = search_catalog();
    catalog
        .records
        .push(record("esc-4s", ComponentKind::Esc(esc(100.0, (4, 4)))));
    let maps = search_maps();
    let rejected = propulsion_bundles(&catalog, &maps);
    assert!(rejected.rejected.iter().any(|r| {
        r.ids.esc_id == "esc-4s" && r.issues.iter().any(|i| i.rule == BundleRule::Voltage)
    }));

    // A motor without a published maximum current cannot be checked, so it is
    // rejected rather than assumed compatible.
    let mut catalog = search_catalog();
    let blind = motor(None, (6, 6), 14.0, 500.0);
    catalog
        .records
        .push(record("motor-blind", ComponentKind::Motor(blind)));
    let mut maps = search_maps();
    maps.push(map("motor-blind", "propeller-reviewed", 6, 1, 25.0));
    let rejected = propulsion_bundles(&catalog, &maps);
    assert!(rejected.rejected.iter().any(|r| {
        r.ids.motor_id == "motor-blind"
            && r.issues
                .iter()
                .any(|i| i.rule == BundleRule::MissingEvidence)
    }));
    assert!(!rejected
        .compatible
        .iter()
        .any(|b| b.motor_id == "motor-blind"));

    // A bundle without any thrust map is missing evidence.
    let catalog = search_catalog();
    let maps = vec![map("motor-reviewed", "propeller-reviewed", 6, 1, 25.0)];
    let rejected = propulsion_bundles(&catalog, &maps);
    assert!(rejected
        .rejected
        .iter()
        .any(|r| r.ids.motor_id == "motor-b" && r.issues[0].rule == BundleRule::MissingEvidence));
}

#[test]
fn every_compatible_bundle_is_drawn_from_the_filtered_list() {
    let catalog = search_catalog();
    let maps = search_maps();
    let compatible = ids(&catalog, &maps);
    // Four 6S bundles (two motors, two packs, reviewed ESC) and one 3S bundle.
    assert_eq!(compatible.len(), 5, "{compatible:?}");
    assert!(compatible.iter().all(|(_, esc, _, _)| esc != "esc-weak"));
    assert!(compatible
        .iter()
        .any(|(motor, _, _, _)| motor == "motor-3s"));

    let problem = competition_problem(&catalog, &maps, "acc-2026", 40, 0x51ed);
    let result = optimize_competition(&problem).expect("search runs");
    assert_eq!(result.evaluations, 40);
    assert_eq!(result.compatible_bundles, 5);
    assert!(result.rejected_bundles > 0);
    if let Some(best) = &result.best {
        let key = (
            best.components.motor_id.clone(),
            best.components.esc_id.clone(),
            best.components.battery_id.clone(),
            best.components.propeller_id.clone(),
        );
        assert!(compatible.contains(&key), "{key:?}");
    }
}

#[test]
fn the_same_seed_reproduces_the_best_candidate_and_counts() {
    let catalog = search_catalog();
    let maps = search_maps();
    let problem = competition_problem(&catalog, &maps, "acc-2026", 60, 0xabc);
    let first = optimize_competition(&problem).expect("search runs");
    let second = optimize_competition(&problem).expect("search runs");
    assert_eq!(first.evaluations, second.evaluations);
    assert_eq!(first.feasible_evaluations, second.feasible_evaluations);
    match (&first.best, &second.best) {
        (Some(a), Some(b)) => {
            assert_eq!(a.variables, b.variables);
            assert_eq!(a.evaluation, b.evaluation);
            assert_eq!(a.components, b.components);
        }
        (None, None) => {}
        _ => panic!("the same seed produced different feasibility outcomes"),
    }
    assert_eq!(first.method, SearchMethod::DifferentialEvolution);
}

/// Search on the two presets at a small budget. The baseline column is the
/// previous optimizer (catalogue-index draws); the ablation column is uniform
/// draws over the compatible space.
#[test]

fn competition_search_beats_the_random_baseline_on_two_presets() {
    let catalog = search_catalog();
    let maps = search_maps();
    for preset in ["acc-2026", "xc-2025-clubs"] {
        let problem = competition_problem(&catalog, &maps, preset, SEARCH_BUDGET, 0x5eed);
        let search = optimize_competition(&problem).expect("search runs");
        let random = optimize_competition_random(&problem).expect("baseline runs");
        let compatible = optimize_competition_random_compatible(&problem).expect("ablation runs");
        assert_eq!(random.method, SearchMethod::RandomCatalogueIndices);
        let score = |result: &alas_uav::optimizer::CompetitionOptimum| {
            result
                .best
                .as_ref()
                .filter(|b| b.evaluation.feasible)
                .and_then(|b| b.evaluation.score)
        };
        println!(
            "PRESET|{preset}|DE feasible {} score {:?}|catalogue-random feasible {} score {:?}|compatible-random feasible {} score {:?}|DE {:.0} eval/s|bundles {}/{}",
            search.feasible_evaluations,
            score(&search),
            random.feasible_evaluations,
            score(&random),
            compatible.feasible_evaluations,
            score(&compatible),
            search.evaluations_per_s,
            search.compatible_bundles,
            search.rejected_bundles,
        );
        let search_score = score(&search).expect("the search finds a feasible design");
        let best = search.best.as_ref().expect("a best candidate exists");
        // Cell count, pack energy and (from M4) the static margin are evaluated.
        assert!(!best
            .evaluation
            .unevaluated_gates
            .iter()
            .any(|g| g == "cells" || g == "pack_energy" || g == "static_margin"));
        if let Some(random_score) = score(&random) {
            assert!(
                search_score > random_score,
                "{preset}: {search_score} <= {random_score}"
            );
        }
    }
}

#[test]
fn observed_search_reports_progress_and_stops_on_cancellation() {
    let catalog = search_catalog();
    let maps = search_maps();
    let problem = competition_problem(&catalog, &maps, "acc-2026", 40, 0x51ed);

    // A full run reports every evaluation, in order, ending at the budget.
    let mut seen = Vec::new();
    let full = optimize_competition_observed(&problem, &mut |progress| {
        seen.push(progress.evaluated);
        true
    })
    .expect("observed search runs");
    assert_eq!(full.evaluations, 40);
    assert_eq!(seen, (1..=40).collect::<Vec<_>>());

    // Returning false from the observer cancels at the next boundary.
    let mut calls = 0;
    let cancelled = optimize_competition_observed(&problem, &mut |_| {
        calls += 1;
        calls < 6
    });
    assert!(matches!(
        cancelled,
        Err(OptimizationError::Cancelled {
            evaluated_candidates: 6
        })
    ));
}
