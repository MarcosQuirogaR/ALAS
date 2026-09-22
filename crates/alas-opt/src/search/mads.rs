// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Bounded mesh adaptive direct search with a progressive barrier.
//!
//! The kernel works in normalized coordinates
//! `y_i = (x_i - lower_i) / (upper_i - lower_i)`, while the evaluator keeps
//! receiving the physical design variables.  A poll is generated from a
//! full-rank integer basis and its positive/negative columns, so every poll
//! contains a maximal positive spanning set.  The basis changes with a
//! deterministic Halton sequence and its integer range grows with the poll
//! index; this gives the finite-run implementation the directional-density
//! mechanism used by MADS rather than a fixed coordinate compass.
//!
//! The progressive barrier uses the evaluator's aggregate, dimensionless
//! hard violation `h`.  Finite points with `h > 0` may improve an infeasible
//! incumbent while the barrier tightens monotonically.  A feasible point is
//! always retained separately and is ranked by its finite scalar objective.
//! Failed analyses are an extreme barrier: they are never accepted, even if
//! their reported cost is artificially low.  Because [`ScoredPoint`] predates
//! a typed failure status, a large violation sentinel (`>= 1e5`) and any
//! nonfinite cost/violation are treated as failures; the evaluator adapter
//! should preserve that mapping.
//!
//! A small mesh or frame is only an algorithmic stopping condition.  This
//! implementation does not claim a global optimum, aircraft convergence, or
//! theorem-level stationarity when hidden analyses, closure noise, or finite
//! budgets violate the assumptions of the MADS convergence results.
//!
//! # What counts as convergence here
//!
//! [`TerminationReason::Converged`] is the only reason that reports the run as
//! converged, and it requires all three of:
//!
//! 1. a feasible incumbent, which for the product objective means every hard
//!    residual is satisfied *and* the coupled sizing loop closed (the
//!    `sizing_not_closed` and `dispatch_not_converged` residuals are hard), so
//!    this is a coupled-analysis criterion and not merely a bound check;
//! 2. the translated mesh has contracted to `convergence_mesh_size`, which
//!    can only happen through consecutive *failed* polls of a positive
//!    spanning set at successively halved frames: mesh-local optimality
//!    against the directions actually polled, not a budget stop.
//!
//!    How strong that statement is depends on which poll the run used, and
//!    the product design space uses the weaker one. With
//!    `minimal_positive_basis` (enabled above four variables, so on every
//!    sixteen-variable product run) a failed poll certifies that `n + 1`
//!    directions did not improve the incumbent at that frame; with the
//!    maximal set it certifies `2n` directions plus their coordinate
//!    enrichment. Both are positive spanning sets, so both make the
//!    contraction meaningful rather than arbitrary, and the basis is redrawn
//!    from the Halton sequence every iteration so the directions polled over
//!    a run are not the same `n + 1` each time. Neither is a proof of local
//!    optimality: this is a finite run of a model with hidden analyses and
//!    closure noise, and the caveat below applies in full;
//! 3. a relative objective improvement of at least
//!    `minimum_relative_improvement` over the first feasible point the run
//!    found, so a run that merely re-confirmed its starting design is not
//!    reported as a converged optimization.
//!
//! Every other reason, including [`TerminationReason::Watchdog`], is reported
//! as *not* converged.  The watchdog is a wall-clock safety and diagnostic
//! limit only; it never stands in for a convergence criterion.
//!
//! # Evaluation blocks, parallelism and determinism
//!
//! The poll is opportunistic: it is cut into blocks of `poll_block_size`
//! points, and polling stops after the first block that improves the
//! incumbent.  The block size is deliberately a search setting and *not* the
//! worker-thread count, so the set of evaluated points and the order they are
//! considered in are identical whether the caller evaluates a block serially
//! or across sixteen threads.  Repeated points are served from a bit-exact
//! cache, which removes the re-evaluation MADS performs whenever two poll
//! centres, a retried successful direction, or two successive frames land on
//! the same mesh node.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use super::{MethodOutcome, ScoredPoint};

/// Violation values at or above this level are the failure sentinel used by
/// the current untyped evaluator adapter.  Physical aggregate violations are
/// dimensionless and expected to be many orders of magnitude smaller.
const FAILURE_VIOLATION_CUTOFF: f64 = 1.0e5;
/// A feasible point must report no positive normalized hard violation.
const FEASIBILITY_TOLERANCE: f64 = 1.0e-12;
/// Relative comparison tolerance for two aggregate violations.
const VIOLATION_RELATIVE_TOLERANCE: f64 = 1.0e-12;

