// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The broad low-resolution scan that seeds the product search, its finalist
//! verification and the batch evaluator the kernel calls.

use super::*;

/// What the broad scan found.
pub(super) struct ScanOutcome {
    /// Finalist design vectors, best first in the scan's own ranking.
    pub(super) candidates: Vec<Vec<f64>>,
    /// Low-resolution analyses executed.
    pub(super) screened: usize,
    /// How many of them were feasible under the reduced model.
    pub(super) feasible_screened: usize,
    /// Wall-clock seconds spent in the scan.
    pub(super) elapsed_s: f64,
    /// Whether the scan stopped on the caller's cancellation flag rather than
    /// exhausting its sample. Its finalists are then drawn from the part of
    /// the envelope it reached, which is why a cancelled scan may not start a
    /// search.
    pub(super) cancelled: bool,
}

impl DesignOptimizer {
    /// Rank a broad deterministic sample of the envelope on the reduced model
    /// and return its best few design vectors.
    ///
    /// The reduced model is defined by `search::staged::screening_config`.
    /// Its evaluations are deliberately *not* merged into the run's history:
    /// they were scored on a coarser mesh and a looser closure, and a history
    /// that mixed the two would let a reader compare objective values that
    /// are not comparable. Their count is reported separately instead.
    pub(super) fn broad_scan(
        &self,
        bounds: &[(f64, f64)],
        initial: Option<&[f64]>,
        settings: &crate::search::staged::Settings,
        scope: &CancelScope<'_>,
        compute_pool: Option<std::sync::Arc<rayon::ThreadPool>>,
    ) -> ScanOutcome {
        let started = Instant::now();
        let empty = ScanOutcome {
            candidates: Vec::new(),
            screened: 0,
            feasible_screened: 0,
            elapsed_s: 0.0,
            cancelled: false,
        };
        let cancel_requested = || scope.requested();
        if settings.scan_points == 0 || settings.scan_finalists == 0 || bounds.is_empty() {
            return empty;
        }
        if cancel_requested() {
            return ScanOutcome {
                cancelled: true,
                ..empty
            };
        }
        let sample = crate::search::staged::conditioned_scan_sample(
            bounds,
            settings.scan_points,
            settings.seed,
            initial,
            self.config
                .optimizer
                .solver
                .seed_near_initial_design
                .then_some(self.config.optimizer.solver.seed_perturbation_fraction),
        );
        if sample.is_empty() {
            return empty;
        }
        let screening = crate::search::staged::screening_config(&self.config);
        let mut objective = match initial.and_then(|values| DesignVector::from_array(values).ok()) {
            Some(nominal) => DesignObjective::new_with_nominal(screening, nominal),
            None => DesignObjective::new(screening),
        };
        let mut objective = match compute_pool {
            Some(pool) => {
                native_pool::NativeObjective::with_pool(&mut objective, pool, scope.flag())
            }
            None => match native_pool::NativeObjective::new(
                &mut objective,
                settings.workers,
                scope.flag(),
            ) {
                Ok(objective) => objective,
                Err(error) => {
                    tracing::warn!(%error, "native screening pool unavailable; proceeding with the main search");
                    return ScanOutcome {
                        elapsed_s: started.elapsed().as_secs_f64(),
                        ..empty
                    };
                }
            },
        };
        // The sample is scored in fixed-size blocks rather than as one batch
        // so the cancellation flag is read at a bounded interval. Blocks are
        // taken in sample order and their scores concatenated in that order,
        // so the scan's history rows, its ranking and the finalists it
        // nominates are identical to the single-batch form for an uncancelled
        // run; only how far it gets changes. `workers` still decides how one
        // block is spread, never which points are evaluated.
        let block_size = settings.scan_block_size.max(1);
        let mut scores: Vec<(f64, bool)> = Vec::with_capacity(sample.len());
        let mut cancelled = false;
        for (index, block) in sample.chunks(block_size).enumerate() {
            scope.enter(CancelPhase::ScreeningScanBlock, index as u64);
            if cancel_requested() {
                cancelled = true;
                scope.work_skipped(format!(
                    "screening scan stopped before block {index} of {}",
                    sample.len().div_ceil(block_size)
                ));
                break;
            }
            // One block is uninterruptible, so its measured duration - not
            // the per-evaluation figure - is the cancellation bound while the
            // scan is running. The block is reduced-fidelity and spread over
            // `workers`, which is why it is timed separately.
            let block_scores = scope.block(block.len() as u64, || {
                objective.evaluate_batch(block, settings.workers)
            });
            scores.extend(block_scores);
        }
        let sample: Vec<Vec<f64>> = sample.into_iter().take(scores.len()).collect();
        if sample.is_empty() {
            return ScanOutcome {
                cancelled,
                elapsed_s: started.elapsed().as_secs_f64(),
                ..empty
            };
        }
        let history = objective.history().clone();
        let mut ranked: Vec<(usize, ScoredPoint)> = sample
            .iter()
            .zip(scores)
            .enumerate()
            .map(|(index, (values, (cost, _)))| {
                (index, scored_point_at(values, cost, &history, index))
            })
            .collect();
        let feasible_screened = ranked.iter().filter(|(_, point)| point.valid).count();
        // Feasibility first, then aggregate violation, then objective: the
        // same order the search ranks candidates by, with the sample index as
        // the deterministic tie-break.
        ranked.sort_by(|left, right| {
            left.1
                .feasibility_key()
                .cmp(&right.1.feasibility_key())
                .then(left.0.cmp(&right.0))
        });
        let candidates = ranked
            .into_iter()
            .take(settings.scan_finalists)
            .map(|(index, _)| sample[index].clone())
            .collect();
        ScanOutcome {
            candidates,
            screened: sample.len(),
            feasible_screened,
            elapsed_s: started.elapsed().as_secs_f64(),
            cancelled,
        }
    }

