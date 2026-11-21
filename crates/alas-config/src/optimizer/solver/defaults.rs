// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The reproducible and the interactive default searches.

use super::{
    default_constraint_tolerance, default_convergence_stagnation_generations,
    default_finite_difference_step, default_optimizer_method, default_refinement_budget,
    default_screening_budget, SolverSettings, StageBudget,
};

/// The default run seed: the date the
/// validation matrix and the earlier measured runs used, so a run that names
/// no seed is reproducible.
pub const DEFAULT_SEED: i64 = 20_260_922;

/// Default candidate plan of the exploration (screening) stage: analysed
/// candidates on the coarse exploration model, a scan of 60 and three
/// search rounds of 108.
///
/// Sized, with [`DEFAULT_REFINEMENT_EVALUATIONS`], for a run of about five
/// minutes on a 16-thread desktop (engineering choice from measurement,
/// Ryzen 7 5800X, 15 workers: an exploration analysis takes 4.3
/// to 4.9 s of one lane on the LJ60, the slowest registered class, and the
/// 576-candidate plan took its search 324 s there and the A320 222 s). The
/// A320's in-loop block-fuel gain was -1.14 % after 384 candidates and
/// -1.19 % after 576, inside the run-to-run spread of the gain, so the
/// shorter plan is the default. The plan, not a clock, ends the search, so
/// the same configuration examines the same candidates on any machine.
pub const DEFAULT_REPRODUCIBLE_EVALUATIONS: i64 = 384;

/// Default candidate plan of the refinement stage: analyses on the
/// refinement model. It covers the three margin calibrations of the
/// exploration (15 each), the promoted initial population (30), the
/// refinement generations, the neighbourhood-support poll (8) and the
/// reserve for the reporting-fidelity verification (10).
pub const DEFAULT_REFINEMENT_EVALUATIONS: i64 = 120;

/// The clean-sheet exploration plan: a scan of 60 and two search rounds of
/// 108 in place of three.
///
/// A clean sheet sizes its fuselage from the cabin and its engine from the
/// thrust requirements on every refinement analysis, which the reference
/// modes do not: measured on the A320-200 clean sheet (60
/// recorded designs, Ryzen 7 5800X, two workers beside six busy threads),
/// an exploration analysis took 7.6 s of one lane and a refinement analysis
/// with the engine held at the nominal's rating 26.4 s (holding the
/// fuselage too halved it but moved the tip-back angle by up to 1.9 deg and
/// flipped 9 of 60 verdicts, so the refinement keeps sizing it). The
/// default plan would take 384 x 7.6 / 15 + 120 x 26.4 / 15 = 406 s of
/// search on 15 workers. The refinement now sizes the engine as well, and a
/// quiet 15-worker run of the plan with three calibrations spent 480 s in
/// its two search stages (A320-200), most of it in the
/// calibration and refinement waves; the plan therefore runs two
/// calibrations (the exploration's `CLEAN_SHEET_ROUNDS`) and keeps the
/// population (engineering choice from measurement).
pub const CLEAN_SHEET_REPRODUCIBLE_EVALUATIONS: i64 = 276;

/// The clean-sheet refinement plan: the two exploration calibrations
/// (15 each), the promoted population (30), the neighbourhood-support poll
/// (8) and the verification reserve (10), with no separate generations; see
/// [`CLEAN_SHEET_REPRODUCIBLE_EVALUATIONS`].
pub const CLEAN_SHEET_REFINEMENT_EVALUATIONS: i64 = 78;

impl SolverSettings {
    /// The plan a clean sheet runs: the clean-sheet counts
    /// ([`CLEAN_SHEET_REPRODUCIBLE_EVALUATIONS`],
    /// [`CLEAN_SHEET_REFINEMENT_EVALUATIONS`]) when the settings carry the
    /// default reproducible plan, the settings unchanged otherwise (a plan
    /// the user chose, or a clock-limited search, is kept).
    #[must_use]
    pub fn for_clean_sheet(mut self) -> Self {
        let default_plan = self.stop_on_evaluations_only
            && self.screening.max_evaluations == DEFAULT_REPRODUCIBLE_EVALUATIONS
            && self.refinement.max_evaluations == DEFAULT_REFINEMENT_EVALUATIONS;
        if default_plan {
            self.screening.max_evaluations = CLEAN_SHEET_REPRODUCIBLE_EVALUATIONS;
            self.refinement.max_evaluations = CLEAN_SHEET_REFINEMENT_EVALUATIONS;
        }
        self
    }
}

impl Default for SolverSettings {
    /// The reproducible search: [`DEFAULT_SEED`] and the candidate plan
    /// ([`DEFAULT_REPRODUCIBLE_EVALUATIONS`] exploration candidates,
    /// [`DEFAULT_REFINEMENT_EVALUATIONS`] refinement analyses) as the only
    /// stop (time limits ignored), so the same configuration gives the same
    /// design at any worker count and on any machine. Presets, the command
    /// line and the validation matrix start here; [`Self::interactive`] is
    /// the wall-clock-limited alternative.
    fn default() -> Self {
        Self {
            method: default_optimizer_method(),
            finite_difference_step: default_finite_difference_step(),
            constraint_tolerance: default_constraint_tolerance(),
            screening: reproducible_budget(DEFAULT_REPRODUCIBLE_EVALUATIONS, 30.0),
            refinement: reproducible_budget(DEFAULT_REFINEMENT_EVALUATIONS, 120.0),
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

pub(super) fn reproducible_budget(max_evaluations: i64, time_limit_s: f64) -> StageBudget {
    StageBudget {
        max_evaluations,
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
