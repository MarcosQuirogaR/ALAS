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
