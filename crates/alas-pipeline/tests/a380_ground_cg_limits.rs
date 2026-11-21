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

/// The flown takeoff CG (560 t) must be inside the ground aft limit. The limit
/// uses the A380 own published 4.9 % nose share at its 43 % MAC aft CG at 562 t
/// (Airbus A380 AC Dec 01/25, Figure 7-3-0-991-006-A01, weight variant 000).
///
/// History: this test was ignored while the operating empty mass sat about
/// 7 % below the reference, which forced about 6 t of the 560 t takeoff fuel
/// into the trim tank (aft of the wing) and put the CG at 42.8 % MAC against
/// a 42.0 % limit. On the current mass model the operating empty mass is
/// 267.9 t, the takeoff fuel (236.1 t) fits in the wing tanks (the trim tank
/// fills last and empties first), the takeoff CG is 41.0 % MAC and the
/// limit is 43.0 % MAC. Two things moved since the ignore:
/// the CG (42.8 to 41.0, mass model and trim-tank fill) and the limit itself
/// (42.0 to 43.0, the per-state ground limit reading the published 4.9 % nose
/// share). It passes on either limit; the requirement is unchanged.
#[test]
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
