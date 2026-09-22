// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Focused analytic checks for the MADS progressive-barrier kernel.

use super::*;

fn point(values: &[f64], cost: f64, valid: bool, violation: f64) -> ScoredPoint {
    ScoredPoint {
        values: values.to_vec(),
        cost,
        valid,
        constraint_violation: violation,
        objectives: [cost, 0.0, 0.0],
    }
}

#[test]
fn constrained_solution_restores_feasibility_and_reduces_objective() {
    let mut evaluate = |values: &[f64]| {
        let violation = (1.0 - values[0] - values[1]).max(0.0);
        let cost = (values[0] - 0.8).powi(2) + (values[1] - 0.2).powi(2);
        point(values, cost, violation <= FEASIBILITY_TOLERANCE, violation)
    };
    let outcome = run(
        &[(0.0, 1.0), (0.0, 1.0)],
        Some(&[0.0, 0.0]),
        Settings {
            max_iterations: 80,
            max_evaluations: 1_500,
            seed: 11,
            initial_mesh_size: 0.25,
            minimum_mesh_size: 1.0e-5,
            ..Settings::default()
        },
        &mut evaluate,
        None,
    );
    assert!(outcome.outcome.winner.valid);
    assert!(outcome.outcome.winner.values.iter().sum::<f64>() >= 1.0 - 1.0e-10);
    assert!(
        outcome.outcome.winner.cost < 0.03,
        "{}",
        outcome.outcome.winner.cost
    );
}

#[test]
fn progressive_barrier_improves_quantitative_miss_without_false_feasibility() {
    let mut evaluate = |values: &[f64]| {
        // The requested requirement is outside the box, so no feasible
        // result exists; h still gives a meaningful restoration signal.
        let violation = (2.2 - values[0] - values[1]).max(0.0);
        point(values, -values[0] - values[1], false, violation)
    };
    let outcome = run(
        &[(0.0, 1.0), (0.0, 1.0)],
        Some(&[0.0, 0.0]),
        Settings {
            max_iterations: 40,
            max_evaluations: 800,
            seed: 3,
            ..Settings::default()
        },
        &mut evaluate,
        None,
    );
    assert!(!outcome.outcome.winner.valid);
    assert!(outcome.outcome.winner.constraint_violation < 2.2);
    assert!(outcome.outcome.winner.constraint_violation.is_finite());
}

#[test]
fn poll_basis_contains_signed_oblique_directions_and_is_full_rank() {
    let directions = poll_directions(4, 7, 23, true);
    assert!(directions.len() >= 8);
    assert!(directions
        .iter()
        .any(|direction| { direction.iter().filter(|&&value| value != 0).count() > 1 }));
    for direction in directions.iter().take(8) {
        let opposite = direction.iter().map(|value| -value).collect::<Vec<_>>();
        assert!(directions.contains(&opposite));
    }
    let matrix = direction_matrix(4, 3, 7, 23);
    assert!(full_rank(&matrix));
}

#[test]
fn hidden_failure_is_an_extreme_barrier_even_with_low_cost() {
    let mut evaluate = |values: &[f64]| {
        if values[0] < 0.5 {
            point(values, -1_000.0, false, FAILURE_VIOLATION_CUTOFF)
        } else {
            point(values, (values[0] - 0.8).powi(2), true, 0.0)
        }
    };
    let outcome = run(
        &[(0.0, 1.0)],
        Some(&[0.8]),
        Settings {
            max_iterations: 20,
            max_evaluations: 300,
            seed: 9,
            ..Settings::default()
        },
        &mut evaluate,
        None,
    );
    assert!(outcome.outcome.winner.valid);
    assert!(outcome.outcome.winner.values[0] >= 0.5);
}

#[test]
fn oblique_corner_solution_is_reached_with_non_axis_poll_directions() {
    let mut evaluate = |values: &[f64]| {
        let violation = (values[0] + values[1] - 1.0).max(0.0);
        let cost = (values[0] - 0.5).powi(2) + (values[1] - 0.5).powi(2);
        point(values, cost, violation <= FEASIBILITY_TOLERANCE, violation)
    };
    let outcome = run(
        &[(0.0, 1.0), (0.0, 1.0)],
        Some(&[0.9, 0.9]),
        Settings {
            max_iterations: 60,
            max_evaluations: 1_200,
            seed: 5,
            ..Settings::default()
        },
        &mut evaluate,
        None,
    );
    assert!(outcome.outcome.winner.valid);
    assert!(outcome.outcome.winner.values.iter().sum::<f64>() <= 1.0 + 1.0e-10);
    assert!(
        outcome.outcome.winner.cost < 0.05,
        "{}",
        outcome.outcome.winner.cost
    );
}

