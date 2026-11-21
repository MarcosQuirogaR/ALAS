// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The refinement kernel: `current-to-pbest/1/bin` differential evolution
//! with an external archive, midpoint bound repair, linear population size
//! reduction, and Deb's feasibility rules with an epsilon-level comparison.
//!
//! # Basis and citations
//!
//! - **Operator, archive, population reduction**: R. Tanabe and A. S.
//!   Fukunaga, "Improving the Search Performance of SHADE Using Linear
//!   Population Size Reduction," IEEE CEC 2014, DOI 10.1109/CEC.2014.6900380
//!   (L-SHADE): `current-to-pbest/1` with `p = 0.11`, archive `2.6 N`,
//!   memory size `H = 6`, both memories updated by the weighted Lehmer mean,
//!   the crossover-rate terminal value, and population reduction linear in
//!   evaluations. The operator and archive are J. Zhang and A. C. Sanderson,
//!   "JADE: Adaptive Differential Evolution With Optional External Archive,"
//!   IEEE Trans. Evol. Comput. 13(5), 2009, DOI 10.1109/TEVC.2009.2014613.
//! - **Static parameters by default**: R. Tanabe and A. S. Fukunaga,
//!   "Reviewing and Benchmarking Parameter Control Methods in Differential
//!   Evolution," IEEE Trans. Cybern. 50(3), 2020, DOI
//!   10.1109/TCYB.2019.2892735 (arXiv 2010.01035): with `current-to-pbest/1/bin`,
//!   static `F = 0.5, CR = 0.9` outperformed 24 parameter-control methods
//!   within `800 D` evaluations. Every budget this product affords in minutes
//!   is below that, so success-history adaptation is a switch
//!   ([`Settings::adaptation`]) recommended only above `800 D`.
//! - **Constraints**: K. Deb, "An Efficient Constraint Handling Method for
//!   Genetic Algorithms," CMAME 186(2-4), 2000, DOI
//!   10.1016/S0045-7825(99)00389-8, extended by the candidate tiers of
//!   [`super::Tier`]; the epsilon-level comparison is T. Takahama and S. Sakai,
//!   CEC 2006, DOI 10.1109/CEC.2006.1688283, and CEC 2010, DOI
//!   10.1109/CEC.2010.5586484. No additive penalty enters the comparison.
//!
//! The epsilon schedule `epsilon(0)` = the 0.2 quantile of the initial
//! violations, decaying as `(1 - n / Tc)^5` to exactly zero at `Tc = 0.2 B`
//! evaluations, is an engineering choice for this product: the values the
//! cited papers tabulate were not verified against the primary text.
//!
//! Two deliberate deviations from Takahama and Sakai, both engineering
//! choices:
//!
//! - In their comparison an epsilon-feasible candidate (violation within
//!   epsilon) ranks with the feasible ones by objective alone. Here a
//!   strictly feasible candidate always ranks ahead of an epsilon-feasible
//!   one ([`ops::epsilon_key`]); epsilon only lets closed-infeasible
//!   candidates compete with each other by objective. A design with any
//!   violated hard constraint can therefore never displace a feasible parent.
//! - `epsilon(0)` is taken from the closed-infeasible members of the initial
//!   population only. Not-closed and pre-gate-rejected candidates carry
//!   barrier violations that are not physical misses, and including them
//!   would set epsilon from failure sentinels.
//!
//! When adaptation is on, the success weights of the memory update are the
//! relative improvement of each parent, with a tier change weighted 1
//! ([`ops::selection_improvement`]), where L-SHADE uses the absolute
//! objective change; the relative form lets a wholly infeasible population
//! still learn. Adaptation is off by default.
//!
//! # What the run reports
//!
//! [`Outcome::winner`] is the strict [`ScoredPoint::feasibility_key`] minimum
//! over every candidate evaluated, never the epsilon-relaxed survivor, so
//! relaxing the comparison changes what is explored and never what is
//! reported as feasible.
//!
//! # Determinism, budget and time
//!
//! A generation's trials are built in index order from the seeded stream
//! before any is evaluated, and the generation is evaluated as one batch
//! whose scores come back in index order, so a seeded run that stops on its
//! evaluation budget replays bit-identically at any worker count. Every
//! score that passes the design-vector pre-gate counts against
//! [`Settings::max_evaluations`]; [`Settings::schedule_evaluations`] sets
//! the population schedule independently of this hard ceiling. A
//! generation never requests more trials than the budget has left, so the
//! budget is never exceeded. Pre-gate rejections count against
//! [`Settings::max_rejects`], checked between generations, so the cap is
//! overrun by at most one generation. The kernel projects the next batch's
//! wall time at generation boundaries. A deadline-aware evaluator may also
//! stop taking candidates inside a batch and return its started prefix;
//! only those scores enter the budget and selection. A time-limited stop
//! therefore depends on machine speed and worker count. The recorded
//! evaluation count replays the same ordered prefix with
//! [`Settings::stop_after`] and no time limit, including a partial initial
//! population or generation. The population schedule depends on the
//! configured budget, never on the clock or where the run stopped.

