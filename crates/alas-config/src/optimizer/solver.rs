// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/optimizer_config.py (`SolverSettings`)

//! How the aircraft-design search is run: which optimization method, how
//! long, how wide, and from where.
//!
//! These settings decide how many aircraft get built and analysed, and each
//! evaluation is a full geometry build, mass breakdown and vortex-lattice
//! solve. The one search is the mission-sized differential evolution (L-SHADE
//! under epsilon constraints).
//!
//! # Stopping rules
//!
//! Each stage stops on its evaluation budget or its wall-clock limit,
//! whichever comes first; the limit is checked only at generation (batch)
//! boundaries, the first right after the initial population. A time-limited
//! stage's stopping point depends on machine speed and worker count. With a
//! time limit the refinement plans its evaluation budget, which sets its
//! population schedule, from its measured throughput, at most
//! [`StageBudget::max_evaluations`]. The result records each stage's replay
//! count (pre-gate-passed candidates, including repeats) and the
//! refinement's planned budget; replaying with those counts
//! ([`StageBudget::replay_evaluations`]) and the planned budget
//! ([`StageBudget::replay_planned_evaluations`]), or running with
//! [`SolverSettings::stop_on_evaluations_only`], gives a bit-identical result
//! at any worker count.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Settings for the differential-evolution solver.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct SolverSettings {
    /// The optimization method, always [`PRODUCT_DE_METHOD`].
    ///
    /// Kept in the file so a saved configuration names its method; a token an
    /// earlier build accepted is migrated at load time (see
    /// [`LEGACY_METHOD_TOKENS`]).
    #[serde(default = "default_optimizer_method")]
    #[config(
        options = OptimizerMethod,
        label = "Optimization profile",
        help = "The search that runs: mission-sized differential evolution (L-SHADE under epsilon constraints), seeded by a screening sample of the design box, with bounded feasibility restoration."
    )]
    pub method: String,

    /// Finite-difference step for the retired gradient-based driver.
    ///
    /// No kernel reads this any more; it is retained only so a file saved by
    /// an earlier build that carried an explicit value still loads without
    /// losing it.
    #[serde(
        default = "default_finite_difference_step",
        skip_serializing_if = "is_default_finite_difference_step"
    )]
    #[config(
        label = "Finite-difference step (retired)",
        help = "Unused: the SQP driver this setting configured has been removed. Retained so a configuration saved by an earlier build still loads with its value intact."
    )]
    pub finite_difference_step: f64,

    /// Normalized constraint violation accepted as feasible by the retired
    /// gradient-based driver.
    ///
    /// No kernel reads this any more; see [`Self::finite_difference_step`].
    #[serde(
        default = "default_constraint_tolerance",
        skip_serializing_if = "is_default_constraint_tolerance"
    )]
    #[config(
        label = "Constraint tolerance (retired)",
        help = "Unused: the SQP driver this setting configured has been removed. Retained so a configuration saved by an earlier build still loads with its value intact."
    )]
    pub constraint_tolerance: f64,

    /// Budget of the screening stage: a space-filling sample of the design
    /// box evaluated with the screening model, from which the refinement's
    /// elite is drawn.
    #[serde(default = "default_screening_budget")]
    #[config(
        nested,
        label = "Screening stage",
        help = "Budget of the screening stage, which evaluates a space-filling sample of the design box plus the baseline with the screening model (the same physics as the full evaluation) and keeps a diverse elite to seed the refinement."
    )]
    pub screening: StageBudget,

    /// Budget of the refinement stage: differential evolution at full
    /// in-loop fidelity, seeded with the screening elite and the baseline.
    #[serde(default = "default_refinement_budget")]
    #[config(
        nested,
        label = "Refinement stage",
        help = "Budget of the refinement stage, which runs differential evolution at full in-loop fidelity from the screening elite and the baseline. Its evaluation budget also sets the initial population and its reduction schedule. A reserved share of its evaluations and of its time limit pays for the reporting-fidelity verification of the finalists, the baseline analysis and the final analysis."
    )]
    pub refinement: StageBudget,

    /// Success-history adaptation of the mutation factor and crossover rate.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[config(
        label = "Adapt mutation and crossover (L-SHADE)",
        help = "Off: static F = 0.5 and CR = 0.9, which outperformed parameter adaptation below 800 evaluations per design variable in a published benchmark (Tanabe and Fukunaga 2020). On: L-SHADE success-history adaptation, recommended only for refinement budgets above 800 evaluations per free design variable."
    )]
    pub parameter_adaptation: bool,

    /// Normalized design-space spread below which a stagnated refinement
    /// population counts as converged.
    #[config(
        label = "Convergence spread tolerance",
        help = "Normalized design-space spread (mean population range over bound width) below which a refinement that has stopped improving is reported as converged rather than stagnated."
    )]
    pub tolerance: f64,

    /// Generations without a relative improvement of the feasible best above
    /// 1e-4 before the refinement stops as converged or stagnated.
    #[serde(
        default = "default_convergence_stagnation_generations",
        skip_serializing_if = "is_default_convergence_stagnation_generations"
    )]
    #[config(
        label = "Stagnation window",
        min = 1,
        help = "Refinement generations in which the best feasible objective improves by less than 1e-4 (relative) before the search stops as converged or stagnated. The window used is at least 2 N_init / 8 generations (two initial populations of trials at the final population size of 8), and stagnation never stops a run before half its planned budget is spent."
    )]
    pub convergence_stagnation_generations: i64,

    /// The random seed, or unset for a different search each run.
    #[config(
        label = "Random seed",
        help = "Set an integer for a reproducible run (same seed -> same result); leave blank for a different search each run."
    )]
    pub seed: Option<i64>,

    /// Native compute threads a candidate batch is spread across.
    ///
    /// `0` means every thread the machine reports. The worker count changes
    /// how a batch is distributed, never what a design scores; it changes
    /// which designs are evaluated only through a stage that stops on its
    /// time limit. A positive value is used exactly as given.
    #[config(
        label = "Native compute worker threads",
        help = "Threads a generation's candidates are spread across; 0 uses every thread the machine reports. Each candidate runs on one thread. The product search evaluates deterministic whole-generation batches: with evaluation-budget stops or replay counts the result is bit-identical at any worker count. Time-limited: the stopping point depends on machine speed and worker count; replay with the recorded evaluation counts for a bit-identical result at any worker count. External evaluator adapters remain serial because they own mutable process/session state."
    )]
    pub workers: i64,

    /// Ignore the stage time limits and stop every stage on its evaluation
    /// budget, so a seeded run is bit-identical at any worker count.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[config(
        label = "Stop on evaluation budgets only",
        help = "Off: each stage also stops at its time limit. Time-limited: the stopping point depends on machine speed and worker count; replay with the recorded evaluation counts for a bit-identical result at any worker count. On: the time limits are ignored and each stage runs to its evaluation budget, so a seeded run is bit-identical at any worker count however long it takes."
    )]
    pub stop_on_evaluations_only: bool,
}

