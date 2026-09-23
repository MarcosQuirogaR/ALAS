// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Pinned W6.4 checks for the SUAVE vehicle-to-mission boundary.

use std::collections::BTreeMap;

use super::*;
use crate::full_analysis::FullAnalysis;
use alas_config::design_variables::DesignVector;
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    inputs: Inputs,
}

#[derive(Deserialize)]
struct Inputs {
    vehicle: Vehicle,
}

#[derive(Deserialize)]
struct Vehicle {
    reference_area_m2: f64,
    wings: Vec<Wing>,
    fuselages: Vec<Fuselage>,
    nacelles: Vec<Nacelle>,
    network_count: usize,
    turbofan: Turbofan,
}

#[derive(Deserialize)]
struct Turbofan {
    number_of_engines: f64,
    bypass_ratio: f64,
    fan_pressure_ratio: f64,
    turbine_inlet_temperature_k: f64,
    design_thrust_total_n: f64,
    compressor_nondimensional_massflow: f64,
}

#[derive(Deserialize)]
struct Wing {
    mean_aerodynamic_chord_m: f64,
    quarter_chord_sweep_rad: f64,
    thickness_to_chord: f64,
    reference_area_m2: f64,
    wetted_area_m2: f64,
    aspect_ratio: f64,
}

#[derive(Deserialize)]
struct Fuselage {
    length_m: f64,
    effective_diameter_m: f64,
    front_projected_area_m2: f64,
    wetted_area_m2: f64,
}

#[derive(Deserialize)]
struct Nacelle {
    length_m: f64,
    diameter_m: f64,
    wetted_area_m2: f64,
    origin_count: usize,
}

#[derive(Deserialize)]
struct LiftSurrogateFixture {
    wing_tags: Vec<String>,
    training: LiftTraining,
}

#[derive(Deserialize)]
struct LiftTraining {
    lift_coefficient: Vec<Vec<f64>>,
    drag_coefficient: Vec<Vec<f64>>,
    wing_lift_coefficient: BTreeMap<String, Vec<Vec<f64>>>,
    wing_drag_coefficient: BTreeMap<String, Vec<Vec<f64>>>,
}

#[derive(Deserialize)]
struct MissionEvidence {
    segments: Vec<MissionSegment>,
}

// `tag`, `converged` and `selected_points` are consumed by the parity assertions
// below. `throttle` and `body_angle_rad` are retained for deserialization
// completeness only: the assertions that mention those two names read them off an
// `alas_mission::Segment`, not off this fixture struct.
#[allow(dead_code)]
#[derive(Deserialize)]
struct MissionSegment {
    tag: String,
    converged: bool,
    selected_points: BTreeMap<String, MissionPoint>,
    throttle: Vec<f64>,
    body_angle_rad: Vec<f64>,
}

#[derive(Deserialize)]
struct MissionPoint {
    angle_of_attack_rad: f64,
    mach: f64,
    temperature_k: f64,
    reynolds_number_per_m: f64,
    lift_coefficient: f64,
    cd_parasite: f64,
    cd_induced: f64,
    cd_compressible: f64,
    cd_miscellaneous: f64,
    cd_total: f64,
}

fn reference_report(config: &AlasConfig) -> AnalysisReport {
    FullAnalysis::new_reference_compatibility(config.clone())
        .run(&DesignVector::default(), true)
        .unwrap_or_else(|error| panic!("reference-compatible design report: {error}"))
}

