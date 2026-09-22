// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Stage A of the product search: a broad low-resolution scan that seeds the
//! L-SHADE population's starting point.
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
//! objective before any of them can be called a candidate, and even then only
//! seed the search's population; the search itself decides the winner (see
//! `search_methods::lshade_de`).
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

use alas_config::{AlasConfig, SolverSettings};

/// Chordwise vortex-lattice panels per strip during the broad scan.
pub(crate) const SCAN_CHORDWISE_RESOLUTION: i64 = 2;
/// Outer sizing passes allowed during the broad scan.
pub(crate) const SCAN_SIZING_PASSES: i64 = 3;
/// Takeoff-mass closure tolerance during the broad scan, kg.
pub(crate) const SCAN_SIZING_TOLERANCE_KG: f64 = 1_000.0;

/// How the staged scan is sized for one run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Settings {
    /// Deterministic low-resolution samples drawn over the envelope.
    pub scan_points: usize,
    /// Scan finalists re-evaluated by the full objective.
    pub scan_finalists: usize,
    /// Points evaluated per scan block, so the cancellation flag is checked
    /// at a bounded interval rather than only once for the whole sample.
    pub scan_block_size: usize,
    /// Worker threads used inside one evaluation block.
    pub workers: usize,
    /// Seed for the deterministic scan sample.
    pub seed: u64,
}

impl Settings {
    /// Derive the staged scan settings from the saved solver group.
    pub(crate) fn from_solver(solver: &SolverSettings, dimension: usize) -> Self {
        Self {
            // Broad enough to cover the envelope without consuming the
            // coupled budget: two low-resolution samples per design variable,
            // bounded so a large space stays affordable.
            scan_points: (2 * dimension).clamp(8, 64),
            scan_finalists: 3,
            // Sixteen points per block keeps a cancellation check at a
            // bounded interval while staying a fixed, hardware-independent
            // number.
            scan_block_size: 16,
            workers: solver.resolved_workers(),
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
        .clamp(1, SCAN_CHORDWISE_RESOLUTION);
    let objective = &mut screening.optimizer.objective;
    objective.sizing_max_iterations = objective.sizing_max_iterations.clamp(1, SCAN_SIZING_PASSES);
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

fn from_normalized(values: &[f64], bounds: &[(f64, f64)]) -> Vec<f64> {
    values
        .iter()
        .zip(bounds)
        .map(|(&value, &(lower, upper))| {
            if upper == lower {
                lower
            } else {
                lower + value.clamp(0.0, 1.0) * (upper - lower)
            }
        })
        .collect()
}

fn permutation(length: usize, mut state: u64) -> Vec<usize> {
    let mut values = (0..length).collect::<Vec<_>>();
    for index in (1..length).rev() {
        let swap = (next_u64(&mut state) % (index as u64 + 1)) as usize;
        values.swap(index, swap);
    }
    values
}

fn mix_seed(seed: u64, stream: u64) -> u64 {
    let mut state = seed.wrapping_add(stream.wrapping_mul(0x9e37_79b9_7f4a_7c15));
    next_u64(&mut state)
}

fn next_u64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
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
    fn the_settings_are_a_pure_function_of_the_solver_and_dimension() {
        let solver = SolverSettings::default();
        let settings = Settings::from_solver(&solver, 16);
        assert_eq!(settings.scan_points, 32);
        assert_eq!(settings.scan_finalists, 3);
        assert_eq!(settings.scan_block_size, 16);
        assert!(settings.workers >= 1);
    }
}
