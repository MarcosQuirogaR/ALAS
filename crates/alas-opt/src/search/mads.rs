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

use std::time::Instant;

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
    /// Evaluation budget, including the initial point.
    pub max_evaluations: usize,
    /// Reproducible seed for search points and poll directions.
    pub seed: u64,
    /// Initial normalized mesh spacing.
    pub initial_mesh_size: f64,
    /// Smallest normalized mesh spacing at which the run stops.
    pub minimum_mesh_size: f64,
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
    /// The callback budget, including the initial evaluation, was exhausted.
    EvaluationBudget,
    /// The translated mesh reached the configured minimum spacing.
    MeshLimit,
    /// The configured poll iteration count was exhausted.
    IterationLimit,
    /// All candidate poll moves were blocked by fixed bounds.
    FixedBounds,
    /// Bounds, initial values, or mesh settings failed validation.
    InvalidInput,
}

impl TerminationReason {
    /// Stable progress/log label retained for the existing callback channel.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::EvaluationBudget => "evaluation_budget",
            Self::MeshLimit => "mesh_limit",
            Self::IterationLimit => "iteration_limit",
            Self::FixedBounds => "fixed_bounds",
            Self::InvalidInput => "invalid_input",
        }
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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_iterations: 20,
            max_evaluations: 1_000,
            seed: 0,
            initial_mesh_size: 0.25,
            minimum_mesh_size: 1.0e-4,
        }
    }
}

/// Run the bounded MADS progressive-barrier search.
///
/// The callback is invoked at most `settings.max_evaluations` times.  Bounds
/// are finite SI/physical input units supplied by the caller; all poll and
/// mesh arithmetic is performed in the unitless normalized box.  A malformed
/// bound/initial vector or invalid mesh setting returns an invalid fallback
/// without invoking the callback, because this legacy result interface has no
/// `Result` channel for input errors.
pub(crate) fn run(
    bounds: &[(f64, f64)],
    initial: Option<&[f64]>,
    settings: Settings,
    evaluate: &mut dyn FnMut(&[f64]) -> ScoredPoint,
    mut progress: Option<&mut dyn FnMut(&str)>,
) -> MadsOutcome {
    let started = Instant::now();
    if !valid_inputs(bounds, initial, settings) {
        return MadsOutcome {
            outcome: invalid_outcome(bounds, initial),
            termination: TerminationReason::InvalidInput,
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

    // A zero budget is a valid bounded request, but no callback may be made.
    // Return the projected initial vector as an unevaluated extreme barrier.
    if max_evaluations == 0 {
        return MadsOutcome {
            outcome: MethodOutcome {
                winner: invalid_point(current_values),
                pareto_front: Vec::new(),
            },
            termination: TerminationReason::EvaluationBudget,
        };
    }

    let mut evaluations = 0usize;
    let first = evaluate_candidate(&current_values, evaluate);
    evaluations += 1;
    let mut best_feasible = None;
    let mut best_infeasible = None;
    let mut barrier = f64::INFINITY;
    if !first.extreme {
        if first.h == 0.0 {
            best_feasible = Some(first.point.clone());
            barrier = 0.0;
        } else {
            barrier = first.h;
            best_infeasible = Some(first.point.clone());
        }
    }
    let initial_fallback = first.point.clone();

    // MADS permits a bounded search phase before polling.  This deterministic
    // Latin-hypercube phase is snapped to the translated initial mesh so it
    // remains part of the same mesh sequence and is reproducible for a seed.
    if settings.max_iterations > 0 && evaluations < max_evaluations && dimension > 0 {
        let search_points = initial_search_points(bounds, &origin, mesh_size, settings.seed);
        for point in search_points {
            if evaluations >= max_evaluations {
                break;
            }
            if same_point(&point, &current_values, bounds) {
                continue;
            }
            let candidate = evaluate_candidate(&point, evaluate);
            evaluations += 1;
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
            if best_feasible.is_some() {
                barrier = 0.0;
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
        && evaluations < max_evaluations
        && mesh_size > minimum_mesh_size
    {
        let feasible_before = best_feasible.as_ref().map(|point| point.cost);
        let directions = poll_directions(dimension, iteration, settings.seed);
        let centers = poll_centers(&current_values, best_infeasible.as_ref(), bounds);
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

        let mut infeasible_improved = false;
        for (point, direction) in poll {
            if evaluations >= max_evaluations {
                break;
            }
            let candidate = evaluate_candidate(&point, evaluate);
            evaluations += 1;
            let change = consider_candidate(
                candidate,
                barrier,
                &mut best_feasible,
                &mut best_infeasible,
                &mut current_values,
            );
            infeasible_improved |= change.infeasible_improved;
            if change.accepted {
                last_success_direction = Some(direction);
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
                "mads iteration {iteration} | evaluations {evaluations} | mesh {:.3e} | frame {:.3e} | barrier {:.3e} | feasible {} | violation {:.3e} | objective {:.6}",
                mesh_size,
                frame_size,
                barrier,
                point.valid,
                point.constraint_violation,
                point.cost
            ));
        }
    }

    if evaluations >= max_evaluations {
        termination = TerminationReason::EvaluationBudget;
    } else if mesh_size <= minimum_mesh_size {
        termination = TerminationReason::MeshLimit;
    } else if iteration >= settings.max_iterations {
        termination = TerminationReason::IterationLimit;
    }
    if let Some(callback) = progress.as_mut() {
        let point = best_feasible
            .as_ref()
            .or(best_infeasible.as_ref())
            .unwrap_or(&initial_fallback);
        (**callback)(&format!(
            "mads termination {} | evaluations {evaluations} | mesh {:.3e} | feasible {} | violation {:.3e} | objective {:.6} | feasible_found {} | elapsed_s {:.3e}",
            termination.as_str(),
            mesh_size,
            point.valid,
            point.constraint_violation,
            point.cost,
            best_feasible.is_some(),
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

fn evaluate_candidate(
    values: &[f64],
    evaluate: &mut dyn FnMut(&[f64]) -> ScoredPoint,
) -> EvaluatedPoint {
    let mut point = evaluate(values);
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
#[path = "directions.rs"]
mod directions;
use directions::{
    clamp_to_bounds, initial_search_points, midpoint, poll_centers, poll_directions, poll_point,
    same_point, to_normalized,
};

#[cfg(test)]
use directions::{direction_matrix, full_rank};

#[cfg(test)]
#[path = "mads_tests.rs"]
mod tests;
