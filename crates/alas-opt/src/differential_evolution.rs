// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/optimization/optimizer.py
// Reference: alas @ rust-port-baseline.

//! Global gradient-free design optimization using Differential Evolution.

use std::collections::BTreeMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use serde::{Deserialize, Serialize};

use crate::evaluator::{ObjectiveEvaluation, ObjectiveEvaluator};
use crate::history::OptimizationHistory;
use crate::objective::{apply_candidate_payload_load_case, DesignObjective};
use crate::python_rng::{Pcg64, RandomState};
use crate::search_methods::{
    run_cma_es, run_feasibility_first_de, run_nsga2, run_turbo_1, MethodOutcome, ScoredPoint,
};

/// One member of a retained multi-objective Pareto set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParetoCandidate {
    /// Candidate design vector.
    pub design: DesignVector,
    /// Scalar objective retained for deterministic downstream winner selection.
    pub cost: f64,
    /// Lift-to-drag ratio objective.
    pub l_over_d: f64,
    /// Wing span objective, in meters.
    pub span_m: f64,
    /// Wing reference area objective, in square meters.
    pub area_m2: f64,
    /// Whether every product feasibility check passed.
    pub valid: bool,
}

/// Outcome of a design optimization run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptimizationResult {
    /// Winning design candidate found by the optimizer.
    pub best_design: DesignVector,
    /// Objective function cost of the winning design.
    pub best_cost: f64,
    /// Whether the winning design passed the evaluator's physical checks.
    ///
    /// A search with no feasible candidate is represented explicitly instead
    /// of handing a lower-cost invalid design to the downstream pipeline.
    #[serde(default)]
    pub best_valid: bool,
    /// Full evaluation history collected during the run.
    pub history: OptimizationHistory,
    /// Elapsed wall-clock time in seconds.
    pub wall_time_s: f64,
    /// Stable identifier of the search method that produced this result.
    #[serde(default = "default_result_method")]
    pub method: String,
    /// Mutation/crossover strategy used by the search.
    #[serde(default = "default_result_strategy")]
    pub strategy: String,
    /// Final nondominated set for a multi-objective method.
    #[serde(default)]
    pub pareto_front: Vec<ParetoCandidate>,
}

/// Evidence returned when a search evaluated candidates but none passed the
/// objective's physical validity checks.
///
/// Rejected candidates are deliberately not promoted to
/// [`OptimizationResult`].  A caller that wants to inspect the failed search
/// can use the counts below without accidentally treating a review artifact as
/// an aircraft design.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoFeasibleDesign {
    /// Number of candidates evaluated before the search ended.
    pub evaluated_candidates: usize,
    /// Counts of the machine-readable rejection categories observed.
    pub rejection_reason_counts: BTreeMap<String, usize>,
}

impl NoFeasibleDesign {
    fn from_history(history: &OptimizationHistory) -> Self {
        let mut rejection_reason_counts = BTreeMap::new();
        for reason in &history.reject_reason {
            for category in reason.split('+').filter(|category| !category.is_empty()) {
                *rejection_reason_counts
                    .entry(category.to_owned())
                    .or_insert(0) += 1;
            }
        }
        if rejection_reason_counts.is_empty() && history.n_evaluations() > 0 {
            rejection_reason_counts.insert("unknown".to_owned(), history.n_evaluations());
        }
        Self {
            evaluated_candidates: history.n_evaluations(),
            rejection_reason_counts,
        }
    }
}

/// Failure from the public optimizer boundary.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OptimizationError {
    /// The selected method or strategy is not implemented by this build.
    #[error("invalid optimizer configuration: {0}")]
    InvalidConfiguration(String),
    /// The supplied design-space bounds cannot be searched safely.
    #[error("invalid optimizer bounds: {0}")]
    InvalidBounds(String),
    /// Every evaluated candidate was rejected by the physical objective.
    #[error("no feasible design: {0:?}")]
    NoFeasibleDesign(NoFeasibleDesign),
}

fn default_result_method() -> String {
    "differential_evolution".to_owned()
}

fn default_result_strategy() -> String {
    "best1bin".to_owned()
}

/// Searches the aircraft design space to minimize the [`DesignObjective`].
#[derive(Debug, Clone)]
pub struct DesignOptimizer {
    /// Active aircraft configuration.
    pub config: AlasConfig,
    reference_mass_coordinates: bool,
}

