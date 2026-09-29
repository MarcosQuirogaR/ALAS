// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Account for the optional restoration phase without creating a new evaluator.

use super::scan::BatchEvaluator;
use super::{CancelScope, RestorationDiagnostics, SearchObjective};
use crate::search_methods::{product_de, restoration};
use std::time::Instant;

pub(super) fn run<E: SearchObjective + ?Sized>(
    bounds: &[(f64, f64)],
    outcome: &mut product_de::Outcome,
    generations: usize,
    scope: &CancelScope<'_>,
    evaluator: &mut BatchEvaluator<'_, E>,
) -> Option<RestorationDiagnostics> {
    if generations == 0
        || outcome.winner.valid
        || outcome.cancelled
        || !outcome.winner.constraint_violation.is_finite()
        || !bounds.iter().any(|&(lo, hi)| hi > lo)
    {
        return None;
    }
    let started = Instant::now();
    let before = evaluator.objective.history().n_evaluations();
    let hits = evaluator.cache.hits;
    let initial_violation = outcome.winner.constraint_violation;
    let repaired = restoration::run(
        bounds,
        outcome.winner.clone(),
        evaluator.workers,
        scope,
        &mut |points| evaluator.evaluate_block(points),
    );
    let diagnostics = RestorationDiagnostics {
        evaluation_budget: repaired.budget,
        analysis_evaluations: evaluator.objective.history().n_evaluations() - before,
        cache_hits: evaluator.cache.hits - hits,
        iterations: repaired.iterations,
        wall_time_s: started.elapsed().as_secs_f64(),
        initial_violation,
        final_violation: repaired.winner.constraint_violation,
        final_radius_normalized: repaired.radius,
        feasible: repaired.winner.valid,
    };
    outcome.winner = repaired.winner;
    outcome.cancelled |= repaired.cancelled;
    outcome.converged = false;
    outcome.evaluations += repaired.requested_scores;
    outcome.first_feasible_cost = outcome.first_feasible_cost.or(repaired.first_feasible_cost);
    outcome.relative_improvement = outcome
        .first_feasible_cost
        .map(|first| (first - outcome.winner.cost) / first.abs().max(1e-12));
    Some(diagnostics)
}
