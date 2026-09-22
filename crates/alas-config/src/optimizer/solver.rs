// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/optimizer_config.py (`SolverSettings`)
// Reference: alas @ rust-port-baseline.

//! How the aircraft-design search is run: which algorithm, how long, how wide,
//! and from where.
//!
//! These settings decide how many aircraft get built and analysed, and each
//! evaluation is a full geometry build, mass breakdown and vortex-lattice
//! solve. Generations times population size times the number of design
//! variables is the run's cost, so this is the one group where a value chosen
//! carelessly is felt as hours rather than as a wrong number.
//!
//! # Why the population starts clustered rather than spread
//!
//! The upstream default seeds generation zero as a tight cluster of small
//! perturbations around the initial design, plus that design unperturbed,
//! instead of the uniform latin-hypercube coverage the solver would otherwise
//! use. A design that trims, balances and closes its weight budget is a narrow
//! region of the sixteen-dimensional box the bounds describe; a uniform sample
//! of that box is almost entirely made of aircraft that do not balance, and
//! the search spends its budget rediscovering feasibility rather than
//! improving on it. Seeding near a known-good design starts inside the region
//! and refines. [`SolverSettings::seed_near_initial_design`] turns it off for
//! a deliberately broad search.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Settings for the differential-evolution solver.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct SolverSettings {
    /// Top-level optimizer selected for product searches.
    ///
    /// This is the only value a saved file may carry going forward. A legacy
    /// token from an earlier build (`sqp`, `nsga2`, `turbo_1`, `cma_es`,
    /// `feasibility_first_de`) is migrated to this one at load time, with a
    /// note the caller can surface (`alas_config::settings_load_notes`); it
    /// is never silently accepted as a distinct algorithm.
    #[serde(default = "default_optimizer_method")]
    #[config(
        options = OptimizerMethod,
        label = "Optimization method",
        help = "The one search algorithm this build runs: L-SHADE differential evolution under the epsilon-constrained method (success-history parameter adaptation, current-to-pbest/1 mutation with an archive, linear population size reduction, and a constraint boundary that decays to strict feasibility as the search proceeds)."
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
    /// Read only by the frozen SciPy-parity replay
    /// (`DesignOptimizer::new_reference_compatibility`), which no product
    /// pipeline or GUI path constructs. The product L-SHADE search always
    /// uses current-to-pbest/1/bin and does not read this field.
    #[config(
        options = Strategy,
        label = "DE mutation/crossover strategy (parity replay only)",
        help = "SciPy differential_evolution strategy name (e.g. 'best1bin', 'rand1bin', 'best2bin'), read only by the frozen reference-compatibility replay used for regression comparison against the Python baseline. The product search always uses current-to-pbest/1/bin and ignores this field."
    )]
    pub strategy: String,

    /// How many generations the search runs for.
    #[config(
        label = "Max generations",
        help = "Maximum number of generations (iterations) the solver runs before stopping."
    )]
    pub max_iterations: i64,

    /// Population size, as a multiple of the number of design variables.
    #[config(
        label = "Population size multiplier",
        help = "Population size as a multiplier on the number of design variables: more candidates per generation explores more broadly but costs more evaluations."
    )]
    pub population_size: i64,

    /// How converged the population has to be before stopping early.
    #[config(
        label = "Convergence tolerance",
        help = "The search stops early once two things both hold: the population's normalized design-space spread has fallen below this fraction of the bounds, and the best feasible cost's relative improvement has stayed below this fraction for the stagnation window below."
    )]
    pub tolerance: f64,

    /// Generations the best feasible cost may fail to improve by more than
    /// `tolerance` before a converged spread is honoured.
    #[serde(
        default = "default_convergence_stagnation_generations",
        skip_serializing_if = "is_default_convergence_stagnation_generations"
    )]
    #[config(
        label = "Convergence stagnation window",
        help = "Consecutive generations the best feasible cost may fail to improve by more than the convergence tolerance before the search may report convergence, once the population's design-space spread has also fallen below that tolerance."
    )]
    pub convergence_stagnation_generations: i64,

    /// The random seed, or unset for a different search each run.
    #[config(
        label = "Random seed",
        help = "Set an integer for a reproducible run (same seed -> same result); leave blank for a different search each run."
    )]
    pub seed: Option<i64>,

    /// How many native-objective workers evaluate a candidate batch at once.
    ///
    /// `0` means "decide from the machine". The worker count changes only how
    /// a batch is distributed, never which designs are evaluated or what they
    /// score, so this is a wall-clock setting and not a modelling one; the
    /// batch is a fixed set of points and each point is scored independently.
    /// A positive value is used exactly as given, so a configuration that
    /// states `1` keeps one worker.
    #[config(
        label = "Parallel worker processes",
        help = "Number of native worker threads for candidate batches. 0 (the default) picks a count from the machine's available parallelism; a positive value is used exactly as written; negative values are treated as 1. The product L-SHADE search always evaluates one whole generation as a single deterministic batch, in the order it built the generation from its seed, so this setting changes only how long a batch takes, never which points are evaluated or the winner: a seeded run replays bit-identically at any worker count. The frozen reference-compatibility replay is the one exception: its legacy driver defers a whole generation only when more than one worker is requested, which changes the trial interleaving and is preserved that way for exact regression comparison against the Python baseline. External evaluator adapters remain serial because they own mutable process/session state."
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
    /// Read only by the frozen SciPy-parity replay, like [`Self::strategy`];
    /// the product L-SHADE search always seeds its population's first
    /// individual directly from the supplied design and draws the rest from
    /// a Latin hypercube over the bounds.
    #[config(
        label = "Seed search near the initial design (parity replay only)",
        help = "Initialize the population as a tight cluster of small perturbations around the initial/preset design (plus the design itself, unperturbed) instead of SciPy's default uniform latin-hypercube coverage of the whole bounds space. Read only by the frozen reference-compatibility replay; the product L-SHADE search seeds its population's first individual directly from the supplied design instead and does not read this field."
    )]
    pub seed_near_initial_design: bool,

    /// How tight that cluster is.
    ///
    /// Read only by the frozen SciPy-parity replay; see
    /// [`Self::seed_near_initial_design`].
    #[config(
        label = "Seed cluster perturbation size (parity replay only)",
        help = "Size of the initial random perturbation around the initial design, as a fraction of each design variable's (upper - lower) bound range. Read only by the frozen reference-compatibility replay when seed_near_initial_design is enabled; the product L-SHADE search does not read this field."
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
            workers: 0,
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

/// Method tokens an earlier build accepted and this one no longer implements
/// as a distinct kernel. A saved configuration document that carries one of
/// these is migrated to `"differential_evolution"` at load time, with a note
/// the caller can surface (see `crate::settings_load_notes`); a
/// [`SolverSettings`] built directly with one of them, bypassing that
/// boundary, is rejected by [`SolverSettings::is_supported_method`] rather
/// than silently running an algorithm this build does not have.
pub const LEGACY_METHOD_TOKENS: &[&str] =
    &["feasibility_first_de", "nsga2", "turbo_1", "cma_es", "sqp"];

impl SolverSettings {
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

    /// Whether `method` names an optimizer implemented by the product.
    ///
    /// `"differential_evolution"` is the only supported value; see
    /// [`LEGACY_METHOD_TOKENS`] for the names a saved file may still carry
    /// and where they are migrated.
    pub fn is_supported_method(method: &str) -> bool {
        method == "differential_evolution"
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
    "differential_evolution".to_owned()
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
