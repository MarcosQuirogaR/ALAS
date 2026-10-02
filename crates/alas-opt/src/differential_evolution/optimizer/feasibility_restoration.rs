// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Run restoration inside what is left of the refinement budget, only when
//! the refinement found no feasible design.

use std::time::{Duration, Instant};

use super::batch::BatchEvaluator;
use super::{CancelScope, RestorationDiagnostics, SearchObjective};
use crate::search_methods::{product_de, restoration};

/// `outcome` after restoration, with at most `max_scores` more requested
/// scores, no new poll wave once `stop_after` more scores passed the
/// pre-gate (a replay stop) or `stage_started + time_limit` has passed.
pub(super) fn run<E: SearchObjective + ?Sized>(
    bounds: &[(f64, f64)],
    outcome: &mut product_de::Outcome,
    (max_scores, stop_after): (usize, usize),
    (stage_started, time_limit): (Instant, Duration),
    scope: &CancelScope<'_>,
    evaluator: &mut BatchEvaluator<'_, E>,
) -> Option<RestorationDiagnostics> {
    if max_scores == 0
        || outcome.winner.valid()
        || outcome.termination == product_de::Termination::Cancelled
        || !outcome.winner.constraint_violation.is_finite()
        || !bounds.iter().any(|&(lo, hi)| hi > lo)
    {
        return None;
    }
    let started = Instant::now();
    let before = evaluator.analyses();
    let hits = evaluator.cache.hits;
    let initial_violation = outcome.winner.constraint_violation;
    let repaired = restoration::run(
        bounds,
        outcome.winner.clone(),
        restoration::Limits {
            max_scores,
            stop_after,
            started: stage_started,
            time_limit: Some(time_limit),
        },
        scope,
        &mut |points| evaluator.evaluate_block(points),
    );
    let diagnostics = RestorationDiagnostics {
        evaluation_budget: repaired.budget,
        analysis_evaluations: evaluator.analyses() - before,
        cache_hits: evaluator.cache.hits - hits,
        iterations: repaired.iterations,
        wall_time_s: started.elapsed().as_secs_f64(),
        initial_violation,
        final_violation: repaired.winner.constraint_violation,
        final_radius_normalized: repaired.radius,
        feasible: repaired.winner.valid(),
    };
    outcome.winner = repaired.winner;
    if repaired.cancelled {
        outcome.termination = product_de::Termination::Cancelled;
    }
    outcome.evaluations += repaired.requested_scores - repaired.rejected_scores;
    outcome.rejected += repaired.rejected_scores;
    outcome.first_feasible_cost = outcome.first_feasible_cost.or(repaired.first_feasible_cost);
    Some(diagnostics)
}
