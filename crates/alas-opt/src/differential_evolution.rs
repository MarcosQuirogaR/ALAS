// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Global gradient-free design optimization using Differential Evolution.

mod diagnostics;
mod optimizer;
mod result;
#[cfg(test)]
mod tests;
mod trial;

use trial::{
    converged, latin_hypercube_population, promote_best, trial_vector, TrialInputs, TrialState,
};

pub use diagnostics::{DiagnosticSearchOutcome, NoFeasibleDesign, RestorationDiagnostics};
pub use result::{
    DeliveredAcceptance, OptimizationError, OptimizationResult, ParetoCandidate, SearchDiagnostics,
    CANCELLED, REPORTING_FIDELITY_FALLBACK, REPORTING_FIDELITY_REJECTED,
};

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;

use crate::evaluator::{ObjectiveEvaluation, ObjectiveEvaluator};
use crate::history::OptimizationHistory;
use crate::objective::{restore_winning_payload_load_case, DesignObjective};
use crate::python_rng::{Pcg64, RandomState};
use crate::search_methods::{MethodOutcome, ScoredPoint};

/// Searches the aircraft design space to minimize the [`DesignObjective`].
#[derive(Debug, Clone)]
pub struct DesignOptimizer {
    /// Active aircraft configuration.
    pub config: AlasConfig,
    reference_mass_coordinates: bool,
}

fn runtime_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0)
}

fn validate_bounds(bounds: &[(f64, f64)]) -> Result<(), OptimizationError> {
    if bounds.len() != alas_config::DESIGN_VARIABLE_SPECS.len() {
        return Err(OptimizationError::InvalidBounds(format!(
            "expected {} design-variable bounds, got {}",
            alas_config::DESIGN_VARIABLE_SPECS.len(),
            bounds.len()
        )));
    }
    for (index, &(lower, upper)) in bounds.iter().enumerate() {
        if !lower.is_finite() || !upper.is_finite() {
            return Err(OptimizationError::InvalidBounds(format!(
                "bound {index} must contain finite values, got [{lower:?}, {upper:?}]"
            )));
        }
        if lower > upper {
            return Err(OptimizationError::InvalidBounds(format!(
                "bound {index} has lower {lower} greater than upper {upper}"
            )));
        }
        if !(upper - lower).is_finite() {
            return Err(OptimizationError::InvalidBounds(format!(
                "bound {index} has a non-finite width [{lower}, {upper}]"
            )));
        }
    }
    Ok(())
}

fn ensure_feasible(result: OptimizationResult) -> Result<OptimizationResult, OptimizationError> {
    if result.best_valid {
        Ok(result)
    } else {
        Err(OptimizationError::NoFeasibleDesign(
            NoFeasibleDesign::from_history(&result.history),
        ))
    }
}

