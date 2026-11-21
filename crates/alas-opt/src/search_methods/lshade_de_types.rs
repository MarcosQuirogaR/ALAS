// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The settings, stop reasons and outcome of one L-SHADE kernel run.

use std::time::Duration;

use super::ScoredPoint;

/// Resolved settings for one refinement run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Settings {
    /// Hard evaluation ceiling after reserving reporting work.
    pub(crate) max_evaluations: usize,
    /// Estimated affordable evaluations for population and epsilon schedules.
    pub(crate) schedule_evaluations: usize,
    /// Evaluations after which the run stops; `max_evaluations` unless a
    /// time-limited run is being replayed.
    pub(crate) stop_after: usize,
    /// Initial population `N_init`.
    pub(crate) population: usize,
    /// Floor of linear population reduction, capped by the initial size.
    pub(crate) minimum_population: usize,
    /// Whole-batch quantum of the population schedule: the reduced size is
    /// rounded to a multiple of it (never below the floor), so every
    /// generation dispatches whole evaluator waves. `1` keeps the plain
    /// linear schedule.
    pub(crate) population_quantum: usize,
    pub(crate) seed: u64,
    /// Success-history adaptation of `F` and `CR` instead of the static pair.
    pub(crate) adaptation: bool,
    /// Normalized design-space spread below which a stagnated population
    /// counts as converged.
    pub(crate) spread_tolerance: f64,
    /// Generations without an improvement above [`super::IMPROVEMENT_TOLERANCE`]
    /// before the run stops as stagnated or converged; the effective window
    /// and the budget share spent first are [`ops::stagnation_stops`].
    pub(crate) stagnation_generations: usize,
    /// Wall-clock limit of the generation guard; a deadline-aware evaluator
    /// also checks between individual candidate dispatches.
    pub(crate) time_limit: Option<Duration>,
    /// The evaluator stops its queue at the deadline, so the generation
    /// guard need not reserve an entire last batch's projected wall time.
    pub(crate) intra_batch_deadline: bool,
    /// Evaluations kept back for feasibility restoration while no feasible
    /// design has been found.
    pub(crate) infeasible_reserve: usize,
    /// Pre-gate rejections after which no further generation starts.
    pub(crate) max_rejects: usize,
}

/// Why the run stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Termination {
    /// Stagnated with the population collapsed below the spread tolerance.
    Converged,
    /// The feasible best stopped improving; the population is still spread.
    Stagnated,
    EvaluationBudget,
    TimeBudget,
    /// The design-vector pre-gate rejected as many candidates as the stage
    /// allows.
    PregateExhausted,
    Cancelled,
}

impl Termination {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Converged => "converged",
            Self::Stagnated => "stagnated",
            Self::EvaluationBudget => "evaluation_budget",
            Self::TimeBudget => "time_budget",
            Self::PregateExhausted => "pregate_exhausted",
            Self::Cancelled => crate::differential_evolution::CANCELLED,
        }
    }
}

/// What one run measured about itself.
#[derive(Debug, Clone)]
pub(crate) struct Outcome {
    pub(crate) winner: ScoredPoint,
    /// The first initial member's score: the caller's baseline when it
    /// passes one first.
    pub(crate) first_scored: Option<ScoredPoint>,
    pub(crate) termination: Termination,
    pub(crate) generations_completed: usize,
    /// Scores that passed the design-vector pre-gate, initial population
    /// included: the count the budget bounds.
    pub(crate) evaluations: usize,
    /// Scores the pre-gate rejected ([`super::Tier::PreGateFailed`]).
    pub(crate) rejected: usize,
    pub(crate) population_initial: usize,
    pub(crate) population_final: usize,
    pub(crate) epsilon_final: f64,
    /// Fraction of the final population that was strictly feasible.
    pub(crate) feasible_fraction: f64,
    /// Objective of the first feasible candidate, in evaluation order.
    pub(crate) first_feasible_cost: Option<f64>,
    /// Best feasible cost after the initial population and after every
    /// completed generation; `None` while nothing feasible was found.
    pub(crate) best_by_generation: Vec<Option<f64>>,
    /// Normalized design-space spread of the final population
    /// (`ops::normalized_spread`): `0` collapsed, `1` the whole box.
    pub(crate) spread_final: f64,
}