/// Settings for one MADS run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Settings {
    /// Maximum poll/search iterations.
    pub max_iterations: usize,
    /// Evaluation budget, including the initial point.  Cache hits are not
    /// charged against it: a repeated mesh node costs no analysis.
    pub max_evaluations: usize,
    /// Reproducible seed for search points and poll directions.
    pub seed: u64,
    /// Initial normalized mesh spacing.
    pub initial_mesh_size: f64,
    /// Smallest normalized mesh spacing at which the run stops.
    pub minimum_mesh_size: f64,
    /// Normalized mesh spacing at or below which a feasible, improved
    /// incumbent is reported as converged.  Normalized means a fraction of
    /// each variable's bound width, so `1e-2` is 0.2 m on a 60-80 m span
    /// bound and 0.2 deg on a 25-45 deg sweep bound: a conceptual-design
    /// resolution rather than a floating-point one.
    pub convergence_mesh_size: f64,
    /// Relative improvement in the feasible objective, against the first
    /// feasible point of the run, required before convergence may be
    /// reported.  Dimensionless.
    pub minimum_relative_improvement: f64,
    /// How many poll points are evaluated before the opportunistic success
    /// check.  Independent of the caller's thread count on purpose: see the
    /// determinism note in the module documentation.
    pub poll_block_size: usize,
    /// Hard wall-clock safety limit.  Exceeding it stops the run with
    /// [`TerminationReason::Watchdog`], which is *not* convergence.
    ///
    /// It is checked between evaluation blocks, not inside one, because a
    /// block is handed to the caller as a unit and may be spread over
    /// threads.  A run therefore stops at the limit plus at most the time of
    /// the block in flight: on the ATR-72, whose coupled analysis costs about
    /// thirteen seconds, a 900 s limit was observed to stop at 1296 s.  That
    /// is a safety bound, not a deadline.
    pub watchdog: Option<Duration>,
    /// Whether the poll is enriched with adjacent-coordinate diagonals.
    /// See `search::directions::poll_directions` for why this is off for the
    /// sixteen-variable product design space.
    pub pair_diagonal_directions: bool,
    /// Whether an unsuccessful poll only has to exhaust a *minimal* positive
    /// basis of `n + 1` directions before the mesh may contract, instead of
    /// the maximal `2n` spanning set with its coordinate enrichment.
    ///
    /// Both satisfy the positive-spanning condition MADS requires; the
    /// minimal one costs a third of the analyses per failed poll, which is
    /// what decides the wall-clock cost of reaching the convergence mesh when
    /// one evaluation is a full aircraft sizing.
    pub minimal_positive_basis: bool,
}

/// Why a bounded MADS run stopped.
///
/// This is deliberately separate from the legacy [`MethodOutcome`] because
/// the latter is shared by older optimizers and cannot carry algorithm
/// lifecycle information.  A feasible winner is reported through
/// `MadsOutcome::outcome.winner`; the termination reason describes the search
/// budget or mesh state, not a proof of global optimality or aircraft-level
/// convergence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminationReason {
    /// A feasible incumbent reached mesh-local optimality with a real
    /// objective improvement.  This is the only converged reason; see the
    /// module documentation for the exact three conditions.
    Converged,
    /// The callback budget, including the initial evaluation, was exhausted.
    EvaluationBudget,
    /// The translated mesh reached the configured minimum spacing without
    /// satisfying every convergence condition (typically: no feasible
    /// incumbent, or no improvement over the starting design).
    MeshLimit,
    /// The configured poll iteration count was exhausted.
    IterationLimit,
    /// All candidate poll moves were blocked by fixed bounds.
    FixedBounds,
    /// The wall-clock safety limit was reached.  A diagnostic stop, never
    /// convergence.
    Watchdog,
    /// The caller's cooperative cancellation flag was observed at an
    /// evaluation-block or iteration boundary.  Never convergence: the search
    /// was stopped from outside before any stopping criterion of its own was
    /// reached, so its incumbent is whatever the run had reached by then.
    Cancelled,
    /// Bounds, initial values, or mesh settings failed validation.
    InvalidInput,
}

impl TerminationReason {
    /// Stable progress/log label retained for the existing callback channel.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Converged => "converged",
            Self::EvaluationBudget => "evaluation_budget",
            Self::MeshLimit => "mesh_limit",
            Self::IterationLimit => "iteration_limit",
            Self::FixedBounds => "fixed_bounds",
            Self::Watchdog => "watchdog",
            Self::Cancelled => "cancelled",
            Self::InvalidInput => "invalid_input",
        }
    }

    /// Whether this reason reports a converged search.
    pub(crate) const fn is_converged(self) -> bool {
        matches!(self, Self::Converged)
    }

    /// Whether the run stopped on the caller's cancellation flag.
    pub(crate) const fn is_cancelled(self) -> bool {
        matches!(self, Self::Cancelled)
    }
}