#[path = "differential_evolution_optimizer.rs"]
mod differential_evolution_optimizer;
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

fn scored_point(values: &[f64], cost: f64, history: &OptimizationHistory) -> ScoredPoint {
    let index = history.n_evaluations().saturating_sub(1);
    let valid = history.valid.get(index).copied().unwrap_or(false) && cost.is_finite();
    let l_over_d = history.l_over_d.get(index).copied().unwrap_or(0.0);
    let span_m = history.span_m.get(index).copied().unwrap_or(f64::INFINITY);
    let area_m2 = history.area_m2.get(index).copied().unwrap_or(f64::INFINITY);
    let reason = history
        .reject_reason
        .get(index)
        .map(String::as_str)
        .unwrap_or("evaluation_failure");
    let constraint_violation = if valid {
        0.0
    } else {
        let category_count = reason
            .split('+')
            .filter(|part| !part.is_empty())
            .count()
            .max(1) as f64;
        // Solver failures must not outrank recoverable constraint misses.
        if !l_over_d.is_finite() || l_over_d <= 0.0 {
            1_000.0 + category_count
        } else {
            category_count
        }
    };
    ScoredPoint {
        values: values.to_vec(),
        cost,
        valid,
        constraint_violation,
        objectives: [
            if l_over_d.is_finite() {
                -l_over_d
            } else {
                f64::INFINITY
            },
            span_m,
            area_m2,
        ],
    }
}

fn result_from_method(
    outcome: MethodOutcome,
    method: &str,
    strategy: &str,
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
                l_over_d: -point.objectives[0],
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
        pareto_front,
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

struct TrialState<'a> {
    population: &'a mut [Vec<f64>],
    costs: &'a mut [f64],
    validities: &'a mut [bool],
    feasibility_first: bool,
}

impl TrialState<'_> {
    fn apply(&mut self, index: usize, trial: Vec<f64>, trial_cost: f64, trial_valid: bool) {
        if candidate_is_at_least_as_good(
            trial_cost,
            trial_valid,
            self.costs[index],
            self.validities[index],
            self.feasibility_first,
        ) {
            self.population[index] = trial;
            self.costs[index] = trial_cost;
            self.validities[index] = trial_valid;
            if candidate_is_better(
                trial_cost,
                trial_valid,
                self.costs[0],
                self.validities[0],
                self.feasibility_first,
            ) {
                self.population.swap(0, index);
                self.costs.swap(0, index);
                self.validities.swap(0, index);
            }
        }
    }
}

fn promote_best(
    population: &mut [Vec<f64>],
    costs: &mut [f64],
    validities: &mut [bool],
    feasibility_first: bool,
) {
    let Some((best_index, _)) =
        costs
            .iter()
            .enumerate()
            .min_by(|(left, left_cost), (right, right_cost)| {
                if candidate_is_better(
                    **left_cost,
                    validities[*left],
                    **right_cost,
                    validities[*right],
                    feasibility_first,
                ) {
                    std::cmp::Ordering::Less
                } else if candidate_is_better(
                    **right_cost,
                    validities[*right],
                    **left_cost,
                    validities[*left],
                    feasibility_first,
                ) {
                    std::cmp::Ordering::Greater
                } else {
                    left.cmp(right)
                }
            })
    else {
        return;
    };
    population.swap(0, best_index);
    costs.swap(0, best_index);
    validities.swap(0, best_index);
}

fn latin_hypercube_population(
    bounds: &[(f64, f64)],
    population_size: usize,
    rng: &mut RandomState,
) -> Vec<Vec<f64>> {
    let mut samples = vec![vec![0.0; bounds.len()]; population_size];
    let segment_size = 1.0 / population_size as f64;

    for (row, sample) in samples.iter_mut().enumerate() {
        for value in sample {
            *value = (row as f64 + rng.uniform(0.0, 1.0)) * segment_size;
        }
    }

    for (dimension, _) in bounds.iter().enumerate() {
        let mut order: Vec<usize> = (0..population_size).collect();
        shuffle(&mut order, rng);
        let column: Vec<f64> = order.iter().map(|&row| samples[row][dimension]).collect();
        for (row, value) in column.into_iter().enumerate() {
            samples[row][dimension] = value;
        }
    }

    samples
        .into_iter()
        .map(|sample| {
            sample
                .into_iter()
                .zip(bounds)
                .map(|(normalized, &(lo, hi))| {
                    // Keep the generated point closed over the configured
                    // interval even when a floating-point product rounds one
                    // ulp past an exact endpoint.
                    (lo + normalized * (hi - lo)).clamp(lo, hi)
                })
                .collect()
        })
        .collect()
}

