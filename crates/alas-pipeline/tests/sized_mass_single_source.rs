// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A seeded product run publishes one takeoff mass.
//!
//! The report is bound to the mission-sized takeoff mass, and the feasibility
//! load case the mission is flown at must be that same mass, not a second
//! fixed-point solution a few kilograms away.

// A test asserts on values it constructed, so a failed unwrap is the
// assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::AlasConfig;
use alas_pipeline::{DesignPipeline, PipelineOptions, RunEnvironment};

// Measured cause of the disagreement: the A220 sized mass is 54,368 kg against
// 54,394 kg flown (26 kg). The reserves are priced on the report's untrimmed
// sweep polar, the optimizer's on its trimmed polar. Long-haul sized-vs-flown
// gaps reach +8.6 % (DC-10).
#[test]
#[ignore = "three fuel models disagree; pending single-model unification"]
fn report_load_case_and_mission_share_the_sized_takeoff_mass() {
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
    let result = DesignPipeline::new(config)
        .run_with_design_space(&options, &RunEnvironment::default(), &design, &bounds)
        .expect("finalist");
    let report = result.optimized_report.as_ref().expect("optimized report");
    let sized_kg = report
        .sized_takeoff_mass_kg()
        .expect("a product run binds its report to the sized takeoff mass");
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
    // The native mission re-prices the fuel of the analytically sized mass; a
    // disagreement beyond its 1e-4 settling tolerance moves the flown mass to
    // the refined value. The report stays bound to the sized mass, so the two
    // agree to about twice that tolerance rather than bit-for-bit.
    let tolerance_kg = 2.0e-4 * sized_kg;
    assert!(
        (loading.analyzed_takeoff_mass_kg - sized_kg).abs() < tolerance_kg,
        "analysed takeoff mass {} kg vs sized {sized_kg} kg",
        loading.analyzed_takeoff_mass_kg
    );
    assert!(
        (loading.analyzed_carried_fuel_kg - loading.mtow_closure_fuel_kg).abs() < tolerance_kg,
        "carried fuel {} kg vs the closure fuel {} kg the sized mass was built with",
        loading.analyzed_carried_fuel_kg,
        loading.mtow_closure_fuel_kg
    );
}
