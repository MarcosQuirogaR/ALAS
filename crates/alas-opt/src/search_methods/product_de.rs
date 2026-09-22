// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The product search entry point: resolves `optimizer.solver` into
//! [`lshade_de::Settings`] and runs the one kernel.
//!
//! `optimizer.solver.method` is validated at the configuration boundary
//! (`alas_config::SolverSettings::is_supported_method`) and any saved legacy
//! token (`sqp`, `nsga2`, `turbo_1`, `cma_es`, `feasibility_first_de`) is
//! migrated to `differential_evolution` when a configuration document loads
//! (`alas_config::settings_load_notes`), with a note the caller can surface.
//! By the time a method string reaches this module it names the one kernel
//! this build runs, so there is nothing left to dispatch here.
//!
//! Design-vector units, frames, signs and validity domains are specified in
//! `docs/optimizer-design-vector.md`; this module treats the vector as
//! opaque normalized-to-bounds coordinates and never interprets a component.

use alas_config::SolverSettings;

use crate::cancellation::CancelScope;

use super::{lshade_de, EvaluateBatch, ScoredPoint};

/// The smallest population L-SHADE ever runs at (see
/// `lshade_de::MIN_POPULATION`); an initial population below this is raised
/// to it rather than rejected.
const MINIMUM_POPULATION: usize = 4;

/// The `method` string every product optimization result reports.
pub(crate) const METHOD: &str = "differential_evolution";
/// The `strategy` string every product optimization result reports.
pub(crate) const STRATEGY: &str = "lshade_eps_de";

/// Resolved Differential Evolution settings for one run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Settings {
    /// Initial candidates before L-SHADE's linear population size reduction.
    pub(crate) population: usize,
    /// Generations after the initial population.
    pub(crate) generations: usize,
    /// The seed the whole search replays from.
    pub(crate) seed: u64,
    /// Normalized design-space spread and relative best-cost improvement
    /// threshold for the convergence test.
    pub(crate) spread_tolerance: f64,
    /// Consecutive generations without a meaningful improvement before a
    /// converged spread is honoured.
    pub(crate) stagnation_generations: usize,
}

impl Settings {
    /// Resolve the configured solver settings for `dimension` design
    /// variables against an already-resolved `seed`.
    ///
    /// `solver.population_size` is a multiplier on the number of design
    /// variables, matching the setting's own label and the frozen SciPy
    /// semantics the legacy driver was ported from, so the sixteen-variable
    /// product space at the default multiplier of six starts at ninety-six
    /// candidates and shrinks from there. `solver.max_iterations` is the
    /// generation count; zero is legitimate and records the initial
    /// population only.
    pub(crate) fn from_solver(solver: &SolverSettings, dimension: usize, seed: u64) -> Self {
        let multiplier = usize::try_from(solver.population_size.max(1)).unwrap_or(1);
        let population = multiplier
            .saturating_mul(dimension.max(1))
            .max(MINIMUM_POPULATION);
        let generations = usize::try_from(solver.max_iterations.max(0)).unwrap_or(0);
        let stagnation_generations =
            usize::try_from(solver.convergence_stagnation_generations.max(1)).unwrap_or(1);
        Self {
            population,
            generations,
            seed,
            spread_tolerance: solver.tolerance.max(0.0),
            stagnation_generations,
        }
    }

    /// Analyses this configuration will request in the worst case (no
    /// population-size reduction, which only ever lowers this): the initial
    /// population plus one trial per candidate per generation.
    pub(crate) fn evaluation_budget(self) -> usize {
        self.population
            .saturating_add(self.population.saturating_mul(self.generations))
    }

    fn to_kernel(self) -> lshade_de::Settings {
        lshade_de::Settings {
            population: self.population,
            generations: self.generations,
            seed: self.seed,
            spread_tolerance: self.spread_tolerance,
            stagnation_generations: self.stagnation_generations,
        }
    }
}

