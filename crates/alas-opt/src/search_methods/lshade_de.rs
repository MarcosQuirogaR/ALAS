// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one product search kernel: L-SHADE differential evolution under the
//! epsilon-constrained method, with an explicit convergence test.
//!
//! # Basis and citations
//!
//! - **L-SHADE**: R. Tanabe and A. S. Fukunaga, "Improving the Search
//!   Performance of SHADE Using Linear Population Size Reduction," IEEE
//!   Congress on Evolutionary Computation (CEC) 2014, DOI
//!   10.1109/CEC.2014.6900380. Supplies the success-history parameter
//!   adaptation for the mutation factor `F` and crossover rate `CR`
//!   (weighted Lehmer/arithmetic means into a circular memory of size `H`),
//!   the `current-to-pbest/1` mutation with an external archive (itself from
//!   J. Zhang and A. C. Sanderson, "JADE: Adaptive Differential Evolution
//!   With Optional External Archive," IEEE Trans. Evol. Comput. 13(5), 2009,
//!   DOI 10.1109/TEVC.2009.2014613), and linear population size reduction
//!   (LPSR) from an initial population down to a small floor as the
//!   generation budget is spent.
//! - **Epsilon-constrained method**: T. Takahama and S. Sakai, "Constrained
//!   Optimization by the epsilon Constrained Differential Evolution with
//!   Gradient-Based Mutation and Feasible Elites," CEC 2006, DOI
//!   10.1109/CEC.2006.1688283, and "Constrained Optimization by the epsilon
//!   Constrained Differential Evolution with an Archive and Gradient-Based
//!   Mutation," CEC 2010, DOI 10.1109/CEC.2010.5586484. Supplies the
//!   epsilon-level comparison (two candidates within `epsilon` of feasible
//!   are ranked by objective alone; otherwise the less-violating one wins)
//!   and the schedule that decays `epsilon` to exactly zero at a configured
//!   fraction of the budget, after which the comparison is exactly Deb's
//!   feasibility rule (K. Deb, "An Efficient Constraint Handling Method for
//!   Genetic Algorithms," Computer Methods in Applied Mechanics and
//!   Engineering 186(2-4), 2000).
//!
//! Population sizing, memory size, archive rate and the epsilon schedule's
//! control fraction below are the values commonly reported for these methods
//! in the cited literature; they are engineering defaults for this product,
//! not a reproduction of either paper's own benchmark tuning, and are not
//! independently re-derived here.
//!
//! # What "unphysical results impossible" means in this module
//!
//! [`ScoredPoint::valid`] and [`ScoredPoint::constraint_violation`] come from
//! the caller's full coupled evaluation (see [`crate::mdo`]: geometry and
//! mass build, mission sizing closure, trim/CG closure, then the typed
//! residual table `mdo::residuals` folds into this scalar violation). The
//! epsilon-relaxed comparison used while searching is deliberately never the
//! authority on what the run reports: [`Outcome::winner`] is tracked as the
//! strict [`ScoredPoint::feasibility_key`] minimum over every candidate this
//! run ever evaluated (initial population and every trial), independent of
//! which individuals epsilon-relaxation let survive inside the live
//! population. So relaxing the constraint boundary to escape a local optimum
//! can change what the search explores; it can never change what it reports
//! as feasible. A caller that finds `winner.valid == false` has firm
//! evidence that no evaluated candidate satisfied every hard constraint, not
//! a candidate that merely stopped being tracked.
//!
//! # Determinism and cancellation
//!
//! One generation is built as `population_size` trial vectors, in fixed
//! index order, entirely from the seeded RNG stream, before any of them is
//! evaluated; the whole generation is then evaluated as one batch through
//! `evaluate_batch`. Nothing about which points are evaluated or in what
//! order depends on `solver.workers`; only how the batch is spread across
//! threads does (see `differential_evolution_optimizer::BatchEvaluator`). A
//! seeded run therefore replays bit-identically at any worker count.
//!
//! That batching is also what the cancellation flag is checked against: once
//! per generation, before its batch is dispatched, exactly like the staged
//! scan's own blocks (`search::staged`). A flag already set when a
//! generation would start analyses nothing more; a flag set mid-batch is
//! observed at the next generation boundary, so the bound on stopping is one
//! generation's batch rather than one candidate. This is a real change from
//! the previous serial kernel's per-candidate check, and it is the price of
//! the batch being genuinely parallel and worker-count-independent: a
//! parallel batch has no well-defined "next candidate" to stop before. Work
//! already produced is still never discarded: a cancelled run returns the
//! best point analysed, exactly as an uncancelled one does.

