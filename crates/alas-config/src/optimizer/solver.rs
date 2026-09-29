// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/optimizer_config.py (`SolverSettings`)

//! How the aircraft-design search is run: which optimization profile, how
//! long, how wide, and from where.
//!
//! These settings decide how many aircraft get built and analysed, and each
//! evaluation is a full geometry build, mass breakdown and vortex-lattice
//! solve. The default `scipy_legacy` profile restores the v1.1.0 weighted L/D
//! objective and SciPy-style `best1bin`; `differential_evolution` selects the
//! mission-sized product objective and L-SHADE kernel.
//!
//! # Initial population
//!
//! The Python profile seeds generation zero with small perturbations around
//! the initial design, plus that design unperturbed. The product profile mixes
//! local and global samples after its broad scan. Both fall back to
//! Latin-hypercube coverage when local seeding is disabled or unavailable.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Settings for the differential-evolution solver.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct SolverSettings {
    /// Complete optimization profile selected for this run.
    ///
    /// `scipy_legacy` restores the Python v1.1.0 scalar weighted L/D objective,
    /// penalty table and SciPy-style Differential Evolution. The
    /// `differential_evolution` profile keeps the mission-sized objective,
    /// staged scan, feasibility ranking and L-SHADE product kernel.
    #[serde(default = "default_optimizer_method")]
    #[config(
        options = OptimizerMethod,
        label = "Optimization profile",
        help = "scipy_legacy restores ALAS v1.1.0: weighted lift-to-drag plus scalar penalties, SciPy-style differential_evolution (best1bin by default), a seeded local population or Latin-hypercube fallback, and no separate scan/restoration. differential_evolution selects the mission-sized objective and current L-SHADE product search."
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

    /// How new candidates are generated from the population.
    ///
    /// Used by the `scipy_legacy` profile. The product L-SHADE profile uses
    /// current-to-pbest/1/bin independently of this field.
    #[config(
        options = Strategy,
        label = "SciPy DE mutation/crossover strategy",
        help = "Used by scipy_legacy. SciPy differential_evolution strategy name (e.g. best1bin, rand1bin, best2bin). The product L-SHADE profile always uses current-to-pbest/1/bin."
    )]
    pub strategy: String,

    /// How many generations the search runs for.
    #[config(
        label = "Max generations",
        help = "Maximum number of generations (iterations) the solver runs before stopping."
    )]
    pub max_iterations: i64,

    /// Population size, as a multiple of the number of free design variables.
    #[config(
        label = "Population size multiplier",
        help = "Population size as a multiplier on the number of free design variables. Locked coordinates do not consume search population. More candidates explore more broadly but cost more evaluations."
    )]
    pub population_size: i64,

    /// How converged the population has to be before stopping early.
    #[config(
        label = "Convergence tolerance",
        help = "For scipy_legacy, stops when the population energy standard deviation is at most tolerance times abs(mean energy), matching SciPy's default test. The product profile uses normalized design-space spread and its stagnation window."
    )]
    pub tolerance: f64,

    /// Product-profile-only stagnation window retained for saved configs.
    #[serde(
        default = "default_convergence_stagnation_generations",
        skip_serializing_if = "is_default_convergence_stagnation_generations"
    )]
    #[config(
        label = "Product-profile convergence stagnation window",
        help = "Used only by the mission-sized differential_evolution profile. The scipy_legacy profile uses SciPy's population-energy spread test and ignores this field."
    )]
    pub convergence_stagnation_generations: i64,

    /// The random seed, or unset for a different search each run.
    #[config(
        label = "Random seed",
        help = "Set an integer for a reproducible run (same seed -> same result); leave blank for a different search each run."
    )]
    pub seed: Option<i64>,

    /// Total native compute threads shared by candidates and nested analyses.
    ///
    /// `0` means "decide from the machine". The worker count changes only how
    /// a batch is distributed, never which designs are evaluated or what they
    /// score, so this is a wall-clock setting and not a modelling one; the
    /// batch is a fixed set of points and each point is scored independently.
    /// A positive value is used exactly as given, so a configuration that
    /// states `1` keeps one worker.
    #[config(
        label = "Native compute worker threads",
        help = "Total native compute worker threads shared by candidate batches and nested VLM analyses. The scipy_legacy profile defaults to one worker, matching Python's immediate-update mode; multiple workers use deferred generations and can change the trajectory, as in SciPy. The product profile evaluates deterministic whole-generation batches, so worker count changes runtime only. External evaluator adapters remain serial because they own mutable process/session state."
    )]
    pub workers: i64,

    /// Whether each generation reports itself as it finishes.
    #[config(
        label = "Print progress to console",
        help = "Print a one-line progress summary (valid count, best L/D so far) after each generation."
    )]
    pub display_progress: bool,

    /// Whether generation zero clusters around the initial design.
    ///
    /// The legacy profile seeds around the supplied design. If disabled or
    /// unavailable, it uses Latin-hypercube sampling. The product profile
    /// combines local perturbations, global samples and its scan seed.
    #[config(
        label = "Seed search near the initial design",
        help = "For scipy_legacy, keep the supplied initial design and seed the population with nearby perturbations; if disabled or unavailable, use Latin-hypercube sampling. The product profile also mixes global samples with its scan seed."
    )]
    pub seed_near_initial_design: bool,

    /// How tight that cluster is.
    ///
    /// Used when [`Self::seed_near_initial_design`] is enabled.
    #[config(
        label = "Seed cluster perturbation size",
        help = "Size of local initial perturbations as a fraction of each free variable's bound range. Applies when seed_near_initial_design is enabled; fixed coordinates remain unchanged."
    )]
    pub seed_perturbation_fraction: f64,
}