fn shuffle<T>(values: &mut [T], rng: &mut RandomState) {
    for position in (1..values.len()).rev() {
        let swap_with = rng.randint(position + 1);
        values.swap(position, swap_with);
    }
}

fn select_samples(
    candidate: usize,
    number: usize,
    indices: &mut [usize],
    rng: &mut RandomState,
) -> Vec<usize> {
    shuffle(indices, rng);

    // Match SciPy's shuffled target-removal order.
    indices
        .iter()
        .copied()
        .take(number + 1)
        .filter(|&index| index != candidate)
        .take(number)
        .collect()
}

struct TrialInputs<'a> {
    population: &'a [Vec<f64>],
    bounds: &'a [(f64, f64)],
    strategy: &'a str,
    f_weight: f64,
    crossover_probability: f64,
    sample_indices: &'a mut [usize],
    rng: &'a mut RandomState,
}

fn trial_vector(candidate: usize, inputs: &mut TrialInputs<'_>) -> Vec<f64> {
    let n_dof = inputs.bounds.len();
    let fill_point = inputs.rng.randint(n_dof);
    let samples = select_samples(candidate, 5, inputs.sample_indices, inputs.rng);
    let best = &inputs.population[0];
    let current = &inputs.population[candidate];
    let vector = |index: usize| -> &Vec<f64> { &inputs.population[samples[index]] };

    let mut mutant = vec![0.0; n_dof];
    for d in 0..n_dof {
        mutant[d] = match inputs.strategy {
            "rand1bin" | "rand1exp" => {
                vector(0)[d] + inputs.f_weight * (vector(1)[d] - vector(2)[d])
            }
            "best2bin" | "best2exp" => {
                best[d]
                    + inputs.f_weight * (vector(0)[d] + vector(1)[d] - vector(2)[d] - vector(3)[d])
            }
            "rand2bin" | "rand2exp" => {
                vector(0)[d]
                    + inputs.f_weight * (vector(1)[d] + vector(2)[d] - vector(3)[d] - vector(4)[d])
            }
            "randtobest1bin" | "randtobest1exp" => {
                vector(0)[d]
                    + inputs.f_weight * (best[d] - vector(0)[d])
                    + inputs.f_weight * (vector(1)[d] - vector(2)[d])
            }
            "currenttobest1bin" | "currenttobest1exp" => {
                current[d] + inputs.f_weight * (best[d] - current[d] + vector(0)[d] - vector(1)[d])
            }
            _ => best[d] + inputs.f_weight * (vector(0)[d] - vector(1)[d]),
        };
    }

    let is_exponential = inputs.strategy.ends_with("exp");
    let mut crossovers = Vec::with_capacity(n_dof);
    for _ in 0..n_dof {
        crossovers.push(inputs.rng.uniform(0.0, 1.0) < inputs.crossover_probability);
    }

    let mut trial = current.clone();
    if is_exponential {
        crossovers[0] = true;
        let mut index = 0;
        let mut destination = fill_point;
        while index < n_dof && crossovers[index] {
            trial[destination] = mutant[destination];
            destination = (destination + 1) % n_dof;
            index += 1;
        }
    } else {
        crossovers[fill_point] = true;
        for d in 0..n_dof {
            if crossovers[d] {
                trial[d] = mutant[d];
            }
        }
    }

    // SciPy resamples out-of-range coordinates instead of clamping them.
    for (value, &(lo, hi)) in trial.iter_mut().zip(inputs.bounds) {
        if !value.is_finite() || *value < lo || *value > hi {
            *value = inputs.rng.uniform(lo, hi).clamp(lo, hi);
        }
    }
    trial
}

fn converged(costs: &[f64], tolerance: f64) -> bool {
    if costs.iter().any(|cost| !cost.is_finite()) || costs.is_empty() {
        return false;
    }
    let mean = costs.iter().sum::<f64>() / costs.len() as f64;
    let variance = costs
        .iter()
        .map(|cost| {
            let delta = cost - mean;
            delta * delta
        })
        .sum::<f64>()
        / costs.len() as f64;
    variance.sqrt() <= tolerance * mean.abs()
}

#[cfg(test)]
#[path = "differential_evolution_tests.rs"]
mod tests;
