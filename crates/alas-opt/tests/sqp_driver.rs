// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Behavioural tests for the `sqp` product method (`alas_opt::gradient`
//! driven through `DesignOptimizer`).

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, MtowSizing, ObjectiveKind};
use alas_opt::{DesignOptimizer, ObjectiveEvaluation, OptimizationError};

/// (1) Under a delegated evaluator the driver is a bound-constrained
/// search: a smooth bowl in two design variables is minimised to its known
/// centre, and the result carries the `sqp` method label and the
/// termination reason in the strategy field.
#[test]
fn a_delegated_smooth_objective_is_minimised_to_its_known_centre() {
    let mut config = AlasConfig::default();
    config.optimizer.solver.method = "sqp".to_owned();
    config.optimizer.solver.max_iterations = 40;
    config.optimizer.solver.tolerance = 1e-9;
    config.optimizer.solver.finite_difference_step = 1e-5;
    let mut optimizer = DesignOptimizer::new(config);
    let mut bounds = vec![(0.0, 0.0); alas_config::DESIGN_VARIABLE_SPECS.len()];
    bounds[0] = (60.0, 80.0);
    bounds[1] = (12.0, 19.0);
    let mut evaluator = |design: &DesignVector| ObjectiveEvaluation {
        cost: (design.span_m - 66.0).powi(2) / 100.0 + (design.root_chord_m - 15.0).powi(2),
        valid: true,
        l_over_d: 18.0,
        span_m: design.span_m,
        alpha_deg: 2.0,
        area_m2: 400.0,
        trim_ih_deg: 0.0,
        reject_reason: String::new(),
    };
    let start = DesignVector {
        span_m: 75.0,
        root_chord_m: 18.0,
        ..DesignVector::default()
    };
    let result = optimizer
        .run_with_evaluator(Some(&bounds), Some(&start), &mut evaluator, None)
        .expect("a smooth bowl has a feasible minimiser");
    assert_eq!(result.method, "sqp");
    assert!(result.best_valid);
    assert!(
        (result.best_design.span_m - 66.0).abs() < 0.05,
        "span {}",
        result.best_design.span_m
    );
    assert!(
        (result.best_design.root_chord_m - 15.0).abs() < 0.01,
        "root chord {}",
        result.best_design.root_chord_m
    );
    assert!(
        result.strategy.starts_with("converged"),
        "{}",
        result.strategy
    );
    assert_eq!(result.history.n_evaluations(), result.history.valid.len());
}

/// (2) An evaluator that fails everywhere is reported as no feasible
/// design rather than as a converged result.
#[test]
fn an_always_invalid_evaluator_is_no_feasible_design() {
    let mut config = AlasConfig::default();
    config.optimizer.solver.method = "sqp".to_owned();
    config.optimizer.solver.max_iterations = 3;
    let mut optimizer = DesignOptimizer::new(config);
    let mut evaluator = |_design: &DesignVector| ObjectiveEvaluation::rejected(1.0e3, "trim_solve");
    let error = optimizer
        .run_with_evaluator(None, None, &mut evaluator, None)
        .expect_err("an invalid start cannot produce a design");
    let OptimizationError::NoFeasibleDesign(summary) = error else {
        panic!("expected NoFeasibleDesign, got {error:?}");
    };
    assert_eq!(summary.evaluated_candidates, 1);
    assert_eq!(summary.rejection_reason_counts["trim_solve"], 1);
}

/// (3) Against the native mission-sized objective the driver runs a full
/// major iteration: the finite-difference batch over every free design
/// variable, the subproblem and the line search. The exact default design
/// sits a few parts per million over its wing-area cap (see
/// `tests/mission_sized.rs`), so the run may end feasible or may report the
/// residuals it could not clear within one iteration; both are legitimate,
/// and either way every evaluation is recorded with the mission-sized
/// fields aligned.
#[test]
fn one_major_iteration_on_the_mission_sized_objective_records_every_evaluation() {
    let mut config = AlasConfig::default();
    config.optimizer.objective.kind = ObjectiveKind::BlockFuel;
    config.optimizer.objective.mtow_sizing = MtowSizing::SizedByMission;
    config.optimizer.solver.method = "sqp".to_owned();
    config.optimizer.solver.max_iterations = 1;
    config.optimizer.solver.workers = 4;
    let free_variables = alas_config::DESIGN_VARIABLE_SPECS.len();
    let mut optimizer = DesignOptimizer::new(config);
    let start = DesignVector::default();
    let outcome = optimizer.run(None, Some(&start), None);
    let history = match &outcome {
        Ok(result) => {
            assert_eq!(result.method, "sqp");
            assert!(result.best_valid);
            assert!(result.best_cost.is_finite());
            result.history.clone()
        }
        Err(OptimizationError::NoFeasibleDesign(summary)) => {
            assert!(summary.evaluated_candidates > free_variables);
            assert!(!summary.rejection_reason_counts.is_empty());
            return;
        }
        Err(other) => panic!("unexpected optimizer error: {other}"),
    };
    // The start, the finite-difference batch and at least one line-search
    // trial.
    assert!(
        history.n_evaluations() >= free_variables + 2,
        "{} evaluations",
        history.n_evaluations()
    );
    assert_eq!(history.takeoff_mass_kg.len(), history.n_evaluations());
    assert!(history
        .takeoff_mass_kg
        .iter()
        .zip(&history.valid)
        .all(|(mass, valid)| !valid || mass.is_finite()));
}
