// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;

use super::{
    candidate_is_at_least_as_good, candidate_is_better, converged, latin_hypercube_population,
    scored_point, select_samples, DesignObjective, DesignOptimizer, OptimizationError,
    SearchObjective,
};
use crate::evaluator::ObjectiveEvaluation;
use crate::history::OptimizationHistory;
use crate::python_rng::RandomState;

#[test]
fn convergence_uses_population_standard_deviation() {
    assert!(converged(&[1.0, 2.0, 3.0], 0.6));
    assert!(!converged(&[1.0, 2.0, 3.0], 0.2));
}

#[test]
fn a_solver_failure_is_worse_than_a_recoverable_constraint_violation() {
    let design = DesignVector::default();
    let values = design.to_array();
    let mut failed = OptimizationHistory::new();
    failed.record(design, false, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, "stall_guard");
    let mut recoverable = OptimizationHistory::new();
    recoverable.record(
        design,
        false,
        2.0,
        18.0,
        60.0,
        5.0,
        360.0,
        0.0,
        "body_alpha_window",
    );

    let failed_point = scored_point(&values, 1.0, &failed);
    let recoverable_point = scored_point(&values, 2.0, &recoverable);

    assert!(failed_point.constraint_violation > recoverable_point.constraint_violation);
}

#[test]
fn default_de_never_prefers_a_lower_cost_invalid_candidate_over_a_valid_one() {
    assert!(!candidate_is_better(-100.0, false, 10.0, true, true));
    assert!(candidate_is_better(10.0, true, -100.0, false, true));
    assert!(candidate_is_at_least_as_good(10.0, true, 10.0, true, true));
    assert!(!candidate_is_at_least_as_good(
        -100.0, false, 10.0, true, true
    ));
    // The compatibility replay intentionally retains scalar-only ordering.
    assert!(candidate_is_better(-100.0, false, 10.0, true, false));
}

#[test]
fn default_de_returns_the_valid_candidate_when_invalid_is_cheaper() {
    // The synthetic bounds this fixture used to pass, `(0.0, 1.0)` on every
    // coordinate including `span_m`, predate the design-mode envelope
    // intersection in `DesignOptimizer::effective_bounds`: they sit entirely
    // outside the global `[60, 80]` m span spec and are now rejected before
    // the evaluator ever runs. Every coordinate is pinned at its default
    // value except `span_m`, which is left free across its own global
    // envelope -- an envelope-intersecting request that still lets the
    // search choose between a low, valid span and a high, cheaper-but-
    // rejected one, which is the behavior under test.
    let mut config = AlasConfig::default();
    // A zero iteration budget only evaluates the single bounds-midpoint
    // point under the current single-MADS-driver product path, so there is
    // never a competing invalid candidate to prefer the valid one over.
    // One iteration also runs the search-phase sampling that actually
    // exercises multiple candidates across the free span coordinate.
    config.optimizer.solver.max_iterations = 1;
    config.optimizer.solver.population_size = 1;
    config.optimizer.solver.seed = Some(42);
    config.optimizer.solver.seed_near_initial_design = false;
    let mut optimizer = DesignOptimizer::new(config);
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
    let result = optimizer
        .run_with_evaluator(Some(&bounds), None, &mut evaluator, None)
        .expect("the seeded population contains a feasible candidate");
    assert!(result.best_valid);
    assert_eq!(result.best_cost, 10.0);
}

