// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! After a sized product run every consumer reads the evaluated design and the
//! pipeline's single mass: the published design vector is the one the report
//! was built on, the V-n envelope is evaluated at the structural design mass
//! the wing-box loads use, and the feasibility load case the mission is flown
//! at is the report's sized takeoff mass.
//!
//! One small seeded product-DE run of the A220-300 preset supplies the result.
//! The checks are about numbers flowing consistently between stages, not about
//! the value of any aerodynamic figure, so the run uses the draft analysis
//! resolution and a worker pool (the seeded search is independent of the
//! worker count).

// A test asserts on values it constructed, so a failed unwrap is the
// assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::AlasConfig;
use alas_pipeline::{
    design_mass_config, design_vn_diagram, design_vn_mass_kg, DesignPipeline, LoadCaseSelection,
    PipelineOptions, PipelineResult, RunEnvironment,
};

/// The seeded A220-300 configuration: the smallest product-DE budget that still
/// delivers a finalist, on the draft analysis resolution and up to eight
/// workers.
fn seeded_a220_config() -> AlasConfig {
    let mut config =
        AlasConfig::from_value(&serde_json::json!({ "preset": "A220-300" })).expect("A220 preset");
    config.optimizer.solver.method = alas_config::optimizer::PRODUCT_DE_METHOD.to_owned();
    config.optimizer.solver.refinement.max_evaluations = 128;
    config.optimizer.solver.screening.max_evaluations = 8;
    config.optimizer.solver.workers = std::thread::available_parallelism()
        .map_or(1, |n| n.get().min(8))
        .try_into()
        .unwrap_or(1);
    config.structures.enabled = false;
    // Only the five resolution fields of the draft preset, so no tuned
    // assumption in the analysis configuration is overwritten.
    let draft = &alas_config::fidelity_presets::get("draft")
        .expect("draft fidelity preset")
        .analysis;
    let analysis = &mut config.analysis;
    analysis.sweep_n_points = draft.sweep_n_points;
    analysis.spanwise_resolution = draft.spanwise_resolution;
    analysis.chordwise_resolution = draft.chordwise_resolution;
    analysis.fine_spanwise_resolution = draft.fine_spanwise_resolution;
    analysis.fine_chordwise_resolution = draft.fine_chordwise_resolution;
    config
}

fn run_seeded_a220(config: &AlasConfig) -> PipelineResult {
    let design = alas_config::presets::get("A220-300")
        .expect("A220 preset")
        .design_vector;
    let envelope = config.optimizer.design_space.envelope(&design);
    let bounds: Vec<(f64, f64)> = alas_config::DESIGN_VARIABLE_SPECS
        .iter()
        .map(|spec| {
            let variable = envelope.iter().find(|v| v.name == spec.name).unwrap();
            (variable.lower, variable.upper)
        })
        .collect();
    let options = PipelineOptions {
        optimize: true,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };
    DesignPipeline::new(config.clone())
        .run_with_design_space(&options, &RunEnvironment::default(), &design, &bounds)
        .expect("finalist")
}

#[test]
fn published_design_and_vn_mass_follow_the_sized_evaluation() {
    let config = seeded_a220_config();
    let result = run_seeded_a220(&config);
    let report = result.optimized_report.as_ref().expect("optimized report");

    // The published vector is the evaluated one, and the design space accepts
    // it unchanged (re-evaluating it derives nothing further).
    assert_eq!(result.optimized_design, Some(report.design));
    let replay = alas_opt::mdo::assess_product_candidate(&config, &report.design)
        .expect("the published design can be re-evaluated");
    assert_eq!(replay.resolved.design, report.design);

    // The V-n input mass is the structural design mass the wing-box loads use,
    // which is never lighter than the sized takeoff mass.
    let sized_kg = report.sized_takeoff_mass_kg().expect("sized report");
    let design_kg = design_vn_mass_kg(&config, report);
    let structural = design_mass_config(&config, report);
    assert_eq!(
        design_kg,
        alas_opt::mdo::structural_feasibility::structural_design_mass_kg(&structural)
    );
    assert!(design_kg >= sized_kg - 1.0e-9);
    let vn = design_vn_diagram(&config, report);
    let mut requirements = structural.requirements.clone();
    requirements.mtow_kg = design_kg;
    let expected = alas_perf::performance::build_vn_diagram(
        report.airplane.s_ref,
        &requirements,
        &config.performance,
        config.requirements.cruise_altitude_m,
    );
    assert_eq!(vn.v_s_kt, expected.v_s_kt);
    assert_eq!(vn.v_a_kt, expected.v_a_kt);
    if (design_kg - config.requirements.mtow_kg).abs() > 1.0 {
        let at_limit = alas_perf::performance::build_vn_diagram(
            report.airplane.s_ref,
            &config.requirements,
            &config.performance,
            config.requirements.cruise_altitude_m,
        );
        assert!(
            (vn.v_s_kt - at_limit.v_s_kt).abs() > 1.0e-9,
            "the envelope must move with the design mass"
        );
    }
}

// A seeded product run flies its route off-design and reports the design
// mission it was sized on.
//
// The feasibility load case is the mass the native mission was flown at. That
// mass is the route's own dispatch on the candidate's carried model: the
// planned airway is not the still-air design range, so the flown mass follows
// the route, while the design mission is reported as the closure priced it.
#[test]
fn report_load_case_flies_the_route_and_reports_the_design_mission() {
    let result = run_seeded_a220(&seeded_a220_config());
    let report = result.optimized_report.as_ref().expect("optimized report");
    let sized = report.fuel.sized_fuel().expect("sized fuel");
    let loading = &result.feasibility.fuel_loading;
    let load_case = result
        .mission_load_case
        .as_ref()
        .expect("the mission load case is published");
    // The feasibility loading state is the load case the mission was flown at,
    // exactly.
    assert!(
        (loading.analyzed_takeoff_mass_kg - load_case.takeoff_mass_kg).abs() < 1.0e-6,
        "analysed takeoff mass {} kg vs flown {} kg",
        loading.analyzed_takeoff_mass_kg,
        load_case.takeoff_mass_kg
    );
    let LoadCaseSelection::PolicyClosure(case) = &load_case.selection else {
        panic!("a sized product run flies the policy load case");
    };
    // The design mission is the closure's, as it priced it.
    let design = case.design_mission.expect("the design mission is reported");
    assert_eq!(design.takeoff_mass_kg, sized.takeoff_mass_kg);
    assert_eq!(design.trip_fuel_kg, sized.trip_fuel_kg);
    assert_eq!(design.range_m, sized.design_range_m);
    // The route is flown off-design: a longer route needs a heavier takeoff
    // and more trip fuel than the design mission, a shorter one less.
    let longer = case.route_distance_m > design.range_m;
    assert_eq!(
        case.plan.trip.kg > design.trip_fuel_kg,
        longer,
        "route {} m trip {} kg vs design {} m trip {} kg",
        case.route_distance_m,
        case.plan.trip.kg,
        design.range_m,
        design.trip_fuel_kg
    );
    assert_eq!(load_case.takeoff_mass_kg > design.takeoff_mass_kg, longer);
    assert!(case.reserve_margin_kg >= 0.0 && case.shortfall_kg == 0.0);
}
