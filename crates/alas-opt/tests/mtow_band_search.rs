// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The MTOW band mode through the two-stage product search: the takeoff mass
//! is a 17th search coordinate bounded by `[T (1 - p), T (1 + p)]`, the
//! registered aircraft at `m = T` is in the population, and the winner carries
//! the mass it was scored at.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::MtowSizing;
use alas_opt::{DesignOptimizer, OptimizationError};

mod support;
use support::nominal;

const PRESET: &str = "ATR72-600";

fn band_config() -> alas_config::AlasConfig {
    let mut config = support::reference_search_config(PRESET, 1, 24, 36);
    config.optimizer.objective.mtow_sizing = MtowSizing::MtowBand;
    config
}

#[test]
fn the_search_box_gains_the_mass_coordinate_and_the_start_carries_the_target() {
    let config = band_config();
    let (lower_kg, upper_kg) = config.mtow_band_limits_kg();
    let target_kg = config.mtow_band_target_kg();
    assert!((lower_kg - 0.9 * target_kg).abs() < 1e-9 && (upper_kg - 1.1 * target_kg).abs() < 1e-9);

    let optimizer = DesignOptimizer::new(config);
    let start = nominal(PRESET);
    let bounds = optimizer.resolved_bounds(None, Some(&start)).unwrap();
    assert_eq!(bounds.len(), 17);
    assert_eq!(bounds[16], (lower_kg, upper_kg));
    let canonical = alas_opt::configured_nominal_design(&optimizer.config).unwrap();
    assert_eq!(canonical.mtow_kg, Some(target_kg));
    assert!(canonical.same_geometry(&start));

    // Every other mode keeps the sixteen-coordinate box.
    let mut fixed = band_config();
    fixed.optimizer.objective.mtow_sizing = MtowSizing::FixedRequirement;
    let bounds = DesignOptimizer::new(fixed)
        .resolved_bounds(None, Some(&start))
        .unwrap();
    assert_eq!(bounds.len(), 16);
}

#[test]
fn a_band_search_scores_the_registered_aircraft_at_the_target_and_returns_a_mass_in_the_band() {
    let config = band_config();
    let (lower_kg, upper_kg) = config.mtow_band_limits_kg();
    let target_kg = config.mtow_band_target_kg();
    let start = nominal(PRESET);
    let mut optimizer = DesignOptimizer::new(config);
    let result = match optimizer.run(None, Some(&start), None) {
        Ok(result) => result,
        Err(OptimizationError::NoFeasibleDesign(evidence)) => {
            assert!(evidence.evaluated_candidates > 0);
            return;
        }
        Err(error) => panic!("{error}"),
    };
    let chosen_kg = result
        .best_design
        .mtow_kg
        .expect("the winner carries its mass");
    assert!(chosen_kg >= lower_kg - 1e-6 && chosen_kg <= upper_kg + 1e-6);
    // Every recorded candidate has a mass in the band, and the registered
    // aircraft at the target (the fixed-mode nominal) was evaluated.
    let mut seeded = false;
    for design in result.history.design_vectors.iter().flatten() {
        let mass_kg = design.mtow_kg.expect("every row carries a mass");
        assert!(mass_kg >= lower_kg - 1e-6 && mass_kg <= upper_kg + 1e-6);
        seeded |= mass_kg == target_kg && design.same_geometry(&start);
    }
    assert!(
        seeded,
        "the nominal at m = T is not in the evaluated population"
    );
    // The configuration the run leaves behind describes the chosen mass.
    assert_eq!(
        optimizer
            .config
            .at_band_mass(Some(chosen_kg))
            .requirements
            .mtow_kg,
        chosen_kg
    );
}