impl Default for SolverSettings {
    fn default() -> Self {
        Self {
            method: default_optimizer_method(),
            finite_difference_step: default_finite_difference_step(),
            constraint_tolerance: default_constraint_tolerance(),
            strategy: "best1bin".to_owned(),
            max_iterations: 15,
            population_size: 6,
            tolerance: 0.01,
            convergence_stagnation_generations: default_convergence_stagnation_generations(),
            seed: None,
            workers: 1,
            display_progress: true,
            seed_near_initial_design: true,
            seed_perturbation_fraction: 0.05,
        }
    }
}

/// The largest automatic worker count.
///
/// The measured evidence for worker scaling on this product covers one and
/// eight workers (2.24x on B787-9 and 2.76x on AVE, with the evaluation count
/// and the winning design identical at both). Eight is therefore the largest
/// count the automatic setting will choose on its own: a bigger number is an
/// extrapolation past what has been measured, and on a batch of a few hundred
/// coupled analyses it also starts competing with the desktop session for
/// cores. A configuration that states more than eight is still honoured.
pub const MAXIMUM_AUTOMATIC_WORKERS: usize = 8;

/// The default full v1.1.0-compatible optimization profile.
pub const SCIPY_LEGACY_METHOD: &str = "scipy_legacy";
/// The mission-sized product profile retained as an explicit alternative.
pub const PRODUCT_DE_METHOD: &str = "differential_evolution";

/// Method tokens an earlier build accepted and this one no longer implements
/// as distinct kernels. A saved configuration carrying one of these is
/// migrated to [`PRODUCT_DE_METHOD`] at load time, with a note the caller can
/// surface (see `crate::settings_load_notes`).
pub const LEGACY_METHOD_TOKENS: &[&str] =
    &["feasibility_first_de", "nsga2", "turbo_1", "cma_es", "sqp"];

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
    /// Resolves the `0` automatic setting against the machine, so the
    /// product search and the frozen reference-compatibility replay cannot
    /// disagree about what "automatic" means. A machine that does not report
    /// its parallelism falls back to one worker rather than guessing, which
    /// is the same conservative answer the setting had before it could be
    /// automatic.
    #[must_use]
    pub fn resolved_workers(&self) -> usize {
        if self.workers > 0 {
            // `as` on a checked-positive i64 is the count the user asked for.
            return usize::try_from(self.workers).unwrap_or(usize::MAX);
        }
        if self.workers < 0 {
            return 1;
        }
        std::thread::available_parallelism()
            .map_or(1, |count| count.get().min(MAXIMUM_AUTOMATIC_WORKERS))
    }

    /// Whether `method` names a supported optimization profile.
    ///
    /// The default restores the original SciPy/L/D strategy; the product DE
    /// remains selectable for the mission-sized formulation.
    pub fn is_supported_method(method: &str) -> bool {
        matches!(method, SCIPY_LEGACY_METHOD | PRODUCT_DE_METHOD)
    }

    /// Whether `strategy` is one of the DE mutation/crossover strategies.
    pub fn is_supported_strategy(strategy: &str) -> bool {
        matches!(
            strategy,
            "best1bin"
                | "best1exp"
                | "rand1bin"
                | "rand1exp"
                | "best2bin"
                | "best2exp"
                | "rand2bin"
                | "rand2exp"
                | "randtobest1bin"
                | "randtobest1exp"
                | "currenttobest1bin"
                | "currenttobest1exp"
        )
    }
}

fn default_optimizer_method() -> String {
    SCIPY_LEGACY_METHOD.to_owned()
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
    fn the_strategy_is_offered_as_the_list_the_solver_accepts() {
        // A strategy name the solver does not know is a run that fails after
        // the first evaluation, which is minutes in, so the field is a strict
        // list rather than free text.
        let settings = SolverSettings::default();
        let strategy = leaf("strategy", &settings);
        assert_eq!(strategy.options, Some(OptionSource::Strategy));
        assert!(!OptionSource::Strategy.editable());
        let accepted = OptionSource::Strategy.options().unwrap();
        assert!(accepted.contains(&settings.strategy.as_str()));
    }

    #[test]
    fn every_product_optimizer_method_is_a_strict_gui_choice() {
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
    fn the_default_search_is_small_enough_to_finish_in_an_afternoon() {
        // Generations times population multiplier times the design-variable
        // count is the evaluation budget, and one evaluation is a full build
        // and vortex-lattice solve.
        let settings = SolverSettings::default();
        let evaluations = settings.max_iterations
            * settings.population_size
            * crate::DESIGN_VARIABLE_SPECS.len() as i64;
        assert!(evaluations < 2_000, "{evaluations} evaluations");
    }

    #[test]
    fn optimizer_tokens_are_checked_against_the_dispatch_contract() {
        assert!(SolverSettings::is_supported_method(
            "differential_evolution"
        ));
        assert!(!SolverSettings::is_supported_method(
            "differential_evoluton"
        ));
        assert!(SolverSettings::is_supported_strategy("best1bin"));
        assert!(!SolverSettings::is_supported_strategy("best1bni"));
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