use std::time::{Duration, Instant};

use super::rng::SearchRng;
use super::{latin_hypercube, EvaluateBatch, ScoredPoint, Tier};
use crate::cancellation::{CancelPhase, CancelScope};

#[path = "lshade_de_evaluation.rs"]
mod evaluation;
#[path = "lshade_de_ops.rs"]
mod ops;
#[path = "lshade_de_types.rs"]
mod types;
pub(super) use evaluation::{evaluate, track_winner};
use evaluation::{feasible_fraction, prefix_termination, record, tally};
use ops::{
    choose_distinct, choose_from_union, choose_pbest, epsilon_key, epsilon_schedule, lehmer_mean,
    linear_reduced_size, normalized_spread, push_archive, reduce_population, relative_change,
    repair_midpoint, sample_cr, sample_f, selection_improvement, stagnation_stops, unevaluated,
};
pub(crate) use types::{Outcome, Settings, Termination};

/// Default population floor. The L-SHADE paper uses four; eight preserves
/// a wider late search for evaluator adapters with limited concurrency.
pub(super) const MIN_POPULATION: usize = 8;
/// Below four members `current-to-pbest/1` cannot draw a target, a pbest and
/// two distinct difference vectors, so no generation runs.
const OPERATOR_MIN_POPULATION: usize = 4;
/// Success-history memory slots (`H`).
const MEMORY_SIZE: usize = 6;
/// Archive capacity as a multiple of the live population.
const ARCHIVE_RATE: f64 = 2.6;
/// Fraction of the population the pbest donor is drawn from.
const P_BEST_FRACTION: f64 = 0.11;
/// Static mutation factor and crossover rate.
const STATIC_F: f64 = 0.5;
const STATIC_CR: f64 = 0.9;
/// Initial success-history memory value for both `F` and `CR`.
const INITIAL_MEMORY: f64 = 0.5;
/// Quantile of the initial violations used as `epsilon(0)`.
const EPSILON_INITIAL_QUANTILE: f64 = 0.2;
/// Fraction of the evaluation budget over which epsilon decays to zero.
const EPSILON_CONTROL_FRACTION: f64 = 0.2;
/// Relative improvement of the feasible best below which a generation does
/// not count as progress (engineering estimate: well below the 0.1 % a
/// conceptual fuel comparison can resolve, above round-off).
pub(crate) const IMPROVEMENT_TOLERANCE: f64 = 1.0e-4;

/// Success-history memory, used only when adaptation is enabled.
struct Memory {
    f: [f64; MEMORY_SIZE],
    /// `None` is the terminal value: that slot draws `CR = 0` from then on.
    cr: [Option<f64>; MEMORY_SIZE],
    next: usize,
}

impl Memory {
    fn draw(&self, rng: &mut SearchRng) -> (f64, f64) {
        let slot = rng.below(MEMORY_SIZE);
        (sample_f(self.f[slot], rng), sample_cr(self.cr[slot], rng))
    }

    /// Weighted Lehmer means of the successful `(F, CR, weight)` triples into
    /// one slot; nothing changes when no trial succeeded.
    fn update(&mut self, successes: &[(f64, f64, f64)]) {
        let Some(f) = lehmer_mean(successes.iter().map(|&(f, _, w)| (f, w))) else {
            return;
        };
        let slot = self.next;
        self.f[slot] = f.clamp(0.0, 1.0);
        let all_zero = successes.iter().all(|&(_, cr, _)| cr == 0.0);
        self.cr[slot] = match self.cr[slot] {
            Some(_) if !all_zero => lehmer_mean(successes.iter().map(|&(_, cr, w)| (cr, w)))
                .map(|value| value.clamp(0.0, 1.0)),
            _ => None,
        };
        self.next = (slot + 1) % MEMORY_SIZE;
    }
}

