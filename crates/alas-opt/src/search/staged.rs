// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The staged search strategy: how the run's budget is split between a broad
//! low-resolution scan, the full-fidelity verification of its finalists, and
//! the coupled MADS search.
//!
//! # Why the scan is separate from the search
//!
//! One coupled candidate evaluation is a geometry build, two mass passes with
//! structural feedback, a vortex-lattice trim, a segment-integrated mission
//! and a dispatch fixed point. Spending that on a design vector drawn at
//! random from a sixteen-dimensional box is mostly wasted: the box is nearly
//! all aircraft that neither balance nor close their weight budget. The scan
//! therefore ranks a broad deterministic sample on a *reduced* model and only
//! nominates a handful of starting points, which are re-evaluated by the real
//! objective before any of them can be called a candidate.
//!
//! # What "reduced" means here, exactly
//!
//! [`screening_config`] differs from the run's configuration in three declared
//! ways and in no others:
//!
//! - `analysis.chordwise_resolution` drops to [`SCAN_CHORDWISE_RESOLUTION`]
//!   panels per strip. The product default is 8, which that field's own help
//!   text records as ranking candidates identically to a converged mesh; at 2
//!   the camber line is sampled coarsely, so the trimmed attitude and L/D
//!   carry a bias of order one degree and several percent and the four
//!   Hicks-Henne bump variables lose most of their effect.
//! - `optimizer.objective.sizing_max_iterations` is capped at
//!   [`SCAN_SIZING_PASSES`] outer passes and `sizing_tolerance_kg` is loosened
//!   to [`SCAN_SIZING_TOLERANCE_KG`], so the takeoff-mass fixed point closes
//!   to about a tonne instead of to the configured tolerance.
//! - nothing else. The design space, requirements, mission, engine, cabin,
//!   structures, constraint policies and objective kind stay the run's own, so
//!   a scan candidate is rejected for the same physical reasons a search
//!   candidate is.
//!
//! Those biases are why a scan result is never promoted: it orders candidates,
//! and the full objective decides. A scan that finds nothing better than the
//! caller's nominal design leaves the search starting where it used to.

use std::time::Duration;

use alas_config::{AlasConfig, SolverSettings};

use super::directions::{from_normalized, mix_seed, permutation};

/// Chordwise vortex-lattice panels per strip during the broad scan.
pub(crate) const SCAN_CHORDWISE_RESOLUTION: i64 = 2;
/// Outer sizing passes allowed during the broad scan.
pub(crate) const SCAN_SIZING_PASSES: i64 = 3;
/// Takeoff-mass closure tolerance during the broad scan, kg.
pub(crate) const SCAN_SIZING_TOLERANCE_KG: f64 = 1_000.0;

/// The MADS mesh starts at 0.25 and halves on every failed poll, so reaching
/// the 1e-2 convergence spacing takes five consecutive failed polls plus the
/// successful polls before them. A configured generation count below this
/// would make convergence unreachable by construction, which is exactly the
/// "ran out of iterations" outcome the staged strategy exists to remove.
const MINIMUM_POLL_ITERATIONS: usize = 60;
/// Wall-clock safety limit for one search. A watchdog exit is reported as not
/// converged; it exists so a pathological configuration cannot run unbounded,
/// not as a stopping criterion.
const WATCHDOG_SECONDS: u64 = 900;

/// How the staged strategy is sized for one run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Settings {
    /// Deterministic low-resolution samples drawn over the envelope.
    pub scan_points: usize,
    /// Scan finalists re-evaluated by the full objective.
    pub scan_finalists: usize,
    /// Coupled analyses the MADS stage may execute.
    pub max_evaluations: usize,
    /// Lower bound on poll iterations, so a small configured generation count
    /// cannot stop the search before the mesh can contract to the convergence
    /// spacing.
    pub minimum_poll_iterations: usize,
    /// Normalized mesh spacing that counts as converged.
    pub convergence_mesh_size: f64,
    /// Relative objective improvement convergence requires.
    pub minimum_relative_improvement: f64,
    /// Points evaluated per opportunistic poll block.
    pub poll_block_size: usize,
    /// Worker threads used inside one block.
    pub workers: usize,
    /// Whether an unsuccessful poll only has to exhaust a minimal positive
    /// basis before the mesh contracts.
    pub minimal_positive_basis: bool,
    /// Wall-clock safety limit for the whole search.
    pub watchdog: Option<Duration>,
    /// Seed for the deterministic scan sample.
    pub seed: u64,
}