impl Default for SolverSettings {
    fn default() -> Self {
        Self {
            method: default_optimizer_method(),
            finite_difference_step: default_finite_difference_step(),
            constraint_tolerance: default_constraint_tolerance(),
            screening: default_screening_budget(),
            refinement: default_refinement_budget(),
            parameter_adaptation: false,
            tolerance: 0.02,
            convergence_stagnation_generations: default_convergence_stagnation_generations(),
            seed: None,
            workers: 0,
            stop_on_evaluations_only: false,
        }
    }
}

/// The largest wall-clock limit either search stage accepts, s.
pub const MAXIMUM_STAGE_TIME_LIMIT_S: f64 = 300.0;

/// Evaluation and wall-clock budget of one search stage.
///
/// The evaluation budget counts analysed candidates: those that passed the
/// design-vector pre-gate. Pre-gate rejections cost microseconds and are
/// capped separately ([`Self::max_pregate_rejects`]); a stage that reaches
/// that cap stops as `pregate_exhausted`. The evaluation budget is the
/// reproducible bound: a run that stops on it replays bit-identically from
/// its seed at any worker count. The wall-clock limit is checked only at
/// generation boundaries, the first right after the initial population, so a
/// generation in flight always completes. A time-limited stop depends on
/// machine speed and worker count; the run records the stage's replay count
/// (pre-gate-passed candidates, including repeats) and, for the refinement,
/// the budget it planned from its measured throughput. Setting
/// [`Self::replay_evaluations`] to that count, and the refinement's
/// [`Self::replay_planned_evaluations`] to the planned budget and
/// [`Self::replay_restoration_evaluations`] to its restoration count, replays the run
/// bit-identically at any worker count, with the time limit ignored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct StageBudget {
    /// Largest number of analysed candidates: those that passed the
    /// design-vector pre-gate. Rejections do not count here.
    #[config(
        label = "Evaluation budget",
        min = 1,
        help = "Largest number of candidates this stage analyses. Candidates the design-vector pre-gate rejects in microseconds do not count here; they have their own cap. With a time limit the refinement plans its budget from its measured throughput, at most this ceiling; a run that stops on evaluations only replays exactly from its seed."
    )]
    pub max_evaluations: i64,

    /// Wall-clock limit in seconds, checked between generations.
    #[config(
        label = "Time limit",
        unit = "s",
        min = 1.0,
        max = 300.0,
        help = "Wall-clock limit of this stage, at most 300 s. It is checked only at generation boundaries, the first right after the initial population, so a generation in flight always finishes. Time-limited: the stopping point depends on machine speed and worker count; replay with the recorded evaluation counts for a bit-identical result at any worker count."
    )]
    pub time_limit_s: f64,

    /// Analysed candidates after which the stage stops without changing its
    /// schedule, to replay a run the time limit stopped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        label = "Replay evaluation count",
        min = 1,
        help = "Leave blank for a normal run. To replay a run, enter the replay count it recorded for this stage (pre-gate-passed candidates, including repeats; results card, report figure and run manifest), and for the refinement also its recorded planned budget: the stage then ignores its time limit and stops after exactly that many candidates, with the same schedule, reproducing the run bit-identically at any worker count."
    )]
    pub replay_evaluations: Option<i64>,

    /// The refinement budget `B` a time-limited run planned from its
    /// measured throughput, to replay it: the population schedule then runs
    /// on exactly this budget. Unused by the screening stage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        label = "Replay planned budget",
        min = 1,
        help = "Refinement only. Leave blank for a normal run. To replay a time-limited run, enter the planned budget it recorded (results card, report figure and run manifest) beside the replay count: the refinement then sets its initial population and population reduction on exactly that budget instead of planning one from the measured throughput."
    )]
    pub replay_planned_evaluations: Option<i64>,

    /// The feasibility-restoration share of [`Self::replay_evaluations`]
    /// in the recorded refinement: the kernel replays the rest, restoration
    /// exactly this many. Unset means zero. Unused by the screening stage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        label = "Replay restoration count",
        min = 0,
        help = "Refinement only. Leave blank for a normal run, or to replay a run whose refinement did not enter feasibility restoration. To replay one that did, enter the restoration count it recorded beside the replay count: the search kernel then stops after the replay count less this number and restoration after exactly this number, as in the recorded run."
    )]
    pub replay_restoration_evaluations: Option<i64>,

    /// Largest number of pre-gate rejections; unset means
    /// [`PREGATE_REJECTS_PER_EVALUATION`] times [`Self::max_evaluations`].
    /// Always written to a saved file, where its presence marks a budget
    /// that counts analysed candidates only.
    #[serde(default)]
    #[config(
        label = "Pre-gate rejection cap",
        min = 1,
        help = "Largest number of candidates the design-vector pre-gate may reject in this stage before the stage stops. Leave blank for 20 times the evaluation budget. A rejection costs microseconds and never counts against the evaluation budget."
    )]
    pub max_pregate_rejects: Option<i64>,
}