/// MADS result plus durable search-specific termination state.
///
/// `outcome` intentionally contains the unchanged legacy result type used by
/// the common optimizer adapter.  Integration code can pass that field to
/// the existing adapter while retaining `termination` for reporting and
/// control flow without parsing progress strings.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MadsOutcome {
    pub(crate) outcome: MethodOutcome,
    pub(crate) termination: TerminationReason,
    /// Analyses actually executed, excluding cache hits.
    pub(crate) evaluations: usize,
    /// Repeated mesh nodes served from the run's own cache.
    pub(crate) cache_hits: usize,
    /// Poll/search iterations completed.
    pub(crate) iterations: usize,
    /// Wall-clock seconds from the first instruction of [`run`].
    pub(crate) elapsed_s: f64,
    /// Feasible objective at the first feasible point, and at the winner.
    /// `None` when the run never reached a feasible candidate.
    pub(crate) first_feasible_cost: Option<f64>,
    /// Relative improvement of the feasible incumbent over the first
    /// feasible point, dimensionless; `None` without a feasible candidate.
    pub(crate) relative_improvement: Option<f64>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_iterations: 20,
            max_evaluations: 1_000,
            seed: 0,
            initial_mesh_size: 0.25,
            minimum_mesh_size: 1.0e-4,
            convergence_mesh_size: 1.0e-2,
            minimum_relative_improvement: 1.0e-4,
            poll_block_size: 16,
            watchdog: None,
            pair_diagonal_directions: true,
            minimal_positive_basis: false,
        }
    }
}

/// How the kernel asks for candidate scores.
///
/// A block is a set of independent candidates: the caller is free to spread
/// it over worker threads, and must return one score per point in input
/// order.  Any `FnMut(&[f64]) -> ScoredPoint` is accepted through the blanket
/// implementation below, which evaluates the block serially.
pub(crate) trait Evaluate {
    /// Score `points`, returning one entry per point, in order.
    fn evaluate_block(&mut self, points: &[Vec<f64>]) -> Vec<ScoredPoint>;
}

impl<F: FnMut(&[f64]) -> ScoredPoint> Evaluate for F {
    fn evaluate_block(&mut self, points: &[Vec<f64>]) -> Vec<ScoredPoint> {
        points.iter().map(|point| self(point)).collect()
    }
}

/// Bit-exact memo of the points this run already scored.
///
/// MADS revisits mesh nodes routinely: the retried successful direction, the
/// second poll centre and two successive frames all land on points the run
/// has already paid for.  Keying on the raw bit patterns makes a hit exactly
/// a repeat of the same design vector, so the memo cannot merge two designs
/// that differ below a tolerance.
#[derive(Default)]
struct EvaluationCache {
    entries: HashMap<Vec<u64>, ScoredPoint>,
    hits: usize,
    misses: usize,
}

fn cache_key(values: &[f64]) -> Vec<u64> {
    values.iter().map(|value| value.to_bits()).collect()
}

impl EvaluationCache {
    /// Score `points`, calling `evaluate` only for the ones not already known.
    fn evaluate_block(
        &mut self,
        points: &[Vec<f64>],
        evaluate: &mut dyn Evaluate,
    ) -> Vec<ScoredPoint> {
        let keys: Vec<Vec<u64>> = points.iter().map(|point| cache_key(point)).collect();
        let mut pending = Vec::new();
        let mut pending_keys: Vec<Vec<u64>> = Vec::new();
        for (point, key) in points.iter().zip(&keys) {
            if self.entries.contains_key(key) {
                self.hits += 1;
            } else if !pending_keys.contains(key) {
                pending_keys.push(key.clone());
                pending.push(point.clone());
            }
        }
        if !pending.is_empty() {
            let scores = evaluate.evaluate_block(&pending);
            self.misses += pending.len();
            for (key, score) in pending_keys.into_iter().zip(scores) {
                self.entries.insert(key, score);
            }
        }
        keys.iter()
            .zip(points)
            .map(|(key, point)| match self.entries.get(key) {
                Some(score) => score.clone(),
                // A caller that returns fewer scores than requested is a
                // broken evaluator, not a design decision; treat the missing
                // entry as an unevaluated extreme barrier rather than
                // silently scoring it well.
                None => invalid_point(point.clone()),
            })
            .collect()
    }
}