impl Settings {
    /// Derive the staged settings from the saved solver group.
    ///
    /// `population_size` and `max_iterations` keep their saved meaning as the
    /// run's *budget*: their product is how many coupled analyses the user is
    /// willing to pay for. How that budget is spent is the search's decision,
    /// not a per-generation population as it was under differential
    /// evolution.
    pub(crate) fn from_solver(solver: &SolverSettings, dimension: usize) -> Self {
        let population = (solver.population_size.max(1) as usize).saturating_mul(dimension.max(1));
        let generations = solver.max_iterations.max(1) as usize;
        let budget = population
            .saturating_mul(generations.saturating_add(1))
            .max(1);
        Self {
            // Broad enough to cover the envelope without consuming the
            // coupled budget: two low-resolution samples per design variable,
            // bounded so a large space stays affordable.
            scan_points: (2 * dimension).clamp(8, 64),
            scan_finalists: 3,
            max_evaluations: budget,
            minimum_poll_iterations: MINIMUM_POLL_ITERATIONS,
            convergence_mesh_size: 1.0e-2,
            minimum_relative_improvement: 1.0e-4,
            // Sixteen points per block keeps a failed poll of a 48-direction
            // spanning set to three synchronisation points while staying a
            // fixed, hardware-independent number.
            poll_block_size: 16,
            workers: solver.resolved_workers(),
            // Above a handful of variables the maximal spanning set's extra
            // analyses per failed poll are what decides the run's wall clock,
            // and the failed polls are the ones that contract the mesh.
            minimal_positive_basis: dimension > 4,
            watchdog: Some(Duration::from_secs(WATCHDOG_SECONDS)),
            seed: solver.seed.map_or(0, |value| value as u64),
        }
    }
}

/// Build the reduced configuration the scan ranks candidates on.
pub(crate) fn screening_config(config: &AlasConfig) -> AlasConfig {
    let mut screening = config.clone();
    let analysis = &mut screening.analysis;
    analysis.chordwise_resolution = analysis
        .chordwise_resolution
        .min(SCAN_CHORDWISE_RESOLUTION)
        .max(1);
    let objective = &mut screening.optimizer.objective;
    objective.sizing_max_iterations = objective
        .sizing_max_iterations
        .min(SCAN_SIZING_PASSES)
        .max(1);
    objective.sizing_tolerance_kg = objective.sizing_tolerance_kg.max(SCAN_SIZING_TOLERANCE_KG);
    screening
}

