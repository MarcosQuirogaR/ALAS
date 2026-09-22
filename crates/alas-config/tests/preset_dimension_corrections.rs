// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Independent geometric closure checks for published preset dimensions.

use alas_config::presets;

#[test]
fn atr_active_planform_closes_published_area_and_wing_loading() {
    let preset = presets::get("ATR72-600").unwrap_or_else(|error| panic!("{error}"));
    let planform = preset
        .geometry
        .wing
        .transport_planform(&preset.design_vector)
        .unwrap_or_else(|error| panic!("{error}"));
    // Exact trapezoidal integration includes the side-of-body station used
    // by the builder; integrating only root/kink/tip misses that clipping.
    let area: f64 = planform
        .panels()
        .iter()
        .map(|panel| {
            (panel.outboard.y_m - panel.inboard.y_m)
                * (panel.inboard.chord_m + panel.outboard.chord_m)
        })
        .sum();
    assert!((area - 61.0).abs() < 1e-10);
    assert!((preset.requirements.mtow_kg / area - 23_000.0 / 61.0).abs() < 1e-10);
}

#[test]
fn airbus_dimensions_preserve_cross_section_and_engine_symmetry() {
    let a320 = presets::get("A320-200").unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(a320.geometry.fuselage.height_m, Some(4.14));
    assert_eq!(a320.geometry.fuselage.diameter_m, 3.95);
    assert_eq!(a320.engine_spanwise_positions(), &[5.755, -5.755]);
    let a340 = presets::get("A340-300").unwrap_or_else(|error| panic!("{error}"));
    assert!((2.0 * a340.geometry.empennage.hstab_tip_le_m.1 - 19.4).abs() < 1e-12);
}

#[test]
fn the_a380_tailplane_spans_its_published_dimension() {
    // Airbus A380 Aircraft Characteristics - Airport and Maintenance Planning,
    // Revision 20 Dec 01/25, Subject 2-2-0 General Aircraft Dimensions,
    // FIGURE-2-2-0-991-001-A01 Sheet 1 of 2, page 2-2-0 Page 2: tailplane span
    // 30.37 m (99.64 ft), read from the front elevation where the dimension's
    // extension lines terminate on the tailplane tips.
    let a380 = presets::get("A380-800").unwrap_or_else(|error| panic!("{error}"));
    let empennage = &a380.geometry.empennage;
    assert!((2.0 * empennage.hstab_tip_le_m.1 - 30.37).abs() < 1e-12);
    // The same figure carries the wing span and fuselage width this preset
    // already matches, which is what makes the tailplane reading of the third
    // symmetric dimension on that view unambiguous.
    assert!((a380.design_vector.span_m - 79.75).abs() < 1e-12);
    assert!((a380.geometry.fuselage.diameter_m - 7.14).abs() < 1e-12);
    // Only the span is published: the chords are unsourced and unchanged, so
    // the resolved trapezoidal planform is 174.63 m^2, not the ~205 m^2 that
    // circulates on aggregator pages.
    let area =
        (empennage.hstab_root_chord_m + empennage.hstab_tip_chord_m) * empennage.hstab_tip_le_m.1;
    assert!((area - 174.6275).abs() < 1e-4, "{area}");
}
