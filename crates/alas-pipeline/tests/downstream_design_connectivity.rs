// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! After a sized product run every consumer reads the evaluated design and the
//! pipeline's single mass: the published design vector is the one the report
//! was built on, and the V-n envelope is evaluated at the structural design
//! mass the wing-box loads use.

// A test asserts on values it constructed, so a failed unwrap is the
// assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::AlasConfig;
use alas_pipeline::{
    design_mass_config, design_vn_diagram, design_vn_mass_kg, DesignPipeline, PipelineOptions,
    RunEnvironment,
};

#[test]
fn published_design_and_vn_mass_follow_the_sized_evaluation() {
    let mut config =
        AlasConfig::from_value(&serde_json::json!({ "preset": "A220-300" })).expect("A220 preset");
    config.optimizer.solver.method = alas_config::optimizer::PRODUCT_DE_METHOD.to_owned();
    config.optimizer.solver.max_iterations = 1;
    config.optimizer.solver.population_size = 4;
    config.optimizer.solver.workers = 1;
    config.structures.enabled = false;
    config.optimizer.solver.display_progress = false;
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
    let result = DesignPipeline::new(config.clone())
        .run_with_design_space(&options, &RunEnvironment::default(), &design, &bounds)
        .expect("finalist");
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