/// A deterministic Latin-hypercube sample of the envelope, in physical units.
///
/// Fixed coordinates (a zero-width bound: the cabin-derived fuselage length,
/// or a preset variable the envelope locks) collapse to their single value,
/// so the scan never proposes a design the envelope forbids.
pub(crate) fn scan_sample(bounds: &[(f64, f64)], count: usize, seed: u64) -> Vec<Vec<f64>> {
    let dimension = bounds.len();
    if dimension == 0 || count == 0 {
        return Vec::new();
    }
    let permutations: Vec<Vec<usize>> = (0..dimension)
        .map(|index| permutation(count, mix_seed(seed, index as u64 + 1)))
        .collect();
    (0..count)
        .map(|sample| {
            let normalized: Vec<f64> = (0..dimension)
                .map(|index| (permutations[index][sample] as f64 + 0.5) / count as f64)
                .collect();
            from_normalized(&normalized, bounds)
        })
        .collect()
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_screening_model_only_loosens_the_three_declared_settings() {
        let config = AlasConfig::default();
        let screening = screening_config(&config);

        assert_eq!(screening.analysis.chordwise_resolution, 2);
        assert_eq!(screening.optimizer.objective.sizing_max_iterations, 3);
        assert!(screening.optimizer.objective.sizing_tolerance_kg >= SCAN_SIZING_TOLERANCE_KG);

        // Everything a candidate is judged by is unchanged, so a scan
        // finalist is rejected for the same physical reasons a search
        // candidate is.
        assert_eq!(screening.requirements, config.requirements);
        assert_eq!(
            screening.optimizer.design_space,
            config.optimizer.design_space
        );
        assert_eq!(screening.optimizer.weights, config.optimizer.weights);
        assert_eq!(
            screening.optimizer.objective.kind,
            config.optimizer.objective.kind
        );
        assert_eq!(
            screening.optimizer.objective.mass_constraints,
            config.optimizer.objective.mass_constraints
        );
        assert_eq!(screening.geometry, config.geometry);
        assert_eq!(screening.cabin, config.cabin);
        assert_eq!(screening.mission, config.mission);
        assert_eq!(screening.structures, config.structures);
    }

    #[test]
    fn a_reduced_run_is_never_made_finer_than_the_user_asked_for() {
        // A user who already selected a two-panel mesh or a three-pass
        // closure must not have the scan silently raise the fidelity (and the
        // cost) of their own configuration.
        let mut config = AlasConfig::default();
        config.analysis.chordwise_resolution = 1;
        config.optimizer.objective.sizing_max_iterations = 2;
        config.optimizer.objective.sizing_tolerance_kg = 5_000.0;
        let screening = screening_config(&config);
        assert_eq!(screening.analysis.chordwise_resolution, 1);
        assert_eq!(screening.optimizer.objective.sizing_max_iterations, 2);
        assert_eq!(screening.optimizer.objective.sizing_tolerance_kg, 5_000.0);
    }

    #[test]
    fn the_scan_sample_is_reproducible_and_inside_the_envelope() {
        let bounds = vec![(60.0, 80.0), (12.0, 19.0), (34.0, 34.0)];
        let first = scan_sample(&bounds, 12, 7);
        let second = scan_sample(&bounds, 12, 7);
        assert_eq!(first, second);
        assert_eq!(first.len(), 12);
        for point in &first {
            for (value, &(lower, upper)) in point.iter().zip(&bounds) {
                assert!(
                    *value >= lower && *value <= upper,
                    "{value} in [{lower}, {upper}]"
                );
            }
            // A zero-width bound is a fixed coordinate, never perturbed.
            assert_eq!(point[2], 34.0);
        }
        let different = scan_sample(&bounds, 12, 8);
        assert_ne!(first, different);
    }

    #[test]
    fn a_stratified_sample_covers_every_variable_end_to_end() {
        // One sample per stratum in each coordinate: the point of the scan is
        // that a variable is examined across its whole envelope, not around
        // the nominal only.
        let bounds = vec![(0.0, 1.0), (0.0, 1.0)];
        let sample = scan_sample(&bounds, 20, 3);
        for index in 0..2 {
            let mut values: Vec<f64> = sample.iter().map(|point| point[index]).collect();
            values.sort_by(f64::total_cmp);
            assert!(values[0] < 0.05, "{}", values[0]);
            assert!(values[19] > 0.95, "{}", values[19]);
        }
    }

    #[test]
    fn the_budget_is_the_saved_solver_product_and_iterations_have_a_floor() {
        let solver = SolverSettings::default();
        let settings = Settings::from_solver(&solver, 16);
        assert_eq!(settings.max_evaluations, 6 * 16 * (15 + 1));
        assert_eq!(settings.minimum_poll_iterations, MINIMUM_POLL_ITERATIONS);
        assert!(settings.minimal_positive_basis);
        assert_eq!(settings.scan_points, 32);
        assert!(settings.watchdog.is_some());
    }
}
