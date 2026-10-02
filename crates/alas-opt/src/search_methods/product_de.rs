// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Resolves `optimizer.solver` into the refinement kernel's settings.
//!
//! `optimizer.solver.method` is validated at the configuration boundary
//! (`alas_config::SolverSettings::is_supported_method`); by the time a method
//! string reaches this module it names the one kernel this build runs.
//!
//! Design-vector units, frames, signs and validity domains are specified in
//! `docs/optimizer-design-vector.md`; this module treats the vector as opaque
//! coordinates inside the bounds and never interprets a component.

use std::time::Duration;

use alas_config::{SolverSettings, StageBudget};

pub(crate) use super::lshade_de::{run, Outcome, Settings, Termination};
use super::{ScoredPoint, Tier};

/// The `method` string every product optimization result reports.
pub(crate) const METHOD: &str = "differential_evolution";
/// The `strategy` string every product optimization result reports.
pub(crate) const STRATEGY: &str = "current_to_pbest_1_bin";

/// Lower clamp of the initial population. Below about two dozen members a
/// 13-variable population cannot hold the elite, the baseline and a
/// space-filling top-up at once (engineering estimate).
const MINIMUM_INITIAL_POPULATION: usize = 24;
/// Upper clamp of the initial population as a multiple of the free
/// dimension; larger populations leave too few generations at the budgets
/// this product affords (Piotrowski, "Review of Differential Evolution
/// population size," Swarm Evol. Comput. 32, 2017, DOI
/// 10.1016/j.swevo.2016.05.003: fewer evaluations favour smaller populations).
const MAXIMUM_POPULATION_PER_DIMENSION: usize = 6;
/// Evaluations per initial member: `N_init = B / 10` inside the clamps.
const EVALUATIONS_PER_INITIAL_MEMBER: usize = 10;
/// Share of the refinement budget kept for feasibility restoration while no
/// feasible design has been found.
pub(crate) const RESTORATION_BUDGET_FRACTION: f64 = 0.1;

/// `N_init = clamp(B / 10, 24, 6 D)`, never more than `B`: a function of the
/// budget and the free dimension only, never of the worker count.
pub(crate) fn initial_population(budget: usize, free_dimension: usize) -> usize {
    let upper = (MAXIMUM_POPULATION_PER_DIMENSION * free_dimension).max(MINIMUM_INITIAL_POPULATION);
    (budget / EVALUATIONS_PER_INITIAL_MEMBER)
        .clamp(MINIMUM_INITIAL_POPULATION, upper)
        .min(budget)
}

/// Candidates the reporting-fidelity ladder after the search may verify,
/// the search's own finalist included.
///
/// One verification is a reported analysis plus a flown mission, about one
/// to three seconds on the registered presets. Eight clears a finalist
/// rejected by a mesh-error-sized attitude shift and keeps the reserve small
/// (engineering choice).
pub const MAX_VERIFIED_CANDIDATES: usize = 8;

/// Refinement evaluations reserved for the reporting-fidelity work after the
/// search: the verification ladder, the baseline analysis and the final
/// analysis, each counted as one evaluation.
pub const VERIFICATION_RESERVE_EVALUATIONS: usize = MAX_VERIFIED_CANDIDATES + 2;

/// Share of the refinement time limit reserved for the same work.
/// Engineering estimate: ten reporting-fidelity analyses at one to three
/// seconds each against the 120 s default limit.
pub const VERIFICATION_RESERVE_TIME_FRACTION: f64 = 0.2;

/// What the refinement budget keeps back for the reporting-fidelity work
/// after the search, which therefore runs inside the refinement's declared
/// evaluation budget and time limit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VerificationReserve {
    /// Evaluations reserved: [`VERIFICATION_RESERVE_EVALUATIONS`], at most a
    /// tenth of the refinement budget. The finalist, the baseline and the
    /// final analysis always run, so a reserve below three is overrun and the
    /// run reports by how much.
    pub evaluations: usize,
    /// Seconds reserved, `None` when the run ignores its time limits.
    pub time_s: Option<f64>,
}

/// The reserve `solver` keeps back from its refinement stage.
#[must_use]
pub fn verification_reserve(solver: &SolverSettings) -> VerificationReserve {
    let (budget, _, limit) = stage_limits(&solver.refinement, solver.stop_on_evaluations_only);
    VerificationReserve {
        evaluations: VERIFICATION_RESERVE_EVALUATIONS.min(budget / 10),
        time_s: limit.map(|limit| limit.as_secs_f64() * VERIFICATION_RESERVE_TIME_FRACTION),
    }
}