/// Run the kernel from `seeds` (repaired into `bounds`, best-first, the
/// caller's baseline first), topped up by a Latin hypercube to the initial
/// population. `started` is the stage clock the time limit is measured on.
///
/// A trial the design-vector pre-gate rejects is not redrawn. Measured on
/// the A320-200 box (seed 1, 140 refinement requests): redrawing up to
/// eight times cut the analyses from 87 to 47, refilling the top-up as well
/// to 25, with no gain in lane utilization (0.52 against 0.54 at 16
/// workers), because a generation of at most `N_init` trials, not the
/// rejects, is what bounds the lanes.
#[cfg(test)]
pub(crate) fn run(
    bounds: &[(f64, f64)],
    seeds: &[Vec<f64>],
    settings: Settings,
    started: Instant,
    scope: &CancelScope<'_>,
    evaluate_batch: &mut EvaluateBatch<'_>,
) -> Outcome {
    run_with_admission(
        bounds,
        seeds,
        settings,
        started,
        scope,
        None,
        evaluate_batch,
    )
}

pub(crate) fn run_with_admission(
    bounds: &[(f64, f64)],
    seeds: &[Vec<f64>],
    settings: Settings,
    started: Instant,
    scope: &CancelScope<'_>,
    admits: Option<&super::Admission<'_>>,
    evaluate_batch: &mut EvaluateBatch<'_>,
) -> Outcome {
    run_guided(
        bounds,
        seeds,
        settings,
        started,
        scope,
        admits,
        None,
        evaluate_batch,
    )
}

