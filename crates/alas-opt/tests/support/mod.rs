// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the optimizer integration tests share: a preset's registered design,
//! the clean-sheet configuration of an MTOW sizing mode, and the violated
//! hard residual test.

// Each test binary compiles its own copy of this module and uses part of it.
#![allow(dead_code)]
// A test binary's failed expect is the assertion failing.
#![allow(clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, MtowSizing};

mod assessment_view;
// Each test binary imports the part of the shared-assessment API it uses.
#[allow(unused_imports)]
pub use assessment_view::{assess_view, Assessed, AssessmentView};

/// The registered design vector of `preset`.
pub fn nominal(preset: &str) -> DesignVector {
    alas_config::presets::get(preset)
        .expect("registered preset")
        .design_vector
}

/// The default clean-sheet aircraft sized in `sizing`, with the transport
/// planform preferences disabled.
pub fn clean_sheet(sizing: MtowSizing) -> AlasConfig {
    let mut config = AlasConfig::default();
    let optimizer = &mut config.optimizer;
    optimizer.weights.transport_planform_constraints_enabled = false;
    optimizer.objective.mtow_sizing = sizing;
    config
}

/// Whether `id` appears among the assessment's violated hard residuals.
pub fn violates(assessment: &impl Assessed, id: &str) -> bool {
    assessment
        .violated_hard_ids()
        .into_iter()
        .any(|name| name == id)
}

/// A configuration for `preset` in reference-adaptation mode, with a small
/// but honest search budget so a test measures behaviour rather than
/// hardware: the seed is fixed and the stage budgets are evaluation counts.
pub fn reference_search_config(
    preset: &str,
    workers: i64,
    screening_evaluations: i64,
    refinement_evaluations: i64,
) -> AlasConfig {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    config.optimizer.solver.method = alas_config::optimizer::PRODUCT_DE_METHOD.to_owned();
    config.optimizer.design_space.mode = alas_config::DesignMode::ReferenceAdaptation;
    config.optimizer.solver.seed = Some(7);
    config.optimizer.solver.workers = workers;
    config.optimizer.solver.screening.max_evaluations = screening_evaluations;
    config.optimizer.solver.refinement.max_evaluations = refinement_evaluations;
    config
}

/// What a search must not change when it is spread over more worker threads.
///
/// Every batch is built from the seed before it is evaluated and every score
/// is stored at its own index, so the worker count may change the wall time
/// and nothing else: same winner bits, same history, same per-stage
/// accounting.
pub fn assert_same_search_outcome(
    serial: &alas_opt::OptimizationResult,
    parallel: &alas_opt::OptimizationResult,
    workers: i64,
) {
    assert_eq!(serial.best_design, parallel.best_design, "{workers}");
    assert_eq!(serial.best_cost.to_bits(), parallel.best_cost.to_bits());
    assert_eq!(serial.termination, parallel.termination);
    assert_eq!(
        serial.history.design_vectors,
        parallel.history.design_vectors
    );
    let (a, b) = (
        serial.search_diagnostics.clone().expect("diagnostics"),
        parallel.search_diagnostics.clone().expect("diagnostics"),
    );
    assert_eq!(a.analysis_evaluations, b.analysis_evaluations);
    assert_eq!(a.poll_iterations, b.poll_iterations);
    assert_eq!(
        a.poll_block_size, b.poll_block_size,
        "N_init follows the budget"
    );
    let counts = |d: &alas_opt::SearchDiagnostics| {
        d.stages
            .iter()
            .map(|s| {
                (
                    s.evaluations,
                    s.elite_size,
                    s.termination.clone(),
                    s.sizing_work,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(counts(&a), counts(&b));
    assert_eq!(b.workers, workers as usize);
}
