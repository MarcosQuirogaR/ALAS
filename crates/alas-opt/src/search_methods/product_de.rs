// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The product Differential Evolution driver and the method-name dispatch.
//!
//! `optimizer.solver.method` selects a search kernel here and nowhere else,
//! so the name a configuration carries is the algorithm that runs and the
//! algorithm the result reports. Design-vector units, frames, signs and
//! validity domains are specified in `docs/optimizer-design-vector.md`; this
//! module treats the vector as opaque normalized-to-bounds coordinates and
//! never interprets a component, which is what lets the search algorithm
//! change without changing the physics being evaluated.
//!
//! Reproducibility: the search is a pure function of the resolved seed, the
//! bounds, the starting point and the evaluator. Candidates are evaluated one
//! at a time in a fixed order, so a run does not depend on `solver.workers`
//! and replays exactly from its seed.

use alas_config::SolverSettings;

use crate::cancellation::CancelScope;

use super::{constrained_de, EvaluatePoint, MethodOutcome};

/// The smallest population that still supports best/1/bin: a best, a target
/// and two distinct difference vectors, with margin.
const MINIMUM_POPULATION: usize = 6;

/// Which kernel a configured method name selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kernel {
    /// The gradient-based driver in `sqp_search`.
    Sqp,
    /// Feasibility-first Differential Evolution, this module.
    DifferentialEvolution,
    /// Bounded mesh adaptive direct search, `search::mads`.
    Mads,
}

/// The kernel `method` runs.
///
/// `nsga2`, `turbo_1` and `cma_es` remain loadable so saved configurations
/// keep opening, but no population kernel stands behind them: they run mesh
/// adaptive direct search and the result says `mads`, so a run is never
/// reported as an algorithm that did not execute.
pub(crate) fn kernel_for(method: &str) -> Kernel {
    match method {
        "sqp" => Kernel::Sqp,
        "differential_evolution" | "feasibility_first_de" => Kernel::DifferentialEvolution,
        _ => Kernel::Mads,
    }
}

impl Kernel {
    /// The `method` string the optimization result carries.
    pub(crate) fn reported_name(self) -> &'static str {
        match self {
            Self::Sqp => "sqp",
            Self::DifferentialEvolution => "feasibility_first_de",
            Self::Mads => "mads",
        }
    }

    /// The `strategy` string the optimization result carries.
    pub(crate) fn reported_strategy(self) -> &'static str {
        match self {
            Self::Sqp => "gradient_projection",
            Self::DifferentialEvolution => "best1bin_feasibility_first",
            Self::Mads => "progressive_barrier",
        }
    }
}

/// Resolved Differential Evolution settings for one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Settings {
    /// Candidates per generation.
    pub(crate) population: usize,
    /// Generations after the initial population.
    pub(crate) generations: usize,
    /// The seed the whole search replays from.
    pub(crate) seed: u64,
}

impl Settings {
    /// Resolve the configured solver settings for `dimension` design
    /// variables against an already-resolved `seed`.
    ///
    /// `solver.population_size` is a multiplier on the number of design
    /// variables, matching the setting's own label and the frozen SciPy
    /// semantics the legacy driver was ported from, so the sixteen-variable
    /// product space at the default multiplier of six evaluates ninety-six
    /// candidates per generation. `solver.max_iterations` is the generation
    /// count; zero is legitimate and records the initial population only.
    pub(crate) fn from_solver(solver: &SolverSettings, dimension: usize, seed: u64) -> Self {
        let multiplier = usize::try_from(solver.population_size.max(1)).unwrap_or(1);
        let population = multiplier
            .saturating_mul(dimension.max(1))
            .max(MINIMUM_POPULATION);
        let generations = usize::try_from(solver.max_iterations.max(0)).unwrap_or(0);
        Self {
            population,
            generations,
            seed,
        }
    }

    /// Analyses this configuration will request, worst case: the initial
    /// population plus one trial per candidate per generation.
    pub(crate) fn evaluation_budget(self) -> usize {
        self.population
            .saturating_add(self.population.saturating_mul(self.generations))
    }
}

/// Run the product Differential Evolution search.
///
/// `initial_design` is repaired into `bounds` rather than discarded, so a
/// starting point that a bound change has left outside the envelope still
/// seeds the population instead of being silently dropped. Every candidate
/// the evaluator sees lies inside `bounds`.
///
/// `scope` carries the cooperative cancellation flag and its telemetry. The
/// flag is observed once per candidate evaluation, so the stopping bound is
/// one coupled analysis; the returned `bool` says whether the run stopped that
/// way instead of exhausting `settings.generations` (see
/// `constrained_de::run_feasibility_first_de`).
pub(crate) fn run(
    bounds: &[(f64, f64)],
    initial_design: Option<&[f64]>,
    settings: Settings,
    scope: &CancelScope<'_>,
    evaluate: &mut EvaluatePoint<'_>,
) -> (MethodOutcome, bool) {
    constrained_de::run_feasibility_first_de(
        bounds,
        settings.population,
        settings.generations,
        settings.seed,
        initial_design,
        scope,
        evaluate,
    )
}

