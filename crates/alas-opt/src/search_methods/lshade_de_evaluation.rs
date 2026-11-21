// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Ordered score prefixes and accounting for deadlines and exact-count replays.

use super::{EvaluateBatch, Outcome, ScoredPoint, Termination, Tier};

/// The winner rule shared by every search stage: the first feasible cost is
/// the first valid point seen, and the winner is the strict
/// [`ScoredPoint::feasibility_key`] minimum (the earlier point wins ties).
pub(crate) fn track_winner(
    first_feasible_cost: &mut Option<f64>,
    winner: &mut ScoredPoint,
    point: &ScoredPoint,
) {
    if first_feasible_cost.is_none() && point.valid() {
        *first_feasible_cost = Some(point.cost);
    }
    if point.feasibility_key() < winner.feasibility_key() {
        *winner = point.clone();
    }
}

pub(super) fn record(outcome: &mut Outcome, point: &ScoredPoint) {
    track_winner(&mut outcome.first_feasible_cost, &mut outcome.winner, point);
}

/// Count `scores` against the budget or, rejected by the pre-gate, against
/// the rejection cap.
pub(super) fn tally(outcome: &mut Outcome, scores: &[ScoredPoint]) {
    let rejected = scores
        .iter()
        .filter(|point| point.tier == Tier::PreGateFailed)
        .count();
    outcome.rejected += rejected;
    outcome.evaluations += scores.len() - rejected;
}

pub(super) fn feasible_fraction(scored: &[ScoredPoint]) -> f64 {
    if scored.is_empty() {
        0.0
    } else {
        scored.iter().filter(|point| point.valid()).count() as f64 / scored.len() as f64
    }
}

pub(crate) fn evaluate(
    points: &[Vec<f64>],
    remaining: usize,
    admits: Option<&crate::search_methods::Admission<'_>>,
    evaluate_batch: &mut EvaluateBatch<'_>,
) -> Vec<ScoredPoint> {
    // A replay may end inside a generation: its last prefix holds the
    // recorded number of admitted scores. Trailing pre-gate rejects belong to
    // that same prefix and require no analysis. With the admission check the
    // prefix is known before any analysis and goes out as one batch, so a
    // stage's last generation runs on every lane rather than one candidate
    // at a time (the same points, in the same order, with the same scores).
    // Without it the evaluator's pre-gate decides, one point at a time.
    let evaluated = if remaining >= points.len() {
        evaluate_batch(points)
    } else if let Some(admits) = admits {
        let end = admitted_prefix(points, remaining, admits);
        if end == 0 {
            Vec::new()
        } else {
            evaluate_batch(&points[..end])
        }
    } else {
        let mut scores = Vec::new();
        let mut admitted = 0;
        for point in points {
            if admitted >= remaining {
                break;
            }
            let answer = evaluate_batch(std::slice::from_ref(point));
            if answer.is_empty() {
                break;
            }
            admitted += usize::from(answer[0].tier != Tier::PreGateFailed);
            scores.extend(answer);
        }
        scores
    };
    let mut scores: Vec<ScoredPoint> = evaluated.into_iter().map(ScoredPoint::sanitized).collect();
    scores.truncate(points.len());
    scores
}

/// Length of the prefix of `points` that holds `remaining` admitted points
/// and the pre-gate rejects that follow the last of them, up to the next
/// admitted point.
fn admitted_prefix(
    points: &[Vec<f64>],
    remaining: usize,
    admits: &crate::search_methods::Admission<'_>,
) -> usize {
    let mut admitted = 0;
    for (index, point) in points.iter().enumerate() {
        if admits(point) {
            if admitted >= remaining {
                return index;
            }
            admitted += 1;
        }
    }
    points.len()
}

pub(super) fn prefix_termination(evaluations: usize, stop_after: usize) -> Termination {
    if evaluations >= stop_after {
        Termination::EvaluationBudget
    } else {
        Termination::TimeBudget
    }
}