/// Run the bounded MADS progressive-barrier search.
///
/// At most `settings.max_evaluations` analyses are executed; repeated mesh
/// nodes are served from the run's cache and are not charged.  Bounds are
/// finite SI/physical input units supplied by the caller; all poll and mesh
/// arithmetic is performed in the unitless normalized box.  A malformed
/// bound/initial vector or invalid mesh setting returns an invalid fallback
/// without invoking the evaluator, because this legacy result interface has no
/// `Result` channel for input errors.
pub(crate) fn run(
    bounds: &[(f64, f64)],
    initial: Option<&[f64]>,
    settings: Settings,
    evaluate: &mut dyn Evaluate,
    progress: Option<&mut dyn FnMut(&str)>,
) -> MadsOutcome {
    run_cancellable(bounds, initial, settings, None, evaluate, progress)
}

/// [`run`], observing an optional cooperative cancellation flag.
///
/// `cancel` is read at exactly the boundaries the wall-clock watchdog is read
/// at - between evaluation blocks and at the head of a poll iteration, never
/// inside a block handed to the evaluator - so a cancelled run returns the
/// incumbent it had already scored rather than a half-evaluated block, and an
/// external process the block is waiting on is allowed to finish its call.
/// The run reports [`TerminationReason::Cancelled`], which is never converged.
pub(crate) fn run_cancellable(
    bounds: &[(f64, f64)],
    initial: Option<&[f64]>,
    settings: Settings,
    cancel: Option<&AtomicBool>,
    evaluate: &mut dyn Evaluate,
    mut progress: Option<&mut dyn FnMut(&str)>,
) -> MadsOutcome {
    let started = Instant::now();
    // The flag still arrives as a bare `&AtomicBool`; the scope is the
    // telemetry that happens to own it, if any, and is inert otherwise.
    let scope = crate::cancellation::CancelScope::attach(cancel);
    let cancel_requested = || scope.requested();
    if !valid_inputs(bounds, initial, settings) {
        return MadsOutcome {
            outcome: invalid_outcome(bounds, initial),
            termination: TerminationReason::InvalidInput,
            evaluations: 0,
            cache_hits: 0,
            iterations: 0,
            elapsed_s: started.elapsed().as_secs_f64(),
            first_feasible_cost: None,
            relative_improvement: None,
        };
    }

    let dimension = bounds.len();
    let mut current_values = initial
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| midpoint(bounds));
    clamp_to_bounds(&mut current_values, bounds);
    let origin = to_normalized(&current_values, bounds);
    let minimum_mesh_size = settings.minimum_mesh_size;
    let mut mesh_size = settings.initial_mesh_size.min(1.0);
    let mut frame_size = mesh_size;
    let max_evaluations = settings.max_evaluations;

    // A zero budget is a valid bounded request, but no analysis may be run.
    // Return the projected initial vector as an unevaluated extreme barrier.
    if max_evaluations == 0 {
        return MadsOutcome {
            outcome: MethodOutcome {
                winner: invalid_point(current_values),
                pareto_front: Vec::new(),
            },
            termination: TerminationReason::EvaluationBudget,
            evaluations: 0,
            cache_hits: 0,
            iterations: 0,
            elapsed_s: started.elapsed().as_secs_f64(),
            first_feasible_cost: None,
            relative_improvement: None,
        };
    }

    let mut cache = EvaluationCache::default();
    let block_size = settings.poll_block_size.max(1);
    let watchdog_expired = |started: &Instant| {
        settings
            .watchdog
            .is_some_and(|limit| started.elapsed() >= limit)
    };

    let first = evaluate_candidates(&[current_values.clone()], &mut cache, evaluate)
        .into_iter()
        .next()
        .unwrap_or_else(|| extreme_evaluated(current_values.clone()));
    let mut best_feasible = None;
    let mut best_infeasible = None;
    let mut barrier = f64::INFINITY;
    // The objective of the first feasible point the run reaches, which the
    // convergence criterion measures improvement against.
    let mut first_feasible_cost: Option<f64> = None;
    if !first.extreme {
        if first.h == 0.0 {
            first_feasible_cost = Some(first.point.cost);
            best_feasible = Some(first.point.clone());
            barrier = 0.0;
        } else {
            barrier = first.h;
            best_infeasible = Some(first.point.clone());
        }
    }
    let initial_fallback = first.point.clone();
    let mut watchdog_hit = false;
    let mut cancelled = false;

    // MADS permits a bounded search phase before polling.  This deterministic
    // Latin-hypercube phase is snapped to the translated initial mesh so it
    // remains part of the same mesh sequence and is reproducible for a seed.
    // It is evaluated in blocks for the same reason the poll is: the points
    // are independent of one another.
    if settings.max_iterations > 0 && cache.misses < max_evaluations && dimension > 0 {
        let search_points: Vec<Vec<f64>> =
            initial_search_points(bounds, &origin, mesh_size, settings.seed)
                .into_iter()
                .filter(|point| !same_point(point, &current_values, bounds))
                .collect();
        'search: for (index, block) in search_points.chunks(block_size).enumerate() {
            scope.enter(
                crate::cancellation::CancelPhase::MadsSearchBlock,
                index as u64,
            );
            if cancel_requested() {
                cancelled = true;
                break;
            }
            if cache.misses >= max_evaluations || watchdog_expired(&started) {
                watchdog_hit |= watchdog_expired(&started);
                break;
            }
            let block = truncate_to_budget(block, &cache, max_evaluations);
            for candidate in evaluate_candidates(&block, &mut cache, evaluate) {
                let feasible_before = best_feasible.is_some();
                let change = consider_candidate(
                    candidate,
                    barrier,
                    &mut best_feasible,
                    &mut best_infeasible,
                    &mut current_values,
                );
                if change.infeasible_improved {
                    barrier = best_infeasible
                        .as_ref()
                        .map(|point| point.constraint_violation)
                        .unwrap_or(barrier)
                        .min(barrier);
                }
                if let Some(point) = best_feasible.as_ref() {
                    if !feasible_before {
                        first_feasible_cost = Some(point.cost);
                    }
                    barrier = 0.0;
                }
                if cache.misses >= max_evaluations {
                    break 'search;
                }
            }
        }
    }

    if best_feasible.is_some() {
        barrier = 0.0;
    } else if let Some(point) = best_infeasible.as_ref() {
        barrier = barrier.min(point.constraint_violation);
    }

    let mut iteration = 0usize;
    let mut last_success_direction: Option<Vec<i64>> = None;
    let mut termination = if mesh_size <= minimum_mesh_size {
        TerminationReason::MeshLimit
    } else {
        TerminationReason::IterationLimit
    };
    while iteration < settings.max_iterations
        && cache.misses < max_evaluations
        && mesh_size > minimum_mesh_size
    {
        if cancelled || cancel_requested() {
            cancelled = true;
            break;
        }
        if watchdog_expired(&started) {
            watchdog_hit = true;
            break;
        }
        let feasible_before = best_feasible.as_ref().map(|point| point.cost);
        let directions = if settings.minimal_positive_basis {
            minimal_positive_basis(dimension, iteration, settings.seed)
        } else {
            poll_directions(
                dimension,
                iteration,
                settings.seed,
                settings.pair_diagonal_directions,
            )
        };
        // The second poll centre belongs to the progressive barrier's
        // restoration phase.  Once a feasible incumbent exists the barrier is
        // closed at h = 0, so nothing polled around the infeasible incumbent
        // can be accepted unless it is itself feasible, while the centre
        // doubles the cost of every poll, including the failed polls that
        // contract the mesh.  Keep it only while the run is still looking for
        // its first feasible aircraft.
        let restoration_center = best_feasible
            .is_none()
            .then_some(best_infeasible.as_ref())
            .flatten();
        let centers = poll_centers(&current_values, restoration_center, bounds);
        let mut poll: Vec<(Vec<f64>, Vec<i64>)> = Vec::new();
        if let Some(direction) = last_success_direction.as_ref() {
            let point = poll_point(&current_values, direction, frame_size, bounds);
            if !same_point(&point, &current_values, bounds) {
                poll.push((point, direction.clone()));
            }
        }
        for center in centers {
            for direction in &directions {
                let point = poll_point(&center, direction, frame_size, bounds);
                if !same_point(&point, &center, bounds)
                    && !poll
                        .iter()
                        .any(|(existing, _)| same_point(existing, &point, bounds))
                {
                    poll.push((point, direction.clone()));
                }
            }
        }

        if poll.is_empty() {
            termination = TerminationReason::FixedBounds;
            break;
        }

        // Opportunistic polling in fixed-size blocks: the poll stops at the
        // first block that improves the incumbent, so a successful iteration
        // costs a block rather than the whole positive spanning set.  The
        // block boundary is a search setting, not the caller's thread count,
        // so the evaluated set and the order it is considered in do not
        // change when the caller evaluates a block in parallel.
        let mut infeasible_improved = false;
        for block in poll.chunks(block_size) {
            scope.enter(
                crate::cancellation::CancelPhase::MadsPollBlock,
                iteration as u64,
            );
            if cache.misses >= max_evaluations {
                break;
            }
            if cancel_requested() {
                cancelled = true;
                break;
            }
            if watchdog_expired(&started) {
                watchdog_hit = true;
                break;
            }
            let points: Vec<Vec<f64>> = block.iter().map(|(point, _)| point.clone()).collect();
            let points = truncate_to_budget(&points, &cache, max_evaluations);
            let scored = evaluate_candidates(&points, &mut cache, evaluate);
            let mut block_success = false;
            for (candidate, (_, direction)) in scored.into_iter().zip(block) {
                let was_feasible = best_feasible.is_some();
                let change = consider_candidate(
                    candidate,
                    barrier,
                    &mut best_feasible,
                    &mut best_infeasible,
                    &mut current_values,
                );
                infeasible_improved |= change.infeasible_improved;
                if change.accepted {
                    last_success_direction = Some(direction.clone());
                    block_success = true;
                }
                if let Some(point) = best_feasible.as_ref() {
                    if !was_feasible {
                        first_feasible_cost = Some(point.cost);
                    }
                }
            }
            if block_success {
                break;
            }
        }

        // PB's threshold is monotone nonincreasing.  A newly improved
        // infeasible incumbent becomes the next threshold; finding a feasible
        // point closes the relaxable barrier at h=0.
        if best_feasible.is_some() {
            barrier = 0.0;
        } else if infeasible_improved {
            if let Some(point) = best_infeasible.as_ref() {
                barrier = barrier.min(point.constraint_violation);
            }
        } else if let Some(point) = best_infeasible.as_ref() {
            barrier = barrier.min(point.constraint_violation);
        }

        let feasible_improved = best_feasible
            .as_ref()
            .map(|point| {
                feasible_before
                    .map(|before| point.cost.total_cmp(&before).is_lt())
                    .unwrap_or(true)
            })
            .unwrap_or(false);
        let poll_success = feasible_improved || infeasible_improved;
        iteration += 1;
        if poll_success {
            // The frame may grow on success while the translated mesh remains
            // fixed.  Its ratio to the mesh is a power of two, so candidates
            // remain on the same translated mesh.
            frame_size = (frame_size * 2.0).min(1.0);
        } else {
            mesh_size *= 0.5;
            frame_size = mesh_size;
        }

        if let Some(callback) = progress.as_mut() {
            let point = best_feasible
                .as_ref()
                .or(best_infeasible.as_ref())
                .unwrap_or(&first.point);
            (**callback)(&format!(
                "mads iteration {iteration} | evaluations {} | cache_hits {} | mesh {:.3e} | frame {:.3e} | barrier {:.3e} | feasible {} | violation {:.3e} | objective {:.6} | elapsed_s {:.3}",
                cache.misses,
                cache.hits,
                mesh_size,
                frame_size,
                barrier,
                point.valid,
                point.constraint_violation,
                point.cost,
                started.elapsed().as_secs_f64()
            ));
        }

        // The converged stop is checked here, at the end of an iteration,
        // because all three of its conditions are iteration state: the mesh
        // has just contracted through a failed poll of a maximal positive
        // spanning set, the incumbent is feasible, and its objective has
        // improved on the first feasible point by a stated margin.
        if mesh_size <= settings.convergence_mesh_size {
            if let (Some(point), Some(start)) = (best_feasible.as_ref(), first_feasible_cost) {
                if relative_improvement(start, point.cost)
                    >= settings.minimum_relative_improvement.max(0.0)
                {
                    termination = TerminationReason::Converged;
                    break;
                }
            }
        }
    }

    // Convergence is decided inside the loop; every reason below reports a
    // run that stopped for a budget, safety or structural reason instead.
    if !termination.is_converged() {
        // Cancellation outranks every budget reason: a run stopped from
        // outside has not exhausted anything, and reporting it as
        // `evaluation_budget` or `iteration_limit` would hide that the search
        // never got to decide when to stop.
        if cancelled {
            termination = TerminationReason::Cancelled;
        } else if watchdog_hit {
            termination = TerminationReason::Watchdog;
        } else if cache.misses >= max_evaluations {
            termination = TerminationReason::EvaluationBudget;
        } else if mesh_size <= minimum_mesh_size {
            termination = TerminationReason::MeshLimit;
        } else if iteration >= settings.max_iterations {
            termination = TerminationReason::IterationLimit;
        }
    }
    let improvement = best_feasible
        .as_ref()
        .zip(first_feasible_cost)
        .map(|(point, start)| relative_improvement(start, point.cost));
    if let Some(callback) = progress.as_mut() {
        let point = best_feasible
            .as_ref()
            .or(best_infeasible.as_ref())
            .unwrap_or(&initial_fallback);
        (**callback)(&format!(
            "mads termination {} | converged {} | evaluations {} | cache_hits {} | iterations {iteration} | mesh {:.3e} | feasible {} | violation {:.3e} | objective {:.6} | feasible_found {} | relative_improvement {} | elapsed_s {:.3}",
            termination.as_str(),
            termination.is_converged(),
            cache.misses,
            cache.hits,
            mesh_size,
            point.valid,
            point.constraint_violation,
            point.cost,
            best_feasible.is_some(),
            improvement.map(|value| format!("{value:.6}")).unwrap_or_else(|| "none".to_owned()),
            started.elapsed().as_secs_f64()
        ));
    }

    let winner = best_feasible
        .or(best_infeasible)
        .unwrap_or(initial_fallback);
    MadsOutcome {
        outcome: MethodOutcome {
            winner,
            pareto_front: Vec::new(),
        },
        termination,
        evaluations: cache.misses,
        cache_hits: cache.hits,
        iterations: iteration,
        elapsed_s: started.elapsed().as_secs_f64(),
        first_feasible_cost,
        relative_improvement: improvement,
    }
}

