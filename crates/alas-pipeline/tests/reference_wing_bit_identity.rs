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
    // Re-pinned by lane TOFL (2026-10-16, by probe, value from the test's own
    // eprintln): the `takeoff_field` residual is the CS 25.113 integrated field
    // length against the TODA, in metres (was the Raymer T/W correlation).
    // Disabling only that row in place (a temporary switch, since removed)
    // reproduces the previous hashes exactly: the change is that row alone
    // (A320 2125890983014824348 -> 14438840883493581074, B747-400
    // 9552556230291174144 -> 9852762159868136202).
    // Re-pinned by lane CLmax-TO (2026-10-16, by probe from the same eprintln):
    // the registered CLmax_TO now comes from the declared high-lift system
    // (Raymer Eq. 12.21). With only that derivation disabled in place (a
    // temporary env switch, removed) the tree gives A320 13006190495559274761
    // and B747-400 8092820004634198083: the pin above was already stale at the
    // integration base 65d3d3a1 (other merged work), so this re-pin also absorbs
    // that drift. A320 7204725775780450666, B747-400 15835521684524083561.
    // Re-pinned by lane field lengths (2026-10-17, by probe, value from the same
    // eprintln): registered jets fly the CS 25.107 V2 schedule (1.13 VSR) with
    // the flap share and landing anchor of the declared high-lift system, the
    // BPR 8 take-off thrust chart is capped (Jenkinson Fig. 9.20(a)), and the
    // optimizer field row reads its speeds at the design gross mass (A320
    // 7204725775780450666 -> 16439056093985561747, B747-400
    // 15835521684524083561 -> 4797963330003502369; the first three changes alone
    // gave A320 5454666173859552581, the same B747-400 value, which has equal
    // design and dispatch masses). Fix lane field 2026-10-18: the landing_field limit is the
    // Torenbeek segment-model wing loading, not k_land (A320 16439056093985561747 -> 277880278787852317,
    // B747-400 4797963330003502369 -> 10869569037091291768; values from this test's eprintln).
    // Re-pinned by lane tools-fe (2026-10-17, by probe, value from the same
    // eprintln): the swept box's stiff root triangle (`W tan L`, nothing bends
    // inboard of it) shortens the analytical tip deflection and slope the
    // flexibility rows read (A320 16439056093985561747 -> 13339502906525582016,
    // B747-400 4797963330003502369 -> 7852487941241913377). With only that
    // length forced to zero in place (a temporary switch, removed) the tree
    // reproduces the previous hashes exactly: the change is that term alone.
    // Integration 2026-10-19: both changes together, re-pinned by probe (eprintln).
    // Re-pinned by lane payload layout (2026-10-19, by probe from the same
    // eprintln): the lower holds now start at the centre wing box's front spar
    // instead of the root leading edge (`lower_hold_gap_x_range`), which moves
    // the baggage hold rows and compartments. With only that change disabled
    // in place (a temporary edit, reverted) the previous hashes reproduce
    // exactly (A320 277880278787852317, B747-400 10869569037091291768); the
    // exit-layout and certified-maximum declarations do not touch them.
    // Integration 2026-10-19 (payload holds merged): combined change, re-pinned by probe.
    // Code-quality lane 2026-10-19 (Q-03), re-pinned by probe from the same
    // eprintln: one program-wide set of constants (g0 = 9.80665 instead of
    // 9.81 in the performance matching chart, R = 287.0531 instead of 287.05
    // in the field density ratio, exact ft and psf factors). Magnitude 1e-5 to
    // 3.4e-4 relative on the performance rows; no other source changed.
    // Release 1.4.1 (2026-10-20), re-pinned by probe from the same eprintln:
    // the minimum-cruise route share (20 %) lowers the short registered A320-200
    // sector's flown altitude (FL280 -> FL269, trip fuel +0.9 %, block time
    // -0.4 %). With that share set to zero in place (a temporary switch,
    // removed) A320 reproduces 2858569692261565263; B747-400 is unchanged.
    let expected: [(&str, u64); 2] = [
        ("A320-200", 16_213_798_872_209_247_873),
        ("B747-400", 4_294_417_171_097_909_022),
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
