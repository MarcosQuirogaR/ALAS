// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/optimization/optimizer.py
// Reference: alas @ rust-port-baseline.

//! Global gradient-free design optimization using Differential Evolution.

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
    /// Full evaluation history collected during the run.
    pub history: OptimizationHistory,
    /// Elapsed wall-clock time in seconds.
    pub wall_time_s: f64,
    /// Stable identifier of the search method that produced this result.
    #[serde(default = "default_result_method")]
    pub method: String,
    /// Final nondominated set for a multi-objective method.
    #[serde(default)]
    pub pareto_front: Vec<ParetoCandidate>,
}

fn default_result_method() -> String {
    "differential_evolution".to_owned()
}

/// Searches the aircraft design space to minimize the [`DesignObjective`].
#[derive(Debug, Clone)]
pub struct DesignOptimizer {
    /// Active aircraft configuration.
    pub config: AlasConfig,
    reference_mass_coordinates: bool,
}

impl DesignOptimizer {
    /// Construct a new design optimizer with `config`.
    pub fn new(config: AlasConfig) -> Self {
        Self {
            config,
            reference_mass_coordinates: false,
        }
    }

    /// Construct an optimizer that replays the frozen Python mass coordinate.
    ///
    /// Product runs use [`Self::new`]. This compatibility constructor exists
    /// only for the differential-evolution parity fixture, so a physical
    /// improvement does not masquerade as a translation discrepancy.
    pub fn new_reference_compatibility(config: AlasConfig) -> Self {
        Self {
            config,
            reference_mass_coordinates: true,
        }
    }

    /// Execute the Differential Evolution optimization search.
    pub fn run(
        &mut self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
        progress_callback: Option<&mut dyn FnMut(&str)>,
    ) -> OptimizationResult {
        let mut objective = if self.reference_mass_coordinates {
            DesignObjective::new_reference_compatibility(self.config.clone())
        } else {
            DesignObjective::new(self.config.clone())
        };

        let result = if self.reference_mass_coordinates
            || self.config.optimizer.solver.method == "differential_evolution"
        {
            self.run_search(bounds, initial_design, &mut objective, progress_callback)
        } else {
            self.run_product_search(bounds, initial_design, &mut objective, progress_callback)
        };

        // Keep the final configuration on the same explicit payload load case
        // used to score every candidate.
        let _ = apply_candidate_payload_load_case(&mut self.config, &result.best_design);
        result
    }

    /// Execute the same differential-evolution search with a caller-provided
    /// aerodynamic objective.
    ///
    /// The evaluator receives a typed [`DesignVector`] and returns the same
    /// diagnostics recorded by the native VLM objective. This is the seam used
    /// by the pipeline's AVL adapter; it deliberately contains no process or
    /// CPACS dependency.
    pub fn run_with_evaluator<E: ObjectiveEvaluator + ?Sized>(
        &mut self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
        evaluator: &mut E,
        progress_callback: Option<&mut dyn FnMut(&str)>,
    ) -> OptimizationResult {
        let mut objective =
            DelegatedObjective::new(evaluator, self.config.optimizer.weights.failure_cost);
        let result = if self.reference_mass_coordinates
            || self.config.optimizer.solver.method == "differential_evolution"
        {
            self.run_search(bounds, initial_design, &mut objective, progress_callback)
        } else {
            self.run_product_search(bounds, initial_design, &mut objective, progress_callback)
        };

        let _ = apply_candidate_payload_load_case(&mut self.config, &result.best_design);
        result
    }

