// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;

use super::{converged, latin_hypercube_population, scored_point, select_samples, DesignOptimizer};
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
    let result =
        optimizer.run_with_evaluator(None, Some(&DesignVector::default()), &mut evaluator, None);

    assert_eq!(calls, result.history.n_evaluations());
    assert_eq!(calls, result.history.design_vectors.len());
    assert!(result.history.valid.iter().all(|valid| *valid));
}

#[test]
fn every_native_method_dispatches_through_the_shared_evaluator_contract() {
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

        let result = optimizer.run_with_evaluator(
            None,
            Some(&DesignVector::default()),
            &mut evaluator,
            None,
        );

        assert_eq!(result.method, method);
        assert!(result.best_cost.is_finite(), "{method}");
        assert!(result.history.n_evaluations() > 0, "{method}");
        if method == "nsga2" {
            assert!(!result.pareto_front.is_empty());
        }
    }
}