/// Relative improvement of `current` over `start`, dimensionless.
///
/// The objective the product search minimises can be negative (a normalised
/// block-fuel or mass objective shifted by its reference), so the change is
/// divided by the magnitude of the starting value rather than by the signed
/// value itself, and a zero start falls back to the absolute change.  A
/// worsened incumbent gives a negative result, which never satisfies a
/// nonnegative improvement requirement.
fn relative_improvement(start: f64, current: f64) -> f64 {
    if !start.is_finite() || !current.is_finite() {
        return 0.0;
    }
    let scale = start.abs();
    if scale <= f64::MIN_POSITIVE {
        start - current
    } else {
        (start - current) / scale
    }
}

/// Trim `points` so the block cannot exceed the remaining analysis budget.
///
/// Cached points are free, so only the ones that would be evaluated count
/// against the budget; a block made entirely of repeats is never truncated.
fn truncate_to_budget(
    points: &[Vec<f64>],
    cache: &EvaluationCache,
    max_evaluations: usize,
) -> Vec<Vec<f64>> {
    let mut remaining = max_evaluations.saturating_sub(cache.misses);
    let mut taken = Vec::with_capacity(points.len());
    let mut new_keys: Vec<Vec<u64>> = Vec::new();
    for point in points {
        let key = cache_key(point);
        let known = cache.entries.contains_key(&key) || new_keys.contains(&key);
        if !known {
            if remaining == 0 {
                break;
            }
            remaining -= 1;
            new_keys.push(key);
        }
        taken.push(point.clone());
    }
    taken
}

