// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! W6.4 evidence at the public pipeline boundary.
//!
//! The lower-level mission parity tests prove the native solver against a
//! SUAVE fixture. This test proves that the public pipeline reaches that same
//! native solver while isolating the declared structural-wingbox CG correction
//! from the unchanged force, atmosphere, aerodynamic, and mass-flow outputs.

use std::collections::BTreeMap;

use alas_aero::drag_buildup::DragSettings;
use alas_aero::lift_surrogate::FUSELAGE_LIFT_CORRECTION;
use alas_config::airports::get as get_airport;
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_exec::RunEnvironment;
use alas_pipeline::{DesignPipeline, FullAnalysis, PipelineOptions};
use alas_route::route::{Route, RouteSource, Waypoint, EARTH_RADIUS_M};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[path = "w64_mission_provenance/compare.rs"]
mod compare;

use compare::compare_point;

const TIER: Tier = Tier::Iter { relative: 1e-7 };

#[derive(Debug, Deserialize)]
struct Fixture {
    call_graph: CallGraph,
    solver: Solver,
    inputs: Inputs,
    geometry: Geometry,
    aero_settings: AeroSettings,
    operating_point: PointEvidence,
    segments: Vec<SegmentEvidence>,
    summary: Summary,
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
    all_converged: bool,
    control_points: Vec<usize>,
    tolerance_solution: Vec<f64>,
}

#[derive(Debug, Deserialize)]
struct Inputs {
    mission_tag: String,
    route_distance_m: f64,
}

#[derive(Debug, Deserialize)]
struct Geometry {
    physical_cg_m: [f64; 3],
    reference_area_m2: f64,
    mean_aerodynamic_chord_m: f64,
    reference_span_m: f64,
    wings: Vec<WingEvidence>,
}

#[derive(Debug, Deserialize)]
struct WingEvidence {
    name: String,
    area_m2: f64,
    mean_aerodynamic_chord_m: f64,
    aspect_ratio: f64,
    quarter_chord_sweep_rad: f64,
}

#[derive(Debug, Deserialize)]
struct AeroSettings {
    reference_area_m2: f64,
    maximum_lift_coefficient: Option<f64>,
    fuselage_lift_correction: f64,
    drag_settings: DragEvidence,
    network_count: usize,
    config_tags: Vec<String>,
    turbofan_number_of_engines: f64,
}

#[derive(Debug, Deserialize)]
struct DragEvidence {
    wing_parasite_drag_form_factor: f64,
    fuselage_parasite_drag_form_factor: f64,
    viscous_lift_dependent_drag_factor: f64,
    trim_drag_correction_factor: f64,
    drag_coefficient_increment: f64,
    spoiler_drag_increment: f64,
    lift_to_drag_adjustment: f64,
}

#[derive(Debug, Deserialize)]
struct SegmentEvidence {
    tag: String,
    converged: bool,
    number_control_points: usize,
    tolerance_solution: f64,
    air_speed_m_s: f64,
    selected_points: BTreeMap<String, PointEvidence>,
    throttle: Vec<f64>,
    body_angle_rad: Vec<f64>,
}

#[derive(Debug, Deserialize)]
struct PointEvidence {
    altitude_m: f64,
    temperature_k: f64,
    pressure_pa: f64,
    density_kg_m3: f64,
    speed_of_sound_m_s: f64,
    dynamic_viscosity_pa_s: f64,
    velocity_m_s: f64,
    mach: f64,
    reynolds_number_per_m: f64,
    dynamic_pressure_pa: f64,
    angle_of_attack_rad: f64,
    body_angle_rad: f64,
    lift_coefficient: f64,
    drag_coefficient: f64,
    throttle: f64,
    thrust_n: f64,
    mass_rate_kg_s: f64,
    mass_kg: f64,
    cd_parasite: f64,
    cd_induced: f64,
    cd_compressible: f64,
    cd_miscellaneous: f64,
    cd_total: f64,
}