/// [`run_with_admission`] with trial pre-selection: for every target the
/// kernel builds [`super::Guidance::alternatives`] alternative trials from
/// independent draws of the same operator and the guide names the one to
/// analyse (its index), among the alternatives the admission check passes
/// when any does. The selection and every reported score are
/// unchanged: a guide only decides which trial is paid for (pre-selection
/// by a surrogate: R. G. Regis, "Evolutionary Programming for
/// High-Dimensional Constrained Expensive Black-Box Optimization Using
/// Radial Basis Functions," IEEE Trans. Evol. Comput. 18(3), 2014, DOI
/// 10.1109/TEVC.2013.2262111).
///
/// Local trials around the incumbent for the worst members (Regis and
/// Shoemaker's dynamic coordinate search, Engineering Optimization 45(5),
/// 2013) were measured beside this and dropped: at 384 exploration
/// candidates they changed the refined gain from -1.14 to -1.12 % on the
/// A320-200 and from -4.93 to -4.54 % on the LJ60.
///
/// The guide is called in target order from the seeded stream, so a guide
/// that is a function of the scores so far keeps the run reproducible.
// One argument per independent input of the kernel, as its callers pass them.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_guided(
    bounds: &[(f64, f64)],
    seeds: &[Vec<f64>],
    settings: Settings,
    started: Instant,
    scope: &CancelScope<'_>,
    admits: Option<&super::Admission<'_>>,
    mut guide: Option<super::Guidance<'_, '_>>,
    evaluate_batch: &mut EvaluateBatch<'_>,
) -> Outcome {
    let budget = settings.max_evaluations.max(1);
    let schedule = settings.schedule_evaluations.max(1).min(budget);
    let stop_after = settings.stop_after.min(budget);
    let population_initial = settings.population.min(budget);
    let minimum = settings.minimum_population.max(1).min(population_initial);
    let mut rng = SearchRng::stream(settings.seed, 2);
    let mut outcome = Outcome {
        winner: unevaluated(seeds.first().map_or(&[], Vec::as_slice)),
        first_scored: None,
        termination: Termination::EvaluationBudget,
        generations_completed: 0,
        evaluations: 0,
        population_initial,
        population_final: population_initial,
        epsilon_final: 0.0,
        feasible_fraction: 0.0,
        first_feasible_cost: None,
        rejected: 0,
        best_by_generation: Vec::new(),
        spread_final: 1.0,
    };

    scope.enter(CancelPhase::DeInitialPopulation, 0);
    if scope.requested() {
        scope.work_skipped("refinement stopped before its initial population");
        outcome.termination = Termination::Cancelled;
        return outcome;
    }
    if population_initial == 0 {
        return outcome;
    }
    let mut population: Vec<Vec<f64>> = seeds.iter().take(population_initial).cloned().collect();
    for point in &mut population {
        super::clamp_to_bounds(point, bounds);
    }
    let top_up = population_initial - population.len();
    population.extend(latin_hypercube(bounds, top_up, &mut rng));

    let initial_started = Instant::now();
    let mut scored = evaluate(&population, stop_after, admits, evaluate_batch);
    tally(&mut outcome, &scored);
    outcome.first_scored = scored.first().cloned();
    for point in &scored {
        record(&mut outcome, point);
    }
    if scope.requested() {
        outcome.termination = Termination::Cancelled;
        return outcome;
    }
    if scored.len() < population.len() {
        outcome.population_final = scored.len();
        outcome.feasible_fraction = feasible_fraction(&scored);
        outcome.termination = prefix_termination(outcome.evaluations, stop_after);
        return outcome;
    }

    outcome
        .best_by_generation
        .push(outcome.winner.valid().then_some(outcome.winner.cost));
    let mut violations: Vec<f64> = scored
        .iter()
        .filter(|point| point.tier == Tier::ClosedInfeasible)
        .map(|point| point.constraint_violation)
        .collect();
    violations.sort_by(f64::total_cmp);
    let epsilon0 = ops::quantile(&violations, EPSILON_INITIAL_QUANTILE);
    let control_evaluations = (schedule as f64 * EPSILON_CONTROL_FRACTION).floor() as usize;
    outcome.epsilon_final = epsilon_schedule(epsilon0, outcome.evaluations, control_evaluations);

    let active: Vec<usize> = (0..bounds.len())
        .filter(|&index| bounds[index].1 > bounds[index].0)
        .collect();
    let mut memory = Memory {
        f: [INITIAL_MEMORY; MEMORY_SIZE],
        cr: [Some(INITIAL_MEMORY); MEMORY_SIZE],
        next: 0,
    };
    let mut archive: Vec<Vec<f64>> = Vec::new();
    let mut size = population_initial;
    let mut reference_cost: Option<f64> = outcome.winner.valid().then_some(outcome.winner.cost);
    let mut stalled = 0usize;
    // The guard projects the next generation's wall time from the last
    // batch; before the first generation that batch is the initial
    // population, at least as large as any generation.
    let mut last_generation = initial_started.elapsed();

    while !active.is_empty() && size >= OPERATOR_MIN_POPULATION {
        if outcome.evaluations >= stop_after {
            break;
        }
        if outcome.rejected >= settings.max_rejects {
            outcome.termination = Termination::PregateExhausted;
            break;
        }
        let limit = if outcome.winner.valid() {
            budget
        } else {
            schedule.saturating_sub(settings.infeasible_reserve)
        };
        let count = size.min(limit.saturating_sub(outcome.evaluations));
        if count == 0 {
            break;
        }
        if settings.time_limit.is_some_and(|limit| {
            let projected = if settings.intra_batch_deadline {
                Duration::ZERO
            } else {
                last_generation
            };
            started.elapsed() + projected >= limit
        }) {
            outcome.termination = Termination::TimeBudget;
            break;
        }
        let generation = outcome.generations_completed;
        scope.enter(CancelPhase::DeGeneration, generation as u64);
        if scope.requested() {
            scope.work_skipped(format!("generation {generation} stopped before dispatch"));
            outcome.termination = Termination::Cancelled;
            break;
        }
        let generation_started = Instant::now();
        let epsilon = epsilon_schedule(epsilon0, outcome.evaluations, control_evaluations);
        outcome.epsilon_final = epsilon;

        let mut trials = Vec::with_capacity(count);
        let mut parameters = Vec::with_capacity(count);
        for target in 0..count {
            let (f, cr) = if settings.adaptation {
                memory.draw(&mut rng)
            } else {
                (STATIC_F, STATIC_CR)
            };
            parameters.push((f, cr));
            let alternatives = guide
                .as_ref()
                .map_or(1, |guidance| guidance.alternatives.max(1));
            let mut built = Vec::with_capacity(alternatives);
            for _ in 0..alternatives {
                let pbest = choose_pbest(&scored[..size], epsilon, P_BEST_FRACTION, &mut rng);
                let r1 = choose_distinct(size, &[target], &mut rng);
                let r2 = choose_from_union(size, archive.len(), &[target, r1], &mut rng);
                let r2_vec = population.get(r2).unwrap_or_else(|| &archive[r2 - size]);
                let base = &population[target];
                let forced = active[rng.below(active.len())];
                let mut trial = base.clone();
                for j in 0..bounds.len() {
                    if j == forced || rng.unit() < cr {
                        let value = base[j]
                            + f * (population[pbest][j] - base[j])
                            + f * (population[r1][j] - r2_vec[j]);
                        trial[j] = repair_midpoint(value, base[j], bounds[j]);
                    }
                }
                built.push(trial);
            }
            let chosen = match guide.as_mut() {
                Some(guidance) if built.len() > 1 => {
                    // The guide ranks the alternatives the pre-gate admits
                    // when there are any: a trial no analysis will see
                    // costs its generation a lane.
                    let admitted: Vec<usize> = admits.map_or_else(Vec::new, |admits| {
                        (0..built.len()).filter(|&i| admits(&built[i])).collect()
                    });
                    if admitted.is_empty() || admitted.len() == built.len() {
                        (guidance.choose)(&built).min(built.len() - 1)
                    } else {
                        let subset: Vec<Vec<f64>> =
                            admitted.iter().map(|&i| built[i].clone()).collect();
                        admitted[(guidance.choose)(&subset).min(subset.len() - 1)]
                    }
                }
                _ => 0,
            };
            trials.push(built.swap_remove(chosen));
        }

        let trial_scores = evaluate(
            &trials,
            stop_after.saturating_sub(outcome.evaluations),
            admits,
            evaluate_batch,
        );
        let completed = trial_scores.len();
        tally(&mut outcome, &trial_scores);
        for point in &trial_scores {
            record(&mut outcome, point);
        }
        if scope.requested() {
            outcome.termination = Termination::Cancelled;
            break;
        }
        outcome.generations_completed += 1;
        outcome
            .best_by_generation
            .push(outcome.winner.valid().then_some(outcome.winner.cost));

        let mut successes = Vec::new();
        for (target, trial) in trial_scores.into_iter().enumerate() {
            let trial_key = epsilon_key(&trial, epsilon);
            let parent_key = epsilon_key(&scored[target], epsilon);
            if trial_key < parent_key {
                let (f, cr) = parameters[target];
                successes.push((f, cr, selection_improvement(&scored[target], &trial)));
                let parent = std::mem::take(&mut population[target]);
                push_archive(&mut archive, parent, size, ARCHIVE_RATE, &mut rng);
            }
            if trial_key <= parent_key {
                population[target] = trials[target].clone();
                scored[target] = trial;
            }
        }
        if completed < trials.len() {
            outcome.termination = prefix_termination(outcome.evaluations, stop_after);
            break;
        }
        if settings.adaptation {
            memory.update(&successes);
        }

        let next = ops::quantized_size(
            linear_reduced_size(population_initial, minimum, outcome.evaluations, schedule),
            settings.population_quantum,
            minimum,
            size,
        );
        if next < size {
            reduce_population(&mut population, &mut scored, next, epsilon);
            size = next;
            ops::trim_archive(&mut archive, size, ARCHIVE_RATE, &mut rng);
        }

        if outcome.winner.valid() {
            let improved = reference_cost.is_none_or(|previous| {
                relative_change(previous, outcome.winner.cost) > IMPROVEMENT_TOLERANCE
            });
            if improved {
                reference_cost = Some(outcome.winner.cost);
                stalled = 0;
            } else {
                stalled += 1;
            }
            let window = (settings.stagnation_generations, population_initial, minimum);
            if epsilon == 0.0 && stagnation_stops(stalled, window, outcome.evaluations, schedule) {
                let spread = normalized_spread(&population[..size], bounds);
                outcome.termination = if spread <= settings.spread_tolerance {
                    Termination::Converged
                } else {
                    Termination::Stagnated
                };
                break;
            }
        }
        last_generation = generation_started.elapsed();
    }

    outcome.population_final = size.min(scored.len());
    outcome.spread_final = normalized_spread(
        &population[..outcome.population_final.min(population.len())],
        bounds,
    );
    let live = &scored[..outcome.population_final];
    outcome.feasible_fraction = feasible_fraction(live);
    outcome
}

#[cfg(test)]
#[path = "lshade_de_tests.rs"]
mod lshade_de_tests;
