// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The reproducible and the interactive default searches (audit SIZ-06,
//! SIZ-07, decision D6).

use super::{
    default_constraint_tolerance, default_convergence_stagnation_generations,
    default_finite_difference_step, default_optimizer_method, default_refinement_budget,
    default_screening_budget, SolverSettings, StageBudget,
};

/// The default run seed (audit SIZ-06, decision D6): the date the
/// validation matrix and the earlier measured runs used, so a run that names
/// no seed is reproducible.
pub const DEFAULT_SEED: i64 = 20_260_922;

/// Default analysed-candidate budget of each stage of a reproducible run
/// (engineering choice): the median of the 260-1000 evaluations the two
/// default time limits afforded on the registered jets (590 measured on the
/// A320), so the default effort matches what the time-limited default did on
/// a typical aircraft while the stopping point no longer depends on the
/// machine.
pub const DEFAULT_REPRODUCIBLE_EVALUATIONS: i64 = 600;

impl Default for SolverSettings {
    /// The reproducible search: [`DEFAULT_SEED`], evaluation budgets of
    /// [`DEFAULT_REPRODUCIBLE_EVALUATIONS`] per stage as the only stop
    /// (time limits ignored), so the same configuration gives the same
    /// design at any worker count and on any machine. Presets, the command
    /// line and the validation matrix start here; [`Self::interactive`] is
    /// the wall-clock-limited alternative.
    fn default() -> Self {
        Self {
            method: default_optimizer_method(),
            finite_difference_step: default_finite_difference_step(),
            constraint_tolerance: default_constraint_tolerance(),
            screening: reproducible_budget(30.0),
            refinement: reproducible_budget(120.0),
            parameter_adaptation: false,
            tolerance: 0.02,
            convergence_stagnation_generations: default_convergence_stagnation_generations(),
            seed: default_seed(),
            workers: 0,
            stop_on_evaluations_only: default_stop_on_evaluations_only(),
        }
    }
}

impl SolverSettings {
    /// The wall-clock-limited search the desktop application starts from:
    /// 30 s of screening and 120 s of refinement under a 20 000-evaluation
    /// ceiling, a different clock seed each run. The stopping point depends
    /// on machine speed and worker count; the run records the seed and
    /// replay counts needed to reproduce it.
    #[must_use]
    pub fn interactive() -> Self {
        Self {
            screening: default_screening_budget(),
            refinement: default_refinement_budget(),
            seed: None,
            stop_on_evaluations_only: false,
            ..Self::default()
        }
    }
}

pub(super) fn reproducible_budget(time_limit_s: f64) -> StageBudget {
    StageBudget {
        max_evaluations: DEFAULT_REPRODUCIBLE_EVALUATIONS,
        time_limit_s,
        replay_evaluations: None,
        replay_planned_evaluations: None,
        replay_restoration_evaluations: None,
        max_pregate_rejects: None,
    }
}

pub(super) fn default_seed() -> Option<i64> {
    Some(DEFAULT_SEED)
}

pub(super) fn default_stop_on_evaluations_only() -> bool {
    true
}

// Serialized only when switched off, so a saved default configuration keeps
// the historical key set.
pub(super) fn is_default_stop_on_evaluations_only(value: &bool) -> bool {
    *value == default_stop_on_evaluations_only()
}