/// Run the product Differential Evolution search.
///
/// `initial_design` is repaired into `bounds` rather than discarded, so a
/// starting point that a bound change has left outside the envelope still
/// seeds the population instead of being silently dropped. Every candidate
/// the evaluator sees lies inside `bounds`.
///
/// `evaluate_batch` scores one whole generation at a time, in a fixed order
/// independent of how many worker threads it spreads the batch across; see
/// `lshade_de`'s module documentation for the determinism and cancellation
/// contract this gives the seeded replay.
pub(crate) fn run(
    bounds: &[(f64, f64)],
    initial_design: Option<&[f64]>,
    settings: Settings,
    scope: &CancelScope<'_>,
    evaluate_batch: &mut EvaluateBatch<'_>,
) -> lshade_de::Outcome {
    lshade_de::run(
        bounds,
        settings.to_kernel(),
        initial_design,
        scope,
        evaluate_batch,
    )
}

/// The score of a candidate whose evaluation produced nothing.
///
/// Worst possible on every ordering key, so it can never displace a
/// candidate that was actually analysed, and never silently becomes a winner.
pub(crate) fn unevaluated(values: &[f64]) -> ScoredPoint {
    ScoredPoint {
        values: values.to_vec(),
        cost: f64::INFINITY,
        valid: false,
        constraint_violation: f64::INFINITY,
        objectives: [f64::INFINITY; 3],
    }
}

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn scored(values: &[f64]) -> ScoredPoint {
        let x = values[0];
        ScoredPoint {
            values: values.to_vec(),
            cost: x * x,
            valid: x >= 0.4,
            constraint_violation: (0.4 - x).max(0.0),
            objectives: [x * x, x, x],
        }
    }

    fn batch(points: &[Vec<f64>]) -> Vec<ScoredPoint> {
        points.iter().map(|p| scored(p)).collect()
    }

    fn settings(seed: u64) -> Settings {
        Settings {
            population: 8,
            generations: 10,
            seed,
            spread_tolerance: 0.01,
            stagnation_generations: 5,
        }
    }

    #[test]
    fn population_size_is_a_multiplier_on_the_design_variables() {
        let mut solver = SolverSettings {
            population_size: 6,
            max_iterations: 15,
            ..Default::default()
        };
        let resolved = Settings::from_solver(&solver, 16, 7);
        assert_eq!(resolved.population, 96);
        assert_eq!(resolved.generations, 15);
        assert_eq!(resolved.seed, 7);
        assert_eq!(resolved.evaluation_budget(), 96 + 96 * 15);

        solver.population_size = 0;
        solver.max_iterations = -3;
        let degenerate = Settings::from_solver(&solver, 1, 0);
        assert_eq!(degenerate.population, MINIMUM_POPULATION);
        assert_eq!(degenerate.generations, 0);
    }

    #[test]
    fn a_seeded_search_replays_exactly_and_a_different_seed_searches_elsewhere() {
        let bounds = [(0.0, 1.0)];
        let run_once = |seed| {
            run(
                &bounds,
                Some(&[0.7]),
                settings(seed),
                &CancelScope::attach(None),
                &mut batch,
            )
        };
        let first = run_once(42);
        let second = run_once(42);
        assert_eq!(
            first.winner, second.winner,
            "the same seed must replay exactly"
        );
        assert!(!first.cancelled && !second.cancelled);
        let other = run_once(43);
        assert!(other.winner.values[0].is_finite());
    }

    #[test]
    fn every_evaluated_candidate_stays_inside_the_bounds() {
        let bounds = [(2.0, 5.0), (-1.0, 1.0)];
        let outcome = run(
            &bounds,
            Some(&[900.0, -900.0]),
            settings(11),
            &CancelScope::attach(None),
            &mut |points: &[Vec<f64>]| {
                points
                    .iter()
                    .map(|values| ScoredPoint {
                        values: values.clone(),
                        cost: values[0],
                        valid: true,
                        constraint_violation: 0.0,
                        objectives: [values[0], values[0], values[0]],
                    })
                    .collect()
            },
        );
        for (value, &(lower, upper)) in outcome.winner.values.iter().zip(bounds.iter()) {
            assert!((lower..=upper).contains(value));
        }
    }
}