/// Default pre-gate rejection cap per budgeted analysis. Engineering choice:
/// the worst measured rate on a registered box is about 3.3 rejections per
/// analysis (ATR72-600, uniform box samples; under 0.1 with the screening
/// sampler's root-chord projection), so twenty leaves a factor of six, and a
/// rejection costs about a microsecond against the 0.5 s to 1.5 s of an
/// analysis.
pub const PREGATE_REJECTS_PER_EVALUATION: i64 = 20;

impl StageBudget {
    /// Whether the budget can run: positive counts and a finite time limit
    /// in `(0, MAXIMUM_STAGE_TIME_LIMIT_S]`.
    ///
    /// # Errors
    ///
    /// A sentence naming the offending field.
    pub fn validate(&self, stage: &str) -> Result<(), String> {
        if self.max_evaluations < 1 {
            return Err(format!("{stage}.max_evaluations must be at least 1"));
        }
        if !(self.time_limit_s.is_finite()
            && self.time_limit_s > 0.0
            && self.time_limit_s <= MAXIMUM_STAGE_TIME_LIMIT_S)
        {
            return Err(format!(
                "{stage}.time_limit_s must lie in (0, {MAXIMUM_STAGE_TIME_LIMIT_S}] s, got {}",
                self.time_limit_s
            ));
        }
        if self.replay_evaluations.is_some_and(|count| count < 1) {
            return Err(format!("{stage}.replay_evaluations must be at least 1"));
        }
        if self
            .replay_planned_evaluations
            .is_some_and(|count| count < 1)
        {
            return Err(format!(
                "{stage}.replay_planned_evaluations must be at least 1"
            ));
        }
        if let Some(restoration) = self.replay_restoration_evaluations {
            if restoration < 0
                || self
                    .replay_evaluations
                    .is_some_and(|total| restoration > total)
            {
                return Err(format!(
                    "{stage}.replay_restoration_evaluations must lie in [0, replay_evaluations]"
                ));
            }
        }
        if self.max_pregate_rejects.is_some_and(|count| count < 1) {
            return Err(format!("{stage}.max_pregate_rejects must be at least 1"));
        }
        Ok(())
    }