/// Score a block through the cache and apply the extreme-barrier mapping.
fn evaluate_candidates(
    points: &[Vec<f64>],
    cache: &mut EvaluationCache,
    evaluate: &mut dyn Evaluate,
) -> Vec<EvaluatedPoint> {
    cache
        .evaluate_block(points, evaluate)
        .into_iter()
        .zip(points)
        .map(|(score, values)| classify_candidate(score, values))
        .collect()
}

/// An unevaluated point held behind the extreme barrier.
fn extreme_evaluated(values: Vec<f64>) -> EvaluatedPoint {
    EvaluatedPoint {
        point: invalid_point(values),
        h: f64::INFINITY,
        extreme: true,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CandidateChange {
    accepted: bool,
    infeasible_improved: bool,
}

#[derive(Debug, Clone)]
struct EvaluatedPoint {
    point: ScoredPoint,
    h: f64,
    extreme: bool,
}

fn valid_inputs(bounds: &[(f64, f64)], initial: Option<&[f64]>, settings: Settings) -> bool {
    !bounds.is_empty()
        && bounds.iter().all(|&(lower, upper)| {
            lower.is_finite() && upper.is_finite() && lower <= upper && (upper - lower).is_finite()
        })
        && initial
            .map(|values| values.len() == bounds.len() && values.iter().all(|v| v.is_finite()))
            .unwrap_or(true)
        && settings.initial_mesh_size.is_finite()
        && settings.initial_mesh_size > 0.0
        && settings.minimum_mesh_size.is_finite()
        && settings.minimum_mesh_size > 0.0
}

fn invalid_outcome(bounds: &[(f64, f64)], initial: Option<&[f64]>) -> MethodOutcome {
    let values = if initial
        .map(|point| point.len() == bounds.len() && point.iter().all(|value| value.is_finite()))
        .unwrap_or(false)
    {
        initial.map(ToOwned::to_owned).unwrap_or_default()
    } else {
        bounds
            .iter()
            .map(|&(lower, upper)| {
                if lower.is_finite() && upper.is_finite() {
                    lower + (upper - lower) * 0.5
                } else {
                    0.0
                }
            })
            .collect()
    };
    MethodOutcome {
        winner: invalid_point(values),
        pareto_front: Vec::new(),
    }
}

fn invalid_point(values: Vec<f64>) -> ScoredPoint {
    ScoredPoint {
        values,
        cost: f64::INFINITY,
        valid: false,
        constraint_violation: f64::INFINITY,
        objectives: [f64::INFINITY; 3],
    }
}

fn classify_candidate(mut point: ScoredPoint, values: &[f64]) -> EvaluatedPoint {
    // The design vector being scored is authoritative; this also prevents an
    // evaluator bug from pairing one score with another candidate's values.
    point.values = values.to_vec();
    let finite_cost = point.cost.is_finite();
    if !finite_cost {
        point.cost = f64::INFINITY;
    }
    let finite_objectives = point.objectives.iter().all(|value| value.is_finite());
    let raw_h = point.constraint_violation;
    let finite_h = raw_h.is_finite() && raw_h >= 0.0;
    let feasible = point.valid
        && finite_cost
        && finite_objectives
        && finite_h
        && raw_h <= FEASIBILITY_TOLERANCE;
    let extreme = !feasible
        && (!finite_cost
            || !finite_objectives
            || !finite_h
            || raw_h <= 0.0
            || raw_h >= FAILURE_VIOLATION_CUTOFF);
    let h = if feasible {
        0.0
    } else if extreme {
        f64::INFINITY
    } else {
        raw_h
    };
    point.valid = feasible;
    point.constraint_violation = h;
    EvaluatedPoint { point, h, extreme }
}

fn consider_candidate(
    candidate: EvaluatedPoint,
    barrier: f64,
    best_feasible: &mut Option<ScoredPoint>,
    best_infeasible: &mut Option<ScoredPoint>,
    current_values: &mut Vec<f64>,
) -> CandidateChange {
    if candidate.extreme {
        return CandidateChange {
            accepted: false,
            infeasible_improved: false,
        };
    }
    if candidate.h == 0.0 {
        let improves = best_feasible
            .as_ref()
            .map(|incumbent| candidate.point.cost.total_cmp(&incumbent.cost).is_lt())
            .unwrap_or(true);
        if improves {
            *current_values = candidate.point.values.clone();
            *best_feasible = Some(candidate.point);
        }
        return CandidateChange {
            accepted: improves,
            infeasible_improved: false,
        };
    }

    if !barrier_allows(candidate.h, barrier) {
        return CandidateChange {
            accepted: false,
            infeasible_improved: false,
        };
    }
    let improves = best_infeasible
        .as_ref()
        .map(|incumbent| {
            strictly_less_violation(candidate.h, incumbent.constraint_violation)
                || (same_violation(candidate.h, incumbent.constraint_violation)
                    && candidate.point.cost.total_cmp(&incumbent.cost).is_lt())
        })
        .unwrap_or(true);
    if improves {
        if best_feasible.is_none() {
            *current_values = candidate.point.values.clone();
        }
        *best_infeasible = Some(candidate.point);
    }
    CandidateChange {
        accepted: improves,
        infeasible_improved: improves,
    }
}

fn barrier_allows(h: f64, barrier: f64) -> bool {
    barrier.is_infinite() || h <= barrier + VIOLATION_RELATIVE_TOLERANCE * barrier.abs().max(1.0)
}

fn same_violation(left: f64, right: f64) -> bool {
    (left - right).abs() <= VIOLATION_RELATIVE_TOLERANCE * left.abs().max(right.abs()).max(1.0)
}

fn strictly_less_violation(left: f64, right: f64) -> bool {
    left + VIOLATION_RELATIVE_TOLERANCE * left.abs().max(right.abs()).max(1.0) < right
}

// Geometry and direction generation stay in a separate module so the search
// driver remains small enough to audit independently.
use super::directions::{
    clamp_to_bounds, initial_search_points, midpoint, minimal_positive_basis, poll_centers,
    poll_directions, poll_point, same_point, to_normalized,
};

#[cfg(test)]
use super::directions::{direction_matrix, full_rank};

#[cfg(test)]
#[path = "mads_tests.rs"]
mod tests;
