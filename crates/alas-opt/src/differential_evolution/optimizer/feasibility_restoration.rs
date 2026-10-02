// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Run restoration inside what is left of the refinement budget, only when
//! the refinement found no feasible design.

use super::batch::BatchEvaluator;
use super::{CancelScope, RestorationDiagnostics, SearchObjective};
use crate::search_methods::{product_de, restoration};

/// `outcome` after restoration under `limits`.
pub(super) fn run<E: SearchObjective + ?Sized>(
    bounds: &[(f64, f64)],
    outcome: &mut product_de::Outcome,
    limits: restoration::Limits,
    scope: &CancelScope<'_>,
    evaluator: &mut BatchEvaluator<'_, E>,
) -> Option<RestorationDiagnostics> {
    if limits.max_scores == 0
        || outcome.winner.valid()
        || outcome.termination == product_de::Termination::Cancelled
        || !outcome.winner.constraint_violation.is_finite()
        || !bounds.iter().any(|&(lo, hi)| hi > lo)
    {
        return None;
    }
    let started = std::time::Instant::now();
    let before = evaluator.analyses();
    let hits = evaluator.cache.hits;
    let initial_violation = outcome.winner.constraint_violation;
    let repaired = restoration::run(
        bounds,
        outcome.winner.clone(),
        limits,
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