/// The score of a candidate whose evaluation produced nothing.
///
/// Worst possible on every ordering key, so it can never displace a
/// candidate that was actually analysed, and never silently becomes a winner.
pub(crate) fn unevaluated(values: &[f64]) -> super::ScoredPoint {
    super::ScoredPoint {
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
    use std::cell::RefCell;

    /// A one-dimensional objective whose feasible region is `x >= 0.4` and
    /// whose cost falls towards `x = 0`, so the cheapest point in the box is
    /// infeasible and feasibility-first ordering is observable.
    fn scored(values: &[f64]) -> super::super::ScoredPoint {
        let x = values[0];
        super::super::ScoredPoint {
            values: values.to_vec(),
            cost: x * x,
            valid: x >= 0.4,
            constraint_violation: (0.4 - x).max(0.0),
            objectives: [x * x, x, x],
        }
    }

    fn settings(seed: u64) -> Settings {
        Settings {
            population: 8,
            generations: 6,
            seed,
        }
    }

    #[test]
    fn a_method_name_selects_the_kernel_it_names() {
        assert_eq!(kernel_for("sqp"), Kernel::Sqp);
        assert_eq!(
            kernel_for("differential_evolution"),
            Kernel::DifferentialEvolution
        );
        assert_eq!(
            kernel_for("feasibility_first_de"),
            Kernel::DifferentialEvolution
        );
        for unwired in ["nsga2", "turbo_1", "cma_es", "something_unknown"] {
            assert_eq!(kernel_for(unwired), Kernel::Mads, "{unwired}");
        }
        assert_eq!(
            Kernel::DifferentialEvolution.reported_name(),
            "feasibility_first_de"
        );
        assert_eq!(Kernel::Mads.reported_name(), "mads");
        assert_eq!(Kernel::Sqp.reported_name(), "sqp");
    }

    #[test]
    fn population_size_is_a_multiplier_on_the_design_variables() {
        let mut solver = SolverSettings::default();
        solver.population_size = 6;
        solver.max_iterations = 15;
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
        let run = |seed| {
            let mut evaluate = scored;
            run(
                &bounds,
                Some(&[0.7]),
                settings(seed),
                &CancelScope::attach(None),
                &mut evaluate,
            )
        };
        let (first, first_cancelled) = run(42);
        let (second, second_cancelled) = run(42);
        assert_eq!(first, second, "the same seed must replay exactly");
        assert!(!first_cancelled && !second_cancelled);
        let (other, _) = run(43);
        assert!(other.winner.values[0].is_finite());
    }

    #[test]
    fn a_cancellation_signal_reaches_the_underlying_generation_loop() {
        let bounds = [(0.0, 1.0)];
        let cancel = std::sync::atomic::AtomicBool::new(true);
        let mut evaluate = scored;
        let (outcome, cancelled) = run(
            &bounds,
            Some(&[0.7]),
            Settings {
                population: 8,
                generations: 40,
                seed: 5,
            },
            &CancelScope::attach(Some(&cancel)),
            &mut evaluate,
        );
        assert!(
            cancelled,
            "an already-set flag must be observed before the first evaluation"
        );
        assert!(outcome.winner.values[0].is_finite());
    }

    #[test]
    fn every_evaluated_candidate_stays_inside_the_bounds() {
        let bounds = [(2.0, 5.0), (-1.0, 1.0)];
        let seen = RefCell::new(Vec::new());
        let mut evaluate = |values: &[f64]| {
            seen.borrow_mut().push(values.to_vec());
            let x = values[0];
            super::super::ScoredPoint {
                values: values.to_vec(),
                cost: x,
                valid: true,
                constraint_violation: 0.0,
                objectives: [x, x, x],
            }
        };
        // A starting point far outside the box, which repair must bring in
        // rather than the search either dropping it or carrying it.
        let (outcome, _) = run(
            &bounds,
            Some(&[900.0, -900.0]),
            settings(11),
            &CancelScope::attach(None),
            &mut evaluate,
        );

        let seen = seen.into_inner();
        assert!(seen.len() >= settings(11).population);
        for candidate in &seen {
            for (value, &(lower, upper)) in candidate.iter().zip(bounds.iter()) {
                assert!(
                    (lower..=upper).contains(value),
                    "{value} left [{lower}, {upper}]"
                );
            }
        }
        for (value, &(lower, upper)) in outcome.winner.values.iter().zip(bounds.iter()) {
            assert!((lower..=upper).contains(value));
        }
    }

    #[test]
    fn a_feasible_winner_is_preferred_to_a_cheaper_infeasible_one() {
        let bounds = [(0.0, 1.0)];
        let mut evaluate = scored;
        let (outcome, _) = run(
            &bounds,
            None,
            settings(5),
            &CancelScope::attach(None),
            &mut evaluate,
        );
        assert!(
            outcome.winner.valid,
            "a feasible design exists in [0, 1] and must win: {:?}",
            outcome.winner
        );
        assert_eq!(outcome.winner.constraint_violation, 0.0);
        // Cost falls towards x = 0, which is infeasible, so the winner must
        // sit at the feasible boundary rather than at the cheaper point.
        assert!(outcome.winner.values[0] >= 0.4);
    }

    #[test]
    fn with_no_feasible_design_the_least_violating_candidate_wins() {
        let bounds = [(0.0, 0.3)];
        let mut evaluate = scored;
        let (outcome, _) = run(
            &bounds,
            None,
            settings(9),
            &CancelScope::attach(None),
            &mut evaluate,
        );
        assert!(!outcome.winner.valid, "no point of [0, 0.3] is feasible");
        assert!(
            outcome.winner.constraint_violation <= 0.4 - 0.29,
            "the winner must be the least violating candidate: {:?}",
            outcome.winner
        );
    }
}