use crate::cancellation::{CancelPhase, CancelScope};
use crate::python_rng::RandomState;

use super::{EvaluateBatch, ScoredPoint};

#[path = "lshade_de_ops.rs"]
mod ops;
use ops::{
    choose_distinct, choose_from_union, choose_pbest, clamp_into_bounds, epsilon_key,
    epsilon_schedule, latin_hypercube, linear_reduced_size, min_by_feasibility, normalized_spread,
    push_archive, quantile, reduce_population, relative_change, relative_change_signed,
    repair_midpoint, sample_cr, sample_f, trim_archive, unevaluated, update_memory,
};

/// Smallest population L-SHADE's linear reduction may shrink to. Below four,
/// `current-to-pbest/1` cannot draw a pbest and two distinct difference
/// vectors from the population without touching the target itself.
const MIN_POPULATION: usize = 4;
/// Success-history memory slots (`H` in the cited papers).
const MEMORY_SIZE: usize = 6;
/// Archive capacity as a multiple of the live population.
const ARCHIVE_RATE: f64 = 2.6;
/// Upper bound of the per-trial `pbest` pool fraction; the lower bound is
/// `2 / population_size` each generation, so the pool is never smaller than
/// two individuals.
const P_BEST_MAX_FRACTION: f64 = 0.2;
/// Quantile of the initial population's constraint violation used as
/// `epsilon(0)`: the boundary starts at "about as tolerant as the worse four
///-fifths of a fresh random sample," not at zero.
const EPSILON_INITIAL_QUANTILE: f64 = 0.2;
/// Fraction of the generation budget over which `epsilon` decays to zero.
const EPSILON_CONTROL_FRACTION: f64 = 0.2;
/// Exponent of the epsilon decay curve.
const EPSILON_DECAY_EXPONENT: f64 = 5.0;
/// Initial success-history memory value for both `F` and `CR`.
const INITIAL_MEMORY: f64 = 0.5;

/// Resolved settings for one L-SHADE run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Settings {
    /// Initial population size, before linear reduction.
    pub(crate) population: usize,
    /// Maximum number of generations.
    pub(crate) generations: usize,
    /// The seed the whole search replays from.
    pub(crate) seed: u64,
    /// Normalized design-space spread, and relative improvement of the best
    /// feasible cost, below which the population counts as converged.
    pub(crate) spread_tolerance: f64,
    /// Consecutive generations the best feasible cost must fail to improve
    /// by more than `spread_tolerance` before a converged spread is honoured.
    pub(crate) stagnation_generations: usize,
}

/// What one run measured about itself, for [`crate::SearchDiagnostics`] and
/// the caller's termination label.
pub(crate) struct Outcome {
    pub(crate) winner: ScoredPoint,
    /// Whether the run stopped on the caller's cancellation flag.
    pub(crate) cancelled: bool,
    /// Whether the population's own convergence test fired. Never true
    /// unless a feasible design was found: see the module documentation.
    pub(crate) converged: bool,
    /// Full generations evaluated (a generation cancelled before its batch
    /// was dispatched is not counted).
    pub(crate) generations_completed: usize,
    /// Coupled analyses executed (initial population plus every generation's
    /// batch actually evaluated; the population shrinks, so this is not a
    /// fixed multiple of the generation count).
    pub(crate) evaluations: usize,
    /// The epsilon-constraint boundary at the last generation evaluated, or
    /// `epsilon(0)` when no generation ran.
    pub(crate) epsilon_final: f64,
    /// Fraction of the final population that was strictly feasible.
    pub(crate) feasible_fraction: f64,
    /// Objective of the first strictly feasible candidate this run evaluated,
    /// in chronological order (initial population, then generation by
    /// generation). `None` when no evaluated candidate was ever feasible.
    pub(crate) first_feasible_cost: Option<f64>,
    /// Relative improvement of [`Self::winner`] over
    /// [`Self::first_feasible_cost`], positive for a cost reduction. `None`
    /// unless the winner is itself feasible and a first feasible cost was
    /// recorded.
    pub(crate) relative_improvement: Option<f64>,
}

