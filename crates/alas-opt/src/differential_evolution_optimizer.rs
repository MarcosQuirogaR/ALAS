// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Differential-evolution orchestration for the optimizer boundary.

use super::*;

/// Whether the caller supplied a single literal fuselage-length bound.
///
/// The GUI uses fixed bounds for a reference/full-analysis review.  In that
/// case the design vector is authoritative and the cabin-derived clean-sheet
/// sizing solve must not silently replace it.  A non-degenerate bound keeps
/// the normal derived-coordinate behavior.
fn explicit_fuselage_length_bound(bounds: Option<&[(f64, f64)]>) -> bool {
    let Some(bounds) = bounds else {
        return false;
    };
    let Some(index) = alas_config::design_variables::SPECS
        .iter()
        .position(|spec| spec.name == "fuselage_length_m")
    else {
        return false;
    };
    bounds
        .get(index)
        .is_some_and(|&(lower, upper)| lower == upper)
}

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
    pub fn new_reference_compatibility(mut config: AlasConfig) -> Self {
        // This constructor is the explicit comparison boundary.  Make the
        // selected architecture agree with the replay flag so the legacy
        // optimizer cannot accidentally ask the pure FLOPS mass evaluator for
        // a reference-coordinate run.
        config.mass_model.mass_architecture =
            alas_config::MassArchitecture::LegacyReferenceCompatibleComparison;
        config.mass_model.apply_architecture();
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
            if !self.config.mass_model.mass_architecture.is_production() {
                return Err(OptimizationError::InvalidConfiguration(
                    "the production optimizer requires pure_flops_transport_v1; use the explicit reference-compatibility constructor for legacy comparison"
                        .to_owned(),
                ));
            }
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
        // A caller that supplies no bounds is asking for the design space it
        // configured, and that space is already a complete, anchored envelope:
        // the global box widened to contain the start for a clean sheet, the
        // D09 window around the registered reference for an adaptation, the
        // reference itself for a baseline sandbox.  Intersecting it with the
        // global box as well would be intersecting it with AVE's family, and
        // the registered types do not live there.  Every other type in the
        // catalogue has a span, a chord or a body length outside at least one
        // of those global limits, an A320 by more than twenty metres of
        // fuselage, so the intersection is empty and the search is rejected
        // before it evaluates anything.  The envelope is validated on its own
        // terms below and the evaluator enforces the same one, so nothing is
        // widened by this: what changes is that a registered aircraft can be
        // optimized inside its own declared window without the caller having
        // to restate it.
        if bounds.is_none() {
            let envelope: Vec<(f64, f64)> = declared
                .iter()
                .map(|variable| (variable.lower, variable.upper))
                .collect();
            validate_bounds(&envelope)?;
            return Ok(envelope);
        }
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
                // The evaluator derives this coordinate from the cabin load
                // case.  When the caller uses the optimizer's ordinary
                // global bounds (or those bounds contain the derived value),
                // remove the redundant search dimension and keep the winner
                // self-consistent.  An explicitly incompatible fixed range
                // remains caller-owned for compatibility with review tools
                // that deliberately replay a literal design vector.
                if bounds.is_none()
                    || (requested_lower <= nominal.fuselage_length_m
                        && nominal.fuselage_length_m <= requested_upper)
                {
                    effective.push((nominal.fuselage_length_m, nominal.fuselage_length_m));
                } else {
                    effective.push((requested_lower, requested_upper));
                }
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
        let preserve_explicit_fuselage_length = explicit_fuselage_length_bound(bounds);
        let mut objective = if self.reference_mass_coordinates {
            DesignObjective::new_reference_compatibility(self.config.clone())
        } else {
            DesignObjective::new_with_nominal_and_fuselage_policy(
                self.config.clone(),
                nominal,
                preserve_explicit_fuselage_length,
            )
        };

        let search_initial = if bounds.is_none()
            && self
                .config
                .optimizer
                .design_space
                .sizes_fuselage_from_cabin()
        {
            Some(&nominal)
        } else {
            initial_design
        };
        let result = if self.reference_mass_coordinates {
            self.run_search(
                Some(&effective_bounds),
                initial_design,
                &mut objective,
                progress_callback,
            )
        } else {
            self.run_product_search(
                Some(&effective_bounds),
                search_initial,
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
        let nominal = self.nominal_design(initial_design)?;
        let search_initial = if bounds.is_none()
            && self
                .config
                .optimizer
                .design_space
                .sizes_fuselage_from_cabin()
        {
            Some(&nominal)
        } else {
            initial_design
        };
        let mut objective =
            DelegatedObjective::new(evaluator, self.config.optimizer.weights.failure_cost);
        let result = if self.reference_mass_coordinates {
            self.run_search(
                Some(&effective_bounds),
                initial_design,
                &mut objective,
                progress_callback,
            )
        } else {
            self.run_product_search(
                Some(&effective_bounds),
                search_initial,
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
        // configuration. The setting is resolved in one place so the DE
        // driver, the staged search and SQP cannot disagree about what the
        // automatic count is.
        //
        // Differential evolution is the one driver whose *result* depends on
        // this, because a batched generation defers the population update and
        // a serial one lets an accepted trial influence later trial vectors
        // in the same generation. Those are different algorithms, so the
        // choice between them must come from the configuration, never from
        // how many cores the machine reports: resolving `0` (automatic) to
        // the machine's parallelism here silently moved the frozen replay off
        // the reference interleaving and made its winner core-count
        // dependent. The generation loop therefore batches only when a
        // configuration explicitly asks for more than one worker, and the
        // resolved count then says only how that batch is spread.
        let workers = solver.resolved_workers();
        let deferred_generations = solver.workers > 1;
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
            if !deferred_generations {
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
            search_diagnostics: None,
            delivered_acceptance: None,
        }
    }

    fn run_product_search<E: sqp_search::ConstrainedSearch>(
        &self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
        objective: &mut E,
        progress_callback: Option<&mut dyn FnMut(&str)>,
    ) -> OptimizationResult {
        let solver = &self.config.optimizer.solver;
        if solver.method == "sqp" {
            return sqp_search::run(
                &self.config,
                bounds.unwrap_or(&DesignVector::bounds()),
                initial_design,
                objective,
                progress_callback,
            );
        }
        let default_bounds = DesignVector::bounds();
        let bounds = bounds.unwrap_or(&default_bounds);
        let generations = solver.max_iterations.max(0) as usize;
        let seed = solver.seed.map_or_else(runtime_seed, |value| value as u64);
        let mut initial_values = initial_design.map(DesignVector::to_array);
        // The analysis-start instant for this stage.  Every elapsed time
        // reported below, including the staged scan, is measured from here so
        // a runtime claim covers the whole search rather than its last phase.
        let started = Instant::now();
        let staged = crate::search::staged::Settings::from_solver(solver, bounds.len());
        let mut progress_callback = progress_callback;
        let mut report = |line: &str| {
            if let Some(callback) = progress_callback.as_mut() {
                (**callback)(line);
            }
        };

        // Stage A: a broad low-resolution scan over the whole envelope, then
        // a full-fidelity re-evaluation of its finalists, which supplies the
        // MADS starting point.  The scan ranks candidates on a reduced
        // aerodynamic mesh and a loosened sizing closure, so it may only
        // *nominate* a start; every candidate that can be accepted below is
        // ranked by the full objective.
        let scan = self.broad_scan(bounds, initial_values.as_deref(), &staged);
        let (verified_start, scan_verification) = verify_scan_finalists(
            objective,
            &scan.candidates,
            initial_values.as_deref(),
            staged.workers,
        );
        if verified_start.is_some() {
            initial_values = verified_start;
        }
        report(&format!(
            "staged scan | screened {} | screening_feasible {} | verified {} | elapsed_s {:.3}",
            scan.screened, scan.feasible_screened, scan_verification, scan.elapsed_s
        ));

        // Stage B: MADS over the full coupled objective, from the verified
        // start.  Legacy method names remain loadable for saved
        // configurations, but every product run uses this single driver; the
        // compatibility constructor above is the only path that still replays
        // differential evolution.
        let mut evaluator = BatchEvaluator {
            objective,
            workers: staged.workers,
        };
        let search_result = crate::search::mads::run(
            bounds,
            initial_values.as_deref(),
            crate::search::mads::Settings {
                max_iterations: generations.max(staged.minimum_poll_iterations),
                max_evaluations: staged.max_evaluations,
                seed,
                convergence_mesh_size: staged.convergence_mesh_size,
                minimum_relative_improvement: staged.minimum_relative_improvement,
                poll_block_size: staged.poll_block_size,
                watchdog: staged.watchdog,
                // The sixteen-variable product space pays the adjacent
                // diagonals on every failed poll, which are exactly the polls
                // that contract the mesh; see `search::directions`.
                pair_diagonal_directions: bounds.len() <= 4,
                minimal_positive_basis: staged.minimal_positive_basis,
                ..Default::default()
            },
            &mut evaluator,
            progress_callback,
        );

        let termination = search_result.termination.as_str();
        let mut result = result_from_method(
            search_result.outcome,
            "mads",
            "progressive_barrier",
            termination,
            objective.history(),
            started.elapsed().as_secs_f64(),
        );
        result.search_diagnostics = Some(SearchDiagnostics {
            converged: search_result.termination.is_converged(),
            analysis_evaluations: search_result.evaluations,
            cache_hits: search_result.cache_hits,
            poll_iterations: search_result.iterations,
            screening_evaluations: scan.screened,
            screening_feasible: scan.feasible_screened,
            verification_evaluations: scan_verification,
            scan_wall_time_s: scan.elapsed_s,
            search_wall_time_s: search_result.elapsed_s,
            workers: staged.workers,
            poll_block_size: staged.poll_block_size,
            first_feasible_cost: search_result.first_feasible_cost,
            relative_improvement: search_result.relative_improvement,
        });
        result
    }
}

/// What the broad scan found.
struct ScanOutcome {
    /// Finalist design vectors, best first in the scan's own ranking.
    candidates: Vec<Vec<f64>>,
    /// Low-resolution analyses executed.
    screened: usize,
    /// How many of them were feasible under the reduced model.
    feasible_screened: usize,
    /// Wall-clock seconds spent in the scan.
    elapsed_s: f64,
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
    fn broad_scan(
        &self,
        bounds: &[(f64, f64)],
        initial: Option<&[f64]>,
        settings: &crate::search::staged::Settings,
    ) -> ScanOutcome {
        let started = Instant::now();
        let empty = ScanOutcome {
            candidates: Vec::new(),
            screened: 0,
            feasible_screened: 0,
            elapsed_s: 0.0,
        };
        if settings.scan_points == 0 || settings.scan_finalists == 0 || bounds.is_empty() {
            return empty;
        }
        let sample =
            crate::search::staged::scan_sample(bounds, settings.scan_points, settings.seed);
        if sample.is_empty() {
            return empty;
        }
        let screening = crate::search::staged::screening_config(&self.config);
        let mut objective = match initial.and_then(|values| DesignVector::from_array(values).ok()) {
            Some(nominal) => DesignObjective::new_with_nominal(screening, nominal),
            None => DesignObjective::new(screening),
        };
        let scores = objective.evaluate_batch(&sample, settings.workers);
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
        }
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
fn verify_scan_finalists<E: SearchObjective + ?Sized>(
    objective: &mut E,
    candidates: &[Vec<f64>],
    nominal: Option<&[f64]>,
    workers: usize,
) -> (Option<Vec<f64>>, usize) {
    if candidates.is_empty() {
        return (None, 0);
    }
    let mut points: Vec<Vec<f64>> = Vec::with_capacity(candidates.len() + 1);
    if let Some(values) = nominal {
        points.push(values.to_vec());
    }
    for candidate in candidates {
        if !points.iter().any(|existing| existing == candidate) {
            points.push(candidate.clone());
        }
    }
    let before = objective.history().n_evaluations();
    let scores = objective.evaluate_batch(&points, workers);
    let history = objective.history();
    let mut best: Option<(usize, ScoredPoint)> = None;
    for (offset, (values, (cost, _))) in points.iter().zip(scores).enumerate() {
        let scored = scored_point_at(values, cost, history, before + offset);
        let better = best
            .as_ref()
            .is_none_or(|(_, incumbent)| scored.feasibility_key() < incumbent.feasibility_key());
        if better {
            best = Some((offset, scored));
        }
    }
    (best.map(|(offset, _)| points[offset].clone()), points.len())
}

/// Adapter that evaluates one independent MADS block through the objective's
/// own worker pool.
///
/// The block boundary is chosen by the search (see `search::mads`), so the
/// worker count changes only how the block is distributed, never which points
/// are evaluated or the order they are considered in.
struct BatchEvaluator<'a, E: SearchObjective + ?Sized> {
    objective: &'a mut E,
    workers: usize,
}

impl<E: SearchObjective + ?Sized> crate::search::mads::Evaluate for BatchEvaluator<'_, E> {
    fn evaluate_block(&mut self, points: &[Vec<f64>]) -> Vec<ScoredPoint> {
        let before = self.objective.history().n_evaluations();
        let scores = self.objective.evaluate_batch(points, self.workers);
        let history = self.objective.history();
        points
            .iter()
            .zip(scores)
            .enumerate()
            .map(|(offset, (values, (cost, _)))| {
                scored_point_at(values, cost, history, before + offset)
            })
            .collect()
    }
}
