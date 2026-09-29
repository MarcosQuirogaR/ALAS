// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The SciPy-compatible differential-evolution driver used by the `scipy_legacy`
//! profile and the reference-compatible constructor.

use super::*;

impl DesignOptimizer {
    // Used by the `scipy_legacy` profile and the reference-compatible constructor.
    pub(super) fn run_search<E: SearchObjective>(
        &self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
        objective: &mut E,
        mut progress_callback: Option<&mut dyn FnMut(&str)>,
        cancel: Option<&AtomicBool>,
    ) -> OptimizationResult {
        let solver = self.config.optimizer.solver.clone();
        let scope = CancelScope::attach(cancel);
        let started = Instant::now();

        let default_bounds = DesignVector::bounds();
        let bounds_slice = bounds.unwrap_or(&default_bounds);
        let n_dof = bounds_slice.len();
        let free_dof = bounds_slice
            .iter()
            .filter(|&&(lower, upper)| upper > lower)
            .count();

        let pop_mult = solver.population_size.max(1) as usize;
        // SciPy sizes the population from the number of non-equal bounds,
        // while retaining at least five members. rand2 needs five distinct
        // peers in addition to its target, so that strategy needs six.
        let minimum_population = if solver.strategy.starts_with("rand2") {
            6
        } else {
            5
        };
        let configured_pop_size = (pop_mult * free_dof.max(1)).max(minimum_population);
        // Native objective evaluation is thread-safe after cloning its
        // configuration. The setting is resolved in one place so this
        // replay driver and the product L-SHADE search cannot disagree about
        // what the automatic count is.
        //
        // This replay is the one driver whose *result* depends on
        // this, because a batched generation defers the population update and
        // a serial one lets an accepted trial influence later trial vectors
        // in the same generation. Those are different algorithms, so the
        // choice between them must come from the configuration, never from
        // how many cores the machine reports: resolving `0` (automatic) to
        // the machine's parallelism here silently moved the replay off
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
        // The reference draws the seeded initial array from one generator and
        // evolves with a separately seeded SciPy generator. Reusing one
        // stream here shifts every mutation after generation zero.
        let mut init_rng = Pcg64::seed(seed_val);
        let mut rng = RandomState::seed(seed_val);

        // Initialize population
        let mut population: Vec<Vec<f64>> = Vec::with_capacity(configured_pop_size);

        let mut seeded = false;
        if solver.seed_near_initial_design {
            if let Some(x0) = initial_design {
                let x0_arr = x0.to_array();
                let in_bounds = x0_arr
                    .iter()
                    .zip(bounds_slice)
                    .all(|(&val, &(lo, hi))| val >= lo && val <= hi);
                if in_bounds {
                    for _ in 0..configured_pop_size {
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
            population = latin_hypercube_population(bounds_slice, configured_pop_size, &mut rng);
        }

        // Initial evaluation. In SciPy's serial mode, each completed member is
        // a cancellation boundary. Parallel mode evaluates the whole initial
        // population as one deferred batch, matching SciPy's worker semantics.
        scope.enter(CancelPhase::DeInitialPopulation, 0);
        let evaluations = if scope.requested() {
            scope.work_skipped("initial population stopped before scoring");
            Vec::new()
        } else if deferred_generations {
            scope.block(configured_pop_size as u64, || {
                objective.evaluate_batch(&population, workers)
            })
        } else {
            let mut evaluated = Vec::with_capacity(configured_pop_size);
            for candidate in &population {
                if scope.requested() {
                    scope.work_skipped("remaining initial population stopped by cancellation");
                    break;
                }
                let (cost, valid) = scope.evaluation(|| {
                    let cost = objective.evaluate(candidate);
                    let valid = objective.history().valid.last().copied().unwrap_or(false)
                        && cost.is_finite();
                    (cost, valid)
                });
                evaluated.push((cost, valid));
            }
            evaluated
        };
        let (mut costs, mut validities): (Vec<f64>, Vec<bool>) = evaluations.into_iter().unzip();

        if costs.is_empty() {
            let best_design = initial_design.copied().unwrap_or_default();
            scope.search_finished(CANCELLED);
            return OptimizationResult {
                best_design,
                best_cost: f64::INFINITY,
                best_valid: false,
                history: objective.history().clone(),
                wall_time_s: started.elapsed().as_secs_f64(),
                method: "differential_evolution".to_owned(),
                strategy: solver.strategy,
                termination: CANCELLED.to_owned(),
                pareto_front: Vec::new(),
                search_diagnostics: None,
                delivered_acceptance: None,
            };
        }
        population.truncate(costs.len());
        let pop_size = costs.len();

        // Both explicit compatibility replays and the configured `scipy_legacy`
        // profile use SciPy's scalar-cost selection, independent of the
        // objective's physical validity annotations.
        let feasibility_first = !self.uses_scipy_compatible_profile();
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

        let mut cancelled = scope.requested();
        if cancelled {
            termination = CANCELLED;
        }

        // Evolution loop
        for gen in 0..if cancelled { 0 } else { max_iters } {
            scope.enter(CancelPhase::DeGeneration, gen as u64);
            if scope.requested() {
                cancelled = true;
                scope.work_skipped(format!(
                    "generation {gen} stopped before its trial vectors were built"
                ));
                termination = CANCELLED;
                break;
            }
            // SciPy's default mutation is a per-generation dither in
            // [0.5, 1.0), not a fixed F=0.8.
            let f_weight = rng.uniform(0.5, 1.0);
            if !deferred_generations {
                // Preserve the reference interleaving in the serial mode:
                // accepted trials immediately influence later trial vectors.
                for i in 0..pop_size {
                    if scope.requested() {
                        cancelled = true;
                        scope.work_skipped(format!(
                            "remaining trials in generation {gen} stopped by cancellation"
                        ));
                        break;
                    }
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

                    let (trial_cost, trial_valid) = scope.evaluation(|| {
                        let trial_cost = objective.evaluate(&trial);
                        let trial_valid =
                            objective.history().valid.last().copied().unwrap_or(false)
                                && trial_cost.is_finite();
                        (trial_cost, trial_valid)
                    });
                    TrialState {
                        population: &mut population,
                        costs: &mut costs,
                        validities: &mut validities,
                        feasibility_first,
                    }
                    .apply(i, trial, trial_cost, trial_valid);
                    if scope.requested() {
                        cancelled = true;
                        break;
                    }
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
                let trial_evaluations = scope.block(pop_size as u64, || {
                    objective.evaluate_batch(&trials, workers)
                });
                if scope.requested() {
                    // A parallel backend can return interrupted evaluations
                    // as finite penalty rows. Keep the last wholly scored
                    // population when cancellation landed inside its batch.
                    cancelled = true;
                    scope.work_skipped(format!(
                        "generation {gen} results discarded after cancellation during its batch"
                    ));
                } else {
                    let mut next_population = population.clone();
                    let mut next_costs = costs.clone();
                    let mut next_validities = validities.clone();
                    for (i, (trial, (trial_cost, trial_valid))) in
                        trials.into_iter().zip(trial_evaluations).enumerate()
                    {
                        if candidate_is_at_least_as_good(
                            trial_cost,
                            trial_valid,
                            costs[i],
                            validities[i],
                            feasibility_first,
                        ) {
                            next_population[i] = trial;
                            next_costs[i] = trial_cost;
                            next_validities[i] = trial_valid;
                        }
                    }
                    population = next_population;
                    costs = next_costs;
                    validities = next_validities;
                    promote_best(
                        &mut population,
                        &mut costs,
                        &mut validities,
                        feasibility_first,
                    );
                }
            }

            best_cost = costs[0];

            if cancelled {
                termination = CANCELLED;
                break;
            }

            if let Some(ref mut cb) = progress_callback {
                let h = objective.history();
                let best_ld = h
                    .l_over_d
                    .iter()
                    .zip(&h.valid)
                    .filter_map(|(&ld, &valid)| (valid && ld.is_finite()).then_some(ld))
                    .max_by(f64::total_cmp)
                    .unwrap_or(0.0);
                let msg = format!(
                    "generation {}/{} | valid: {}/{} total | best L/D so far: {:.4}",
                    gen + 1,
                    max_iters,
                    h.n_valid(),
                    h.n_evaluations(),
                    best_ld
                );
                cb(&msg);
            }

            if scope.requested() {
                cancelled = true;
                termination = CANCELLED;
                break;
            }

            // SciPy uses population standard deviation, not the max-min
            // spread. The latter prevents convergence on a normal population
            // with one merely average member still present.
            if converged(&costs, solver.tolerance) {
                termination = "converged";
                break;
            }
        }

        if scope.requested() {
            cancelled = true;
        }
        if cancelled {
            termination = CANCELLED;
        }
        scope.search_finished(termination);
        let wall_time_s = started.elapsed().as_secs_f64();
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
}
