// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The product search: broad scan seeding followed by the L-SHADE kernel.

use super::scan::{verify_scan_finalists, BatchEvaluator};
use super::*;

impl DesignOptimizer {
    /// Run the one product search kernel: L-SHADE differential evolution
    /// under the epsilon-constrained method (`search_methods::lshade_de`).
    ///
    /// Stage A below only ever *seeds* the population; the kernel is what
    /// selects the winner, and every candidate it reports as feasible was
    /// scored by `objective`, the same full coupled evaluation (geometry and
    /// mass build, mission sizing closure, trim/CG closure; see
    /// [`crate::mdo`]) as everything else in this run's history.
    pub(super) fn run_product_search<E: SearchObjective>(
        &self,
        bounds: Option<&[(f64, f64)]>,
        initial_design: Option<&DesignVector>,
        objective: &mut E,
        progress_callback: Option<&mut dyn FnMut(&str)>,
        cancel: Option<&AtomicBool>,
        compute_pool: Option<std::sync::Arc<rayon::ThreadPool>>,
    ) -> OptimizationResult {
        let solver = &self.config.optimizer.solver;
        let scope = CancelScope::attach(cancel);
        let default_bounds = DesignVector::bounds();
        let bounds = bounds.unwrap_or(&default_bounds);
        let seed = solver.seed.map_or_else(runtime_seed, |value| value as u64);
        let mut initial_values = initial_design.map(DesignVector::to_array);
        if let Some(initial) = initial_values.as_mut() {
            crate::search_methods::clamp_to_bounds(initial, bounds);
        }
        // The analysis-start instant for this stage.  Every elapsed time
        // reported below, including the staged scan, is measured from here so
        // a runtime claim covers the whole search rather than its last phase.
        let started = Instant::now();
        let dimension = bounds.iter().filter(|(lower, upper)| upper > lower).count();
        let mut staged = crate::search::staged::Settings::from_solver(solver, dimension);
        staged.seed = seed;
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
        let scan = self.broad_scan(
            bounds,
            initial_values.as_deref(),
            &staged,
            &scope,
            compute_pool,
        );
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

        let de = product_de::Settings::from_solver(solver, dimension, seed);
        report(&format!(
            "differential evolution | population {} | generations {} | seed {} | evaluation_budget {}",
            de.population,
            de.generations,
            de.seed,
            de.evaluation_budget()
        ));
        // The adapter times only cache misses; the kernel owns cancellation
        // checks and generation phases but never counts reused scores as new
        // coupled analyses.
        let mut evaluator = BatchEvaluator {
            objective,
            workers: staged.workers,
            scope: CancelScope::attach(cancel),
            cache: evaluation_cache::EvaluationCache::new(verified_start.as_ref()),
        };
        let before_search = evaluator.objective.history().n_evaluations();
        let mut outcome = product_de::run(
            bounds,
            initial_values.as_deref(),
            de,
            &scope,
            &mut |points: &[Vec<f64>]| evaluator.evaluate_block(points),
        );
        let restoration = feasibility_restoration::run(
            bounds,
            &mut outcome,
            de.generations,
            &scope,
            &mut evaluator,
        );
        let analysis_evaluations = evaluator.objective.history().n_evaluations() - before_search;
        let cache_hits = evaluator.cache.hits;
        debug_assert_eq!(analysis_evaluations + cache_hits, outcome.evaluations);
        // `converged` is the kernel's own verdict (population spread plus
        // best-feasible-cost stagnation; see `search_methods::lshade_de`) and
        // is never true without a feasible design. `iteration_limit` is the
        // shared termination vocabulary a budget-exhausted, non-converged
        // search reports elsewhere (`run_search`'s own DE path);
        // inventing a distinct string here would silently break every caller
        // that checks termination against that fixed set. `cancelled` is the
        // one lifecycle this kernel can reach that is neither: a caller
        // observed the pipeline's own cancellation signal at a block
        // boundary and stopped before either budget or convergence decided
        // the run.
        let termination = if outcome.cancelled {
            CANCELLED
        } else if dimension == 0 {
            "fixed_bounds"
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
            restoration,
            converged: outcome.converged,
            analysis_evaluations,
            cache_hits,
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
