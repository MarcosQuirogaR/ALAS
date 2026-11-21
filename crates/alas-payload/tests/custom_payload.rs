// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A custom payload carries nothing but the declared zones and point masses.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use alas_config::{presets, AlasConfig, MassCategory, PointMassConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::build_payload_layout;

fn b787() -> (AlasConfig, alas_geom::aircraft::airplane::Airplane) {
    let preset = presets::get("B787-9").expect("registered B787-9 preset");
    let config = AlasConfig::from_value(&serde_json::json!({"preset": "B787-9"}))
        .expect("B787-9 configuration");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("B787-9 geometry");
    (config, plane)
}

#[test]
fn a_custom_payload_is_empty_until_items_are_placed_and_then_is_exactly_their_mass() {
    let (mut config, plane) = b787();
    let standard = build_payload_layout(&plane, &config, 0.0, 0.0).expect("standard layout");
    assert!(standard.total_mass > 10_000.0, "{}", standard.total_mass);

    config.cabin.payload_items.custom_only = true;
    let empty = build_payload_layout(&plane, &config, 0.0, 0.0).expect("empty custom layout");
    assert!(empty.total_mass.abs() < 1e-9, "{}", empty.total_mass);

    config
        .cabin
        .payload_items
        .point_masses
        .push(PointMassConfig {
            id: "block".to_owned(),
            name: "Block".to_owned(),
            x_m: 30.0,
            y_m: 0.0,
            z_m: 0.0,
            mass_kg: 4_000.0,
            category: MassCategory::Payload,
            inertia_kgm2: None,
            releasable: false,
            drag_area_m2: None,
            movable: false,
            x_range_m: None,
        });
    let placed = build_payload_layout(&plane, &config, 0.0, 0.0).expect("custom layout");
    assert!(
        (placed.total_mass - 4_000.0).abs() < 1e-6,
        "{}",
        placed.total_mass
    );
    assert!((placed.cg_x - 30.0).abs() < 1e-6, "{}", placed.cg_x);
}
