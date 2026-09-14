// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Public optimizer-boundary probes for valid/invalid candidate ordering.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_opt::{DesignOptimizer, ObjectiveEvaluation, OptimizationError};

#[test]
fn public_de_returns_a_valid_candidate_when_an_invalid_one_is_cheaper() {
    for seed in [0, 1, 42, 99] {
        let mut config = AlasConfig::default();
        // A zero-iteration product search only evaluates the midpoint, so it
        // cannot exercise feasibility-first ordering. One search iteration
        // supplies the initial population and a competing trial set.
        config.optimizer.solver.max_iterations = 1;
        config.optimizer.solver.population_size = 1;
        config.optimizer.solver.seed = Some(seed);
        config.optimizer.solver.seed_near_initial_design = false;

        // Every fixed coordinate is a physically valid nominal value. The
        // only free coordinate is span, using its declared [60, 80] m range;
        // the invalid side is deliberately cheaper so scalar-only ranking
        // would select it. The old [0, 1] fixture was rejected by the
        // design-mode envelope before the evaluator could run.
        let nominal = DesignVector::default();
        let span_bounds = DesignVector::bounds()[0];
        let mut bounds: Vec<(f64, f64)> = nominal
            .to_array()
            .into_iter()
            .map(|value| (value, value))
            .collect();
        bounds[0] = span_bounds;
        let threshold = 0.5 * (span_bounds.0 + span_bounds.1);
        let mut evaluator = |design: &DesignVector| {
            if design.span_m >= threshold {
                ObjectiveEvaluation {
                    cost: 10.0,
                    valid: true,
                    l_over_d: 10.0,
                    span_m: design.span_m,
                    alpha_deg: 3.0,
                    area_m2: 20.0,
                    trim_ih_deg: 0.0,
                    reject_reason: String::new(),
                }
            } else {
                ObjectiveEvaluation::rejected(-100.0, "static_margin")
            }
        };

        let result = DesignOptimizer::new(config)
            .run_with_evaluator(Some(&bounds), None, &mut evaluator, None)
            .expect("at least one candidate is deliberately feasible");

        assert!(result.best_valid, "seed={seed}");
        assert_eq!(result.best_cost, 10.0, "seed={seed}");
        assert!(result.history.n_valid() > 0, "seed={seed}");
        assert!(
            result.history.n_valid() < result.history.n_evaluations(),
            "seed={seed}"
        );
        assert!(result.best_design.span_m >= 0.5, "seed={seed}");
    }
}

#[test]
fn public_de_returns_typed_failure_for_an_all_invalid_population() {
    let mut config = AlasConfig::default();
    config.optimizer.solver.max_iterations = 0;
    config.optimizer.solver.population_size = 1;
    config.optimizer.solver.seed = Some(7);

    let mut evaluator = |_design: &DesignVector| {
        ObjectiveEvaluation::rejected(-1_000.0, "static_margin+cg_envelope")
    };
    let error = DesignOptimizer::new(config)
        .run_with_evaluator(None, None, &mut evaluator, None)
        .expect_err("an all-invalid population must not produce a design");

    let OptimizationError::NoFeasibleDesign(summary) = error else {
        panic!("expected NoFeasibleDesign, got {error:?}");
    };
    assert!(summary.evaluated_candidates > 0);
    assert_eq!(
        summary.rejection_reason_counts["static_margin"],
        summary.evaluated_candidates
    );
    assert_eq!(
        summary.rejection_reason_counts["cg_envelope"],
        summary.evaluated_candidates
    );
}
