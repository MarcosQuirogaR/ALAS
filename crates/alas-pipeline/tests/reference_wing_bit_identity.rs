// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Tests assert on values they construct here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]

//! The reference-wing convention and the nacelle model default to the
//! historical behaviour: with both unset, the product candidate evaluation
//! (geometry, VLM drag, mass, mission closure, constraint residuals) is
//! bit-identical to the tree before the options existed. The pinned hashes
//! were produced on that tree (777848d3) by this same test body.

use alas_config::{presets, AlasConfig, ReferenceWingMode};

/// FNV-1a over text; `f64` `Debug` round-trips, so equal hashes mean equal bits.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn physics_text(name: &str, trapezoid: bool) -> String {
    let preset = presets::get(name).unwrap();
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
    if trapezoid {
        config.geometry.wing.reference_wing.mode = ReferenceWingMode::Trapezoid;
    }
    let a = alas_opt::assess_product_candidate(&config, &preset.design_vector)
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    let residuals: Vec<_> = a
        .residuals
        .iter()
        .map(|r| (r.id, r.actual, r.limit))
        .collect();
    format!(
        "{:?}",
        (
            a.objective_value,
            a.sized.block_fuel_kg,
            a.sized.takeoff_mass_kg,
            a.sized.operating_empty_mass_kg,
            residuals
        )
    )
}

#[test]
fn unset_options_leave_the_physics_output_bit_identical() {
    // Re-pinned by lane defects-a (2026-10-13; the hash covers the whole
    // assessed candidate, so any numerical change moves it): single-pass
    // closures re-freeze their mission plan at the closed mass (A320
    // 10911607668874415680 -> 16195648067557271306, B747-400
    // 4192227757127408331 -> 4132062552483617869), and the seat packer keeps
    // the counted rows (A320 seats 150, was 149).
    // Re-pinned again by the integration re-pin (2026-10-14): kappa/m calibration
    // (default induced-drag factor 0.43 -> 0.25, margin 0.97 -> 1.01) and the
    // mass/headroom merge move every default-config output (A320 16195648067557271306
    // -> 9241216442103265343, B747-400 4132062552483617869 -> 7549613672140328539);
    // attrib-integration-2026-10-14 item 5: either merge alone keeps it red.
    // Re-pinned by lane deck-takeoff (2026-10-14, by probe): the take-off thrust
    // lapse is Jenkinson's take-off charts instead of the Bartel-Young
    // correlation (V2 thrust ratio A320 0.750 -> 0.754, B747-400 0.766 -> 0.783),
    // which moves the take-off ground-roll burn, the OEI/approach/landing climb
    // rows and so the whole assessed candidate (A320 9241216442103265343 ->
    // 18366408510121255721, B747-400 7549613672140328539 ->
    // 16046562567036580943). Disabling only the new lapse in place (a temporary
    // switch, since removed) reproduces the old hashes exactly: the change is
    // the lapse alone.
    // Re-pinned by the final re-pin (2026-10-14, by probe, tree 12a0202b): the
    // merged lanes since deck-takeoff (cruise thrust at the planned levels and
    // planner ceiling, fuel chain with installation losses and printed 747 wing
    // thickness, FE wing load cases and Torenbeek non-optimum wing weight, CG)
    // move the whole assessed candidate (A320 18366408510121255721 ->
    // 2125890983014824348, B747-400 16046562567036580943 ->
    // 16787373391136039566). Value from the test's own eprintln.
    // Re-pinned by lane route-ladder-ceiling (2026-10-15, by probe, value from the
    // test's own eprintln): the first level of a climb ladder is held to the
    // planner's flyability rule (B747-400 7480989053690636170 ->
    // 9552556230291174144; A320 unchanged: its configured level qualifies).
    let expected: [(&str, u64); 2] = [
        ("A320-200", 2_125_890_983_014_824_348),
        ("B747-400", 9_552_556_230_291_174_144),
    ];
    let got = expected.map(|(name, _)| (name, fnv1a(&physics_text(name, false))));
    eprintln!("{got:?}");
    assert_eq!(got, expected);
}

#[test]
fn trapezoid_mode_changes_the_reference_area_and_hence_the_coefficients() {
    let drawn = physics_text("B747-400", false);
    let reference = physics_text("B747-400", true);
    assert_ne!(drawn, reference);
}