    /// The pre-gate rejection cap: the configured one, else
    /// [`PREGATE_REJECTS_PER_EVALUATION`] times the evaluation budget.
    #[must_use]
    pub fn resolved_max_pregate_rejects(&self) -> i64 {
        self.max_pregate_rejects.unwrap_or_else(|| {
            self.max_evaluations
                .max(1)
                .saturating_mul(PREGATE_REJECTS_PER_EVALUATION)
        })
    }
}

/// Default evaluation ceiling of both stages: a ceiling the time limits
/// reach first, not a budget. Measured on a 32-thread machine: 0.3 s to
/// 1.5 s of lane time per coupled analysis and about 8 analyses per second
/// in aggregate, so even the 300 s maximum limit affords about 2 400; the
/// ceiling binds only above about 67 analyses per second at 300 s (167 at
/// the 120 s default), some eight times the measured throughput
/// (engineering choice). With a time limit the refinement plans its own
/// budget below it from the screening throughput. A run on evaluation
/// budgets only should set its budgets explicitly: at the measured rate this
/// ceiling is about 40 minutes per stage.
const DEFAULT_EVALUATION_CEILING: i64 = 20_000;

/// Screening default: the 30 s limit is the interactive budget and ends the
/// stage under the [`DEFAULT_EVALUATION_CEILING`].
fn default_screening_budget() -> StageBudget {
    StageBudget {
        max_evaluations: DEFAULT_EVALUATION_CEILING,
        time_limit_s: 30.0,
        replay_evaluations: None,
        replay_planned_evaluations: None,
        replay_restoration_evaluations: None,
        max_pregate_rejects: None,
    }
}

/// Refinement default: the 120 s limit ends the stage, whose population
/// schedule is planned on what that limit affords at the measured screening
/// throughput, at most the [`DEFAULT_EVALUATION_CEILING`].
fn default_refinement_budget() -> StageBudget {
    StageBudget {
        max_evaluations: DEFAULT_EVALUATION_CEILING,
        time_limit_s: 120.0,
        replay_evaluations: None,
        replay_planned_evaluations: None,
        replay_restoration_evaluations: None,
        max_pregate_rejects: None,
    }
}

