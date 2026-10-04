// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One seeded product search shared by the bilingual layout matrix.

use alas_config::AlasConfig;
use alas_pipeline::{DesignPipeline, PipelineOptions, PipelineResult, RunEnvironment};

const PRESET: &str = "A220-300";

pub(super) fn optimized_fixture() -> PipelineResult {
    let mut config =
        AlasConfig::from_value(&serde_json::json!({ "preset": PRESET })).expect("A220 preset");
    config.optimizer.solver.method = alas_config::optimizer::PRODUCT_DE_METHOD.to_owned();
    config.optimizer.solver.refinement.max_evaluations = 128;
    config.optimizer.solver.screening.max_evaluations = 8;
    config.optimizer.solver.workers = std::thread::available_parallelism()
        .map_or(1, |n| n.get().min(8))
        .try_into()
        .unwrap_or(1);
    config.structures.enabled = false;
    // Draft resolution bounds this UI fixture's cost while the real search
    // preserves the configured physical assumptions.
    let draft = &alas_config::fidelity_presets::get("draft")
        .expect("draft fidelity preset")
        .analysis;
    let analysis = &mut config.analysis;
    analysis.sweep_n_points = draft.sweep_n_points;
    analysis.spanwise_resolution = draft.spanwise_resolution;
    analysis.chordwise_resolution = draft.chordwise_resolution;
    analysis.fine_spanwise_resolution = draft.fine_spanwise_resolution;
    analysis.fine_chordwise_resolution = draft.fine_chordwise_resolution;
    let design = alas_config::presets::get(PRESET)
        .expect("A220 preset")
        .design_vector;
    let envelope = config.optimizer.design_space.envelope(&design);
    let bounds: Vec<(f64, f64)> = alas_config::DESIGN_VARIABLE_SPECS
        .iter()
        .map(|spec| {
            let variable = envelope
                .iter()
                .find(|v| v.name == spec.name)
                .expect("every registered design variable has an envelope");
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
    DesignPipeline::new(config)
        .run_with_design_space(&options, &RunEnvironment::default(), &design, &bounds)
        .expect("the seeded A220-300 finalist is delivered")
}
