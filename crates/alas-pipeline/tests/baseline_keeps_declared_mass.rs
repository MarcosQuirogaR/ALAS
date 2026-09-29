// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A baseline (non-optimizing) run stays on the declared MTOW.
//!
//! Only a sized product-optimizer finalist is bound to a mission-sized
//! takeoff mass. The baseline and preset/parity reports must publish no sized
//! mass, so every consumer that draws payload-range, landing and take-off or
//! static thrust-to-weight through `analysis_takeoff_mass_kg` falls back to
//! the declared `requirements.mtow_kg`.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig, DesignMode};
use alas_pipeline::baseline::analyze_baseline;
use alas_pipeline::full_analysis::AnalysisReport;
use alas_pipeline::{DesignPipeline, FullAnalysis, PipelineOptions, RunEnvironment};

const PRESETS: [&str; 2] = ["A320-200", "ATR72-600"];

fn preset_config(name: &str) -> AlasConfig {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    config
}

/// The report carries no sized-mass binding, and the analyzed takeoff mass
/// every figure reads is the declared MTOW.
fn assert_declared_basis(report: &AnalysisReport, declared_mtow_kg: f64, what: &str) {
    assert!(declared_mtow_kg > 0.0, "{what}: declared MTOW");
    let flag = report
        .geometry_summary
        .get("analysis_mass_basis_is_sized")
        .copied();
    assert!(
        flag.is_none_or(|value| value <= 0.5),
        "{what}: baseline report flagged as sized ({flag:?})"
    );
    assert_eq!(report.sized_takeoff_mass_kg(), None, "{what}");
    // Payload-range, LTO and static T/W all take this mass.
    assert_eq!(
        report.analysis_takeoff_mass_kg(declared_mtow_kg),
        declared_mtow_kg,
        "{what}"
    );
    if let Some(basis) = report.geometry_summary.get("analysis_mass_basis_kg") {
        assert!(
            (basis - declared_mtow_kg).abs() < 1.0e-6,
            "{what}: mass basis {basis} kg vs declared {declared_mtow_kg} kg"
        );
    }
}

#[test]
fn baseline_pipeline_run_keeps_the_declared_mtow() {
    for name in PRESETS {
        let mut config = preset_config(name);
        config.mission.enabled = false;
        config.mses.enabled = false;
        config.structures.enabled = false;
        let declared_mtow_kg = config.requirements.mtow_kg;
        let options = PipelineOptions {
            optimize: false,
            compare_baseline: true,
            parallel: false,
            aerodynamic_solver: Default::default(),
            optimization_solver: Default::default(),
            output_dir: None,
            save_plots: false,
            seed: Some(42),
            quiet: true,
        };
        let result = DesignPipeline::new(config)
            .run(&options, &RunEnvironment::default())
            .unwrap();
        let mut checked = 0;
        for report in [&result.baseline_analysis, &result.optimized_report]
            .into_iter()
            .flatten()
        {
            assert_declared_basis(report, declared_mtow_kg, name);
            checked += 1;
        }
        assert!(checked > 0, "{name}: the run produced no report to check");
        assert!(
            result.baseline_analysis.is_some(),
            "{name}: {:?}",
            result.baseline_analysis_error
        );
    }
}

#[test]
fn preset_parity_analysis_keeps_the_declared_mtow() {
    for name in PRESETS {
        let config = preset_config(name);
        let declared_mtow_kg = config.requirements.mtow_kg;
        let design = presets::get(name).unwrap().design_vector;
        let report = FullAnalysis::new(config).run(&design, true).unwrap();
        assert_declared_basis(&report, declared_mtow_kg, name);
    }
}

#[test]
fn a_sized_binding_is_the_only_thing_that_moves_the_basis() {
    // Control: the same preset bound to a sized mass does report it, so the
    // absence checked above is a property of the baseline path and not of a
    // key the report never writes.
    let config = preset_config("A320-200");
    let declared_mtow_kg = config.requirements.mtow_kg;
    let design = presets::get("A320-200").unwrap().design_vector;
    let sized_kg = 0.9 * declared_mtow_kg;
    let report = FullAnalysis::new(config)
        .run_at_sized_takeoff_mass(&design, true, sized_kg)
        .unwrap();
    assert_eq!(report.sized_takeoff_mass_kg(), Some(sized_kg));
    assert_eq!(report.analysis_takeoff_mass_kg(declared_mtow_kg), sized_kg);
}

#[test]
fn preset_baseline_balance_is_computed_at_the_declared_mtow() {
    for name in PRESETS {
        let config = preset_config(name);
        let design = presets::get(name).unwrap().design_vector;
        let baseline = analyze_baseline(&config, &design);
        assert_eq!(baseline.status, "ok", "{name}: {:?}", baseline.error);
        // The lumped baseline is the unsized preset: its masses come from the
        // declared MTOW basis, never from a mission-sized one.
        let mtow_kg = config.requirements.mtow_kg;
        let total: f64 = baseline.component_masses.values().sum();
        assert!(
            total > 0.0 && total <= mtow_kg * 1.001,
            "{name}: component sum {total} kg vs declared MTOW {mtow_kg} kg"
        );
    }
}