/// The search's view of one recorded evaluation, read at its own history row.
///
/// A batched evaluation appends one row per candidate in candidate order, so a
/// block's scores must be read at their own indices: reading the last row
/// would give every point in the block the score of whichever candidate
/// happened to be appended last.
fn scored_point_at(
    values: &[f64],
    cost: f64,
    history: &OptimizationHistory,
    index: usize,
) -> ScoredPoint {
    let valid = history.valid.get(index).copied().unwrap_or(false) && cost.is_finite();
    let l_over_d = history.l_over_d.get(index).copied().unwrap_or(0.0);
    // The mission objective when the native path recorded one; the scalar
    // cost is the only objective a delegated evaluator reports.
    let objective_value = history
        .objective_value
        .get(index)
        .copied()
        .filter(|value| value.is_finite())
        .unwrap_or(cost);
    let span_m = history.span_m.get(index).copied().unwrap_or(f64::INFINITY);
    let area_m2 = history.area_m2.get(index).copied().unwrap_or(f64::INFINITY);
    let reason = history
        .reject_reason
        .get(index)
        .map(String::as_str)
        .unwrap_or("evaluation_failure");
    let hard_violation = history
        .hard_violation
        .get(index)
        .copied()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(f64::NAN);
    let constraint_violation = if valid {
        // A strictly feasible candidate has no hard violation at all, so this
        // is zero and the ranking key reduces to the objective, exactly as
        // before. A candidate admitted only by the controlled-relaxation
        // policy still carries the violations that were relaxed, and the key
        // is lexicographic in (admissible, violation, cost), so every fully
        // feasible design ranks ahead of every relaxed one whatever their
        // objectives - obtained without a tuned penalty.
        if hard_violation.is_finite() && hard_violation > 0.0 {
            hard_violation
        } else {
            0.0
        }
    } else if hard_violation.is_finite() && hard_violation > 0.0 {
        // A physical miss is ordered by its dimensionless aggregate
        // violation. Counting reject labels made a severe single miss appear
        // better than several small misses and discarded the actual physics.
        hard_violation
    } else {
        // Analysis failures are an extreme barrier and remain behind every
        // completed physical miss, even when the delegated evaluator did not
        // provide a residual table.
        let category_count = reason
            .split('+')
            .filter(|part| !part.is_empty())
            .count()
            .max(1) as f64;
        if !l_over_d.is_finite() || l_over_d <= 0.0 {
            1_000_000.0 + category_count
        } else {
            category_count
        }
    };
    ScoredPoint {
        values: values.to_vec(),
        cost,
        valid,
        constraint_violation,
        objectives: [objective_value, span_m, area_m2],
    }
}

fn result_from_method(
    outcome: MethodOutcome,
    method: &str,
    strategy: &str,
    termination: &str,
    history: &OptimizationHistory,
    wall_time_s: f64,
) -> OptimizationResult {
    let winner = outcome.winner;
    let best_design = DesignVector::from_array(&winner.values).unwrap_or_default();
    let pareto_front = outcome
        .pareto_front
        .into_iter()
        .filter_map(|point| {
            let design = DesignVector::from_array(&point.values).ok()?;
            Some(ParetoCandidate {
                design,
                cost: point.cost,
                objective_value: point.objectives[0],
                span_m: point.objectives[1],
                area_m2: point.objectives[2],
                valid: point.valid,
            })
        })
        .collect();
    OptimizationResult {
        best_design,
        best_cost: winner.cost,
        best_valid: winner.valid,
        history: history.clone(),
        wall_time_s,
        method: method.to_owned(),
        strategy: strategy.to_owned(),
        termination: termination.to_owned(),
        pareto_front,
        // Filled in by the product search, which is the only caller that
        // measures its own lifecycle.
        search_diagnostics: None,
        // Filled in by the application that re-evaluates the finalist at
        // reporting fidelity; the search cannot answer this about itself.
        delivered_acceptance: None,
    }
}

trait SearchObjective {
    fn evaluate(&mut self, design: &[f64]) -> f64;
    fn history(&self) -> &OptimizationHistory;

    /// Evaluate a candidate batch and return `(cost, valid)` in input order.
    ///
    /// Backends with mutable external state use the serial default. The native
    /// objective overrides this to parallelize its CPU-bound, cloned analyses.
    fn evaluate_batch(&mut self, designs: &[Vec<f64>], _workers: usize) -> Vec<(f64, bool)> {
        designs
            .iter()
            .map(|design| {
                let cost = self.evaluate(design);
                let valid =
                    self.history().valid.last().copied().unwrap_or(false) && cost.is_finite();
                (cost, valid)
            })
            .collect()
    }
}

impl SearchObjective for DesignObjective {
    fn evaluate(&mut self, design: &[f64]) -> f64 {
        DesignObjective::evaluate(self, design)
    }

    fn history(&self) -> &OptimizationHistory {
        &self.history
    }