#[derive(Debug, Deserialize)]
struct Summary {
    initial_mass_kg: f64,
    final_mass_kg: f64,
    fuel_burned_kg: f64,
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
    let lift_to_drag = |analysis: &alas_pipeline::AnalysisReport| {
        analysis
            .trimmed_design_point
            .map(|point| point.l_over_d)
            .unwrap_or(analysis.design_point.l_over_d)
    };
    let product_lift_to_drag = lift_to_drag(report);
    let reference_lift_to_drag = lift_to_drag(&reference_report);
    let expected_throttle_scale = product_lift_to_drag / reference_lift_to_drag;
    assert!(
        product_lift_to_drag.is_finite()
            && reference_lift_to_drag.is_finite()
            && expected_throttle_scale.is_finite()
            && expected_throttle_scale > 0.0,
        "invalid engine-resizing inputs: product L/D={product_lift_to_drag}, reference L/D={reference_lift_to_drag}"
    );
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

    let mut comparison = Comparison::new("W6.4 public native mission", TIER);
    comparison
        .scalar(
            "geometry/reference_area_m2",
            report.airplane.s_ref,
            fixture.geometry.reference_area_m2,
        )
        .scalar(
            "geometry/mean_aerodynamic_chord_m",
            report.airplane.c_ref,
            fixture.geometry.mean_aerodynamic_chord_m,
        )
        .scalar(
            "geometry/reference_span_m",
            report.airplane.b_ref,
            fixture.geometry.reference_span_m,
        );
    for (actual, expected) in report.airplane.wings.iter().zip(&fixture.geometry.wings) {
        comparison
            .exact(
                &format!("geometry/{}/name", expected.name),
                &actual.name,
                &expected.name,
            )
            .scalar(
                &format!("geometry/{}/area_m2", expected.name),
                actual.area(),
                expected.area_m2,
            )
            .scalar(
                &format!("geometry/{}/mean_aerodynamic_chord_m", expected.name),
                actual.mean_aerodynamic_chord(),
                expected.mean_aerodynamic_chord_m,
            )
            .scalar(
                &format!("geometry/{}/aspect_ratio", expected.name),
                actual.aspect_ratio(),
                expected.aspect_ratio,
            )
            .scalar(
                &format!("geometry/{}/quarter_chord_sweep_rad", expected.name),
                actual.mean_sweep_angle(0.25).to_radians(),
                expected.quarter_chord_sweep_rad,
            );
    }
    comparison.slice(
        "geometry/reference_compatible_physical_cg_m",
        &reference_report.physical_cg,
        &fixture.geometry.physical_cg_m,
    );
    comparison.exact(
        "mission tag",
        &fixture.inputs.mission_tag,
        &"LEMD_to_HKJK".to_owned(),
    );

    let settings = DragSettings::default();
    comparison
        .scalar(
            "aero/reference_area_m2",
            report.airplane.s_ref,
            fixture.aero_settings.reference_area_m2,
        )
        .exact(
            "aero/maximum_lift_coefficient",
            &None::<f64>,
            &fixture.aero_settings.maximum_lift_coefficient,
        )
        .scalar(
            "aero/fuselage_lift_correction",
            FUSELAGE_LIFT_CORRECTION,
            fixture.aero_settings.fuselage_lift_correction,
        )
        .scalar(
            "aero/wing_form_factor",
            settings.wing_parasite_drag_form_factor,
            fixture
                .aero_settings
                .drag_settings
                .wing_parasite_drag_form_factor,
        )
        .scalar(
            "aero/fuselage_form_factor",
            settings.fuselage_parasite_drag_form_factor,
            fixture
                .aero_settings
                .drag_settings
                .fuselage_parasite_drag_form_factor,
        )
        .scalar(
            "aero/viscous_factor",
            settings.viscous_lift_dependent_drag_factor,
            fixture
                .aero_settings
                .drag_settings
                .viscous_lift_dependent_drag_factor,
        )
        .scalar(
            "aero/trim_correction",
            settings.trim_drag_correction_factor,
            fixture
                .aero_settings
                .drag_settings
                .trim_drag_correction_factor,
        )
        .scalar(
            "aero/drag_increment",
            settings.drag_coefficient_increment,
            fixture
                .aero_settings
                .drag_settings
                .drag_coefficient_increment,
        )
        .scalar(
            "aero/spoiler_increment",
            settings.spoiler_drag_increment,
            fixture.aero_settings.drag_settings.spoiler_drag_increment,
        )
        .scalar(
            "aero/lift_to_drag_adjustment",
            settings.lift_to_drag_adjustment,
            fixture.aero_settings.drag_settings.lift_to_drag_adjustment,
        );
    comparison.exact(
        "aero/network_count",
        &1usize,
        &fixture.aero_settings.network_count,
    );
    comparison.exact(
        "aero/config_tags",
        &fixture.aero_settings.config_tags,
        &vec![
            "base".to_owned(),
            "cruise".to_owned(),
            "takeoff".to_owned(),
            "cutback".to_owned(),
            "landing".to_owned(),
            "short_field_takeoff".to_owned(),
        ],
    );
    comparison.scalar(
        "aero/turbofan_number_of_engines",
        config.geometry.engine.spanwise_positions_m.len() as f64,
        fixture.aero_settings.turbofan_number_of_engines,
    );

