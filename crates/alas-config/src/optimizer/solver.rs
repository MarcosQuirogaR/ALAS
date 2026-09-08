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
    #[serde(default = "default_optimizer_method")]
    #[config(
        options = OptimizerMethod,
        label = "Optimization method",
        help = "Select the search algorithm. Differential evolution preserves the historical scalar search; feasibility-first DE gives physical validity priority; NSGA-II retains a Pareto set; TuRBO-1 uses a local trust-region surrogate for expensive evaluations; CMA-ES adapts correlated continuous design steps; SQP is the gradient-based sequential quadratic programming driver over the converged sizing loop, with finite-difference derivatives and explicit inequality constraints."
    )]
    pub method: String,

    /// Finite-difference step for the gradient-based driver.
    #[serde(
        default = "default_finite_difference_step",
        skip_serializing_if = "is_default_finite_difference_step"
    )]
    #[config(
        label = "Finite-difference step",
        help = "Forward-difference step of the SQP driver, as a fraction of each design variable's bound range. It must exceed the numerical noise of the sizing closure (a 1 kg takeoff-mass tolerance on a 100 t aircraft is 1e-5) and stay below the scale on which the objective bends; 0.001-0.005 is the usual window."
    )]
    pub finite_difference_step: f64,

    /// Normalized constraint violation accepted as feasible by the driver.
    #[serde(
        default = "default_constraint_tolerance",
        skip_serializing_if = "is_default_constraint_tolerance"
    )]
    #[config(
        label = "Constraint tolerance",
        help = "Largest normalized inequality-constraint violation (violation divided by the limit) the SQP driver accepts at convergence. 1e-4 is one part in ten thousand of every limit."
    )]
    pub constraint_tolerance: f64,

    /// How new candidates are generated from the population.
    #[config(
        options = Strategy,
        label = "DE mutation/crossover strategy",
        help = "SciPy differential_evolution strategy name (e.g. 'best1bin', 'rand1bin', 'best2bin') -- controls how new candidate designs are generated from the population each generation."
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
        help = "Population size as a multiplier on the number of design variables -- more candidates per generation explores more broadly but costs more evaluations."
    )]
    pub population_size: i64,

    /// How converged the population has to be before stopping early.
    #[config(
        label = "Convergence tolerance",
        help = "Relative tolerance for convergence; the solver stops early once the population's cost spread falls below this."
    )]
    pub tolerance: f64,

    /// The random seed, or unset for a different search each run.
    #[config(
        label = "Random seed",
        help = "Set an integer for a reproducible run (same seed -> same result); leave blank for a different search each run."
    )]
    pub seed: Option<i64>,

    /// How many native-objective workers evaluate a candidate batch at once.
    #[config(
        label = "Parallel worker processes",
        help = "Number of native worker threads for differential-evolution candidate batches (>1 enables parallel evaluation; non-positive values are treated as 1). External evaluator adapters remain serial because they own mutable process/session state."
    )]
    pub workers: i64,

    /// Whether each generation reports itself as it finishes.
    #[config(
        label = "Print progress to console",
        help = "Print a one-line progress summary (valid count, best L/D so far) after each generation."
    )]
    pub display_progress: bool,

    /// Whether generation zero clusters around the initial design.
    #[config(
        label = "Seed search near the initial design",
        help = "Initialize the population as a tight cluster of small perturbations around the initial/preset design (plus the design itself, unperturbed) instead of SciPy's default uniform latin-hypercube coverage of the whole bounds space. Guarantees at least one known-valid, physically-balanced design is in generation 0, and lets the solver refine from there instead of having to rediscover CG/stability balance from scratch across the full 16-D space. Disable to fall back to the old full-space exploration (e.g. if you specifically want to explore far from the initial design)."
    )]
    pub seed_near_initial_design: bool,

    /// How tight that cluster is.
    #[config(
        label = "Seed cluster perturbation size",
        help = "Size of the initial random perturbation around the initial design, as a fraction of each design variable's (upper - lower) bound range. Only used when seed_near_initial_design is enabled. Small values (e.g. 0.05) start with a tight, mostly-valid cluster; larger values explore more broadly from the start at the cost of more of the population starting off invalid."
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
            seed: None,
            workers: 1,
            display_progress: true,
            seed_near_initial_design: true,
            seed_perturbation_fraction: 0.05,
        }
    }
}

impl SolverSettings {
    /// Whether `method` names an optimizer implemented by the product.
    ///
    /// Keep this list next to the serialized setting so configuration
    /// validation and dispatch cannot drift into different spellings.
    pub fn is_supported_method(method: &str) -> bool {
        matches!(
            method,
            "differential_evolution"
                | "feasibility_first_de"
                | "nsga2"
                | "turbo_1"
                | "cma_es"
                | "sqp"
        )
    }

    /// Whether `method` is the gradient-based driver, which reads the
    /// finite-difference and constraint-tolerance settings.
    pub fn is_gradient_method(method: &str) -> bool {
        method == "sqp"
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
}
