// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The A400M preset against the dimensioned features of the Airbus brochure
//! three-view (TMMA0026/01/2025 p025) and EASA TCDS A.169: overall length to
//! the tail-plane tips, high wing with anhedral, engine axes. Tolerances are
//! the drawing's reading error (about 0.2 m), not a calibration.

// A test asserts on values it built here, so a failed expect is the assertion
// failing rather than a library invariant being broken.
#![allow(clippy::expect_used)]

use alas_config::presets;
use alas_geom::builder::AircraftBuilder;

fn a400m() -> alas_geom::aircraft::airplane::Airplane {
    let preset = presets::get("A400M").expect("A400M preset");
    AircraftBuilder::new(Some(preset.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("finite geometry")
}

#[test]
fn overall_length_is_set_by_the_tail_plane_tips_not_the_fuselage() {
    let plane = a400m();
    let tail_tip_te = plane.wings[1]
        .xsecs
        .iter()
        .map(|s| s.xyz_le[0] + s.chord)
        .fold(f64::MIN, f64::max);
    // TCDS A.169 III.4 overall length 45.091 m.
    assert!((tail_tip_te - 45.091).abs() < 0.1, "{tail_tip_te}");
    let fuselage_end = plane.fuselages[0]
        .xsecs
        .iter()
        .map(|s| s.xyz_c[0])
        .fold(f64::MIN, f64::max);
    // The drawn fuselage ends at 39.5 m +- 0.3 m.
    assert!((fuselage_end - 39.5).abs() < 0.5, "{fuselage_end}");
}

#[test]
fn wing_is_high_with_anhedral_and_the_drawn_leading_edge() {
    let plane = a400m();
    let wing = &plane.wings[0];
    let root = &wing.xsecs[0];
    let tip = wing.xsecs.last().expect("tip");
    // Leading edge on x = 12.5 + 0.332 y (brochure top view).
    assert!((root.xyz_le[0] - 12.5).abs() < 0.2, "{}", root.xyz_le[0]);
    let slope = (tip.xyz_le[0] - root.xyz_le[0]) / tip.xyz_le[1].abs();
    assert!((slope - 0.332).abs() < 0.02, "{slope}");
    // High wing above the 2.8 m crown, tip 1.75 m below the root (front view).
    assert!(root.xyz_le[2] > 2.8);
    let drop = root.xyz_le[2] - tip.xyz_le[2];
    assert!((drop - 1.75).abs() < 0.25, "{drop}");
}

#[test]
fn engine_axes_are_on_the_drawn_stations() {
    let plane = a400m();
    let mut ys: Vec<f64> = plane
        .fuselages
        .iter()
        .filter(|f| f.name != "Fuselage")
        .map(|f| f.xsecs[0].xyz_c[1].abs())
        .collect();
    ys.sort_by(f64::total_cmp);
    assert_eq!(ys.len(), 4);
    assert!(
        (ys[0] - 6.75).abs() < 0.2 && (ys[1] - 6.75).abs() < 0.2,
        "{ys:?}"
    );
    assert!(
        (ys[2] - 12.72).abs() < 0.2 && (ys[3] - 12.72).abs() < 0.2,
        "{ys:?}"
    );
}
