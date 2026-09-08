// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! W6.4 evidence at the public pipeline boundary.
//!
//! The lower-level mission parity tests prove the native solver against a
//! SUAVE fixture. This test proves that the public pipeline reaches that same
//! native solver while isolating the declared structural-wingbox CG correction
//! from the unchanged force, atmosphere, aerodynamic, and mass-flow outputs.

use alas_config::airports::get as get_airport;
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_exec::RunEnvironment;
use alas_pipeline::{DesignPipeline, FullAnalysis, PipelineOptions};
use alas_route::route::{Route, RouteSource, Waypoint, EARTH_RADIUS_M};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Fixture {
    call_graph: CallGraph,
    solver: Solver,
    inputs: Inputs,
    geometry: Geometry,
}

#[derive(Debug, Deserialize)]
struct CallGraph {
    reference: Vec<String>,
    native: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Solver {
    reference: String,
    native: String,
    segment_count: usize,
    control_points: Vec<usize>,
}

#[derive(Debug, Deserialize)]
struct Inputs {
    mission_tag: String,
    route_distance_m: f64,
}

#[derive(Debug, Deserialize)]
struct Geometry {
    reference_area_m2: f64,
    reference_span_m: f64,
}

fn fixture() -> Fixture {
    alas_testkit::load("mission", "w64_provenance")
}

fn route(config: &AlasConfig, distance_m: f64) -> Route {
    let origin = get_airport(&config.departure_airport)
        .unwrap_or_else(|error| panic!("configured origin: {error}"));
    let destination = get_airport(&config.arrival_airport)
        .unwrap_or_else(|error| panic!("configured destination: {error}"));
    let latitude = origin.latitude_deg.to_radians();
    let central_angle = distance_m / EARTH_RADIUS_M;
    let longitude_delta = 2.0
        * ((central_angle / 2.0).sin() / latitude.cos())
            .asin()
            .to_degrees();
    Route {
        waypoints: vec![
            Waypoint::named(
                origin.latitude_deg,
                origin.longitude_deg,
                origin.icao.clone(),
            ),
            Waypoint::named(
                origin.latitude_deg,
                origin.longitude_deg + longitude_delta,
                "W64_FIXTURE",
            ),
        ],
        source: RouteSource::SimbriefApi,
        origin_airport: Some(origin.clone()),
        dest_airport: Some(destination.clone()),
    }
}

fn options() -> PipelineOptions {
    PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: None,
        quiet: true,
    }
}