    fn evaluate_batch(&mut self, designs: &[Vec<f64>], workers: usize) -> Vec<(f64, bool)> {
        if workers <= 1 || designs.len() <= 1 {
            return designs
                .iter()
                .map(|design| {
                    let cost = DesignObjective::evaluate(self, design);
                    let valid =
                        self.history.valid.last().copied().unwrap_or(false) && cost.is_finite();
                    (cost, valid)
                })
                .collect();
        }

        let worker_count = workers.min(designs.len());
        let chunk_size = designs.len().div_ceil(worker_count);
        let mut baseline = self.clone();
        // Only the new batch belongs in each worker's returned trace. The
        // caller's prior history is merged once after all joins succeed.
        baseline.history = OptimizationHistory::new();

        let mut handles = Vec::with_capacity(worker_count);
        for chunk in designs.chunks(chunk_size) {
            let mut local = baseline.clone();
            let candidates = chunk.to_vec();
            handles.push(std::thread::spawn(move || {
                let mut evaluations = Vec::with_capacity(candidates.len());
                for candidate in candidates {
                    let cost = DesignObjective::evaluate(&mut local, &candidate);
                    let valid =
                        local.history.valid.last().copied().unwrap_or(false) && cost.is_finite();
                    evaluations.push((cost, valid));
                }
                (evaluations, local.history)
            }));
        }

        let mut merged = Vec::with_capacity(designs.len());
        let mut histories = Vec::with_capacity(handles.len());
        for handle in handles {
            match handle.join() {
                Ok((evaluations, history)) => {
                    merged.extend(evaluations);
                    histories.push(history);
                }
                Err(_) => {
                    // A worker panic is not allowed to lose the optimizer's
                    // history or leave it partially merged. Re-run this batch
                    // serially in the caller thread, where any ordinary
                    // objective rejection remains represented as data.
                    return designs
                        .iter()
                        .map(|design| {
                            let cost = DesignObjective::evaluate(self, design);
                            let valid = self.history.valid.last().copied().unwrap_or(false)
                                && cost.is_finite();
                            (cost, valid)
                        })
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

struct DelegatedObjective<'a, E: ObjectiveEvaluator + ?Sized> {
    evaluator: &'a mut E,
    history: OptimizationHistory,
    failure_cost: f64,
}

impl<'a, E: ObjectiveEvaluator + ?Sized> DelegatedObjective<'a, E> {
    fn new(evaluator: &'a mut E, failure_cost: f64) -> Self {
        Self {
            evaluator,
            history: OptimizationHistory::new(),
            failure_cost,
        }
    }
}

impl<E: ObjectiveEvaluator + ?Sized> SearchObjective for DelegatedObjective<'_, E> {
    fn evaluate(&mut self, design: &[f64]) -> f64 {
        let typed = match DesignVector::from_array(design) {
            Ok(value) => value,
            Err(_) => {
                let evaluation = ObjectiveEvaluation::rejected(self.failure_cost, "geometry_build");
                self.history.record(
                    DesignVector::default(),
                    evaluation.valid,
                    evaluation.cost,
                    evaluation.l_over_d,
                    evaluation.span_m,
                    evaluation.alpha_deg,
                    evaluation.area_m2,
                    evaluation.trim_ih_deg,
                    evaluation.reject_reason,
                );
                return evaluation.cost;
            }
        };
        let evaluation = self.evaluator.evaluate(&typed);
        self.history.record(
            typed,
            evaluation.valid,
            evaluation.cost,
            evaluation.l_over_d,
            evaluation.span_m,
            evaluation.alpha_deg,
            evaluation.area_m2,
            evaluation.trim_ih_deg,
            evaluation.reject_reason,
        );
        evaluation.cost
    }

    fn history(&self) -> &OptimizationHistory {
        &self.history
    }
}

fn candidate_is_better(
    candidate_cost: f64,
    candidate_valid: bool,
    incumbent_cost: f64,
    incumbent_valid: bool,
    feasibility_first: bool,
) -> bool {
    if feasibility_first && candidate_valid != incumbent_valid {
        return candidate_valid;
    }
    candidate_cost.total_cmp(&incumbent_cost).is_lt()
}

fn candidate_is_at_least_as_good(
    candidate_cost: f64,
    candidate_valid: bool,
    incumbent_cost: f64,
    incumbent_valid: bool,
    feasibility_first: bool,
) -> bool {
    if feasibility_first && candidate_valid != incumbent_valid {
        return candidate_valid;
    }
    candidate_cost <= incumbent_cost
}