    /// The result a product search reports when cancellation was observed
    /// before the L-SHADE search could start.
    ///
    /// `verified` is the best point the bounded verification block scored with
    /// the run's own full objective; when the scan was cancelled before it
    /// nominated anything and the caller supplied no nominal, there is no
    /// scored candidate at all and the winner is the explicit unevaluated
    /// sentinel (infinite cost, invalid), which can never be mistaken for an
    /// analysed design. Either way `termination` is [`CANCELLED`] and
    /// `search_diagnostics.converged` is false.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn cancelled_before_search_result<E: SearchObjective + ?Sized>(
        &self,
        objective: &mut E,
        start: Option<&[f64]>,
        verified: Option<ScoredPoint>,
        scan: &ScanOutcome,
        scan_verification: usize,
        workers: usize,
        started: Instant,
    ) -> OptimizationResult {
        let winner = verified.unwrap_or_else(|| product_de::unevaluated(start.unwrap_or(&[])));
        let elapsed = started.elapsed().as_secs_f64();
        let mut result = result_from_method(
            MethodOutcome {
                winner,
                pareto_front: Vec::new(),
            },
            product_de::METHOD,
            product_de::STRATEGY,
            CANCELLED,
            objective.history(),
            elapsed,
        );
        result.search_diagnostics = Some(SearchDiagnostics {
            restoration: None,
            converged: false,
            // The search never ran, so no analysis is attributable to it;
            // the scan and verification counts below are the whole cost of
            // this run.
            analysis_evaluations: 0,
            cache_hits: 0,
            poll_iterations: 0,
            screening_evaluations: scan.screened,
            screening_feasible: scan.feasible_screened,
            verification_evaluations: scan_verification,
            scan_wall_time_s: scan.elapsed_s,
            search_wall_time_s: (elapsed - scan.elapsed_s).max(0.0),
            workers,
            poll_block_size: 0,
            first_feasible_cost: None,
            relative_improvement: None,
            feasible_fraction: 0.0,
            epsilon_level: 0.0,
        });
        result
    }
}

