// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Tests assert on values they construct here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]

//! The reference-wing convention and the nacelle model default to the
//! historical behaviour: with both unset, the product candidate evaluation
//! (geometry, VLM drag, mass, mission closure, constraint residuals) is
//! bit-identical to the behaviour before the options existed. The pinned hashes
//! are produced by this same test body.

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
    // The hash covers the whole assessed candidate (geometry, VLM drag, mass, mission
    // closure and constraint residuals), so any intended numerical change in those
    // stages moves it. After attributing such a change by switching the new
    // behaviour off, take the new value from the `eprintln` below.
    let expected: [(&str, u64); 2] = [
        ("A320-200", 14_295_107_566_895_686_562),
        ("B747-400", 1_619_505_736_229_626_502),
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

/// Probe for re-pin isolation: every value the hash covers, one per line, so
/// two trees can be diffed row by row (new rows show as added lines).
#[test]
#[ignore = "diagnostic: prints the hashed table"]
// A probe prints its numbers; it asserts nothing.
#[allow(clippy::print_stderr)]
fn physics_table_probe() {
    for name in ["A320-200", "B747-400"] {
        let preset = presets::get(name).unwrap();
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        let a = alas_opt::assess_product_candidate(&config, &preset.design_vector).unwrap();
        eprintln!(
            "TABLE {name} summary {:?}",
            (
                a.objective_value,
                a.sized.block_fuel_kg,
                a.sized.takeoff_mass_kg,
                a.sized.operating_empty_mass_kg
            )
        );
        for r in &a.residuals {
            eprintln!("TABLE {name} {} {:?} {:?}", r.id, r.actual, r.limit);
        }
    }
}