/// The pre-gate rejection cap of one stage
/// ([`StageBudget::resolved_max_pregate_rejects`]).
pub(crate) fn reject_cap(stage: &StageBudget) -> usize {
    usize::try_from(stage.resolved_max_pregate_rejects().max(1)).unwrap_or(usize::MAX)
}

/// The analysed-candidate budget, the replay stop and the time limit of one
/// stage.
///
/// The time limit is `None` when the run stops on evaluation budgets only
/// or the stage replays a recorded count: a replay must reach exactly that
/// count whatever the machine, or it would not reproduce the run.
pub(crate) fn stage_limits(
    stage: &StageBudget,
    evaluations_only: bool,
) -> (usize, usize, Option<Duration>) {
    let budget = usize::try_from(stage.max_evaluations.max(1)).unwrap_or(usize::MAX);
    let replay = stage
        .replay_evaluations
        .and_then(|count| usize::try_from(count).ok());
    let stop_after = replay.map_or(budget, |count| count.clamp(1, budget));
    let limit = (!evaluations_only && replay.is_none())
        .then(|| Duration::from_secs_f64(stage.time_limit_s.clamp(0.0, 1.0e6)));
    (budget, stop_after, limit)
}

/// Safety factor on the measured throughput when the refinement budget is
/// planned from its time limit. Engineering estimate: refinement
/// generations shrink to eight trials and keep fewer lanes busy than a
/// screening batch (lane utilization measured 0.55 to 0.68 against 0.77 to
/// 0.91 on a 32-thread machine), and the time guard stops the run anyway, so
/// the plan only has to be close.
pub const THROUGHPUT_SAFETY_FACTOR: f64 = 0.9;

/// Smallest planned refinement budget: ten evaluations per member of the
/// smallest initial population, below which the population schedule has no
/// room to shrink (engineering choice).
pub const MIN_PLANNED_EVALUATIONS: usize =
    EVALUATIONS_PER_INITIAL_MEMBER * MINIMUM_INITIAL_POPULATION;

/// The refinement budget `B` the run plans its population schedule on,
/// verification reserve included, so it is the value
/// `optimizer.solver.refinement.replay_planned_evaluations` takes to replay
/// the run.
///
/// In time mode it is what the refinement's search time affords at the
/// measured throughput `rate` (analyses per second; the product search
/// measures it on the refinement model, see `run_product_search`):
/// `floor(rate (1 - f) T eta) + R`, with `f` the
/// [`VERIFICATION_RESERVE_TIME_FRACTION`], `T` the time limit, `eta` the
/// [`THROUGHPUT_SAFETY_FACTOR`] and `R` the reserved evaluations, at least
/// [`MIN_PLANNED_EVALUATIONS`] and at most the configured budget. Without a
/// time limit (evaluations-only mode or a replay) or without a measured
/// rate it is the configured budget.
///
/// A recorded `replay_planned_evaluations` is the plan, at most the ceiling.
#[must_use]
pub fn planned_refinement_budget(solver: &SolverSettings, rate: Option<f64>) -> usize {
    let (budget, _, limit) = stage_limits(&solver.refinement, solver.stop_on_evaluations_only);
    if let Some(recorded) = solver.refinement.replay_planned_evaluations {
        return usize::try_from(recorded.max(1)).map_or(budget, |planned| planned.min(budget));
    }
    let (Some(limit), Some(rate)) = (limit, rate.filter(|rate| rate.is_finite() && *rate > 0.0))
    else {
        return budget;
    };
    let search_s = limit.as_secs_f64() * (1.0 - VERIFICATION_RESERVE_TIME_FRACTION);
    let affordable = (rate * search_s * THROUGHPUT_SAFETY_FACTOR).floor();
    let planned = if affordable < budget as f64 {
        affordable as usize + VERIFICATION_RESERVE_EVALUATIONS
    } else {
        budget
    };
    planned.max(MIN_PLANNED_EVALUATIONS).min(budget)
}

