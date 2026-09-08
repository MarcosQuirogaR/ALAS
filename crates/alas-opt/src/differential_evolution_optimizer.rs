// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Differential-evolution orchestration for the optimizer boundary.

use super::*;

#[path = "sqp_search.rs"]
mod sqp_search;

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

    fn invalid_solver_setting_reason(&self) -> Option<String> {
        let solver = &self.config.optimizer.solver;
        if !alas_config::SolverSettings::is_supported_method(&solver.method) {
            return Some(format!("unknown optimizer method {:?}", solver.method));
        }
        if !alas_config::SolverSettings::is_supported_strategy(&solver.strategy) {
            return Some(format!("unknown optimizer strategy {:?}", solver.strategy));
        }
        None
    }

    fn validate_request(&self, bounds: Option<&[(f64, f64)]>) -> Result<(), OptimizationError> {
        if let Some(reason) = self.invalid_solver_setting_reason() {
            return Err(OptimizationError::InvalidConfiguration(reason));
        }
        if !self.reference_mass_coordinates {
            self.config
                .optimizer
                .design_space
                .validate()
                .map_err(OptimizationError::InvalidConfiguration)?;
        }
        let default_bounds;
        let bounds = match bounds {
            Some(bounds) => bounds,
            None => {
                default_bounds = DesignVector::bounds();
                &default_bounds
            }
        };
        validate_bounds(bounds)
    }

    fn effective_bounds(
        &self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
    ) -> Result<Vec<(f64, f64)>, OptimizationError> {
        let default_bounds = DesignVector::bounds();
        let requested = bounds.unwrap_or(&default_bounds);
        validate_bounds(requested)?;
        if self.reference_mass_coordinates {
            return Ok(requested.to_vec());
        }
        let nominal = self.nominal_design(initial_design)?;
        let design_space = &self.config.optimizer.design_space;
        let declared = design_space.envelope(&nominal);
        let mut effective = Vec::with_capacity(requested.len());
        for (index, (&(requested_lower, requested_upper), variable)) in
            requested.iter().zip(&declared).enumerate()
        {
            // The clean-sheet fuselage length is a derived coordinate solved
            // from the requested cabin load case, not a caller-chosen search
            // freedom (see the matching skip in
            // `DesignObjective::validate_design_space`). A caller replaying an
            // explicit design (for example a fixed-design finalist review)
            // legitimately pins this coordinate at the design's own literal
            // length; only the global spec bounds validated above apply.
            if design_space.sizes_fuselage_from_cabin() && variable.name == "fuselage_length_m" {
                effective.push((requested_lower, requested_upper));
                continue;
            }
            let (declared_lower, declared_upper) = (variable.lower, variable.upper);
            let lower = requested_lower.max(declared_lower);
            let upper = requested_upper.min(declared_upper);
            if lower > upper {
                return Err(OptimizationError::InvalidBounds(format!(
                    "bound {index} [{requested_lower}, {requested_upper}] does not intersect the design-mode envelope [{declared_lower}, {declared_upper}]"
                )));
            }
            effective.push((lower, upper));
        }
        Ok(effective)
    }

    fn nominal_design(
        &self,
        initial_design: Option<&DesignVector>,
    ) -> Result<DesignVector, OptimizationError> {
        let nominal = initial_design.copied().unwrap_or_else(|| {
            self.config
                .preset
                .as_str()
                .is_empty()
                .then_some(DesignVector::default())
                .or_else(|| {
                    alas_config::presets::get(&self.config.preset)
                        .ok()
                        .map(|preset| preset.design_vector)
                })
                .unwrap_or_default()
        });
        if self.reference_mass_coordinates {
            return Ok(nominal);
        }
        crate::mdo::canonical_nominal_design(&self.config, nominal)
            .map_err(OptimizationError::InvalidConfiguration)
    }

    /// Execute the Differential Evolution optimization search.
    ///
    /// # Errors
    ///
    /// Returns [`OptimizationError::NoFeasibleDesign`] when every evaluated
    /// candidate fails the active objective/analysis policy. Invalid bounds
    /// and unknown solver tokens are reported before any objective evaluation
    /// begins.
    pub fn run(
        &mut self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
        progress_callback: Option<&mut dyn FnMut(&str)>,
    ) -> Result<OptimizationResult, OptimizationError> {
        self.validate_request(bounds)?;
        let effective_bounds = self.effective_bounds(bounds, initial_design)?;
        let nominal = self.nominal_design(initial_design)?;
        let mut objective = if self.reference_mass_coordinates {
            DesignObjective::new_reference_compatibility(self.config.clone())
        } else {
            DesignObjective::new_with_nominal(self.config.clone(), nominal)
        };

        let result = if self.reference_mass_coordinates {
            self.run_search(Some(&effective_bounds), initial_design, &mut objective, progress_callback)
        } else {
            self.run_product_search(
                Some(&effective_bounds),
                initial_design,
                &mut objective,
                progress_callback,
            )
        };

        let result = ensure_feasible(result)?;

        // Keep the run on the same explicit payload load case every
        // candidate was scored with.
        let _ = apply_candidate_payload_load_case(&mut self.config, &result.best_design);
        Ok(result)
    }

    /// Execute the same differential-evolution search with a caller-provided
    /// aerodynamic objective.
    ///
    /// The evaluator receives a typed [`DesignVector`] and returns the same
    /// diagnostics recorded by the native VLM objective. This is the seam used
    /// by the pipeline's AVL adapter; it deliberately contains no process or
    /// CPACS dependency.
    ///
    /// # Errors
    ///
    /// Returns [`OptimizationError::NoFeasibleDesign`] when every evaluated
    /// candidate fails the delegated evaluator's active validity policy.
    pub fn run_with_evaluator<E: ObjectiveEvaluator + ?Sized>(
        &mut self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
        evaluator: &mut E,
        progress_callback: Option<&mut dyn FnMut(&str)>,
    ) -> Result<OptimizationResult, OptimizationError> {
        self.validate_request(bounds)?;
        let effective_bounds = self.effective_bounds(bounds, initial_design)?;
        let mut objective =
            DelegatedObjective::new(evaluator, self.config.optimizer.weights.failure_cost);
        let result = if self.reference_mass_coordinates {
            self.run_search(Some(&effective_bounds), initial_design, &mut objective, progress_callback)
        } else {
            self.run_product_search(
                Some(&effective_bounds),
                initial_design,
                &mut objective,
                progress_callback,
            )
        };

        let result = ensure_feasible(result)?;

        let _ = apply_candidate_payload_load_case(&mut self.config, &result.best_design);
        Ok(result)
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
        // The mutation strategies draw five distinct peers. Keep a small
        // custom design space from reaching the zero/short-population panic
        // path while leaving the normal sixteen-variable population unchanged.
        let pop_size = (pop_mult * n_dof).max(6);
        // Native objective evaluation is thread-safe after cloning its
        // configuration. A non-positive setting keeps the historical serial
        // behaviour rather than creating an invalid worker count.
        let workers = solver.workers.max(1) as usize;
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
        let evaluations = objective.evaluate_batch(&population, workers);
        let (mut costs, mut validities): (Vec<f64>, Vec<bool>) = evaluations.into_iter().unzip();

        let feasibility_first = !self.reference_mass_coordinates;
        promote_best(
            &mut population,
            &mut costs,
            &mut validities,
            feasibility_first,
        );
        let mut best_cost = costs[0];
        let mut sample_indices: Vec<usize> = (0..pop_size).collect();

        let cr = 0.7;
        let max_iters = solver.max_iterations.max(0) as usize;
        let mut termination = "iteration_limit";

        // Evolution loop
        for gen in 0..max_iters {
            // SciPy's default mutation is a per-generation dither in
            // [0.5, 1.0), not a fixed F=0.8.
            let f_weight = rng.uniform(0.5, 1.0);
            if workers <= 1 {
                // Preserve the reference interleaving in the serial mode:
                // accepted trials immediately influence later trial vectors.
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
                    let trial_valid = objective.history().valid.last().copied().unwrap_or(false)
                        && trial_cost.is_finite();
                    TrialState {
                        population: &mut population,
                        costs: &mut costs,
                        validities: &mut validities,
                        feasibility_first,
                    }
                    .apply(i, trial, trial_cost, trial_valid);
                }
            } else {
                // Build the complete generation before evaluating it. This
                // makes the expensive native objective calls independent and
                // merges their histories in stable candidate order.
                let mut trials = Vec::with_capacity(pop_size);
                for i in 0..pop_size {
                    let mut inputs = TrialInputs {
                        population: &population,
                        bounds: bounds_slice,
                        strategy: solver.strategy.as_str(),
                        f_weight,
                        crossover_probability: cr,
                        sample_indices: &mut sample_indices,
                        rng: &mut rng,
                    };
                    trials.push(trial_vector(i, &mut inputs));
                }
                let trial_evaluations = objective.evaluate_batch(&trials, workers);
                for (i, (trial, (trial_cost, trial_valid))) in
                    trials.into_iter().zip(trial_evaluations).enumerate()
                {
                    TrialState {
                        population: &mut population,
                        costs: &mut costs,
                        validities: &mut validities,
                        feasibility_first,
                    }
                    .apply(i, trial, trial_cost, trial_valid);
                }
            }

            best_cost = costs[0];

            if let Some(ref mut cb) = progress_callback {
                let h = objective.history();
                let msg = format!(
                    "generation {}/{} | valid: {}/{} total | best cost so far: {:.4}",
                    gen + 1,
                    max_iters,
                    h.n_valid(),
                    h.n_evaluations(),
                    best_cost
                );
                cb(&msg);
            }

            // SciPy uses population standard deviation, not the max-min
            // spread. The latter prevents convergence on a normal population
            // with one merely average member still present.
            if converged(&costs, solver.tolerance) {
                termination = "converged";
                break;
            }
        }

        let wall_time_s = start_time.elapsed().as_secs_f64();
        let best_vec = DesignVector::from_array(&population[0]).unwrap_or_default();

        OptimizationResult {
            best_design: best_vec,
            best_cost,
            best_valid: validities[0],
            history: objective.history().clone(),
            wall_time_s,
            method: "differential_evolution".to_owned(),
            strategy: solver.strategy.clone(),
            termination: termination.to_owned(),
            pareto_front: Vec::new(),
        }
    }

    fn run_product_search<E: sqp_search::ConstrainedSearch>(
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
        let mut evaluate = |values: &[f64]| {
            let cost = objective.evaluate(values);
            scored_point(values, cost, objective.history())
        };
        // Legacy method names remain loadable for saved configurations, but
        // every product run uses this single MADS driver. The compatibility
        // constructor above is the only path that still replays DE.
        let search_result = crate::search::mads::run(
            bounds,
            initial_values.as_deref(),
            crate::search::mads::Settings {
                max_iterations: generations,
                max_evaluations: (population_size.saturating_mul(generations.max(1) + 1))
                    .max(1),
                seed,
                ..Default::default()
            },
            &mut evaluate,
            progress_callback,
        );

        let termination = search_result.termination.as_str();
        result_from_method(
            search_result.outcome,
            "mads",
            "progressive_barrier",
            termination,
            objective.history(),
            started.elapsed().as_secs_f64(),
        )
    }
}
