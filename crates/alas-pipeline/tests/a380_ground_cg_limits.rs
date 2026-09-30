// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The registered A380-800 flown takeoff CG against the model's ground aft
//! limit, on the product basis (full analysis and the item-ledger CG gate the
//! feasibility report uses). Frame: % MAC from LEMAC.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::AlasConfig;
use alas_pipeline::{assess_physical_feasibility, FullAnalysis};

/// KNOWN DEFECT: the flown takeoff CG is about 42.7 % MAC, aft of the 39.3 %
/// ground aft limit. The trim-tank fill order is fixed (trim fuel fills last
/// and burns first, `alas_mass::tanks::order`), but the zero-fuel CG is
/// 37.2 % MAC and the landing fuel still sits in the outer cells because the
/// feed and transfer cells are lumped in `alas_config::preset_fuel_tanks`.
/// The physical requirement is kept: the takeoff CG must be inside the limits.
#[test]
#[ignore = "known defect: A380 takeoff CG is aft of the ground limit because feed and transfer fuel cells are lumped"]
fn the_a380_flown_takeoff_cg_is_inside_the_ground_aft_limit() {
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A380-800" })).unwrap();
    let design = alas_config::presets::get("A380-800").unwrap().design_vector;
    let report = FullAnalysis::new(config.clone())
        .run(&design, true)
        .unwrap();
    let physical = assess_physical_feasibility(&config, &design, &report, None);
    let takeoff_cg = physical
        .mass_balance
        .as_ref()
        .expect("mass statement")
        .states
        .iter()
        .find(|state| state.label == "flown takeoff")
        .expect("flown takeoff state")
        .cg_pct_mac;
    let ground_aft = physical
        .model_cg
        .as_ref()
        .expect("model CG assessment")
        .ground_aft_limit_pct_mac;
    assert!(
        takeoff_cg <= ground_aft,
        "takeoff CG {takeoff_cg:.2} % MAC is aft of the ground limit {ground_aft:.2} % MAC"
    );
}