/// Re-evaluate the scan finalists, and the caller's nominal design, with the
/// run's own full objective, and return the best start plus how many coupled
/// analyses that cost.
///
/// This is the boundary the reduced model may not cross: a scan finalist only
/// becomes the search's starting point after a full coupled evaluation ranks
/// it ahead of the nominal, and those evaluations enter the run's history like
/// any other. Including the nominal in the same block is what makes the
/// comparison a like-for-like one.
/// Re-evaluate the scan's finalists with the run's own full objective.
///
/// Under an already-requested cancellation the list is cut to the nominal
/// point: the search will not start, so ranking the finalists decides nothing,
/// and each one is a full coupled analysis of drain. Scoring the nominal is
/// what keeps a cancelled run's reported winner an analysed design rather
/// than the unevaluated sentinel - the smallest amount of work that preserves
/// a real answer. With no nominal to fall back on, nothing is evaluated.
pub(super) fn verify_scan_finalists<E: SearchObjective + ?Sized>(
    objective: &mut E,
    candidates: &[Vec<f64>],
    nominal: Option<&[f64]>,
    workers: usize,
    scope: &CancelScope<'_>,
) -> (Option<ScoredPoint>, usize) {
    if candidates.is_empty() && nominal.is_none() {
        return (None, 0);
    }
    scope.enter(CancelPhase::ScanVerification, 0);
    let cancelled = scope.requested();
    let mut points: Vec<Vec<f64>> = Vec::with_capacity(candidates.len() + 1);
    if let Some(values) = nominal {
        points.push(values.to_vec());
    }
    if cancelled {
        scope.work_skipped(format!(
            "finalist verification cut to {} of {} points on the cancellation request",
            points.len(),
            candidates.len() + usize::from(nominal.is_some())
        ));
    } else {
        for candidate in candidates {
            if !points.iter().any(|existing| existing == candidate) {
                points.push(candidate.clone());
            }
        }
    }
    if points.is_empty() {
        return (None, 0);
    }
    let before = objective.history().n_evaluations();
    let scores = scope.block(points.len() as u64, || {
        objective.evaluate_batch(&points, workers)
    });
    let history = objective.history();
    let mut best: Option<ScoredPoint> = None;
    for (offset, (values, (cost, _))) in points.iter().zip(scores).enumerate() {
        let scored = scored_point_at(values, cost, history, before + offset);
        let better = best
            .as_ref()
            .is_none_or(|incumbent| scored.feasibility_key() < incumbent.feasibility_key());
        if better {
            best = Some(scored);
        }
    }
    (best, points.len())
}

/// Adapter that evaluates one independent generation batch through the
/// objective's own worker pool.
///
/// The batch boundary is chosen by the search (one L-SHADE generation; see
/// `search_methods::lshade_de`), so the worker count changes only how the
/// batch is distributed, never which points are evaluated or the order they
/// are considered in: candidates are scored in `points`' own order and that
/// order is what the kernel built deterministically from its seed.
pub(super) struct BatchEvaluator<'a, E: SearchObjective + ?Sized> {
    pub(super) objective: &'a mut E,
    pub(super) workers: usize,
    /// Cancellation telemetry for the block boundary, or an inert scope where
    /// the caller times its own evaluations.
    pub(super) scope: CancelScope<'a>,
    pub(super) cache: evaluation_cache::EvaluationCache,
}

impl<E: SearchObjective + ?Sized> BatchEvaluator<'_, E> {
    pub(super) fn evaluate_block(&mut self, points: &[Vec<f64>]) -> Vec<ScoredPoint> {
        let pending = self.cache.missing(points);
        let before = self.objective.history().n_evaluations();
        let scores = {
            let objective = &mut *self.objective;
            let workers = self.workers;
            self.scope.block(pending.len() as u64, || {
                objective.evaluate_batch(&pending, workers)
            })
        };
        let history = self.objective.history();
        for (offset, (values, (cost, _))) in pending.iter().zip(scores).enumerate() {
            self.cache
                .insert(scored_point_at(values, cost, history, before + offset));
        }
        self.cache.resolve(points)
    }
}