#[test]
fn narrow_hidden_region_is_found_by_the_bounded_search_step() {
    let mut evaluate = |values: &[f64]| {
        if (0.39..=0.41).contains(&values[0]) {
            point(values, (values[0] - 0.4).powi(2), true, 0.0)
        } else {
            point(values, -1_000.0, false, f64::INFINITY)
        }
    };
    let outcome = run(
        &[(0.0, 1.0)],
        Some(&[0.9]),
        Settings {
            max_iterations: 20,
            max_evaluations: 200,
            seed: 6,
            ..Settings::default()
        },
        &mut evaluate,
        None,
    );
    assert!(outcome.outcome.winner.valid);
    assert!((0.39..=0.41).contains(&outcome.outcome.winner.values[0]));
}

#[test]
fn nonfinite_scores_are_rejected_without_false_convergence() {
    let mut evaluate = |values: &[f64]| point(values, f64::NAN, true, 0.0);
    let outcome = run(
        &[(0.0, 1.0)],
        Some(&[0.5]),
        Settings {
            max_iterations: 10,
            max_evaluations: 100,
            ..Settings::default()
        },
        &mut evaluate,
        None,
    );
    assert!(!outcome.outcome.winner.valid);
    assert!(outcome.outcome.winner.cost.is_infinite());
    assert!(outcome.outcome.winner.constraint_violation.is_infinite());
}

#[test]
fn feasible_incumbent_is_preserved_against_invalid_low_cost_probes() {
    let mut evaluate = |values: &[f64]| {
        if values[0] < 0.5 {
            point(values, -1_000.0, false, f64::INFINITY)
        } else {
            point(values, 1.0 + (values[0] - 0.8).powi(2), true, 0.0)
        }
    };
    let outcome = run(
        &[(0.0, 1.0)],
        Some(&[0.8]),
        Settings {
            max_iterations: 20,
            max_evaluations: 300,
            seed: 1,
            ..Settings::default()
        },
        &mut evaluate,
        None,
    );
    assert!(outcome.outcome.winner.valid);
    assert!(outcome.outcome.winner.cost <= 1.0 + 1.0e-12);
}

#[test]
fn fixed_dimensions_are_never_perturbed() {
    let mut evaluated = Vec::new();
    let mut evaluate = |values: &[f64]| {
        evaluated.push(values.to_vec());
        point(values, values[1].powi(2), true, 0.0)
    };
    let outcome = run(
        &[(0.4, 0.4), (-1.0, 1.0)],
        Some(&[0.4, 0.8]),
        Settings {
            max_iterations: 8,
            max_evaluations: 100,
            seed: 4,
            ..Settings::default()
        },
        &mut evaluate,
        None,
    );
    assert!(outcome.outcome.winner.valid);
    assert!(evaluated
        .iter()
        .all(|values| values[0].to_bits() == 0.4_f64.to_bits()));
}

#[test]
fn seeded_runs_are_reproducible() {
    let execute = || {
        let mut trace = Vec::new();
        let mut evaluate = |values: &[f64]| {
            trace.push(values.to_vec());
            let cost = values.iter().map(|value| value.powi(2)).sum::<f64>();
            point(values, cost, true, 0.0)
        };
        let outcome = run(
            &[(-1.0, 1.0), (-2.0, 2.0), (0.0, 1.0)],
            Some(&[0.4, -0.8, 0.7]),
            Settings {
                max_iterations: 10,
                max_evaluations: 250,
                seed: 77,
                ..Settings::default()
            },
            &mut evaluate,
            None,
        );
        (outcome, trace)
    };
    let (first, first_trace) = execute();
    let (second, second_trace) = execute();
    // Everything the seed determines, compared field by field: `elapsed_s` is
    // a wall-clock measurement and is the one part of the outcome that is
    // meant to differ between two runs of the same search.
    assert_eq!(first.outcome, second.outcome);
    assert_eq!(first.termination, second.termination);
    assert_eq!(first.evaluations, second.evaluations);
    assert_eq!(first.cache_hits, second.cache_hits);
    assert_eq!(first.iterations, second.iterations);
    assert_eq!(first.first_feasible_cost, second.first_feasible_cost);
    assert_eq!(first.relative_improvement, second.relative_improvement);
    assert_eq!(first_trace, second_trace);
}