    fn run_search<E: SearchObjective>(
        &self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
        objective: &mut E,
        mut progress_callback: Option<&mut dyn FnMut(&str)>,
    ) -> OptimizationResult {
        let solver = self.config.optimizer.solver.clone();

        let default_bounds = DesignVector::bounds();
        let bounds_slice = bounds.unwrap_or(&default_bounds);
        let n_dof = bounds_slice.len();

        let pop_mult = solver.population_size.max(1) as usize;
        let pop_size = pop_mult * n_dof;
        let seed_val = solver.seed.map(|seed| seed as u64).unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos() as u64)
                .unwrap_or(0)
        });
        // Python deliberately uses one generator for the seeded initial array
        // and a separately seeded SciPy generator for evolution. Reusing one
        // stream here shifts every mutation after generation zero.
        let mut init_rng = Pcg64::seed(seed_val);
        let mut rng = RandomState::seed(seed_val);

        // Initialize population
        let mut population: Vec<Vec<f64>> = Vec::with_capacity(pop_size);

        let mut seeded = false;
        if solver.seed_near_initial_design {
            if let Some(x0) = initial_design {
                let x0_arr = x0.to_array();
                let in_bounds = x0_arr
                    .iter()
                    .zip(bounds_slice)
                    .all(|(&val, &(lo, hi))| val >= lo && val <= hi);
                if in_bounds {
                    for _ in 0..pop_size {
                        let mut ind = Vec::with_capacity(n_dof);
                        for (j, &(lo, hi)) in bounds_slice.iter().enumerate() {
                            let span = hi - lo;
                            let jitter = init_rng.uniform(-1.0, 1.0)
                                * span
                                * solver.seed_perturbation_fraction;
                            let val = (x0_arr[j] + jitter).clamp(lo, hi);
                            ind.push(val);
                        }
                        population.push(ind);
                    }
                    // NumPy constructs the complete jitter array, then
                    // overwrites row zero with the unperturbed design.
                    population[0] = x0_arr.to_vec();
                    seeded = true;
                }
            }
        }

        if !seeded {
            population = latin_hypercube_population(bounds_slice, pop_size, &mut rng);
        }

        let start_time = Instant::now();

        // Initial evaluation
        let mut costs = Vec::with_capacity(pop_size);
        for ind in &population {
            let c = objective.evaluate(ind);
            costs.push(c);
        }

        promote_best(&mut population, &mut costs);
        let mut best_cost = costs[0];
        let mut sample_indices: Vec<usize> = (0..pop_size).collect();

        let cr = 0.7;
        let max_iters = solver.max_iterations.max(0) as usize;

        // Evolution loop
        for gen in 0..max_iters {
            // SciPy's default mutation is a per-generation dither in
            // [0.5, 1.0), not a fixed F=0.8.
            let f_weight = rng.uniform(0.5, 1.0);
            for i in 0..pop_size {
                let trial = {
                    let mut inputs = TrialInputs {
                        population: &population,
                        bounds: bounds_slice,
                        strategy: solver.strategy.as_str(),
                        f_weight,
                        crossover_probability: cr,
                        sample_indices: &mut sample_indices,
                        rng: &mut rng,
                    };
                    trial_vector(i, &mut inputs)
                };

                let trial_cost = objective.evaluate(&trial);
                if trial_cost <= costs[i] {
                    population[i] = trial;
                    costs[i] = trial_cost;
                    if trial_cost < costs[0] {
                        population.swap(0, i);
                        costs.swap(0, i);
                    }
                }
            }

            best_cost = costs[0];

            if let Some(ref mut cb) = progress_callback {
                let h = objective.history();
                let max_ld = h.l_over_d.iter().copied().fold(0.0_f64, f64::max);
                let msg = format!(
                    "generation {}/{} | valid: {}/{} total | best L/D so far: {:.2}",
                    gen + 1,
                    max_iters,
                    h.n_valid(),
                    h.n_evaluations(),
                    max_ld
                );
                cb(&msg);
            }

            // SciPy uses population standard deviation, not the max-min
            // spread. The latter prevents convergence on a normal population
            // with one merely average member still present.
            if converged(&costs, solver.tolerance) {
                break;
            }
        }

        let wall_time_s = start_time.elapsed().as_secs_f64();
        let best_vec = DesignVector::from_array(&population[0]).unwrap_or_default();

        OptimizationResult {
            best_design: best_vec,
            best_cost,
            history: objective.history().clone(),
            wall_time_s,
            method: "differential_evolution".to_owned(),
            pareto_front: Vec::new(),
        }
    }

    fn run_product_search<E: SearchObjective>(
        &self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
        objective: &mut E,
        mut progress_callback: Option<&mut dyn FnMut(&str)>,
    ) -> OptimizationResult {
        let solver = &self.config.optimizer.solver;
        let default_bounds = DesignVector::bounds();
        let bounds = bounds.unwrap_or(&default_bounds);
        let population_size = (solver.population_size.max(1) as usize * bounds.len()).max(2);
        let generations = solver.max_iterations.max(0) as usize;
        let seed = solver.seed.map_or_else(runtime_seed, |value| value as u64);
        let initial_values = initial_design.map(DesignVector::to_array);
        let started = Instant::now();
        let method = solver.method.as_str();

        let outcome = {
            let mut evaluate = |values: &[f64]| {
                let cost = objective.evaluate(values);
                scored_point(values, cost, objective.history())
            };
            match method {
                "feasibility_first_de" => run_feasibility_first_de(
                    bounds,
                    population_size,
                    generations,
                    seed,
                    initial_values.as_deref(),
                    &mut evaluate,
                ),
                "nsga2" => run_nsga2(
                    bounds,
                    population_size,
                    generations,
                    seed,
                    initial_values.as_deref(),
                    &mut evaluate,
                ),
                "turbo_1" => run_turbo_1(
                    bounds,
                    population_size,
                    generations,
                    seed,
                    initial_values.as_deref(),
                    &mut evaluate,
                ),
                "cma_es" => run_cma_es(
                    bounds,
                    population_size,
                    generations,
                    seed,
                    initial_values.as_deref(),
                    &mut evaluate,
                ),
                _ => run_feasibility_first_de(
                    bounds,
                    population_size,
                    generations,
                    seed,
                    initial_values.as_deref(),
                    &mut evaluate,
                ),
            }
        };

        if let Some(callback) = progress_callback.as_mut() {
            callback(&format!(
                "{} complete | valid: {}/{} total | best cost: {:.4}",
                method,
                objective.history().n_valid(),
                objective.history().n_evaluations(),
                outcome.winner.cost
            ));
        }

        result_from_method(
            outcome,
            method,
            objective.history(),
            started.elapsed().as_secs_f64(),
        )
    }
}

fn runtime_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0)
}

fn scored_point(values: &[f64], cost: f64, history: &OptimizationHistory) -> ScoredPoint {
    let index = history.n_evaluations().saturating_sub(1);
    let valid = history.valid.get(index).copied().unwrap_or(false);
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
        history: history.clone(),
        wall_time_s,
        method: method.to_owned(),
        pareto_front,
    }
}

trait SearchObjective {
    fn evaluate(&mut self, design: &[f64]) -> f64;
    fn history(&self) -> &OptimizationHistory;
}

impl SearchObjective for DesignObjective {
    fn evaluate(&mut self, design: &[f64]) -> f64 {
        DesignObjective::evaluate(self, design)
    }

    fn history(&self) -> &OptimizationHistory {
        &self.history
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

fn promote_best(population: &mut [Vec<f64>], costs: &mut [f64]) {
    let Some((best_index, _)) = costs
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| left.total_cmp(right))
    else {
        return;
    };
    population.swap(0, best_index);
    costs.swap(0, best_index);
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
                .map(|(normalized, &(lo, hi))| lo + normalized * (hi - lo))
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
        if *value < lo || *value > hi {
            *value = inputs.rng.uniform(lo, hi);
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
