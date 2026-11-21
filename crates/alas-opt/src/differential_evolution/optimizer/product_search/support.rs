// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A winner must be supported by its neighbourhood.
//!
//! The objective of a design search is a physical quantity and varies
//! smoothly with the design. A candidate whose objective lies far below
//! every design around it is not a better aircraft: it is a defect of the
//! analysis at that one point, and a search that ranks by the objective
//! alone will find such points and deliver them. Measured on this product
//! (fourteen searches of 418 to 790 analyses): four delivered a
//! design whose block fuel was 0.8 to 3.6 % below all of its neighbours,
//! each an isolated lattice defect 0.03 m of wing position wide.
//!
//! After the refinement kernel the best feasible candidate is therefore
//! polled: [`NEIGHBOURS`] designs at a normalized distance of
//! [`NEIGHBOUR_RADIUS`] around it are analysed with the same model, and any
//! member of that group (the candidate or a neighbour) whose objective lies
//! more than [`SUPPORT_TOLERANCE`] below the group median is flagged
//! `isolated_optimum` and can be neither the winner nor a finalist. The
//! poll repeats on the next best candidate, at most [`MAX_ROUNDS`] times.
//! A poll of fewer than three analysed neighbours cannot judge and accepts.
//!
//! This is the poll step of a pattern search used as a robustness test
//! rather than as a descent (C. Audet and J. E. Dennis, "Analysis of
//! Generalized Pattern Searches," SIAM J. Optim. 13(3), 2003, DOI
//! 10.1137/S1052623400378742, for the poll; the acceptance rule is an
//! engineering choice of this product).

use super::*;
use crate::search_methods::rng::SearchRng;
use crate::search_methods::{normalized_distance, Tier};

/// Designs analysed around a candidate per poll (about half a lane wave).
pub(super) const NEIGHBOURS: usize = 8;
/// Normalized distance of a neighbour from the candidate: 1 % of the box.
const NEIGHBOUR_RADIUS: f64 = 0.01;
/// Relative objective deficit against the group median beyond which a
/// design is isolated: 0.3 %. A smooth objective moves by at most about
/// 0.1 % over [`NEIGHBOUR_RADIUS`] here (5 to 10 % across the whole box),
/// and the smallest defect measured was 0.8 %.
const SUPPORT_TOLERANCE: f64 = 3.0e-3;
/// Polls a run may spend; each flags at least one design or ends the check.
pub(super) const MAX_ROUNDS: usize = 3;

/// The history reason of a design the poll flagged.
pub(crate) const ISOLATED_OPTIMUM: &str = "isolated_optimum";

/// What the support check did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Support {
    /// Scores that passed the pre-gate (they count against the budget).
    pub(super) evaluations: usize,
    /// Designs flagged as isolated.
    pub(super) flagged: usize,
    /// Polls run.
    pub(super) rounds: usize,
}

/// `count` neighbours of `centre` at [`NEIGHBOUR_RADIUS`], each a uniform
/// random direction over the free coordinates, clamped to the box.
fn neighbours(
    centre: &[f64],
    bounds: &[(f64, f64)],
    count: usize,
    rng: &mut SearchRng,
) -> Vec<Vec<f64>> {
    let free: Vec<usize> = (0..bounds.len())
        .filter(|&index| bounds[index].1 > bounds[index].0)
        .collect();
    (0..count)
        .map(|_| {
            // A direction uniform on the sphere: normalized normal deviates.
            let direction: Vec<f64> = free
                .iter()
                .map(|_| {
                    let radius = (-2.0 * (1.0 - rng.unit()).ln()).sqrt();
                    radius * (2.0 * std::f64::consts::PI * rng.unit()).cos()
                })
                .collect();
            let norm = direction
                .iter()
                .map(|value| value * value)
                .sum::<f64>()
                .sqrt()
                .max(1.0e-12);
            let scale = NEIGHBOUR_RADIUS * (free.len() as f64).sqrt() / norm;
            let mut point = centre.to_vec();
            for (&index, step) in free.iter().zip(&direction) {
                let (lower, upper) = bounds[index];
                point[index] = (centre[index] + scale * step * (upper - lower)).clamp(lower, upper);
            }
            point
        })
        .collect()
}