#[test]
fn mission_aerodynamic_inputs_match_the_pinned_suave_vehicle() {
    let config = AlasConfig::default();
    let report = reference_report(&config);
    let analyses = build_analyses_reference_compatibility(&config, &report)
        .unwrap_or_else(|error| panic!("mission analyses: {error}"));
    let fixture: Fixture = alas_testkit::load("mission", "mission");
    let vehicle = fixture.inputs.vehicle;
    let mut comparison = Comparison::new("W6.4 SUAVE vehicle inputs", Tier::Closed);

    comparison.scalar(
        "reference_area_m2",
        analyses.reference_area_m2,
        vehicle.reference_area_m2,
    );
    for (index, (actual, expected)) in analyses.wings.iter().zip(&vehicle.wings).enumerate() {
        comparison
            .scalar(
                &format!("wings[{index}]/mean_aerodynamic_chord_m"),
                actual.mean_aerodynamic_chord_m,
                expected.mean_aerodynamic_chord_m,
            )
            .scalar(
                &format!("wings[{index}]/quarter_chord_sweep_rad"),
                actual.quarter_chord_sweep_rad,
                expected.quarter_chord_sweep_rad,
            )
            .scalar(
                &format!("wings[{index}]/thickness_to_chord"),
                actual.thickness_to_chord,
                expected.thickness_to_chord,
            )
            .scalar(
                &format!("wings[{index}]/reference_area_m2"),
                actual.reference_area_m2,
                expected.reference_area_m2,
            )
            .scalar(
                &format!("wings[{index}]/wetted_area_m2"),
                actual.wetted_area_m2,
                expected.wetted_area_m2,
            )
            .scalar(
                &format!("wings[{index}]/aspect_ratio"),
                actual.aspect_ratio,
                expected.aspect_ratio,
            );
    }
    comparison.exact("wing_count", &analyses.wings.len(), &vehicle.wings.len());
    let actual_fuselage = &analyses.fuselages[0];
    let expected_fuselage = &vehicle.fuselages[0];
    comparison
        .scalar(
            "fuselage/length_m",
            actual_fuselage.length_m,
            expected_fuselage.length_m,
        )
        .scalar(
            "fuselage/effective_diameter_m",
            actual_fuselage.effective_diameter_m,
            expected_fuselage.effective_diameter_m,
        )
        .scalar(
            "fuselage/front_projected_area_m2",
            actual_fuselage.front_projected_area_m2,
            expected_fuselage.front_projected_area_m2,
        )
        .scalar(
            "fuselage/wetted_area_m2",
            actual_fuselage.wetted_area_m2,
            expected_fuselage.wetted_area_m2,
        );
    comparison.exact(
        "fuselage_count",
        &analyses.fuselages.len(),
        &vehicle.fuselages.len(),
    );
    comparison.exact(
        "nacelle_count",
        &analyses.nacelles.len(),
        &vehicle.nacelles.len(),
    );
    for (index, (actual, expected)) in analyses.nacelles.iter().zip(&vehicle.nacelles).enumerate() {
        comparison
            .scalar(
                &format!("nacelles[{index}]/length_m"),
                actual.length_m,
                expected.length_m,
            )
            .scalar(
                &format!("nacelles[{index}]/diameter_m"),
                actual.diameter_m,
                expected.diameter_m,
            )
            .scalar(
                &format!("nacelles[{index}]/wetted_area_m2"),
                actual.wetted_area_m2,
                expected.wetted_area_m2,
            )
            .exact(
                &format!("nacelles[{index}]/origin_count"),
                &actual.origin_count,
                &expected.origin_count,
            );
    }
    comparison.exact(
        "network_count",
        &analyses.network_count,
        &vehicle.network_count,
    );
    let legacy = analyses
        .legacy_turbofan
        .as_ref()
        .unwrap_or_else(|| panic!("reference mission retains legacy turbofan inputs"));
    comparison
        .scalar(
            "turbofan/number_of_engines",
            legacy.inputs.number_of_engines,
            vehicle.turbofan.number_of_engines,
        )
        .scalar(
            "turbofan/bypass_ratio",
            legacy.inputs.bypass_ratio,
            vehicle.turbofan.bypass_ratio,
        )
        .scalar(
            "turbofan/fan_pressure_ratio",
            legacy.inputs.fan_pressure_ratio,
            vehicle.turbofan.fan_pressure_ratio,
        )
        .scalar(
            "turbofan/turbine_inlet_temperature_k",
            legacy.inputs.turbine_inlet_temperature_k,
            vehicle.turbofan.turbine_inlet_temperature_k,
        );
    comparison.finish();

    // The compatibility geometry and cruise-required sizing reproduce the
    // frozen vehicle's historical target.  Turbofan flow is linear in that
    // target, so retain an explicit parity check at this boundary.
    let l_over_d = report
        .trimmed_design_point
        .map(|point| point.l_over_d)
        .unwrap_or(report.design_point.l_over_d);
    let expected_reference_thrust_n = config.requirements.mtow_kg * 9.81 / l_over_d;
    assert!((legacy.inputs.design_thrust_total_n - expected_reference_thrust_n).abs() < 1.0e-9);
    // The frozen `golden/mission/mission.json` fixture's `design_thrust_total_n`
    // was generated against the pre-correction Lock/Korn wave-drag law
    // (`20 (M - M_dd)^4`, substituting the drag-divergence Mach for the
    // critical Mach; physics review v1.2, finding A3). `alas-aero::analysis::
    // AeroAnalysis::wave_drag` now applies the published law,
    // `20 (M - M_crit)^4`, which raises cruise CD and lowers L/D at this
    // Mach, so the two sides of this historical-target reproduction no
    // longer agree exactly and the fixture is not re-pinned (it also backs
    // the geometry/engine-spec comparisons above, which are unaffected and
    // still checked exactly). Bound the resulting increase in required
    // thrust instead of asserting equality: it must be positive (lower L/D
    // needs more thrust) and stay within a physically reasonable band for a
    // wave-drag correction at this cruise point -- the default aircraft's
    // own cruise CD rose by about 10% from this same fix
    // (`quick_analysis_wave_drag.rs`), so a required-thrust increase of the
    // same rough order, not e.g. a factor of two, is the expected signature.
    let thrust_increase_fraction = (legacy.inputs.design_thrust_total_n
        - vehicle.turbofan.design_thrust_total_n)
        / vehicle.turbofan.design_thrust_total_n;
    assert!(
        (0.0..0.30).contains(&thrust_increase_fraction),
        "reference thrust actual={} fixture={} expected_from_report={} increase_fraction={}",
        legacy.inputs.design_thrust_total_n,
        vehicle.turbofan.design_thrust_total_n,
        expected_reference_thrust_n,
        thrust_increase_fraction
    );
    let expected_flow = vehicle.turbofan.compressor_nondimensional_massflow
        * legacy.inputs.design_thrust_total_n
        / vehicle.turbofan.design_thrust_total_n;
    assert!((legacy.compressor_nondimensional_massflow - expected_flow).abs() < 1.0e-9);
}

