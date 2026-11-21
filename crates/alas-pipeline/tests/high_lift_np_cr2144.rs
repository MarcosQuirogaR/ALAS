// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The high-lift neutral-point condition of the registered B-747-400 against
//! the one flight-derivative set that measured it: NASA CR-2144 (Heffley and
//! Jewell 1972), B-747 at 636,600 lb, CG 0.25 c, Tables IX-1 and IX-2:
//!
//! - landing (flaps down, 131 KTAS, sea level): CLa 5.67/rad, Cma -1.45/rad,
//!   neutral point 0.25 - Cma/CLa = 0.506 MAC;
//! - power approach: CLa 5.70, Cma -1.26, neutral point 0.471 MAC;
//! - clean flexible trend at the same dynamic pressures, 0.511 - 0.00396 q:
//!   0.500 and 0.494 MAC (scatter 0.016), so the measured flap effect is
//!   +0.006 and -0.023 MAC;
//! - CLa ratio to the clean flexible curve of p. 220 (4.8-4.9/rad at M 0.2):
//!   1.16-1.18.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig, DesignMode};
use alas_pipeline::FullAnalysis;

#[test]
fn the_747_high_lift_condition_reproduces_the_measured_slope_and_bounds_the_np_shift() {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "B747-400" })).unwrap();
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    config.mission.enabled = false;
    config.mses.enabled = false;
    config.structures.enabled = false;
    // The 747 carries only Krueger flaps at the leading edge (Rudolph, NASA
    // CR-4746, PDF p. 61), so the measured configuration has no slat term.
    assert_eq!(config.performance.high_lift_chord_extension.slat, 0.0);
    let design = presets::get("B747-400").unwrap().design_vector;
    let report = FullAnalysis::new(config).run(&design, true).unwrap();
    let np = report
        .neutral_point_conditions
        .as_ref()
        .expect("the conditions are produced");
    // Lift-slope rise with the deployed flaps: measured 1.16-1.18 (the clean
    // extrapolation is read off a plot, +-0.02).
    assert!(
        (1.14..=1.21).contains(&np.high_lift_slope_scale),
        "slope scale {}",
        np.high_lift_slope_scale
    );
    // Neutral-point shift in MAC. Measured 0 to -2 points (CR-2144 scatter 1.6).
    // The model's -2.3 points with the tail at the geometry's wing-wake ratio
    // (1.0 for the 747); -1.2 points with the former 0.78 default (the band of
    // -0.075 to -0.03 that this test carried before was already outside the
    // lattice's NP after the camber collocation fix). It is the flap
    // increment's own term, -d/(1+d) (x_NP - x_inc), acting through the lever
    // from the model NP (a rigid-NP gap above NASA's 0.50) to the added flap
    // lift at 0.40 MAC. The band is the measured range widened by the
    // CR-2144 scatter. With the wing-body lift interference K_I (Torenbeek 1982
    // E-33) the model NP is 0.686 (was 0.700), the lever to the flap lift
    // shortens and the shift is -0.9 points, inside the measured 0 to -2.
    let mac = report.airplane.c_ref;
    let shift = (np.high_lift - np.clean_low_speed) / mac;
    assert!(
        (-0.04..=-0.005).contains(&shift),
        "high-lift NP shift {shift} MAC"
    );
}