#[test]
fn public_native_mission_matches_pinned_provenance_checkpoints() {
    let fixture = fixture();
    assert_eq!(
        fixture.call_graph.reference,
        vec![
            "alas.pipeline -> alas.integration.suave_mission.build_mission_request",
            "external tools/suave_runner/vehicle_builder.build_vehicle",
            "external tools/suave_runner/mission_builder.mission_setup",
            "SUAVE.Analyses.Mission.Sequential_Segments.evaluate",
            "SUAVE.Methods.Missions.Segments.converge_root",
        ]
    );
    assert_eq!(
        fixture.call_graph.native,
        vec![
            "DesignPipeline.run_with_environment_and_route",
            "alas_pipeline::mission_stage::evaluate",
            "alas_pipeline::mission_stage::build_analyses",
            "alas_mission::Mission::evaluate",
            "alas_mission::converge_root",
        ]
    );
    assert_eq!(
        fixture.solver.reference,
        "scipy.optimize.fsolve / MINPACK hybrd"
    );
    assert_eq!(fixture.solver.native, "alas_math::hybrd");

    let mut config = AlasConfig {
        departure_airport: "LEMD".to_owned(),
        arrival_airport: "HKJK".to_owned(),
        ..AlasConfig::default()
    };
    // This fixture predates the product transport-planform stations. Keep its
    // frozen three-station geometry so the test continues to isolate the
    // declared structural-wingbox CG correction instead of mixing a geometry
    // model change into the W6.4 mission-solver parity evidence.
    config.geometry.wing.side_of_body_span_fraction = None;
    config.geometry.wing.side_of_body_chord_ratio = None;
    config.geometry.wing.kink_span_fraction = None;
    config.geometry.wing.outboard_le_sweep_deg = None;
    let active_route = route(&config, fixture.inputs.route_distance_m);
    let result = DesignPipeline::new(config.clone())
        .run_with_environment_and_route(
            &options(),
            &RunEnvironment::default(),
            Some(active_route.clone()),
        )
        .unwrap_or_else(|error| panic!("public native mission path: {error}"));
    assert_eq!(result.route, Some(active_route));

    let report = result
        .optimized_report
        .as_ref()
        .unwrap_or_else(|| panic!("public path did not publish its analysis report"));
    let mission = result
        .mission_result
        .as_ref()
        .unwrap_or_else(|| panic!("public path did not publish mission telemetry"));
    let reference_report = FullAnalysis::new_reference_compatibility(config.clone())
        .run(&DesignVector::default(), true)
        .unwrap_or_else(|error| panic!("reference-compatible report: {error}"));
    let cg_shift_m = report
        .physical_cg
        .iter()
        .zip(reference_report.physical_cg)
        .map(|(&product, reference)| (product - reference).powi(2))
        .sum::<f64>()
        .sqrt();
    assert!(
        cg_shift_m > 0.1,
        "the public path must retain the structural-wingbox correction: product={:?}, reference={:?}",
        report.physical_cg,
        reference_report.physical_cg
    );

    // Product reference propagation: the published summary and every public
    // mission consumer must use projected XY area/lateral span.  The frozen
    // geometry fixture intentionally remains the unfolded compatibility
    // evidence and is checked through the explicit reference report below.
    let main_wing = match report.airplane.wings.first() {
        Some(main_wing) => main_wing,
        None => panic!("product report has a main wing"),
    };
    assert!((report.airplane.s_ref - main_wing.reference_area()).abs() < 1.0e-9);
    assert!((report.airplane.b_ref - main_wing.reference_span()).abs() < 1.0e-9);
    assert!((report.geometry_summary["wing_area_m2"] - report.airplane.s_ref).abs() < 1.0e-9);
    assert!((report.geometry_summary["span_m"] - report.airplane.b_ref).abs() < 1.0e-9);
    assert!(
        (report.geometry_summary["aspect_ratio"]
            - report.airplane.b_ref * report.airplane.b_ref / report.airplane.s_ref)
            .abs()
            < 1.0e-9
    );
    assert!((report.airplane.s_ref - main_wing.unfolded_area()).abs() > 1.0e-6);

    assert!((reference_report.airplane.s_ref - fixture.geometry.reference_area_m2).abs() < 1.0e-6);
    assert!((reference_report.airplane.b_ref - fixture.geometry.reference_span_m).abs() < 1.0e-6);
    assert!(
        (reference_report.geometry_summary["wing_area_m2"] - fixture.geometry.reference_area_m2)
            .abs()
            < 1.0e-6
    );
    assert_eq!(fixture.inputs.mission_tag, "LEMD_to_HKJK");

    // The installed GE9X rating, not the historical cruise-required target,
    // sizes product turbofan flow.  This is 2 * 467 kN = 934,000 N.
    let rated_total_thrust_n = config.geometry.engine.thrust_kn()
        * config.geometry.engine.spanwise_positions_m.len() as f64
        * 1000.0;
    assert!((rated_total_thrust_n - 934_000.0).abs() < 1.0e-9);

    assert_eq!(mission.segments.len(), fixture.solver.segment_count);
    assert!(mission.solutions.iter().all(|solution| solution.converged));
    assert_eq!(
        mission.segments[0].conditions.len(),
        fixture.solver.control_points[0]
    );
    assert!(mission
        .segments
        .iter()
        .flat_map(|segment| segment.throttle.iter())
        .all(|throttle| throttle.is_finite() && *throttle >= 0.0 && *throttle <= 1.0 + 1.0e-9));
    assert!(mission.initial_mass_kg().is_finite());
    assert!(mission.final_mass_kg().is_finite());
    assert!(mission.fuel_burned_kg().is_finite() && mission.fuel_burned_kg() > 0.0);
    assert!(mission.final_mass_kg() < mission.initial_mass_kg());
    let final_range_m = mission
        .segments
        .last()
        .and_then(|segment| segment.conditions.aircraft_range_m.last())
        .copied()
        .unwrap_or(f64::NAN);
    assert!(
        (final_range_m - fixture.inputs.route_distance_m).abs() < 1.0e-3,
        "mission route closure: final range {final_range_m}, requested {}",
        fixture.inputs.route_distance_m
    );
}