/// The one optimization method: mission-sized differential evolution.
pub const PRODUCT_DE_METHOD: &str = "differential_evolution";

/// Method tokens an earlier build accepted and this one no longer implements
/// as distinct kernels. A saved configuration carrying one of these is
/// migrated to [`PRODUCT_DE_METHOD`] at load time, with a note the caller can
/// surface (see `crate::settings_load_notes`).
pub const LEGACY_METHOD_TOKENS: &[&str] = &[
    "feasibility_first_de",
    "nsga2",
    "turbo_1",
    "cma_es",
    "sqp",
    "scipy_legacy",
];

/// A run seed above what the configuration's signed [`SolverSettings::seed`]
/// holds.
///
/// Refused rather than wrapped, so a seed given on the command line or to
/// the pipeline never silently replays a different, negative seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("optimizer seed exceeds the supported integer range (0 to {max}), got {seed}", max = i64::MAX)]
pub struct SeedOutOfRange {
    /// The refused seed.
    pub seed: u64,
}

impl SolverSettings {
    /// Store a run seed given as an unsigned integer.
    ///
    /// The one range check for every front end that accepts a seed: values
    /// up to `i64::MAX` are stored unchanged, larger ones are refused with
    /// [`SeedOutOfRange`] and leave the setting untouched.
    pub fn set_seed(&mut self, seed: u64) -> Result<(), SeedOutOfRange> {
        self.seed = Some(i64::try_from(seed).map_err(|_| SeedOutOfRange { seed })?);
        Ok(())
    }

    /// The worker count to actually evaluate a batch with.
    ///
    /// Resolves the `0` automatic setting to every thread the machine
    /// reports. A machine that does not report its parallelism falls back to
    /// one worker rather than guessing.
    #[must_use]
    pub fn resolved_workers(&self) -> usize {
        if self.workers > 0 {
            return usize::try_from(self.workers).unwrap_or(usize::MAX);
        }
        if self.workers < 0 {
            return 1;
        }
        std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
    }

    /// Whether both stage budgets can run.
    ///
    /// # Errors
    ///
    /// A sentence naming the offending field.
    pub fn validate_budgets(&self) -> Result<(), String> {
        self.screening.validate("screening")?;
        self.refinement.validate("refinement")?;
        if self.convergence_stagnation_generations < 1 {
            return Err("convergence_stagnation_generations must be at least 1".to_owned());
        }
        if !(self.tolerance.is_finite() && self.tolerance >= 0.0) {
            return Err(format!(
                "tolerance must be finite and non-negative, got {}",
                self.tolerance
            ));
        }
        Ok(())
    }

    /// Whether `method` names the supported optimization method.
    pub fn is_supported_method(method: &str) -> bool {
        method == PRODUCT_DE_METHOD
    }
}

fn default_optimizer_method() -> String {
    PRODUCT_DE_METHOD.to_owned()
}

fn default_finite_difference_step() -> f64 {
    0.002
}

// The two gradient-driver settings are serialized only when changed, so a
// saved configuration and the frozen solver-preset fixtures keep the
// historical key set.
fn is_default_finite_difference_step(value: &f64) -> bool {
    *value == default_finite_difference_step()
}

fn default_constraint_tolerance() -> f64 {
    1.0e-4
}

fn is_default_constraint_tolerance(value: &f64) -> bool {
    *value == default_constraint_tolerance()
}

fn default_convergence_stagnation_generations() -> i64 {
    5
}

