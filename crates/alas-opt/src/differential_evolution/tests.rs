// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::sync::atomic::{AtomicBool, Ordering};

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;

use super::{
    scored_point_at, DesignObjective, DesignOptimizer, OptimizationError, SearchObjective,
};
use crate::evaluator::ObjectiveEvaluation;
use crate::history::OptimizationHistory;

/// A cheap synthetic product objective: every candidate is accepted, and the
/// cost is a smooth function of the whole vector, so no design or seed makes
/// the search stall on an all-infeasible generation.
fn synthetic_objective(design: &DesignVector) -> ObjectiveEvaluation {
    let values = design.to_array();
    ObjectiveEvaluation {
        cost: values.iter().map(|value| value * value).sum(),
        valid: true,
        l_over_d: 1.0,
        span_m: design.span_m,
        alpha_deg: 3.0,
        area_m2: design.span_m * design.root_chord_m,
        trim_ih_deg: 0.0,
        reject_reason: String::new(),
    }
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

    let failed_point = scored_point_at(&values, 1.0, &failed, 0);
    let recoverable_point = scored_point_at(&values, 2.0, &recoverable, 0);

    assert!(failed_point.constraint_violation > recoverable_point.constraint_violation);
}

#[test]
fn a_candidate_that_exhausts_its_work_cap_ranks_as_not_closed() {
    use crate::mdo::mission_model::{SizingBudget, SIZING_BUDGET_EXHAUSTED};
    use crate::search_methods::Tier;
    let values = DesignVector::default().to_array();
    let mut objective = DesignObjective::new(AlasConfig::default());
    let uncapped = crate::mdo::evaluate_mission_sized(&mut objective, &values);
    let sized = scored_point_at(&values, uncapped, &objective.history, 0);
    assert_ne!(sized.tier, Tier::NotClosed);
    assert!(objective.history.trip_flights[0] > 1);
    objective.sizing_controls.budget = Some(SizingBudget {
        max_trip_flights: 1,
        max_deck_evals: u64::MAX,
        max_outer_passes: u32::MAX,
    });
    let capped = crate::mdo::evaluate_mission_sized(&mut objective, &values);
    assert_eq!(objective.history.reject_reason[1], SIZING_BUDGET_EXHAUSTED);
    assert_eq!(objective.history.trip_flights[1], 0);
    let point = scored_point_at(&values, capped, &objective.history, 1);
    assert_eq!(point.tier, Tier::NotClosed);
    assert!(point.feasibility_key() > sized.feasibility_key());
}