#[test]
fn default_de_reports_no_feasible_design_instead_of_returning_an_invalid_one() {
    let mut config = AlasConfig::default();
    config.optimizer.solver.max_iterations = 1;
    config.optimizer.solver.population_size = 1;
    config.optimizer.solver.seed = Some(7);
    let mut optimizer = DesignOptimizer::new(config);
    let mut evaluator = |_design: &DesignVector| {
        ObjectiveEvaluation::rejected(-1_000.0, "static_margin+cg_envelope")
    };

    let error = optimizer
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

#[test]
fn default_de_accepts_exact_and_near_bound_designs_for_every_seeded_strategy() {
    let nominal = DesignVector::default();
    let exact_bounds: Vec<(f64, f64)> = nominal
        .to_array()
        .into_iter()
        .map(|value| (value, value))
        .collect();
    let near_bounds: Vec<(f64, f64)> = nominal
        .to_array()
        .into_iter()
        .map(|value| (value - 1.0e-12, value + 1.0e-12))
        .collect();

    for strategy in [
        "best1bin",
        "best1exp",
        "rand1bin",
        "rand1exp",
        "best2bin",
        "best2exp",
        "rand2bin",
        "rand2exp",
        "randtobest1bin",
        "randtobest1exp",
        "currenttobest1bin",
        "currenttobest1exp",
    ] {
        for (seed, bounds) in [
            (0_i64, exact_bounds.as_slice()),
            (42, near_bounds.as_slice()),
        ] {
            let mut config = AlasConfig::default();
            config.optimizer.solver.max_iterations = 1;
            config.optimizer.solver.population_size = 1;
            config.optimizer.solver.seed = Some(seed);
            config.optimizer.solver.strategy = strategy.to_owned();
            let mut optimizer = DesignOptimizer::new(config);
            let mut evaluator = |design: &DesignVector| ObjectiveEvaluation {
                cost: design.to_array().iter().map(|value| value * value).sum(),
                valid: true,
                l_over_d: 10.0,
                span_m: design.span_m,
                alpha_deg: 3.0,
                area_m2: 20.0,
                trim_ih_deg: 0.0,
                reject_reason: String::new(),
            };
            let result = optimizer
                .run_with_evaluator(Some(bounds), Some(&nominal), &mut evaluator, None)
                .unwrap_or_else(|error| panic!("strategy={strategy} seed={seed}: {error}"));
            assert!(result.best_valid, "strategy={strategy} seed={seed}");
            assert!(result
                .best_design
                .to_array()
                .iter()
                .zip(bounds)
                .all(|(value, &(lower, upper))| *value >= lower && *value <= upper));
        }
    }
}

#[test]
fn configured_methods_and_seeds_keep_boundary_results_typed_and_feasible() {
    let nominal = DesignVector::default();
    let exact_bounds: Vec<(f64, f64)> = nominal
        .to_array()
        .into_iter()
        .map(|value| (value, value))
        .collect();
    let near_bounds: Vec<(f64, f64)> = nominal
        .to_array()
        .into_iter()
        .map(|value| (value - 1.0e-12, value + 1.0e-12))
        .collect();

    for method in [
        "differential_evolution",
        "feasibility_first_de",
        "nsga2",
        "turbo_1",
        "cma_es",
    ] {
        for seed in [0_i64, 42_i64] {
            for bounds in [exact_bounds.as_slice(), near_bounds.as_slice()] {
                let mut config = AlasConfig::default();
                config.optimizer.solver.method = method.to_owned();
                config.optimizer.solver.max_iterations = 0;
                config.optimizer.solver.population_size = 1;
                config.optimizer.solver.seed = Some(seed);
                config.optimizer.solver.strategy = "best1bin".to_owned();
                let mut optimizer = DesignOptimizer::new(config);
                let mut evaluator = |design: &DesignVector| ObjectiveEvaluation {
                    cost: design.to_array().iter().map(|value| value * value).sum(),
                    valid: true,
                    l_over_d: 10.0,
                    span_m: design.span_m,
                    alpha_deg: 3.0,
                    area_m2: 20.0,
                    trim_ih_deg: 0.0,
                    reject_reason: String::new(),
                };
                let result = optimizer
                    .run_with_evaluator(Some(bounds), Some(&nominal), &mut evaluator, None)
                    .unwrap_or_else(|error| {
                        panic!("method={method} seed={seed} boundary search failed: {error}")
                    });
                assert!(result.best_valid, "method={method} seed={seed}");
                assert!(result
                    .best_design
                    .to_array()
                    .iter()
                    .zip(bounds)
                    .all(|(value, &(lower, upper))| *value >= lower && *value <= upper));
            }

            let mut config = AlasConfig::default();
            config.optimizer.solver.method = method.to_owned();
            config.optimizer.solver.max_iterations = 0;
            config.optimizer.solver.population_size = 1;
            config.optimizer.solver.seed = Some(seed);
            let mut optimizer = DesignOptimizer::new(config);
            let mut evaluator = |_design: &DesignVector| {
                ObjectiveEvaluation::rejected(-1_000.0, "static_margin+cg_envelope")
            };
            let error = optimizer
                .run_with_evaluator(Some(&exact_bounds), Some(&nominal), &mut evaluator, None)
                .expect_err("an all-invalid configured method must not publish a design");
            let OptimizationError::NoFeasibleDesign(summary) = error else {
                panic!("method={method} seed={seed} returned {error:?}");
            };
            assert!(
                summary.evaluated_candidates > 0,
                "method={method} seed={seed}"
            );
        }
    }
}

#[test]
fn unknown_optimizer_tokens_do_not_start_a_fallback_search() {
    let mut config = AlasConfig::default();
    config.optimizer.solver.method = "differential_evoluton".to_owned();
    let mut optimizer = DesignOptimizer::new(config);
    let mut calls = 0;
    let mut evaluator = |_design: &DesignVector| {
        calls += 1;
        ObjectiveEvaluation::rejected(0.0, "should_not_run")
    };
    let error = optimizer
        .run_with_evaluator(None, None, &mut evaluator, None)
        .expect_err("an unknown method must fail before evaluating candidates");
    assert_eq!(calls, 0);
    assert!(matches!(
        error,
        OptimizationError::InvalidConfiguration(reason) if reason.contains("unknown optimizer method")
    ));
}

#[test]
fn unknown_strategy_does_not_fall_back_to_best1() {
    let mut config = AlasConfig::default();
    config.optimizer.solver.strategy = "best1bni".to_owned();
    let mut optimizer = DesignOptimizer::new(config);
    let mut calls = 0;
    let mut evaluator = |_design: &DesignVector| {
        calls += 1;
        ObjectiveEvaluation::rejected(0.0, "should_not_run")
    };
    let error = optimizer
        .run_with_evaluator(None, None, &mut evaluator, None)
        .expect_err("an unknown strategy must fail before evaluating candidates");
    assert_eq!(calls, 0);
    assert!(matches!(
        error,
        OptimizationError::InvalidConfiguration(reason) if reason.contains("unknown optimizer strategy")
    ));
}

#[test]
fn sample_selection_excludes_the_candidate_and_has_distinct_indices() {
    let mut rng = RandomState::seed(42);
    let mut indices: Vec<usize> = (0..16).collect();
    let samples = select_samples(3, 5, &mut indices, &mut rng);

    assert_eq!(samples.len(), 5);
    assert!(samples.iter().all(|&sample| sample != 3));
    assert!(samples
        .iter()
        .enumerate()
        .all(|(index, sample)| !samples[index + 1..].contains(sample)));
}

#[test]
fn latin_hypercube_initialization_uses_each_stratum_once_per_dimension() {
    let bounds = [(0.0, 1.0), (10.0, 20.0)];
    let mut rng = RandomState::seed(42);
    let population = latin_hypercube_population(&bounds, 4, &mut rng);

    for dimension in 0..bounds.len() {
        let mut strata: Vec<usize> = population
            .iter()
            .map(|candidate| {
                let normalized = (candidate[dimension] - bounds[dimension].0)
                    / (bounds[dimension].1 - bounds[dimension].0);
                (normalized * 4.0).floor() as usize
            })
            .collect();
        strata.sort_unstable();
        assert_eq!(strata, vec![0, 1, 2, 3]);
    }
}

#[test]
fn delegated_objective_keeps_the_optimizer_history_contract() {
    let mut config = AlasConfig::default();
    config.optimizer.solver.max_iterations = 0;
    config.optimizer.solver.population_size = 1;
    config.optimizer.solver.seed = Some(42);
    config.optimizer.solver.seed_near_initial_design = true;
    let mut optimizer = DesignOptimizer::new(config);
    let mut calls = 0usize;
    let mut evaluator = |design: &DesignVector| {
        calls += 1;
        ObjectiveEvaluation {
            cost: design.to_array()[0].abs(),
            valid: true,
            l_over_d: 12.0,
            span_m: 10.0,
            alpha_deg: 2.0,
            area_m2: 20.0,
            trim_ih_deg: 0.0,
            reject_reason: String::new(),
        }
    };
    let result = optimizer
        .run_with_evaluator(None, Some(&DesignVector::default()), &mut evaluator, None)
        .expect("the delegated objective accepts every candidate");

    assert_eq!(calls, result.history.n_evaluations());
    assert_eq!(calls, result.history.design_vectors.len());
    assert!(result.history.valid.iter().all(|valid| *valid));
}

#[test]
fn clean_sheet_optimizer_publishes_the_cabin_sized_fuselage() {
    let mut config = AlasConfig::default();
    config.optimizer.solver.max_iterations = 0;
    config.optimizer.solver.population_size = 1;
    config.optimizer.solver.seed = Some(42);
    let expected = crate::mdo::canonicalize_design(&config, DesignVector::default())
        .expect("the default clean-sheet load case has a sized fuselage");
    let mut optimizer = DesignOptimizer::new(config);
    let mut evaluator = |design: &DesignVector| ObjectiveEvaluation {
        cost: design.fuselage_length_m,
        valid: true,
        l_over_d: 10.0,
        span_m: design.span_m,
        alpha_deg: 2.0,
        area_m2: design.root_chord_m * design.span_m,
        trim_ih_deg: 0.0,
        reject_reason: String::new(),
    };

    let result = optimizer
        .run_with_evaluator(None, None, &mut evaluator, None)
        .expect("the synthetic evaluator accepts the canonical point");

    assert_eq!(
        result.best_design.fuselage_length_m,
        expected.fuselage_length_m
    );
}

#[test]
fn native_worker_batches_merge_history_in_candidate_order() {
    let mut objective = DesignObjective::new(AlasConfig::default());
    let candidates = vec![Vec::new(), Vec::new(), Vec::new(), Vec::new()];

    let evaluations = objective.evaluate_batch(&candidates, 2);

    assert_eq!(evaluations.len(), candidates.len());
    assert_eq!(objective.history.n_evaluations(), candidates.len());
    // `DesignObjective::new` builds a product (non-reference) objective, so
    // `evaluate` (`objective_evaluate.rs:41`) routes straight to
    // `crate::mdo::evaluate_mission_sized` rather than the frozen-replay
    // path below it that records "geometry_build". The mission-sized path
    // validates the design-vector shape first
    // (`DesignObjective::validate_design_space`,
    // `objective_model.rs:252-286`) and records the earlier, more precise
    // "design_space" reason before ever reaching geometry construction, so
    // an empty (wrong-length) candidate now fails at that shape check.
    assert!(objective
        .history
        .reject_reason
        .iter()
        .all(|reason| reason == "design_space"));
}

#[test]
fn every_legacy_method_name_still_dispatches_through_the_single_mads_driver() {
    // Every product run uses one MADS driver now (see the comment on
    // `DesignOptimizer::run_product_search`); the legacy per-method names
    // (`feasibility_first_de`, `nsga2`, `turbo_1`, `cma_es`) remain valid,
    // loadable `optimizer.solver.method` values for saved configurations
    // (`SolverSettings::is_supported_method`) but no longer select a
    // distinct search algorithm or produce a genetic Pareto front. The
    // shared contract this test actually verifies is that none of those
    // saved values are rejected and every one reaches a finite, evaluated
    // result through the same evaluator.
    for method in ["feasibility_first_de", "nsga2", "turbo_1", "cma_es"] {
        let mut config = AlasConfig::default();
        config.optimizer.solver.method = method.to_owned();
        config.optimizer.solver.max_iterations = 1;
        config.optimizer.solver.population_size = 1;
        config.optimizer.solver.seed = Some(42);
        let mut optimizer = DesignOptimizer::new(config);
        let mut evaluator = |design: &DesignVector| {
            let values = design.to_array();
            let cost = values.iter().map(|value| value * value).sum();
            ObjectiveEvaluation {
                cost,
                valid: true,
                l_over_d: 1.0 / (1.0 + cost),
                span_m: design.span_m,
                alpha_deg: 3.0,
                area_m2: design.span_m * design.root_chord_m,
                trim_ih_deg: 0.0,
                reject_reason: String::new(),
            }
        };

        let result = optimizer
            .run_with_evaluator(None, Some(&DesignVector::default()), &mut evaluator, None)
            .expect("the test objective accepts every candidate");

        assert_eq!(result.method, "mads", "{method}");
        assert!(result.best_cost.is_finite(), "{method}");
        assert!(result.history.n_evaluations() > 0, "{method}");
    }
}