/// The refinement kernel's settings for `free_dimension` free variables on
/// the `planned` budget ([`planned_refinement_budget`]): exactly the
/// settings of the same run with the configured budget set to `planned`,
/// less the [`verification_reserve`], the stage time limit kept.
pub(crate) fn refinement_settings(
    solver: &SolverSettings,
    free_dimension: usize,
    seed: u64,
    planned: usize,
) -> Settings {
    let mut solver = solver.clone();
    solver.refinement.max_evaluations = i64::try_from(planned.max(1)).unwrap_or(i64::MAX);
    let (budget, stop_after, limit) =
        stage_limits(&solver.refinement, solver.stop_on_evaluations_only);
    let reserve = verification_reserve(&solver);
    let budget = budget - reserve.evaluations;
    // A replay stops the kernel where the recorded kernel stopped; the
    // recorded restoration share replays after it (`restoration_replay`).
    let stop_after = stop_after.saturating_sub(restoration_replay(&solver.refinement).unwrap_or(0));
    Settings {
        max_evaluations: budget,
        stop_after: stop_after.min(budget),
        population: initial_population(budget, free_dimension),
        seed,
        adaptation: solver.parameter_adaptation,
        spread_tolerance: solver.tolerance.max(0.0),
        stagnation_generations: usize::try_from(solver.convergence_stagnation_generations.max(1))
            .unwrap_or(1),
        time_limit: limit.map(|limit| limit.mul_f64(1.0 - VERIFICATION_RESERVE_TIME_FRACTION)),
        infeasible_reserve: (budget as f64 * RESTORATION_BUDGET_FRACTION).ceil() as usize,
        max_rejects: reject_cap(&solver.refinement),
    }
}

/// The restoration evaluations a replayed refinement runs, `None` outside a
/// replay: the recorded `replay_restoration_evaluations`, zero when unset.
/// The kernel replays `replay_evaluations` less this count, so each phase
/// stops exactly where the recorded run's did; replaying the combined count
/// through the kernel alone would let it generate trials in place of the
/// recorded restoration polls.
pub(crate) fn restoration_replay(stage: &StageBudget) -> Option<usize> {
    stage.replay_evaluations?;
    Some(
        stage
            .replay_restoration_evaluations
            .and_then(|count| usize::try_from(count).ok())
            .unwrap_or(0),
    )
}

