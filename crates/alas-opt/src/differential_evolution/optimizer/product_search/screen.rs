// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The screening stage of the product search: a space-filling scan of the
//! anchored box under its own budget, and the diverse elite it hands on.

use super::*;

impl DesignOptimizer {
    /// The screening stage on `model`. With `keep_cache` the exact scores are
    /// handed to the refinement, which evaluates with the same model.
    pub(super) fn screen<S: SearchObjective + ?Sized>(
        &self,
        run: &Run<'_>,
        model: &mut S,
        elite_size: usize,
        keep_cache: bool,
    ) -> Screened {
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
        let outcome = screening::run(
            run.bounds,
            run.baseline.as_deref(),
            screening::Settings {
                max_evaluations: budget,
                stop_after,
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
        evaluator.objective.set_deadline(None);
        let wall = started.elapsed().as_secs_f64();
        let cancelled = outcome.termination == Termination::Cancelled;
        let excluded: Vec<Vec<f64>> = run.baseline.iter().cloned().collect();
        let mut elite = if cancelled {
            Vec::new()
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
            generations: outcome.batches,
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
        }
    }
}