#[test]
fn a_block_evaluated_in_parallel_reaches_the_same_result_as_a_serial_one() {
    // The poll block boundary is a search setting, so a caller that spreads a
    // block over threads must evaluate exactly the same points, in the same
    // order, and reach the same winner.  This double records the blocks it is
    // handed and scores them the way a threaded caller would: all at once.
    struct Blocks {
        seen: Vec<Vec<Vec<f64>>>,
    }
    impl Evaluate for Blocks {
        fn evaluate_block(&mut self, points: &[Vec<f64>]) -> Vec<ScoredPoint> {
            self.seen.push(points.to_vec());
            points
                .iter()
                .map(|values| {
                    let cost = values
                        .iter()
                        .map(|value| (value - 0.3).powi(2))
                        .sum::<f64>();
                    point(values, cost, true, 0.0)
                })
                .collect()
        }
    }

    let settings = Settings {
        max_iterations: 12,
        max_evaluations: 400,
        seed: 31,
        ..Settings::default()
    };
    let mut batched = Blocks { seen: Vec::new() };
    let batched_outcome = run(
        &[(0.0, 1.0), (0.0, 1.0), (-1.0, 1.0)],
        Some(&[0.9, 0.1, -0.5]),
        settings,
        &mut batched,
        None,
    );

    let mut serial_trace = Vec::new();
    let mut serial = |values: &[f64]| {
        serial_trace.push(values.to_vec());
        let cost = values
            .iter()
            .map(|value| (value - 0.3).powi(2))
            .sum::<f64>();
        point(values, cost, true, 0.0)
    };
    let serial_outcome = run(
        &[(0.0, 1.0), (0.0, 1.0), (-1.0, 1.0)],
        Some(&[0.9, 0.1, -0.5]),
        settings,
        &mut serial,
        None,
    );

    assert_eq!(batched_outcome.outcome, serial_outcome.outcome);
    assert_eq!(batched_outcome.termination, serial_outcome.termination);
    assert_eq!(batched_outcome.evaluations, serial_outcome.evaluations);
    let batched_trace: Vec<Vec<f64>> = batched.seen.into_iter().flatten().collect();
    assert_eq!(batched_trace, serial_trace);
    // The blocks really were blocks, not one point at a time.
    assert!(batched_trace.len() > 1);
}

#[test]
fn a_repeated_mesh_node_is_served_from_the_cache_rather_than_re_analysed() {
    let analyses = std::cell::Cell::new(0usize);
    let mut evaluate = |values: &[f64]| {
        analyses.set(analyses.get() + 1);
        point(values, values[0].powi(2), true, 0.0)
    };
    let outcome = run(
        &[(-1.0, 1.0), (-1.0, 1.0)],
        Some(&[0.5, 0.5]),
        Settings {
            max_iterations: 25,
            max_evaluations: 5_000,
            seed: 13,
            ..Settings::default()
        },
        &mut evaluate,
        None,
    );
    assert_eq!(outcome.evaluations, analyses.get());
    // The retried successful direction and the two poll centres put the
    // search back on nodes it has already paid for; the cache is what stops
    // those from being analysed twice.
    assert!(outcome.cache_hits > 0);
}

