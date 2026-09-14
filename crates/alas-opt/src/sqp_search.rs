// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The `sqp` product method: the gradient driver of `crate::gradient` run
//! against the design objective, with the residual table of the
//! mission-sized objective exposed as individual constraints.
//!
//! The native objective evaluates finite-difference batches on worker
//! threads exactly as the population methods do, merging each worker's
//! history in candidate order. A delegated evaluator (the AVL adapter, a
//! test double) sees only a scalar cost and a validity flag, so under it the
//! driver reduces to a bound-constrained search that treats invalid
//! candidates as failed steps.

use std::time::Instant;

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;

use crate::evaluator::ObjectiveEvaluator;
use crate::gradient::{run_sqp, ConstrainedEvaluator, ConstrainedPoint, SqpSettings};
use crate::history::OptimizationHistory;
use crate::mdo::evaluate_mission_sized_with_assessment;
use crate::objective::DesignObjective;

use super::{DelegatedObjective, OptimizationResult, SearchObjective};

/// A search objective that can also report the constraint vector.
pub(super) trait ConstrainedSearch: SearchObjective {
    /// Evaluate `designs` (physical units), recording history, and return
    /// the driver's view of each.
    fn evaluate_constrained_batch(
        &mut self,
        designs: &[Vec<f64>],
        workers: usize,
    ) -> Vec<ConstrainedPoint>;
}

/// The driver's view of one native evaluation.
fn native_point(objective: &mut DesignObjective, design: &[f64]) -> ConstrainedPoint {
    if !objective.is_reference_replay() {
        let (cost, assessment) = evaluate_mission_sized_with_assessment(objective, design);
        return match assessment {
            Some(assessment) => {
                // The cost is the normalised objective plus the weighted
                // soft residuals, plus a feasibility offset when a hard
                // residual is violated (`mdo::cost::assemble`). The driver
                // minimises the first two and receives every hard residual
                // as a constraint, so the offset is removed here.
                let hard_term = if assessment.hard_feasible {
                    0.0
                } else {
                    1.0 + assessment.hard_violation_sum
                };
                let objective_value = cost - hard_term;
                ConstrainedPoint {
                    objective: objective_value,
                    constraints: assessment
                        .residuals
                        .iter()
                        .filter(|residual| residual.policy == alas_config::ConstraintPolicy::Hard)
                        .map(|residual| residual.signed_normalized())
                        .collect(),
                    valid: cost.is_finite(),
                    cost,
                }
            }
            None => ConstrainedPoint::invalid(cost),
        };
    }
    let cost = DesignObjective::evaluate(objective, design);
    let valid = objective.history.valid.last().copied().unwrap_or(false) && cost.is_finite();
    if valid {
        ConstrainedPoint {
            objective: cost,
            constraints: Vec::new(),
            valid: true,
            cost,
        }
    } else {
        ConstrainedPoint::invalid(cost)
    }
}

impl ConstrainedSearch for DesignObjective {
    fn evaluate_constrained_batch(
        &mut self,
        designs: &[Vec<f64>],
        workers: usize,
    ) -> Vec<ConstrainedPoint> {
        if workers <= 1 || designs.len() <= 1 {
            return designs
                .iter()
                .map(|design| native_point(self, design))
                .collect();
        }
        let worker_count = workers.min(designs.len());
        let chunk_size = designs.len().div_ceil(worker_count);
        let mut baseline = self.clone();
        baseline.history = OptimizationHistory::new();
        let mut handles = Vec::with_capacity(worker_count);
        for chunk in designs.chunks(chunk_size) {
            let mut local = baseline.clone();
            let candidates = chunk.to_vec();
            handles.push(std::thread::spawn(move || {
                let points: Vec<ConstrainedPoint> = candidates
                    .iter()
                    .map(|design| native_point(&mut local, design))
                    .collect();
                (points, local.history)
            }));
        }
        let mut merged = Vec::with_capacity(designs.len());
        let mut histories = Vec::with_capacity(handles.len());
        for handle in handles {
            match handle.join() {
                Ok((points, history)) => {
                    merged.extend(points);
                    histories.push(history);
                }
                Err(_) => {
                    // A worker panic must not lose or partially merge the
                    // history; re-run the whole batch serially here.
                    return designs
                        .iter()
                        .map(|design| native_point(self, design))
                        .collect();
                }
            }
        }
        for history in histories {
            self.history.append(history);
        }
        merged
    }
}

impl<E: ObjectiveEvaluator + ?Sized> ConstrainedSearch for DelegatedObjective<'_, E> {
    fn evaluate_constrained_batch(
        &mut self,
        designs: &[Vec<f64>],
        _workers: usize,
    ) -> Vec<ConstrainedPoint> {
        designs
            .iter()
            .map(|design| {
                let cost = SearchObjective::evaluate(self, design);
                let valid =
                    self.history().valid.last().copied().unwrap_or(false) && cost.is_finite();
                if valid {
                    ConstrainedPoint {
                        objective: cost,
                        constraints: Vec::new(),
                        valid: true,
                        cost,
                    }
                } else {
                    ConstrainedPoint::invalid(cost)
                }
            })
            .collect()
    }
}

struct BatchAdapter<'a, E: ConstrainedSearch + ?Sized> {
    objective: &'a mut E,
    workers: usize,
}

impl<E: ConstrainedSearch + ?Sized> ConstrainedEvaluator for BatchAdapter<'_, E> {
    fn evaluate_batch(&mut self, designs: &[Vec<f64>]) -> Vec<ConstrainedPoint> {
        self.objective
            .evaluate_constrained_batch(designs, self.workers)
    }
}

/// Run the `sqp` method and adapt its outcome to the optimizer result.
pub(super) fn run<E: ConstrainedSearch + ?Sized>(
    config: &AlasConfig,
    bounds: &[(f64, f64)],
    initial_design: Option<&DesignVector>,
    objective: &mut E,
    progress_callback: Option<&mut dyn FnMut(&str)>,
) -> OptimizationResult {
    let solver = &config.optimizer.solver;
    let started = Instant::now();
    let settings = SqpSettings {
        max_iterations: solver.max_iterations.max(1) as usize,
        finite_difference_step: solver.finite_difference_step.max(1e-9),
        constraint_tolerance: solver.constraint_tolerance.max(0.0),
        objective_tolerance: solver.tolerance.max(0.0),
        step_tolerance: 1.0e-4,
    };
    let initial = initial_design.copied().unwrap_or_default().to_array();
    let workers = solver.workers.max(1) as usize;
    let outcome = {
        let mut adapter = BatchAdapter { objective, workers };
        run_sqp(bounds, &initial, &settings, &mut adapter, progress_callback)
    };
    let best_design = DesignVector::from_array(&outcome.best_values).unwrap_or_default();
    OptimizationResult {
        best_design,
        best_cost: outcome.best.cost,
        best_valid: outcome.best.valid
            && outcome.best.max_violation() <= settings.constraint_tolerance,
        history: objective.history().clone(),
        wall_time_s: started.elapsed().as_secs_f64(),
        method: "sqp".to_owned(),
        strategy: outcome.termination.to_owned(),
        termination: outcome.termination.to_owned(),
        pareto_front: Vec::new(),
    }
}
