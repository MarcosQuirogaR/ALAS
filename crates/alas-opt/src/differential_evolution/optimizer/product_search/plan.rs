// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! How the product search divides its candidate plan between its stages:
//! the elite a refinement keeps, the analyses a staged search sets aside
//! for its calibrations and its neighbourhood-support poll, the replayed
//! pilot prefix, and the result every exit reports.

use super::*;

/// Take the work a staged search spends outside its refinement kernel out
/// of the kernel's own budget, so the stage's analyses stay inside the
/// configured plan: the calibration analyses of the exploration beyond the
/// baseline (which the kernel counts as its first seed), then one
/// neighbourhood-support poll (`support`). Only for a `staged` search on a
/// candidate plan (no clock) with room above its population. Returns
/// whether the poll was reserved.
pub(super) fn reserve_staged_work(
    refinement: &mut product_de::Settings,
    staged: bool,
    calibration_analyses: usize,
) -> bool {
    let on_plan = staged
        && refinement.time_limit.is_none()
        && refinement.stop_after == refinement.max_evaluations;
    let mut take = |count: usize| {
        let admitted = on_plan && refinement.max_evaluations > refinement.population + count;
        if admitted {
            refinement.max_evaluations -= count;
            refinement.stop_after = refinement.max_evaluations;
            refinement.schedule_evaluations = refinement
                .schedule_evaluations
                .min(refinement.max_evaluations);
        }
        admitted
    };
    take(calibration_analyses.saturating_sub(1));
    take(support::NEIGHBOURS)
}

impl DesignOptimizer {
    /// The result, with the diagnostics every exit reports.
    pub(super) fn finish(
        &self,
        run: &Run<'_>,
        history: &OptimizationHistory,
        outcome: MethodOutcome,
        termination: &str,
        started: Instant,
        mut diagnostics: SearchDiagnostics,
    ) -> OptimizationResult {
        let screening = diagnostics.stages.iter().find(|s| s.stage == "screening");
        diagnostics.screening_evaluations = screening.map_or(0, |s| s.evaluations);
        diagnostics.screening_feasible = screening.map_or(0, |s| s.feasible);
        diagnostics.scan_wall_time_s = screening.map_or(0.0, |s| s.wall_time_s);
        diagnostics.workers = run.workers;
        diagnostics.seed = Some(run.seed);
        diagnostics.baseline_clamped = run.baseline_clamped;
        diagnostics.scope = SEARCH_SCOPE.to_owned();
        let mut result = result_from_method(
            outcome,
            product_de::METHOD,
            product_de::STRATEGY,
            termination,
            history,
            started.elapsed().as_secs_f64(),
        );
        result.search_diagnostics = Some(diagnostics);
        result
    }
}

/// Screening elite members an initial population of `population` keeps
/// beside the baseline: [`ELITE_POPULATION_FRACTION`] of it after a scan,
/// all of it after a `staged` exploration, whose promoted candidates are
/// the result of a search rather than of a sample.
pub(super) fn elite_members_kept(population: usize, staged: bool) -> usize {
    let others = population.saturating_sub(1);
    if staged {
        others
    } else {
        (others as f64 * ELITE_POPULATION_FRACTION) as usize
    }
}

/// Screening elite members a native refinement keeps: the refinement pilot
/// (baseline plus elite) is then one whole lane wave
/// ([`product_de::NATIVE_LANE_QUANTUM`]) and fits inside the smallest
/// native population, so it neither leaves lanes idle in a second partial
/// wave nor forces the population off a whole number of waves.
pub(super) fn native_elite_cap(has_baseline: bool) -> usize {
    product_de::NATIVE_LANE_QUANTUM - usize::from(has_baseline)
}

/// A replay ending inside the pilot requests the same admitted prefix.
/// Every started pilot seed is retained, so its cached initial population
/// gives the kernel precisely that recorded count when the deadline expires.
pub(super) fn replay_pilot_prefix<'a>(
    run: &Run<'_>,
    seeds: &'a [Vec<f64>],
    stage: &alas_config::StageBudget,
) -> &'a [Vec<f64>] {
    let Some(recorded) = stage
        .replay_evaluations
        .and_then(|value| usize::try_from(value).ok())
    else {
        return seeds;
    };
    let remaining = recorded.saturating_sub(product_de::restoration_replay(stage).unwrap_or(0));
    let mut admitted = 0;
    for (index, values) in seeds.iter().enumerate() {
        if run.admits(values) {
            if admitted >= remaining {
                return &seeds[..index];
            }
            admitted += 1;
        }
    }
    seeds
}