/// The score of a candidate whose evaluation produced nothing: the worst tier
/// with an infinite violation, so last on every
/// ordering key and never displaces an analysed or pre-gated candidate.
pub(crate) fn unevaluated(values: &[f64]) -> ScoredPoint {
    ScoredPoint {
        values: values.to_vec(),
        cost: f64::INFINITY,
        tier: Tier::PreGateFailed,
        constraint_violation: f64::INFINITY,
        objectives: [f64::INFINITY; 3],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_initial_population_follows_the_budget_and_never_the_workers() {
        // B / 10 inside [24, 6 D], and never above B itself.
        assert_eq!(initial_population(600, 13), 60);
        assert_eq!(initial_population(100, 13), 24);
        assert_eq!(initial_population(5_000, 13), 78);
        assert_eq!(initial_population(10, 13), 10);
        assert_eq!(initial_population(600, 2), 24);
        let mut solver = SolverSettings::default();
        solver.refinement.max_evaluations = 400;
        let serial = configured(
            &SolverSettings {
                workers: 1,
                ..solver.clone()
            },
            13,
            7,
        );
        let wide = configured(
            &SolverSettings {
                workers: 32,
                ..solver
            },
            13,
            7,
        );
        assert_eq!(serial, wide);
        let searched = 400 - VERIFICATION_RESERVE_EVALUATIONS;
        assert_eq!(serial.max_evaluations, searched);
        assert_eq!(serial.population, initial_population(searched, 13));
    }

    #[test]
    fn the_verification_reserve_sits_inside_the_refinement_budget_and_limit() {
        let solver = SolverSettings::default();
        let settings = configured(&solver, 13, 1);
        let reserve = verification_reserve(&solver);
        let budget = usize::try_from(solver.refinement.max_evaluations).unwrap_or(0);
        assert_eq!(settings.max_evaluations + reserve.evaluations, budget);
        let limit = settings.time_limit.map(|limit| limit.as_secs_f64());
        let total = limit
            .zip(reserve.time_s)
            .map(|(search, kept)| search + kept);
        assert!(total.is_some_and(|total| (total - solver.refinement.time_limit_s).abs() < 1e-9));
        // A tiny budget never loses more than a tenth of itself.
        let mut tiny = solver.clone();
        tiny.refinement.max_evaluations = 16;
        assert_eq!(verification_reserve(&tiny).evaluations, 1);
    }

    #[test]
    fn a_replay_or_an_evaluations_only_run_ignores_the_time_limits() {
        let mut solver = SolverSettings::default();
        assert!(configured(&solver, 13, 1).time_limit.is_some());
        solver.stop_on_evaluations_only = true;
        assert_eq!(configured(&solver, 13, 1).time_limit, None);
        assert_eq!(verification_reserve(&solver).time_s, None);
        assert_eq!(stage_limits(&solver.screening, true).2, None);
        solver.stop_on_evaluations_only = false;
        solver.screening.replay_evaluations = Some(70);
        assert_eq!(stage_limits(&solver.screening, false).2, None);
    }

    #[test]
    fn a_replay_count_stops_early_without_changing_the_schedule() {
        let mut solver = SolverSettings::default();
        solver.refinement.max_evaluations = 300;
        let normal = configured(&solver, 13, 1);
        solver.refinement.replay_evaluations = Some(137);
        let replay = configured(&solver, 13, 1);
        assert_eq!(replay.stop_after, 137);
        assert_eq!(replay.max_evaluations, normal.max_evaluations);
        assert_eq!(replay.population, normal.population);
    }

    /// The settings on the configured budget, as with no measured rate.
    fn configured(solver: &SolverSettings, free_dimension: usize, seed: u64) -> Settings {
        let planned = planned_refinement_budget(solver, None);
        refinement_settings(solver, free_dimension, seed, planned)
    }

    #[test]
    fn the_planned_budget_follows_the_measured_rate_inside_its_clamps() {
        let mut solver = SolverSettings::default();
        solver.refinement.max_evaluations = 20_000;
        solver.refinement.time_limit_s = 120.0;
        let search_s = 120.0 * (1.0 - VERIFICATION_RESERVE_TIME_FRACTION);
        let mut previous = 0;
        for rate in [3.0, 5.5, 8.0, 12.0] {
            let planned = planned_refinement_budget(&solver, Some(rate));
            let searched = (planned - VERIFICATION_RESERVE_EVALUATIONS) as f64;
            // What the search time affords at the rate, less the safety factor.
            assert!((searched - rate * search_s * THROUGHPUT_SAFETY_FACTOR).abs() < 1.0);
            assert!(planned > previous);
            previous = planned;
            // The plan sets the schedule exactly as the same configured budget.
            let settings = refinement_settings(&solver, 13, 1, planned);
            let mut replay = solver.clone();
            replay.refinement.replay_planned_evaluations = Some(planned as i64);
            replay.refinement.replay_evaluations = Some(100);
            let replayed = configured(&replay, 13, 1);
            assert_eq!(settings.max_evaluations, replayed.max_evaluations);
            assert_eq!(settings.population, replayed.population);
            assert_eq!(settings.infeasible_reserve, replayed.infeasible_reserve);
            assert_eq!(settings.max_rejects, replayed.max_rejects);
            assert!(settings.time_limit.is_some() && replayed.time_limit.is_none());
        }
        // A doubled time limit plans about twice the budget.
        let short = planned_refinement_budget(&solver, Some(8.0));
        solver.refinement.time_limit_s = 240.0;
        let long = planned_refinement_budget(&solver, Some(8.0));
        let ratio = (long - VERIFICATION_RESERVE_EVALUATIONS) as f64
            / (short - VERIFICATION_RESERVE_EVALUATIONS) as f64;
        assert!((ratio - 2.0).abs() < 0.01, "{ratio}");
        // Clamps: the configured ceiling, the floor, no rate, no time limit.
        assert_eq!(planned_refinement_budget(&solver, Some(1.0e6)), 20_000);
        assert_eq!(
            planned_refinement_budget(&solver, Some(1.0e-3)),
            MIN_PLANNED_EVALUATIONS
        );
        for rate in [None, Some(0.0), Some(f64::NAN), Some(f64::INFINITY)] {
            assert_eq!(planned_refinement_budget(&solver, rate), 20_000);
        }
        solver.stop_on_evaluations_only = true;
        assert_eq!(planned_refinement_budget(&solver, Some(8.0)), 20_000);
        solver.stop_on_evaluations_only = false;
        solver.refinement.replay_evaluations = Some(500);
        assert_eq!(planned_refinement_budget(&solver, Some(8.0)), 20_000);
        // A recorded plan is the plan, whatever the rate, within the ceiling.
        solver.refinement.replay_planned_evaluations = Some(915);
        assert_eq!(planned_refinement_budget(&solver, Some(8.0)), 915);
        assert_eq!(planned_refinement_budget(&solver, None), 915);
        solver.refinement.replay_evaluations = None;
        assert_eq!(planned_refinement_budget(&solver, Some(8.0)), 915);
        solver.refinement.replay_planned_evaluations = None;
        solver.refinement.max_evaluations = 100;
        assert_eq!(planned_refinement_budget(&solver, Some(1.0e-3)), 100);
    }
}