#[test]
fn convergence_needs_feasibility_a_small_mesh_and_a_real_improvement() {
    // (1) A run that starts at the optimum has nothing to improve on, so it
    // reaches the mesh floor and reports that, not convergence.
    let mut at_optimum = |values: &[f64]| point(values, values[0].powi(2), true, 0.0);
    let no_improvement = run(
        &[(-1.0, 1.0)],
        Some(&[0.0]),
        Settings {
            max_iterations: 40,
            max_evaluations: 2_000,
            seed: 2,
            minimum_mesh_size: 1.0e-6,
            ..Settings::default()
        },
        &mut at_optimum,
        None,
    );
    assert!(!no_improvement.termination.is_converged());
    assert_eq!(no_improvement.termination, TerminationReason::MeshLimit);

    // (2) A run with no feasible point anywhere cannot converge either, no
    // matter how far the mesh contracts.
    let mut never_feasible =
        |values: &[f64]| point(values, -values[0], false, 1.0 + values[0].abs());
    let infeasible = run(
        &[(0.0, 1.0)],
        Some(&[0.5]),
        Settings {
            max_iterations: 40,
            max_evaluations: 2_000,
            seed: 2,
            ..Settings::default()
        },
        &mut never_feasible,
        None,
    );
    assert!(!infeasible.termination.is_converged());
    assert!(infeasible.relative_improvement.is_none());

    // (3) A feasible start with room to improve does converge, and reports
    // the improvement it converged on.
    let mut improvable = |values: &[f64]| point(values, (values[0] - 0.25).powi(2), true, 0.0);
    let converged = run(
        &[(-1.0, 1.0)],
        Some(&[0.9]),
        Settings {
            max_iterations: 40,
            max_evaluations: 2_000,
            seed: 2,
            ..Settings::default()
        },
        &mut improvable,
        None,
    );
    assert_eq!(converged.termination, TerminationReason::Converged);
    assert!(converged.termination.is_converged());
    assert!(converged.outcome.winner.valid);
    assert!(converged.relative_improvement.unwrap_or(0.0) > 0.0);
}

#[test]
fn a_watchdog_stop_is_reported_as_not_converged() {
    // The watchdog is a safety limit, so a run that hits it must not be able
    // to claim the convergence the remaining polls would have had to earn.
    let mut slow = |values: &[f64]| {
        std::thread::sleep(std::time::Duration::from_millis(2));
        point(values, (values[0] - 0.25).powi(2), true, 0.0)
    };
    let outcome = run(
        &[(-1.0, 1.0), (-1.0, 1.0)],
        Some(&[0.9, 0.9]),
        Settings {
            max_iterations: 200,
            max_evaluations: 100_000,
            seed: 2,
            watchdog: Some(std::time::Duration::from_millis(30)),
            ..Settings::default()
        },
        &mut slow,
        None,
    );
    assert_eq!(outcome.termination, TerminationReason::Watchdog);
    assert_eq!(outcome.termination.as_str(), "watchdog");
    assert!(!outcome.termination.is_converged());
}

#[test]
fn dropping_the_pair_diagonals_leaves_a_positive_spanning_poll() {
    // The sixteen-variable product space polls without the adjacent
    // diagonals.  What must survive is the property the theory needs: the
    // remaining directions still positively span the space, which for a
    // signed set means every coordinate is reachable both ways.
    let directions = poll_directions(16, 3, 19, false);
    let enriched = poll_directions(16, 3, 19, true);
    assert!(directions.len() < enriched.len());
    for direction in &directions {
        let opposite: Vec<i64> = direction.iter().map(|value| -value).collect();
        assert!(directions.contains(&opposite));
    }
    for index in 0..16 {
        let mut unit = vec![0_i64; 16];
        unit[index] = 1;
        let negative: Vec<i64> = unit.iter().map(|value| -value).collect();
        assert!(directions.contains(&unit), "coordinate {index}");
        assert!(directions.contains(&negative), "coordinate {index}");
    }
    assert!(full_rank(&direction_matrix(16, 3, 3, 19)));
}

#[test]
fn evaluation_budget_is_strict_and_malformed_inputs_do_not_call_back() {
    let evaluations = std::cell::Cell::new(0usize);
    let mut evaluate = |values: &[f64]| {
        evaluations.set(evaluations.get() + 1);
        point(values, 0.0, true, 0.0)
    };
    let outcome = run(
        &[(0.0, 1.0), (0.0, 1.0)],
        None,
        Settings {
            max_iterations: 20,
            max_evaluations: 3,
            ..Settings::default()
        },
        &mut evaluate,
        None,
    );
    assert!(evaluations.get() <= 3);
    assert!(outcome.outcome.winner.valid);

    let before = evaluations.get();
    let malformed = run(
        &[(f64::NAN, 1.0)],
        Some(&[0.0]),
        Settings::default(),
        &mut evaluate,
        None,
    );
    assert_eq!(evaluations.get(), before);
    assert!(!malformed.outcome.winner.valid);
    assert!(malformed.outcome.winner.constraint_violation.is_infinite());
}

