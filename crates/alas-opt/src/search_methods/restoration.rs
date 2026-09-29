// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Bounded derivative-free feasibility restoration in the original envelope.
//! Four coordinate-poll waves cost at most 4 * (2 * free_dimensions + 1)
//! requested scores beyond the DE budget. An optional normalized-gradient
//! trial uses measured violations and must itself be evaluated before selection.
//! No constraint, bound, or reference anchor changes during this phase.

use super::{EvaluateBatch, ScoredPoint};
use crate::cancellation::{CancelPhase, CancelScope};

const MAX_POLLS: usize = 4;
const INITIAL_RADIUS: f64 = 0.05;
const MIN_RADIUS: f64 = 0.005;
const MAX_RADIUS: f64 = 0.10;

pub(crate) struct Outcome {
    pub(crate) winner: ScoredPoint,
    pub(crate) requested_scores: usize,
    pub(crate) budget: usize,
    pub(crate) iterations: usize,
    pub(crate) radius: f64,
    pub(crate) cancelled: bool,
    pub(crate) first_feasible_cost: Option<f64>,
}

pub(crate) fn run(
    bounds: &[(f64, f64)],
    initial: ScoredPoint,
    block_size: usize,
    scope: &CancelScope<'_>,
    evaluate: &mut EvaluateBatch<'_>,
) -> Outcome {
    let active: Vec<usize> = bounds
        .iter()
        .enumerate()
        .filter_map(|(index, &(lo, hi))| (hi > lo).then_some(index))
        .collect();
    let mut result = Outcome {
        winner: initial,
        requested_scores: 0,
        budget: 0,
        iterations: 0,
        radius: INITIAL_RADIUS,
        cancelled: scope.requested(),
        first_feasible_cost: None,
    };
    if result.winner.valid
        || result.cancelled
        || active.is_empty()
        || !result.winner.constraint_violation.is_finite()
    {
        return result;
    }
    result.budget = MAX_POLLS * (2 * active.len() + 1);
    for iteration in 0..MAX_POLLS {
        if scope.requested() {
            result.cancelled = true;
            break;
        }
        scope.enter(CancelPhase::FeasibilityRestoration, iteration as u64);
        let center = result.winner.clone();
        let mut points = Vec::with_capacity(2 * active.len());
        for &index in &active {
            let (lo, hi) = bounds[index];
            for direction in [-1.0, 1.0] {
                let mut values = center.values.clone();
                values[index] =
                    (values[index] + direction * result.radius * (hi - lo)).clamp(lo, hi);
                points.push(values);
            }
        }
        let samples = evaluate_wave(&points, block_size, scope, evaluate, &mut result);
        if result.cancelled {
            break;
        }
        result.iterations += 1;
        // Finish a deterministic full wave before stopping on feasibility:
        // stopping at a worker block would change the winner with worker count.
        if result.winner.valid {
            break;
        }
        if let Some(trial) = gradient_trial(bounds, &active, &center, &samples, result.radius) {
            evaluate_wave(&[trial], block_size, scope, evaluate, &mut result);
        }
        if result.cancelled || result.winner.valid {
            break;
        }
        result.radius = if result.winner.feasibility_key() < center.feasibility_key() {
            (result.radius * 1.5).min(MAX_RADIUS)
        } else {
            result.radius * 0.5
        };
        if result.radius < MIN_RADIUS {
            break;
        }
    }
    debug_assert!(result.requested_scores <= result.budget);
    result
}

fn evaluate_wave(
    points: &[Vec<f64>],
    block_size: usize,
    scope: &CancelScope<'_>,
    evaluate: &mut EvaluateBatch<'_>,
    result: &mut Outcome,
) -> Vec<ScoredPoint> {
    let mut samples = Vec::with_capacity(points.len());
    for block in points.chunks(block_size.max(1)) {
        if scope.requested() {
            result.cancelled = true;
            break;
        }
        let scored = evaluate(block);
        result.requested_scores += scored.len();
        for mut point in scored {
            if !point.cost.is_finite() || !point.constraint_violation.is_finite() {
                point.valid = false;
                point.constraint_violation = f64::INFINITY;
            }
            if point.valid && result.first_feasible_cost.is_none() {
                result.first_feasible_cost = Some(point.cost);
            }
            if point.feasibility_key() < result.winner.feasibility_key() {
                result.winner = point.clone();
            }
            samples.push(point);
        }
        if scope.requested() {
            result.cancelled = true;
            break;
        }
    }
    samples
}

fn gradient_trial(
    bounds: &[(f64, f64)],
    active: &[usize],
    center: &ScoredPoint,
    samples: &[ScoredPoint],
    radius: f64,
) -> Option<Vec<f64>> {
    if samples.len() != 2 * active.len() {
        return None;
    }
    let mut gradient = vec![0.0; bounds.len()];
    for (pair, &index) in samples.chunks_exact(2).zip(active) {
        let (lo, hi) = bounds[index];
        let delta = (pair[1].values[index] - pair[0].values[index]) / (hi - lo);
        if delta > 0.0
            && pair
                .iter()
                .all(|point| point.constraint_violation.is_finite())
        {
            gradient[index] = (pair[1].constraint_violation - pair[0].constraint_violation) / delta;
        }
    }
    let scale = gradient
        .iter()
        .map(|value| value.abs())
        .fold(0.0_f64, f64::max);
    if !scale.is_finite() || scale == 0.0 {
        return None;
    }
    let mut values = center.values.clone();
    for &index in active {
        let (lo, hi) = bounds[index];
        values[index] =
            (values[index] - radius * (hi - lo) * gradient[index] / scale).clamp(lo, hi);
    }
    (values != center.values).then_some(values)
}

#[cfg(test)]
#[path = "restoration_tests.rs"]
mod tests;
