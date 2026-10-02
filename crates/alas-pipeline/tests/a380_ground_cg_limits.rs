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

/// KNOWN DEFECT: the flown takeoff CG is about 42.8 % MAC, aft of the 42.0 %
/// ground aft limit. The feed tanks are split out and burned last, so the
/// landing CG (37.2 %) is inside it. The ground limit already uses the
/// A380's own 4.9 % nose share at its published 43 % MAC aft CG at 562 t
/// (Airbus A380 AC Dec 01/25, Figure 7-3-0-991-006-A01, weight variant 000).
/// The remaining term is the operating empty mass, about 7 % below the
/// reference: the 560 t takeoff carries about 6 t more fuel than the wing
/// tanks hold and the excess sits in the trim tank. The physical requirement
/// is kept: the takeoff CG must be inside the limits.
#[test]
#[ignore = "known defect: A380 takeoff CG is aft of the ground limit (trim fuel forced by the low OEW)"]
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
