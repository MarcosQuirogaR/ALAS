// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The screening stage of the product search: a space-filling scan of the
//! anchored box under its own budget, and the diverse elite it hands on.
//!
//! With a distinct screening model (the native coarse-fidelity model) the
//! stage is the exploration of a staged search (`explore`): the scan takes
//! the first [`EXPLORATION_SAMPLE`] analysed candidates of the stage budget
//! and the kernel spends the rest on the same cheap model, in rounds, each
//! followed by a calibration of the model's constraint margins on the best
//! candidates found so far. Many candidates are examined where an analysis
//! is cheap; only the best of them are promoted to the refinement model.
//! Every count is a function of the configured budget and the seed, never
//! of the clock or the worker count.

use super::*;

mod explore;

/// Analysed candidates of the space-filling scan that opens the exploration
/// (engineering choice: four lane waves, twice the exploration population,
/// so the kernel starts from the better half of a sample that covers the
/// anchored box and the nominal neighbourhood).
pub(super) const EXPLORATION_SAMPLE: usize = 4 * product_de::NATIVE_LANE_QUANTUM;

/// Smallest normalized distance between two promoted candidates: 1 % of
/// the box (bound-width-scaled RMS), so the promoted set is not one design
/// repeated to the last digit while still sampling the incumbent's
/// neighbourhood.
pub(super) const PROMOTION_NICHE_RADIUS: f64 = 0.01;

impl DesignOptimizer {
    /// The screening stage on `model`. With `keep_cache` the exact scores are
    /// handed to the refinement, which evaluates with the same model.
    pub(super) fn screen<S: SearchObjective + ?Sized>(
        &self,
        run: &Run<'_>,
        model: &mut S,
        elite_size: usize,
        keep_cache: bool,
        calibrate: Option<&mut calibration::Calibrate<'_>>,
    ) -> Screened {
        // A distinct screening model explores: scan, then search.
        let explores = !keep_cache;
        let solver = &self.config.optimizer.solver;
        let scope = CancelScope::attach(run.cancel);
        let (budget, stop_after, limit) =
            product_de::stage_limits(&solver.screening, solver.stop_on_evaluations_only);
        let max_rejects = product_de::reject_cap(&solver.screening);
        let started = Instant::now();
        let mut evaluator =
            BatchEvaluator::new(model, run.workers, run.bounds, run.pre_gate, run.cancel);
        evaluator.baseline = run.baseline.as_deref();
        evaluator.stage = crate::TraceStage::Screening;
        let before = evaluator.analyses();
        evaluator
            .objective
            .set_deadline(limit.map(|limit| started + limit));
        let sample = if explores {
            budget.min(EXPLORATION_SAMPLE)
        } else {
            budget
        };
        let mut outcome = screening::run(
            run.bounds,
            run.baseline.as_deref(),
            screening::Settings {
                max_evaluations: sample,
                stop_after: stop_after.min(sample),
                max_rejects,
                time_limit: limit,
                workers: run.workers,
                seed: run.seed,
                projection: run
                    .pre_gate
                    .and_then(|config| PlanformProjection::new(config, run.bounds)),
            },
            started,
            &scope,
            &|values: &[f64]| run.admits(values),
            &mut |points: &[Vec<f64>]| evaluator.evaluate_block(points),
        );
        let mut generations = outcome.batches;
        let mut explored = explore::Explored::default();
        let dimension = run
            .bounds
            .iter()
            .filter(|(lower, upper)| upper > lower)
            .count();
        if explores
            && dimension > 0
            && outcome.termination == Termination::EvaluationBudget
            && outcome.analysed < stop_after.min(budget)
        {
            explored = explore::run(
                explore::Stage {
                    run,
                    solver,
                    scope: &scope,
                    started,
                    limit,
                    max_rejects,
                    remaining: stop_after.min(budget) - outcome.analysed,
                    rounds: explore::rounds(self.config.optimizer.design_space.mode),
                },
                &mut evaluator,
                &mut outcome.scored,
                calibrate,
            );
            outcome.analysed += explored.analysed;
            generations += explored.generations;
            outcome.termination = explored.termination;
        }
        evaluator.objective.set_deadline(None);
        let wall = started.elapsed().as_secs_f64();
        let cancelled = outcome.termination == Termination::Cancelled;
        let excluded: Vec<Vec<f64>> = run.baseline.iter().cloned().collect();
        let mut elite = if cancelled {
            Vec::new()
        } else if explores {
            elite::best_distinct(
                &outcome.scored,
                run.bounds,
                elite_size,
                PROMOTION_NICHE_RADIUS,
                &excluded,
            )
        } else {
            elite::select(&outcome.scored, run.bounds, elite_size, &excluded)
        };
        // The niche clearing excludes the baseline's neighbourhood, where the
        // best screened design may sit; it is handed on regardless so the
        // run's best-ever design survives into the refinement.
        let best = outcome
            .scored
            .iter()
            .enumerate()
            .min_by_key(|(index, point)| (point.feasibility_key(), *index))
            .map(|(_, point)| point);
        if let Some(best) = best.filter(|best| {
            elite_size > 0
                && !excluded.contains(&best.values)
                && !elite.iter().any(|member| member.values == best.values)
        }) {
            elite.insert(0, best.clone());
            elite.truncate(elite_size);
        }
        let stage = StageSummary {
            stage: "screening".to_owned(),
            max_evaluations: budget,
            planned_evaluations: budget,
            time_limit_s: solver.screening.time_limit_s,
            time_limited: limit.is_some(),
            evaluations: outcome.analysed,
            generations,
            elite_size: elite.len(),
            termination: outcome.termination.label().to_owned(),
            ..StageSummary::default()
        };
        let (summary, rejections) = evaluator.close_stage(stage, before, wall, max_rejects);
        Screened {
            summary,
            rejections,
            elite,
            cancelled,
            // Without the cache the refinement scores with another model.
            trace: crate::EvaluationTrace {
                screening_separate: !keep_cache,
                ..evaluator.trace
            },
            cache: keep_cache.then_some(evaluator.cache),
            best_by_generation: explored.best_by_generation,
            calibration: explored.calibration,
        }
    }
}
