// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The worker-count determinism of the two-stage product search at the
//! smallest budget that runs both stages: the same seed reaches the same
//! winner, history and per-stage accounting on 1 and on 4 workers.
//!
//! `staged_search` repeats the comparison at 136 evaluations and 16 workers
//! (the nightly test); this is the per-change version of it.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_opt::DesignOptimizer;

mod support;
use support::nominal;

/// Evaluations of each stage: the 24-member initial population plus the first
/// generation's trial members.
const STAGE_EVALUATIONS: i64 = 40;

#[test]
fn the_same_seed_reaches_the_same_winner_at_1_and_4_workers() {
    let preset = "A320-200";
    let start = nominal(preset);
    // Evaluation budgets only: time limits depend on the machine and its load.
    let run_with = |workers: i64| {
        let mut config =
            support::reference_search_config(preset, workers, STAGE_EVALUATIONS, STAGE_EVALUATIONS);
        config.optimizer.solver.stop_on_evaluations_only = true;
        DesignOptimizer::new(config)
            .run(None, Some(&start), None)
            .unwrap_or_else(|error| panic!("{workers} workers: {error}"))
    };
    let serial = run_with(1);
    let parallel = run_with(4);
    support::assert_same_search_outcome(&serial, &parallel, 4);
    let diagnostics = serial.search_diagnostics.expect("diagnostics");
    assert_eq!(diagnostics.stages.len(), 2, "both stages ran");
}