#[test]
fn no_feasible_candidate_cannot_report_false_feasibility() {
    let mut evaluate = |values: &[f64]| point(values, -1.0, false, 1.0);
    let outcome = run(
        &[(0.0, 1.0)],
        Some(&[0.5]),
        Settings {
            max_iterations: 10,
            max_evaluations: 100,
            ..Settings::default()
        },
        &mut evaluate,
        None,
    );
    assert!(!outcome.outcome.winner.valid);
    assert!(outcome.outcome.winner.constraint_violation > 0.0);
}

#[test]
fn termination_reason_is_durable_and_has_stable_label() {
    let mut evaluate = |values: &[f64]| point(values, 0.0, true, 0.0);
    let budget = run(
        &[(0.0, 1.0)],
        Some(&[0.5]),
        Settings {
            max_iterations: 10,
            max_evaluations: 1,
            ..Settings::default()
        },
        &mut evaluate,
        None,
    );
    assert_eq!(budget.termination, TerminationReason::EvaluationBudget);
    assert_eq!(budget.termination.as_str(), "evaluation_budget");

    let invalid = run(
        &[(1.0, 0.0)],
        Some(&[0.5]),
        Settings::default(),
        &mut evaluate,
        None,
    );
    assert_eq!(invalid.termination, TerminationReason::InvalidInput);
    assert_eq!(invalid.termination.as_str(), "invalid_input");

    let fixed = run(
        &[(0.4, 0.4)],
        Some(&[0.4]),
        Settings {
            max_iterations: 10,
            max_evaluations: 20,
            ..Settings::default()
        },
        &mut evaluate,
        None,
    );
    assert_eq!(fixed.termination, TerminationReason::FixedBounds);
}

#[test]
fn a_cancellation_flag_stops_the_poll_and_is_never_reported_as_convergence() {
    // A well-behaved problem with a budget the search would otherwise spend
    // in full: the only thing that stops it here is the flag.
    let generous = Settings {
        max_iterations: 200,
        max_evaluations: 4_000,
        seed: 3,
        initial_mesh_size: 0.25,
        minimum_mesh_size: 1.0e-8,
        convergence_mesh_size: 1.0e-8,
        ..Settings::default()
    };
    let quadratic = |values: &[f64]| {
        let cost = (values[0] - 0.6).powi(2) + (values[1] - 0.3).powi(2);
        point(values, cost, true, 0.0)
    };

    // Set at the first boundary the flag can be read at, which is before the
    // search phase's first block.
    let cancel = std::sync::atomic::AtomicBool::new(true);
    let mut evaluate = quadratic;
    let cancelled = run_cancellable(
        &[(0.0, 1.0), (0.0, 1.0)],
        Some(&[0.0, 0.0]),
        generous,
        Some(&cancel),
        &mut evaluate,
        None,
    );
    assert_eq!(cancelled.termination, TerminationReason::Cancelled);
    assert_eq!(cancelled.termination.as_str(), "cancelled");
    assert!(!cancelled.termination.is_converged());
    assert!(cancelled.termination.is_cancelled());
    assert!(
        cancelled.outcome.winner.values.len() == 2,
        "a cancelled run still returns the incumbent it had scored"
    );

    // Raised mid-run instead: the search must stop well short of the budget
    // it would otherwise have spent, and must not claim the budget as its
    // reason for stopping.
    let midway = std::sync::atomic::AtomicBool::new(false);
    let mut calls = 0usize;
    let mut counted = |values: &[f64]| {
        calls += 1;
        if calls == 40 {
            midway.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        quadratic(values)
    };
    let stopped = run_cancellable(
        &[(0.0, 1.0), (0.0, 1.0)],
        Some(&[0.0, 0.0]),
        generous,
        Some(&midway),
        &mut counted,
        None,
    );
    assert_eq!(stopped.termination, TerminationReason::Cancelled);
    assert!(!stopped.termination.is_converged());
    assert!(
        stopped.evaluations < generous.max_evaluations,
        "cancellation must stop short of the {} analysis budget: {} evaluations",
        generous.max_evaluations,
        stopped.evaluations
    );

    // The same settings without a flag are unaffected: cancellation support
    // must not change an ordinary run.
    let mut plain = quadratic;
    let uncancelled = run_cancellable(
        &[(0.0, 1.0), (0.0, 1.0)],
        Some(&[0.0, 0.0]),
        generous,
        None,
        &mut plain,
        None,
    );
    assert!(!uncancelled.termination.is_cancelled());
    assert!(uncancelled.outcome.winner.valid);
}
