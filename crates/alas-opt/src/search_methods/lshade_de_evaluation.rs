// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Ordered score prefixes and accounting for deadlines and exact-count replays.

use super::{EvaluateBatch, Outcome, ScoredPoint, Termination, Tier};

pub(super) fn record(outcome: &mut Outcome, point: &ScoredPoint) {
    if outcome.first_feasible_cost.is_none() && point.valid() {
        outcome.first_feasible_cost = Some(point.cost);
    }
    if point.feasibility_key() < outcome.winner.feasibility_key() {
        outcome.winner = point.clone();
    }
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
    // A replay may end inside a generation. The pre-gate is owned by the
    // evaluator, so its last prefix is dispatched one point at a time until
    // the recorded number of admitted scores has been reached. Trailing
    // pre-gate rejects belong to that same prefix and require no analysis.
    let evaluated = if remaining >= points.len() {
        evaluate_batch(points)
    } else {
        let mut scores = Vec::new();
        let mut admitted = 0;
        for point in points {
            if admitted >= remaining && admits.is_none_or(|admits| admits(point)) {
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

pub(super) fn prefix_termination(evaluations: usize, stop_after: usize) -> Termination {
    if evaluations >= stop_after {
        Termination::EvaluationBudget
    } else {
        Termination::TimeBudget
    }
}