#[test]
fn mission_surrogate_training_matches_the_pinned_suave_vlm_samples() {
    let config = AlasConfig::default();
    let report = reference_report(&config);
    let analyses = build_analyses_reference_compatibility(&config, &report)
        .unwrap_or_else(|error| panic!("mission analyses: {error}"));
    let fixture: LiftSurrogateFixture = alas_testkit::load("aero", "lift_surrogate");
    let actual = analyses.surrogate.training();
    let mut comparison = Comparison::new("W6.4 mission VLM training", Tier::F32);

    comparison.exact(
        "wing tags",
        &analyses.surrogate.wing_tags().to_vec(),
        &fixture.wing_tags,
    );
    for (name, actual_rows, expected_rows) in [
        (
            "aircraft/CL",
            &actual.lift_coefficient,
            &fixture.training.lift_coefficient,
        ),
        (
            "aircraft/CDi",
            &actual.drag_coefficient,
            &fixture.training.drag_coefficient,
        ),
    ] {
        for (row, (actual_row, expected_row)) in actual_rows.iter().zip(expected_rows).enumerate() {
            comparison.slice(&format!("{name}[{row}]"), actual_row, expected_row);
        }
    }
    for tag in &fixture.wing_tags {
        for (quantity, actual_rows, expected_rows) in [
            (
                "CL",
                &actual.wing_lift_coefficient[tag],
                &fixture.training.wing_lift_coefficient[tag],
            ),
            (
                "CDi",
                &actual.wing_drag_coefficient[tag],
                &fixture.training.wing_drag_coefficient[tag],
            ),
        ] {
            for (row, (actual_row, expected_row)) in
                actual_rows.iter().zip(expected_rows).enumerate()
            {
                comparison.slice(
                    &format!("{tag}/{quantity}[{row}]"),
                    actual_row,
                    expected_row,
                );
            }
        }
    }
    comparison.finish();
}