fn is_default_convergence_stagnation_generations(value: &i64) -> bool {
    *value == default_convergence_stagnation_generations()
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Entry, Kind, OptionSource};

    #[test]
    fn a_seed_is_stored_up_to_the_signed_limit_and_refused_above_it() {
        let mut settings = SolverSettings::default();
        settings.set_seed(i64::MAX as u64).unwrap();
        assert_eq!(settings.seed, Some(i64::MAX));

        let error = settings.set_seed(i64::MAX as u64 + 1).unwrap_err();
        assert_eq!(error.seed, i64::MAX as u64 + 1);
        assert!(error.to_string().starts_with("optimizer seed exceeds"));
        assert_eq!(settings.seed, Some(i64::MAX));
    }

    fn leaf(name: &str, settings: &SolverSettings) -> crate::LeafField {
        match &settings.schema().field(name).unwrap().entry {
            Entry::Leaf(leaf) => leaf.clone(),
            Entry::Node(_) => panic!("{name} is not a group"),
        }
    }

    #[test]
    fn the_method_is_the_single_strict_choice() {
        let settings = SolverSettings::default();
        let method = leaf("method", &settings);
        assert_eq!(method.options, Some(OptionSource::OptimizerMethod));
        assert!(!OptionSource::OptimizerMethod.editable());
        assert!(OptionSource::OptimizerMethod
            .options()
            .unwrap()
            .contains(&settings.method.as_str()));
    }

    #[test]
    fn an_unset_seed_reaches_the_form_as_blank_rather_than_as_a_number() {
        // The default is a different search each run, and a seed box showing
        // 0 would claim the run was reproducible when it was not.
        let leaf = leaf("seed", &SolverSettings::default());
        assert_eq!(leaf.kind, Kind::Optional);
        assert_eq!(leaf.value, serde_json::Value::Null);
    }

    #[test]
    fn a_seed_that_has_been_set_reaches_the_form_as_the_integer_it_is() {
        let settings = SolverSettings {
            seed: Some(42),
            ..Default::default()
        };
        let leaf = leaf("seed", &settings);
        assert_eq!(leaf.kind, Kind::Int);
        assert_eq!(leaf.value, serde_json::json!(42));
    }

    #[test]
    fn no_solver_setting_is_named_in_a_way_that_makes_it_a_weight_slider() {
        // The weight-slider rule keys off the field name, and this group sits
        // next to one whose fields it applies to throughout. A generation
        // count offered as a ratio slider would be unusable.
        let settings = SolverSettings::default();
        for field in settings.schema().fields {
            if let Entry::Leaf(leaf) = field.entry {
                assert_ne!(leaf.kind, Kind::WeightSlider, "{}", field.name);
            }
        }
    }

    #[test]
    fn the_refinement_time_limit_is_capped_at_five_minutes() {
        let mut settings = SolverSettings::default();
        assert_eq!(settings.validate_budgets(), Ok(()));
        assert_eq!(settings.screening.time_limit_s, 30.0);
        assert_eq!(settings.refinement.time_limit_s, 120.0);
        settings.refinement.time_limit_s = MAXIMUM_STAGE_TIME_LIMIT_S;
        assert_eq!(settings.validate_budgets(), Ok(()));
        settings.refinement.time_limit_s = MAXIMUM_STAGE_TIME_LIMIT_S + 1.0;
        let error = settings.validate_budgets().unwrap_err();
        assert!(error.starts_with("refinement.time_limit_s"), "{error}");
        settings.refinement.time_limit_s = 60.0;
        settings.screening.max_evaluations = 0;
        assert!(settings.validate_budgets().is_err());
    }

    #[test]
    fn optimizer_tokens_are_checked_against_the_dispatch_contract() {
        assert!(SolverSettings::is_supported_method(
            "differential_evolution"
        ));
        assert!(!SolverSettings::is_supported_method(
            "differential_evoluton"
        ));
    }

    #[test]
    fn every_legacy_method_token_is_unsupported_directly_and_named_for_migration() {
        // `SolverSettings::is_supported_method` is the dispatch contract's own
        // check, and it must reject every legacy token exactly like an
        // unknown one: migration is a document-loading concern
        // (`crate::settings_load_notes`), not a dispatch fallback.
        for &method in LEGACY_METHOD_TOKENS {
            assert!(!SolverSettings::is_supported_method(method), "{method}");
        }
        assert!(LEGACY_METHOD_TOKENS.contains(&"sqp"));
        assert!(LEGACY_METHOD_TOKENS.contains(&"nsga2"));
        assert!(LEGACY_METHOD_TOKENS.contains(&"turbo_1"));
        assert!(LEGACY_METHOD_TOKENS.contains(&"cma_es"));
        assert!(LEGACY_METHOD_TOKENS.contains(&"feasibility_first_de"));
    }
}