/// Run L-SHADE under the epsilon-constrained method. See the module
/// documentation for the algorithm, the determinism contract and what
/// `winner` may report.
pub(crate) fn run(
    bounds: &[(f64, f64)],
    settings: Settings,
    initial_design: Option<&[f64]>,
    scope: &CancelScope<'_>,
    evaluate_batch: &mut EvaluateBatch<'_>,
) -> Outcome {
    let dimension = bounds.len();
    let population_initial = settings.population.max(MIN_POPULATION);
    let mut rng = RandomState::seed(settings.seed);

    scope.enter(CancelPhase::DeInitialPopulation, 0);
    if scope.requested() {
        scope.work_skipped("initial population stopped before it started");
        return Outcome {
            winner: unevaluated(initial_design.unwrap_or(&[])),
            cancelled: true,
            converged: false,
            generations_completed: 0,
            evaluations: 0,
            epsilon_final: 0.0,
            feasible_fraction: 0.0,
            first_feasible_cost: None,
            relative_improvement: None,
        };
    }

    let mut population = latin_hypercube(bounds, population_initial, &mut rng);
    if let Some(initial) = initial_design.filter(|values| values.len() == dimension) {
        let mut seeded = initial.to_vec();
        clamp_into_bounds(&mut seeded, bounds);
        population[0] = seeded;
    }
    let mut scored = scope.block(population.len() as u64, || evaluate_batch(&population));
    debug_assert_eq!(scored.len(), population.len());

    let mut best_ever = scored
        .iter()
        .min_by_key(|point| point.feasibility_key())
        .cloned()
        .unwrap_or_else(|| unevaluated(&population[0]));
    let mut evaluations = population.len();
    // The first strictly feasible candidate this run ever evaluated, in
    // chronological (candidate-index, then generation) order, independent of
    // the epsilon-relaxed dynamics that decide which candidates stay in the
    // live population.
    let mut first_feasible_cost = scored
        .iter()
        .find(|point| point.valid)
        .map(|point| point.cost);

    let mut violations: Vec<f64> = scored
        .iter()
        .map(|point| point.constraint_violation)
        .collect();
    violations.sort_by(f64::total_cmp);
    let epsilon0 = quantile(&violations, EPSILON_INITIAL_QUANTILE);
    let control_generations =
        ((settings.generations as f64) * EPSILON_CONTROL_FRACTION).floor() as usize;

    let mut memory_f = [INITIAL_MEMORY; MEMORY_SIZE];
    let mut memory_cr = [INITIAL_MEMORY; MEMORY_SIZE];
    let mut memory_index = 0usize;
    let mut archive: Vec<Vec<f64>> = Vec::new();

    let mut last_feasible_cost: Option<f64> = best_ever.valid.then_some(best_ever.cost);
    let mut last_improved_generation: usize = 0;
    let mut generations_completed = 0usize;
    let mut cancelled = false;
    let mut converged = false;
    let mut epsilon_final = epsilon0;
    let mut population_size = population_initial;

    for generation in 0..settings.generations {
        scope.enter(CancelPhase::DeGeneration, generation as u64);
        if scope.requested() {
            cancelled = true;
            scope.work_skipped(format!(
                "generation {generation} stopped before its batch was dispatched"
            ));
            break;
        }

        let epsilon_t = epsilon_schedule(epsilon0, generation, control_generations);
        epsilon_final = epsilon_t;

        let mut trials = Vec::with_capacity(population_size);
        let mut trial_f = Vec::with_capacity(population_size);
        let mut trial_cr = Vec::with_capacity(population_size);
        for target in 0..population_size {
            let slot = rng.randint(MEMORY_SIZE);
            let f = sample_f(memory_f[slot], &mut rng);
            let cr = sample_cr(memory_cr[slot], &mut rng);
            trial_f.push(f);
            trial_cr.push(cr);

            let p_min = 2.0 / population_size as f64;
            let p = rng.uniform(
                p_min.min(P_BEST_MAX_FRACTION),
                P_BEST_MAX_FRACTION.max(p_min),
            );
            let pbest = choose_pbest(&scored[..population_size], epsilon_t, p, &mut rng);
            let r1 = choose_distinct(population_size, &[target], &mut rng);
            let r2 = choose_from_union(population_size, archive.len(), &[target, r1], &mut rng);

            let base = &population[target];
            let pbest_vec = &population[pbest];
            let r1_vec = &population[r1];
            let r2_vec: &[f64] = if r2 < population_size {
                &population[r2]
            } else {
                &archive[r2 - population_size]
            };

            let mut mutant = vec![0.0; dimension];
            for j in 0..dimension {
                let value = base[j] + f * (pbest_vec[j] - base[j]) + f * (r1_vec[j] - r2_vec[j]);
                mutant[j] = repair_midpoint(value, base[j], bounds[j]);
            }

            let forced = rng.randint(dimension);
            let mut trial = base.clone();
            for j in 0..dimension {
                if j == forced || rng.uniform(0.0, 1.0) < cr {
                    trial[j] = mutant[j];
                }
            }
            trials.push(trial);
        }

        let trial_scores = scope.block(trials.len() as u64, || evaluate_batch(&trials));
        evaluations += trial_scores.len();
        generations_completed += 1;

        let mut successes: Vec<(f64, f64, f64)> = Vec::new();
        for target in 0..population_size {
            let trial_point = &trial_scores[target];
            best_ever = min_by_feasibility(best_ever, trial_point.clone());
            if first_feasible_cost.is_none() && trial_point.valid {
                first_feasible_cost = Some(trial_point.cost);
            }
            if epsilon_key(trial_point, epsilon_t) < epsilon_key(&scored[target], epsilon_t) {
                let improvement = (scored[target].cost - trial_point.cost).abs();
                successes.push((trial_f[target], trial_cr[target], improvement));
                push_archive(
                    &mut archive,
                    population[target].clone(),
                    population_size,
                    &mut rng,
                );
                population[target] = trials[target].clone();
                scored[target] = trial_point.clone();
            }
        }
        update_memory(&mut memory_f, &mut memory_cr, &mut memory_index, &successes);

        let fraction =
            ((generation + 1) as f64 / settings.generations.max(1) as f64).clamp(0.0, 1.0);
        let next_size = linear_reduced_size(population_initial, fraction);
        if next_size < population_size {
            reduce_population(&mut population, &mut scored, next_size, epsilon_t);
            population_size = next_size;
            trim_archive(&mut archive, population_size, &mut rng);
        }

        if best_ever.valid {
            let improved = last_feasible_cost.is_none_or(|previous| {
                relative_change(previous, best_ever.cost) > settings.spread_tolerance
            });
            if improved {
                last_improved_generation = generation;
            }
            last_feasible_cost = Some(best_ever.cost);
        }

        let spread = normalized_spread(&population[..population_size], bounds);
        let stagnated =
            generation.saturating_sub(last_improved_generation) >= settings.stagnation_generations;
        if best_ever.valid && spread <= settings.spread_tolerance && stagnated {
            converged = true;
            break;
        }
    }

    let feasible_fraction = if population_size == 0 {
        0.0
    } else {
        scored[..population_size]
            .iter()
            .filter(|point| point.valid)
            .count() as f64
            / population_size as f64
    };

    let relative_improvement = if best_ever.valid {
        first_feasible_cost.map(|start| relative_change_signed(start, best_ever.cost))
    } else {
        None
    };

    Outcome {
        winner: best_ever,
        cancelled,
        converged,
        generations_completed,
        evaluations,
        first_feasible_cost,
        relative_improvement,
        epsilon_final,
        feasible_fraction,
    }
}

#[cfg(test)]
#[path = "lshade_de_tests.rs"]
mod lshade_de_tests;