    comparison.exact(
        "solver/segment_count",
        &mission.segments.len(),
        &fixture.solver.segment_count,
    );
    comparison.exact(
        "solver/all_converged",
        &mission.solutions.iter().all(|s| s.converged),
        &fixture.solver.all_converged,
    );
    comparison.exact(
        "solver/control_points",
        &vec![mission.segments[0].conditions.len()],
        &fixture.solver.control_points,
    );
    comparison.slice(
        "solver/tolerance_solution",
        &[mission.segments[0].numerics.tolerance_solution],
        &fixture.solver.tolerance_solution,
    );

    for (index, (actual, expected)) in mission.segments.iter().zip(&fixture.segments).enumerate() {
        comparison.exact(
            &format!("segment[{index}]/tag"),
            &actual.spec.tag,
            &expected.tag,
        );
        comparison.exact(
            &format!("segment[{index}]/converged"),
            &mission.solutions[index].converged,
            &expected.converged,
        );
        comparison.exact(
            &format!("segment[{index}]/control_points"),
            &actual.conditions.len(),
            &expected.number_control_points,
        );
        comparison.scalar(
            &format!("segment[{index}]/air_speed_m_s"),
            actual.spec.air_speed_m_s,
            expected.air_speed_m_s,
        );
        comparison.scalar(
            &format!("segment[{index}]/tolerance_solution"),
            actual.numerics.tolerance_solution,
            expected.tolerance_solution,
        );
        for (row, (&product_throttle, &reference_throttle)) in
            actual.throttle.iter().zip(&expected.throttle).enumerate()
        {
            assert!(
                reference_throttle > 0.0,
                "fixture segment {index} throttle[{row}] must be positive"
            );
            // Required force is unchanged. Since build_analyses sizes thrust
            // as W/(L/D), the command scales by the inverse engine rating.
            comparison.scalar(
                &format!("segment[{index}]/throttle_scale[{row}]"),
                product_throttle / reference_throttle,
                expected_throttle_scale,
            );
        }
        comparison.slice(
            &format!("segment[{index}]/body_angle_rad"),
            &actual.body_angle_rad,
            &expected.body_angle_rad,
        );
        for (key, point) in &expected.selected_points {
            let row = key
                .parse::<usize>()
                .unwrap_or_else(|error| panic!("fixture row {key}: {error}"));
            compare_point(
                &mut comparison,
                &format!("segment[{index}]/point[{row}]"),
                &actual.conditions,
                row,
                point,
                expected_throttle_scale,
            );
        }
    }
    comparison.scalar(
        "summary/initial_mass_kg",
        mission.initial_mass_kg(),
        fixture.summary.initial_mass_kg,
    );
    comparison.scalar(
        "summary/final_mass_kg",
        mission.final_mass_kg(),
        fixture.summary.final_mass_kg,
    );
    comparison.scalar(
        "summary/fuel_burned_kg",
        mission.fuel_burned_kg(),
        fixture.summary.fuel_burned_kg,
    );
    comparison.scalar(
        "operating_point/angle_of_attack_rad",
        mission.segments[4].conditions.angle_of_attack_rad[8],
        fixture.operating_point.angle_of_attack_rad,
    );
    comparison.scalar(
        "operating_point/mach",
        mission.segments[4].conditions.mach[8],
        fixture.operating_point.mach,
    );
    comparison.scalar(
        "operating_point/drag_coefficient",
        mission.segments[4].conditions.drag_coefficient[8],
        fixture.operating_point.drag_coefficient,
    );
    comparison.finish();
}
