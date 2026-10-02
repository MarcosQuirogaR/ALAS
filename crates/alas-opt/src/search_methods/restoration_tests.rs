// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;
use crate::search_methods::Tier;
use std::sync::atomic::{AtomicBool, Ordering};

fn limits(max_scores: usize) -> Limits {
    Limits {
        max_scores,
        stop_after: usize::MAX,
        started: Instant::now(),
        time_limit: None,
    }
}

fn coupled(values: &[f64]) -> ScoredPoint {
    let x = values[0];
    let y = (values[1] - 1000.0) / 1000.0;
    let violation = (1.15 - x - y).max(0.0) + ((x - y).abs() - 0.02).max(0.0);
    let valid = violation == 0.0;
    ScoredPoint {
        values: values.to_vec(),
        cost: if valid { 100.0 } else { -1e9 },
        tier: if valid {
            Tier::Feasible
        } else {
            Tier::ClosedInfeasible
        },
        constraint_violation: violation,
        objectives: [0.0; 3],
    }
}

#[test]
fn evaluated_gradient_repairs_coupled_constraints_without_changing_original_bounds() {
    let bounds = [(0.0, 1.0), (1000.0, 2000.0), (7.0, 7.0)];
    let initial = coupled(&[0.5, 1500.0, 7.0]);
    let mut trajectories = Vec::new();
    for _ in 0..2 {
        let mut visited = Vec::new();
        let result = run(
            &bounds,
            initial.clone(),
            limits(usize::MAX),
            &CancelScope::attach(None),
            &mut |points| {
                visited.extend_from_slice(points);
                points.iter().map(|values| coupled(values)).collect()
            },
        );
        assert!(result.winner.valid());
        assert_eq!(
            result.winner.cost, 100.0,
            "infeasible negative objective cannot win"
        );
        assert_eq!(result.winner.values[2], 7.0);
        assert_eq!(result.iterations, 2);
        assert_eq!(result.requested_scores, visited.len());
        assert!(visited.iter().all(|values| values
            .iter()
            .zip(bounds)
            .all(|(&x, (lo, hi))| x >= lo && x <= hi)));
        assert!(result.requested_scores <= result.budget);
        trajectories.push((visited, result.winner));
    }
    assert_eq!(
        trajectories[0], trajectories[1],
        "worker count cannot change a poll wave"
    );
}

#[test]
fn restoration_budget_is_explicit_and_unreachable_feasibility_is_not_success() {
    let initial = ScoredPoint {
        values: vec![0.5, 0.5],
        cost: 0.0,
        tier: Tier::ClosedInfeasible,
        constraint_violation: 1.0,
        objectives: [0.0; 3],
    };
    let result = run(
        &[(0.0, 1.0); 2],
        initial.clone(),
        limits(usize::MAX),
        &CancelScope::attach(None),
        &mut |points| {
            points
                .iter()
                .map(|values| ScoredPoint {
                    values: values.clone(),
                    ..initial.clone()
                })
                .collect()
        },
    );
    assert_eq!(result.budget, 4 * (2 * 2 + 1));
    assert_eq!(result.iterations, 4);
    assert_eq!(result.requested_scores, 16);
    assert!(!result.winner.valid());
    assert_eq!(result.winner.constraint_violation, 1.0);
    assert!(result.radius < MIN_RADIUS);
}

#[test]
fn cancellation_in_a_final_poll_block_is_retained_with_completed_scores() {
    let cancel = AtomicBool::new(false);
    let initial = coupled(&[0.5, 1500.0, 7.0]);
    let result = run(
        &[(0.0, 1.0), (1000.0, 2000.0), (7.0, 7.0)],
        initial,
        limits(usize::MAX),
        &CancelScope::attach(Some(&cancel)),
        &mut |points| {
            cancel.store(true, Ordering::Release);
            points.iter().map(|values| coupled(values)).collect()
        },
    );
    assert!(result.cancelled);
    assert_eq!(result.requested_scores, 4);
    assert_eq!(result.iterations, 0);
    assert!(!result.winner.valid());
}

#[test]
fn already_feasible_locked_or_cancelled_requests_do_no_restoration_work() {
    let feasible = coupled(&[0.7, 1700.0, 7.0]);
    let invalid = coupled(&[0.5, 1500.0, 7.0]);
    let cancel = AtomicBool::new(true);
    for (initial, bounds, flag) in [
        (
            feasible,
            vec![(0.0, 1.0), (1000.0, 2000.0), (7.0, 7.0)],
            None,
        ),
        (
            invalid.clone(),
            vec![(0.5, 0.5), (1500.0, 1500.0), (7.0, 7.0)],
            None,
        ),
        (
            invalid,
            vec![(0.0, 1.0), (1000.0, 2000.0), (7.0, 7.0)],
            Some(&cancel),
        ),
    ] {
        let result = run(
            &bounds,
            initial,
            limits(usize::MAX),
            &CancelScope::attach(flag),
            &mut |_| panic!("a short-circuited request must not evaluate"),
        );
        assert_eq!(result.requested_scores, 0);
        assert_eq!(result.budget, 0);
    }
}

#[test]
fn a_nonfinite_analysis_cannot_become_a_restored_feasible_candidate() {
    let initial = coupled(&[0.5, 1500.0, 7.0]);
    let result = run(
        &[(0.0, 1.0), (1000.0, 2000.0), (7.0, 7.0)],
        initial.clone(),
        limits(usize::MAX),
        &CancelScope::attach(None),
        &mut |points| {
            points
                .iter()
                .map(|values| ScoredPoint {
                    values: values.clone(),
                    tier: Tier::Feasible,
                    cost: f64::NAN,
                    constraint_violation: 0.0,
                    objectives: [0.0; 3],
                })
                .collect()
        },
    );
    assert!(!result.winner.valid());
    assert_eq!(result.winner, initial);
}

#[test]
fn restoration_never_exceeds_its_score_budget_and_never_starts_a_wave_that_would_not_fit() {
    let initial = ScoredPoint {
        values: vec![0.5, 0.5],
        cost: 0.0,
        tier: Tier::ClosedInfeasible,
        constraint_violation: 1.0,
        objectives: [0.0; 3],
    };
    for max_scores in [0, 3, 4, 5, 9, 12] {
        let mut requested = 0;
        let result = run(
            &[(0.0, 1.0); 2],
            initial.clone(),
            limits(max_scores),
            &CancelScope::attach(None),
            &mut |points| {
                requested += points.len();
                points
                    .iter()
                    .map(|values| ScoredPoint {
                        values: values.clone(),
                        constraint_violation: 1.0 - 0.1 * values[0],
                        ..initial.clone()
                    })
                    .collect()
            },
        );
        assert!(requested <= max_scores, "{requested} > {max_scores}");
        assert_eq!(result.requested_scores, requested);
        // A wave is four points; fewer than four remaining never starts one.
        assert_eq!(result.iterations == 0, max_scores < 4, "{max_scores}");
    }
}
