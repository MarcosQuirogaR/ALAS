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
    let directions = poll_directions(4, 7, 23);
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
    assert_eq!(first, second);
    assert_eq!(first_trace, second_trace);
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