#[test]
fn default_de_returns_the_valid_candidate_when_invalid_is_cheaper() {
    // Bounds of `(0.0, 1.0)` on every coordinate would sit entirely outside
    // the global `[60, 80]` m span spec and be rejected by the design-mode
    // envelope intersection in `DesignOptimizer::effective_bounds` before
    // the evaluator ever runs. Every coordinate is therefore pinned at its
    // default value except `span_m`, which is left free across its own
    // global envelope: an envelope-intersecting request that still lets the
    // search choose between a low, valid span and a high, cheaper-but-
    // rejected one, which is the behavior under test.
    let mut config = AlasConfig::default();
    config.optimizer.solver.method = alas_config::optimizer::PRODUCT_DE_METHOD.to_owned();
    // A zero iteration budget only evaluates the single bounds-midpoint
    // point under the current single-L-SHADE-driver product path, so there is
    // never a competing invalid candidate to prefer the valid one over.
    // One iteration also runs the search-phase sampling that actually
    // exercises multiple candidates across the free span coordinate.
    config.optimizer.solver.refinement.max_evaluations = 16;
    config.optimizer.solver.screening.max_evaluations = 8;
    config.optimizer.solver.seed = Some(42);
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
    config.optimizer.solver.method = alas_config::optimizer::PRODUCT_DE_METHOD.to_owned();
    config.optimizer.solver.refinement.max_evaluations = 16;
    config.optimizer.solver.screening.max_evaluations = 8;
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
fn cancelled_native_diagnostics_preserve_cancellation_instead_of_infeasibility() {
    let mut config = AlasConfig::default();
    config.optimizer.solver.method = alas_config::optimizer::PRODUCT_DE_METHOD.to_owned();
    config.optimizer.solver.workers = 1;
    let cancel = AtomicBool::new(true);
    let outcome = DesignOptimizer::new(config)
        .run_diagnostics_cancellable(None, None, None, Some(&cancel))
        .expect("cancelled diagnostic work retains completed evidence");
    assert!(outcome.evidence().was_cancelled());
    assert!(!outcome.evidence().is_delivered_feasible());
    assert!(outcome.rejection().is_none());
}

#[test]
fn failed_native_search_diagnostics_preserve_the_strict_rejection_and_candidate() {
    let mut config = AlasConfig::default();
    config.optimizer.solver.method = alas_config::optimizer::PRODUCT_DE_METHOD.to_owned();
    config.optimizer.solver.refinement.max_evaluations = 8;
    config.optimizer.solver.screening.max_evaluations = 8;
    config.optimizer.solver.workers = 1;
    config.optimizer.solver.seed = Some(7);
    config.requirements.min_passenger_capacity = i64::MAX;
    let nominal = DesignVector::default();
    let bounds: Vec<(f64, f64)> = nominal
        .to_array()
        .into_iter()
        .map(|value| (value, value))
        .collect();
    let strict_error = DesignOptimizer::new(config.clone())
        .run(Some(&bounds), Some(&nominal), None)
        .expect_err("the impossible passenger requirement cannot be accepted");
    let OptimizationError::NoFeasibleDesign(rejection) = strict_error else {
        panic!("expected an evaluated rejection, got {strict_error:?}");
    };
    let diagnostic = DesignOptimizer::new(config)
        .run_diagnostics(Some(&bounds), Some(&nominal), None)
        .expect("failed searches retain evidence for review");
    let evidence = diagnostic.evidence();
    assert_eq!(diagnostic.rejection(), Some(&rejection));
    assert!(!evidence.best_valid);
    assert!(!evidence.is_delivered_feasible());
    assert_eq!(evidence.termination, "no_feasible_design");
    assert!(!evidence.search_diagnostics.as_ref().unwrap().converged);
    assert_eq!(evidence.history.n_evaluations(), 1);
    assert_eq!(evidence.history.design_vectors, vec![nominal]);
    assert_eq!(evidence.best_design, nominal);
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

    for method in ["differential_evolution"] {
        for seed in [0_i64, 42_i64] {
            for bounds in [exact_bounds.as_slice(), near_bounds.as_slice()] {
                let mut config = AlasConfig::default();
                config.optimizer.solver.method = method.to_owned();
                config.optimizer.solver.refinement.max_evaluations = 8;
                config.optimizer.solver.screening.max_evaluations = 8;
                config.optimizer.solver.seed = Some(seed);
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
            config.optimizer.solver.refinement.max_evaluations = 8;
            config.optimizer.solver.screening.max_evaluations = 8;
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
fn delegated_objective_keeps_the_optimizer_history_contract() {
    let mut config = AlasConfig::default();
    config.optimizer.solver.refinement.max_evaluations = 8;
    config.optimizer.solver.screening.max_evaluations = 8;
    config.optimizer.solver.seed = Some(42);
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
fn a_fully_fixed_design_costs_one_analysis_and_never_claims_search_convergence() {
    let mut config = AlasConfig::default();
    config.optimizer.solver.method = alas_config::optimizer::PRODUCT_DE_METHOD.to_owned();
    config.optimizer.solver.refinement.max_evaluations = 808;
    config.optimizer.solver.screening.max_evaluations = 8;
    config.optimizer.solver.workers = 4;
    let initial = DesignVector::default();
    let bounds: Vec<(f64, f64)> = initial
        .to_array()
        .iter()
        .map(|&value| (value, value))
        .collect();
    let mut optimizer = DesignOptimizer::new(config);
    let mut calls = 0;
    let mut evaluate = |design: &DesignVector| {
        calls += 1;
        synthetic_objective(design)
    };
    let watch = crate::cancellation::CancelWatch::new();
    let result = optimizer
        .run_with_evaluator_cancellable(
            Some(&bounds),
            Some(&initial),
            &mut evaluate,
            None,
            Some(watch.flag()),
        )
        .expect("the delegated fixed candidate is feasible");
    assert_eq!(calls, 1);
    assert_eq!(watch.snapshot().evaluations_completed, 1);
    assert_eq!(result.history.n_evaluations(), 1);
    assert_eq!(result.best_design, initial);
    assert_eq!(result.termination, "fixed_bounds");
    let diagnostics = result.search_diagnostics.expect("product diagnostics");
    assert_eq!(diagnostics.screening_evaluations, 0);
    assert_eq!(diagnostics.poll_iterations, 0);
    assert!(!diagnostics.converged);
}

#[test]
fn clean_sheet_optimizer_publishes_the_cabin_sized_fuselage() {
    let mut config = AlasConfig::default();
    config.optimizer.solver.method = alas_config::optimizer::PRODUCT_DE_METHOD.to_owned();
    config.optimizer.solver.refinement.max_evaluations = 8;
    config.optimizer.solver.screening.max_evaluations = 8;
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
    // `evaluate` routes to `crate::mdo::evaluate_mission_sized`, which
    // validates the design-vector shape first
    // (`DesignObjective::validate_design_space`) and records "design_space"
    // before any geometry is built, so an empty (wrong-length) candidate
    // fails at that shape check.
    assert!(objective
        .history
        .reject_reason
        .iter()
        .all(|reason| reason == "design_space"));
}

#[test]
fn an_unsupported_method_token_set_directly_is_rejected_rather_than_silently_run() {
    // A saved configuration document migrates an unsupported method token
    // (`sqp`, `nsga2`, `turbo_1`, `cma_es`, `feasibility_first_de`) to
    // `differential_evolution` at the load boundary, with a note the caller
    // can surface (`alas_config::settings_load_notes`). A caller that builds
    // `SolverSettings` directly, bypassing that boundary, gets a clear
    // validation error instead of the token silently running a different
    // algorithm.
    for method in [
        "sqp",
        "nsga2",
        "turbo_1",
        "cma_es",
        "feasibility_first_de",
        "scipy_legacy",
    ] {
        let mut config = AlasConfig::default();
        config.optimizer.solver.method = method.to_owned();
        let mut optimizer = DesignOptimizer::new(config);
        let mut calls = 0;
        let mut evaluator = |_design: &DesignVector| {
            calls += 1;
            ObjectiveEvaluation::rejected(0.0, "should_not_run")
        };
        let error = optimizer
            .run_with_evaluator(None, None, &mut evaluator, None)
            .expect_err("an unsupported token built directly must not silently run a search");
        assert_eq!(calls, 0, "{method}");
        assert!(
            matches!(
                &error,
                OptimizationError::InvalidConfiguration(reason)
                    if reason.contains("unknown optimizer method")
            ),
            "{method}: {error:?}"
        );
    }
}

/// A product configuration whose refinement budget is `budget` evaluations.
/// Below 250 evaluations the initial population is the 24-member floor
/// (`product_de::initial_population`), so the batch sizes below are known.
fn cancellable_de_config(budget: i64) -> AlasConfig {
    let mut config = AlasConfig::default();
    config.optimizer.solver.method = "differential_evolution".to_owned();
    config.optimizer.solver.refinement.max_evaluations = budget;
    config.optimizer.solver.screening.max_evaluations = 8;
    config.optimizer.solver.seed = Some(11);
    config
}

/// The refinement's initial population for [`cancellable_de_config`].
const INITIAL_POPULATION: usize = 24;

#[test]
fn a_cancellation_requested_after_one_generation_stops_before_the_budget() {
    let mut optimizer = DesignOptimizer::new(cancellable_de_config(240));
    let cancel = AtomicBool::new(false);
    let mut calls = 0usize;
    let mut evaluator = |design: &DesignVector| {
        calls += 1;
        // The initial population and the first generation have been scored.
        if calls == 2 * INITIAL_POPULATION {
            cancel.store(true, Ordering::Relaxed);
        }
        synthetic_objective(design)
    };
    let result = optimizer
        .run_with_evaluator_cancellable(
            None,
            Some(&DesignVector::default()),
            &mut evaluator,
            None,
            Some(&cancel),
        )
        .expect("a cancelled search still returns a scored candidate");

    assert_eq!(result.termination, "cancelled");
    assert!(
        result.best_valid,
        "the returned candidate must be a fully scored one, not a partial trial"
    );
    assert_eq!(result.history.n_evaluations(), 2 * INITIAL_POPULATION);
}

/// A cancelled search reports the analyses it ran, not the budget it was
/// given, and the generation the request landed in does not count as
/// completed.
#[test]
fn a_cancelled_de_run_reports_the_analyses_it_executed_and_not_its_budget() {
    let mut optimizer = DesignOptimizer::new(cancellable_de_config(240));
    let cancel = AtomicBool::new(false);
    let mut calls = 0usize;
    let mut evaluator = |design: &DesignVector| {
        calls += 1;
        // Inside the second generation. The delegated evaluator is serial and
        // does not poll the flag, so that whole generation is scored; the
        // kernel observes the request at its end.
        if calls == 2 * INITIAL_POPULATION + 5 {
            cancel.store(true, Ordering::Relaxed);
        }
        synthetic_objective(design)
    };
    let result = optimizer
        .run_with_evaluator_cancellable(
            None,
            Some(&DesignVector::default()),
            &mut evaluator,
            None,
            Some(&cancel),
        )
        .expect("a cancelled search still returns a scored candidate");

    let diagnostics = result
        .search_diagnostics
        .as_ref()
        .expect("the DE path always reports diagnostics");
    assert_eq!(
        diagnostics.analysis_evaluations,
        result.history.n_evaluations(),
        "every analysis in the history belongs to the refinement"
    );
    assert!(diagnostics.analysis_evaluations < 3 * INITIAL_POPULATION);
    assert!(diagnostics.analysis_evaluations > 2 * INITIAL_POPULATION);
    assert_eq!(diagnostics.poll_iterations, 1);
    let refinement = diagnostics.stages.last().expect("a refinement stage");
    assert_eq!(refinement.termination, "cancelled");
    assert_eq!(refinement.evaluations, diagnostics.analysis_evaluations);
}

#[test]
fn the_same_seed_replays_deterministically_and_stops_on_its_budget() {
    let run = || {
        let mut optimizer = DesignOptimizer::new(cancellable_de_config(96));
        optimizer
            .run_with_evaluator_cancellable(
                None,
                Some(&DesignVector::default()),
                &mut synthetic_objective,
                None,
                None,
            )
            .expect("the synthetic objective accepts every candidate")
    };
    let first = run();
    let second = run();
    assert_eq!(first.termination, "evaluation_budget");
    assert_eq!(first.best_design, second.best_design);
    assert_eq!(first.best_cost, second.best_cost);
    // The refinement stops on its budget less the verification reserve.
    let reserve = crate::verification_reserve(&cancellable_de_config(96).optimizer.solver);
    assert_eq!(first.history.n_evaluations(), 96 - reserve.evaluations);
    assert_eq!(second.history.n_evaluations(), 96 - reserve.evaluations);
    let diagnostics = first.search_diagnostics.expect("diagnostics");
    assert_eq!(diagnostics.seed, Some(11));
    assert_eq!(diagnostics.scope, crate::SEARCH_SCOPE);
    // The baseline was the first initial member, so the same-model delta is
    // recorded and the winner is no worse than it.
    let baseline = diagnostics.baseline.expect("baseline comparison");
    assert!(baseline.winner_objective_value <= baseline.baseline_objective_value);
}

#[test]
fn a_flag_already_set_stops_the_run_before_any_analysis_and_reports_it_as_cancelled() {
    let mut optimizer = DesignOptimizer::new(cancellable_de_config(240));
    let cancel = AtomicBool::new(true);
    let mut calls = 0usize;
    let result = optimizer
        .run_with_evaluator_cancellable(
            None,
            Some(&DesignVector::default()),
            &mut |design: &DesignVector| {
                calls += 1;
                synthetic_objective(design)
            },
            None,
            Some(&cancel),
        )
        .expect("a cancelled run returns a cancelled result, not a no-feasible-design error");

    assert_eq!(calls, 0);
    assert_eq!(result.termination, crate::CANCELLED);
    assert!(!result.converged());
    assert!(!result.is_delivered_feasible());
}

#[test]
fn a_cancelled_run_is_never_feasible_even_when_its_winner_scored_valid() {
    // `best_valid` says the winner satisfied the constraints it was scored
    // against; it is not a licence to deliver a design from a search that
    // was stopped before it finished comparing candidates.
    let mut optimizer = DesignOptimizer::new(cancellable_de_config(240));
    let cancel = AtomicBool::new(false);
    let mut calls = 0usize;
    let mut evaluator = |design: &DesignVector| {
        calls += 1;
        if calls == 64 {
            cancel.store(true, Ordering::Relaxed);
        }
        synthetic_objective(design)
    };
    let result = optimizer
        .run_with_evaluator_cancellable(
            None,
            Some(&DesignVector::default()),
            &mut evaluator,
            None,
            Some(&cancel),
        )
        .expect("a cancelled search still returns a scored candidate");

    assert!(result.was_cancelled());
    assert!(result.best_valid);
    assert!(!result.is_delivered_feasible());
    assert!(!result.converged());
}

#[test]
fn a_reporting_fidelity_rejection_also_refuses_the_delivered_feasibility_verdict() {
    let mut optimizer = DesignOptimizer::new(cancellable_de_config(48));
    let mut result = optimizer
        .run_with_evaluator(
            None,
            Some(&DesignVector::default()),
            &mut synthetic_objective,
            None,
        )
        .expect("the synthetic objective accepts every candidate");
    assert!(
        result.is_delivered_feasible(),
        "an uncancelled run with a valid winner and no acceptance record is deliverable"
    );

    result.record_delivered_acceptance(crate::DeliveredAcceptance {
        verified: false,
        finalist_rejected_by: vec!["fuel_policy_unavailable".to_owned()],
        delivered_rejected_by: vec!["fuel_policy_unavailable".to_owned()],
        rejection_messages: vec!["analytic dispatch model could not be built".to_owned()],
        candidates_evaluated: 1,
        delivered_is_search_finalist: true,
        wall_time_s: 0.0,
        analyses: 1,
        baseline: None,
    });

    assert_eq!(result.termination, crate::REPORTING_FIDELITY_REJECTED);
    assert!(!result.best_valid);
    assert!(!result.is_delivered_feasible());
    assert!(!result.converged());
}