/// Poll the best feasible members of `scored` until one is supported, flag
/// the isolated ones in the objective's history, and leave in
/// `outcome.winner` the best feasible design that is not flagged. `budget`
/// is the number of further scores the stage may request.
pub(super) fn check<E: SearchObjective + ?Sized>(
    run: &Run<'_>,
    evaluator: &mut BatchEvaluator<'_, E>,
    scored: &mut Vec<ScoredPoint>,
    outcome: &mut product_de::Outcome,
    mut budget: usize,
) -> Support {
    let mut support = Support::default();
    let mut rng = SearchRng::stream(run.seed, 5);
    let mut flagged: Vec<Vec<f64>> = Vec::new();
    let mut polled: Vec<Vec<f64>> = Vec::new();
    let is_flagged = |flagged: &[Vec<f64>], point: &ScoredPoint| flagged.contains(&point.values);
    for _ in 0..MAX_ROUNDS {
        let candidate = scored
            .iter()
            .filter(|point| point.valid() && !is_flagged(&flagged, point))
            .min_by_key(|point| point.feasibility_key())
            .cloned();
        let Some(candidate) = candidate else {
            break;
        };
        // The baseline is the aircraft the run started from, not a claim of
        // the search; a design already inside a supported poll needs none.
        if run.baseline.as_ref() == Some(&candidate.values)
            || budget < NEIGHBOURS
            || polled.iter().any(|centre| {
                normalized_distance(centre, &candidate.values, run.bounds) <= 1.5 * NEIGHBOUR_RADIUS
            })
        {
            break;
        }
        let points = neighbours(&candidate.values, run.bounds, NEIGHBOURS, &mut rng);
        let scores = evaluator.evaluate_block(&points);
        let analysed = scores
            .iter()
            .filter(|point| point.tier != Tier::PreGateFailed)
            .count();
        support.evaluations += analysed;
        support.rounds += 1;
        budget = budget.saturating_sub(analysed);
        let mut group: Vec<ScoredPoint> = scores
            .iter()
            .filter(|point| {
                matches!(point.tier, Tier::Feasible | Tier::ClosedInfeasible)
                    && point.objectives[0].is_finite()
            })
            .cloned()
            .collect();
        scored.extend(scores);
        if group.len() < 3 {
            break;
        }
        let mut objectives: Vec<f64> = group.iter().map(|point| point.objectives[0]).collect();
        objectives.sort_by(f64::total_cmp);
        let median = objectives[objectives.len() / 2];
        let floor = median - SUPPORT_TOLERANCE * median.abs();
        group.push(candidate.clone());
        let isolated: Vec<Vec<f64>> = group
            .iter()
            .filter(|point| point.objectives[0] < floor)
            .map(|point| point.values.clone())
            .collect();
        let candidate_isolated = isolated.contains(&candidate.values);
        for values in isolated {
            if let Some(row) = evaluator.cache.row(&values) {
                evaluator.objective.flag_row(row, ISOLATED_OPTIMUM);
            }
            flagged.push(values);
            support.flagged += 1;
        }
        if !candidate_isolated {
            polled.push(candidate.values);
            // A neighbour may now lead; it lies inside this supported poll.
            continue;
        }
    }
    if support.flagged > 0 {
        scored.retain(|point| !flagged.contains(&point.values));
        let mut winner = product_de::unevaluated(run.baseline.as_deref().unwrap_or(&[]));
        for point in scored.iter() {
            if point.feasibility_key() < winner.feasibility_key() {
                winner = point.clone();
            }
        }
        outcome.winner = winner;
    } else if let Some(best) = scored.iter().min_by_key(|point| point.feasibility_key()) {
        if best.feasibility_key() < outcome.winner.feasibility_key() {
            outcome.winner = best.clone();
        }
    }
    outcome.evaluations += support.evaluations;
    support
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbours_lie_at_the_radius_inside_the_box_and_move_free_coordinates_only() {
        let bounds = [(0.0, 10.0), (3.0, 3.0), (-1.0, 1.0), (100.0, 200.0)];
        let centre = [5.0, 3.0, 0.0, 150.0];
        let mut rng = SearchRng::seed(4);
        let points = neighbours(&centre, &bounds, 16, &mut rng);
        assert_eq!(points.len(), 16);
        for point in &points {
            assert_eq!(point[1], 3.0);
            let distance = normalized_distance(point, &centre, &bounds);
            assert!((distance - NEIGHBOUR_RADIUS).abs() < 1e-12, "{distance}");
        }
        // At a corner the clamp shortens the step; no coordinate leaves the box.
        let corner = [10.0, 3.0, 1.0, 200.0];
        for point in neighbours(&corner, &bounds, 16, &mut rng) {
            for (value, (lower, upper)) in point.iter().zip(&bounds) {
                assert!(lower <= value && value <= upper);
            }
            assert!(normalized_distance(&point, &corner, &bounds) <= NEIGHBOUR_RADIUS + 1e-12);
        }
    }
}
