// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Differential-evolution orchestration for the optimizer boundary.

use std::sync::atomic::AtomicBool;

use super::*;
use crate::cancellation::{CancelPhase, CancelScope};
use crate::search_methods::product_de;

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
        self.run_cancellable(bounds, initial_design, progress_callback, None)
    }

    /// [`Self::run`], observing an optional pipeline cancellation flag.
    ///
    /// `cancel` is threaded into the L-SHADE generation loop (see
    /// `search_methods::lshade_de::run`) and checked before every block of
    /// resolved-worker-count candidates; a cancelled run still returns `Ok`, with
    /// `OptimizationResult::termination` set to `"cancelled"` rather than
    /// `"converged"` or `"iteration_limit"`, and its winner is always a
    /// fully scored candidate, never a partial trial. The frozen
    /// reference-compatibility replay (`Self::new_reference_compatibility`)
    /// is not on any pipeline cancellation path and does not observe `cancel`.
    ///
    /// # Errors
    ///
    /// Same as [`Self::run`].
    pub fn run_cancellable(
        &mut self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
        progress_callback: Option<&mut dyn FnMut(&str)>,
        cancel: Option<&AtomicBool>,
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
                cancel,
            )
        };

        // A cancelled search is reported as cancelled, not as an infeasible
        // design. `ensure_feasible` answers "did the search find a feasible
        // aircraft"; a run stopped from outside never got to finish asking,
        // and turning it into `NoFeasibleDesign` would tell the caller
        // something about the design space that this run did not establish.
        // The result still carries `best_valid` as scored, so nothing
        // downstream can read a cancelled run as a delivered feasible design
        // (see `OptimizationResult::is_delivered_feasible`).
        if result.was_cancelled() {
            return Ok(result);
        }
        let result = ensure_feasible(result)?;

        // Keep the run on the same explicit payload load case every
        // candidate was scored with.
        restore_winning_payload_load_case(&mut self.config, &result.best_design);
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
        self.run_with_evaluator_cancellable(
            bounds,
            initial_design,
            evaluator,
            progress_callback,
            None,
        )
    }

    /// [`Self::run_with_evaluator`], observing an optional pipeline
    /// cancellation flag. See [`Self::run_cancellable`] for the semantics.
    ///
    /// # Errors
    ///
    /// Same as [`Self::run_with_evaluator`].
    pub fn run_with_evaluator_cancellable<E: ObjectiveEvaluator + ?Sized>(
        &mut self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
        evaluator: &mut E,
        progress_callback: Option<&mut dyn FnMut(&str)>,
        cancel: Option<&AtomicBool>,
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
                cancel,
            )
        };

        // Same cancellation contract as `run_cancellable`.
        if result.was_cancelled() {
            return Ok(result);
        }
        let result = ensure_feasible(result)?;

        restore_winning_payload_load_case(&mut self.config, &result.best_design);
        Ok(result)
    }

    // Reachable only through `Self::new_reference_compatibility`, which no
    // pipeline or GUI caller constructs (`alas-opt/tests/parity_optimizer.rs`
    // is the sole caller): the frozen replay this drives has no cancellation
    // signal threaded into it.
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
        // configuration. The setting is resolved in one place so this frozen
        // replay driver and the product L-SHADE search cannot disagree about
        // what the automatic count is.
        //
        // This frozen replay is the one driver whose *result* depends on
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

    /// Run the one product search kernel: L-SHADE differential evolution
    /// under the epsilon-constrained method (`search_methods::lshade_de`).
    ///
    /// Stage A below only ever *seeds* the population; the kernel is what
    /// selects the winner, and every candidate it reports as feasible was
    /// scored by `objective`, the same full coupled evaluation (geometry and
    /// mass build, mission sizing closure, trim/CG closure; see
    /// [`crate::mdo`]) as everything else in this run's history.
    fn run_product_search<E: SearchObjective>(
        &self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
        objective: &mut E,
        progress_callback: Option<&mut dyn FnMut(&str)>,
        cancel: Option<&AtomicBool>,
    ) -> OptimizationResult {
        let solver = &self.config.optimizer.solver;
        let scope = CancelScope::attach(cancel);
        let default_bounds = DesignVector::bounds();
        let bounds = bounds.unwrap_or(&default_bounds);
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
        // L-SHADE population's seed.  The scan ranks candidates on a reduced
        // aerodynamic mesh and a loosened sizing closure, so it may only
        // *nominate* a start; every candidate that can be accepted below is
        // ranked by the full objective, and the DE kernel itself decides the
        // winner from there (see the method's own doc comment above).
        let scan = self.broad_scan(bounds, initial_values.as_deref(), &staged, &scope);
        // Under an already-observed cancellation the verification block is
        // cut to the nominal point alone. The search is not going to start,
        // so ranking three scan finalists at full fidelity buys nothing and
        // costs three coupled analyses of drain; scoring the nominal keeps a
        // real analysed candidate to report instead of the unevaluated
        // sentinel. See `verify_scan_finalists`.
        let (verified_start, scan_verification) = verify_scan_finalists(
            objective,
            &scan.candidates,
            initial_values.as_deref(),
            staged.workers,
            &scope,
        );
        if let Some(point) = verified_start.as_ref() {
            initial_values = Some(point.values.clone());
        }
        report(&format!(
            "staged scan | screened {} | screening_feasible {} | verified {} | cancelled {} | elapsed_s {:.3}",
            scan.screened, scan.feasible_screened, scan_verification, scan.cancelled, scan.elapsed_s
        ));

        // Cancellation observed during Stage A. The verification block above
        // has already run - it is bounded at the nominal plus three finalists,
        // so it always leaves a fully scored candidate to report - and the
        // search is skipped rather than started on a signal that is already
        // set. A search stopped here has decided nothing: the result is
        // reported `cancelled`, never `converged` or a budget reason, and its
        // winner is the best *verified* point, which is the nominal design
        // unless a scan finalist beat it under the full coupled objective.
        if scan.cancelled || scope.requested() {
            scope.search_finished(CANCELLED);
            return self.cancelled_before_search_result(
                objective,
                initial_values.as_deref(),
                verified_start,
                &scan,
                scan_verification,
                staged.workers,
                started,
            );
        }

        let de = product_de::Settings::from_solver(solver, bounds.len(), seed);
        report(&format!(
            "differential evolution | population {} | generations {} | seed {} | evaluation_budget {}",
            de.population,
            de.generations,
            de.seed,
            de.evaluation_budget()
        ));
        // The DE kernel times each evaluation block itself through the
        // scope, so this adapter stays inert: a block counted twice would
        // make the per-block cancellation bound unreadable.
        let mut evaluator = BatchEvaluator {
            objective,
            workers: staged.workers,
            scope: CancelScope::attach(None),
        };
        let outcome = product_de::run(
            bounds,
            initial_values.as_deref(),
            de,
            &scope,
            &mut |points: &[Vec<f64>]| evaluator.evaluate_block(points),
        );
        // `converged` is the kernel's own verdict (population spread plus
        // best-feasible-cost stagnation; see `search_methods::lshade_de`) and
        // is never true without a feasible design. `iteration_limit` is the
        // shared termination vocabulary a budget-exhausted, non-converged
        // search reports elsewhere (`run_search`'s own frozen DE path);
        // inventing a distinct string here would silently break every caller
        // that checks termination against that fixed set. `cancelled` is the
        // one lifecycle this kernel can reach that is neither: a caller
        // observed the pipeline's own cancellation signal at a block
        // boundary and stopped before either budget or convergence decided
        // the run.
        let termination = if outcome.cancelled {
            CANCELLED
        } else if outcome.converged {
            "converged"
        } else {
            "iteration_limit"
        };
        scope.search_finished(termination);
        let mut result = result_from_method(
            MethodOutcome {
                winner: outcome.winner,
                pareto_front: Vec::new(),
            },
            product_de::METHOD,
            product_de::STRATEGY,
            termination,
            objective.history(),
            started.elapsed().as_secs_f64(),
        );
        result.search_diagnostics = Some(SearchDiagnostics {
            converged: outcome.converged,
            analysis_evaluations: outcome.evaluations,
            cache_hits: 0,
            poll_iterations: outcome.generations_completed,
            screening_evaluations: scan.screened,
            screening_feasible: scan.feasible_screened,
            verification_evaluations: scan_verification,
            scan_wall_time_s: scan.elapsed_s,
            search_wall_time_s: started.elapsed().as_secs_f64() - scan.elapsed_s,
            workers: staged.workers,
            poll_block_size: de.population,
            first_feasible_cost: outcome.first_feasible_cost,
            relative_improvement: outcome.relative_improvement,
            feasible_fraction: outcome.feasible_fraction,
            epsilon_level: outcome.epsilon_final,
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
    /// Whether the scan stopped on the caller's cancellation flag rather than
    /// exhausting its sample. Its finalists are then drawn from the part of
    /// the envelope it reached, which is why a cancelled scan may not start a
    /// search.
    cancelled: bool,
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
        scope: &CancelScope<'_>,
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
    #[allow(clippy::too_many_arguments)] // the pre-search state it reports on, one argument per piece
    fn cancelled_before_search_result<E: SearchObjective + ?Sized>(
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
fn verify_scan_finalists<E: SearchObjective + ?Sized>(
    objective: &mut E,
    candidates: &[Vec<f64>],
    nominal: Option<&[f64]>,
    workers: usize,
    scope: &CancelScope<'_>,
) -> (Option<ScoredPoint>, usize) {
    if candidates.is_empty() {
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
struct BatchEvaluator<'a, E: SearchObjective + ?Sized> {
    objective: &'a mut E,
    workers: usize,
    /// Cancellation telemetry for the block boundary, or an inert scope where
    /// the caller times its own evaluations.
    scope: CancelScope<'a>,
}

impl<E: SearchObjective + ?Sized> BatchEvaluator<'_, E> {
    fn evaluate_block(&mut self, points: &[Vec<f64>]) -> Vec<ScoredPoint> {
        let before = self.objective.history().n_evaluations();
        let scores = {
            let objective = &mut *self.objective;
            let workers = self.workers;
            self.scope.block(points.len() as u64, || {
                objective.evaluate_batch(points, workers)
            })
        };
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