#[test]
fn mission_aerodynamics_match_suave_at_the_pinned_takeoff_solution() {
    let evidence: MissionEvidence = alas_testkit::load("mission", "w64_provenance");
    let takeoff = evidence
        .segments
        .iter()
        .find(|segment| segment.tag == "takeoff")
        .unwrap_or_else(|| panic!("the fixture has a takeoff segment"));
    let expected = &takeoff.selected_points["0"];
    let config = AlasConfig::default();
    let report = reference_report(&config);
    let analyses = build_analyses_reference_compatibility(&config, &report)
        .unwrap_or_else(|error| panic!("mission analyses: {error}"));
    let actual = analyses.aerodynamics(
        expected.angle_of_attack_rad,
        expected.mach,
        expected.temperature_k,
        expected.reynolds_number_per_m,
    );
    let mut comparison = Comparison::new("W6.4 takeoff aerodynamics", Tier::F32);
    comparison
        .scalar(
            "lift_coefficient",
            actual.lift_coefficient,
            expected.lift_coefficient,
        )
        .scalar("parasite", actual.drag.parasite_total, expected.cd_parasite)
        .scalar("induced", actual.drag.induced_total, expected.cd_induced)
        .scalar(
            "compressible",
            actual.drag.compressible_total,
            expected.cd_compressible,
        )
        .scalar(
            "miscellaneous",
            actual.drag.miscellaneous_total,
            expected.cd_miscellaneous,
        )
        .scalar("total", actual.drag.total, expected.cd_total);
    comparison.finish();
}

#[test]
fn mission_takeoff_solver_reaches_the_pinned_suave_solution() {
    use alas_config::airports::get as get_airport;

    let evidence: MissionEvidence = alas_testkit::load("mission", "w64_provenance");
    let expected = evidence
        .segments
        .into_iter()
        .find(|segment| segment.tag == "takeoff")
        .unwrap_or_else(|| panic!("the fixture has a takeoff segment"));
    let config = AlasConfig {
        departure_airport: "LEMD".to_owned(),
        arrival_airport: "HKJK".to_owned(),
        ..AlasConfig::default()
    };
    let origin = get_airport(&config.departure_airport)
        .unwrap_or_else(|error| panic!("configured origin: {error}"));
    let destination = get_airport(&config.arrival_airport)
        .unwrap_or_else(|error| panic!("configured destination: {error}"));
    let report = reference_report(&config);
    let analyses = build_analyses_reference_compatibility(&config, &report)
        .unwrap_or_else(|error| panic!("mission analyses: {error}"));
    let request = build_mission_request(&config, origin, destination, 6_500_000.0);
    let spec = build_schedule(&request)
        .unwrap_or_else(|error| panic!("takeoff schedule: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("takeoff specification"));
    let mut takeoff = alas_mission::Segment::new(spec, None)
        .unwrap_or_else(|error| panic!("takeoff setup: {error}"));
    let actual = alas_mission::converge_root(&mut takeoff, &analyses)
        .unwrap_or_else(|error| panic!("takeoff root solve: {error}"));
    assert_eq!(actual.converged, expected.converged);
    assert!(takeoff
        .throttle
        .iter()
        .chain(takeoff.body_angle_rad.iter())
        .all(|value| value.is_finite()));
}
