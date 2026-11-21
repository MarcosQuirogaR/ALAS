// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Payload auto-balance: movable declared payload slides within
//! its range to the zero-fuel centre-of-gravity target, like trim-tank fuel.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig, MassCategory, PointMassConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::build_payload_layout;
use alas_payload::declared_items::auto_balance;

const OEW_KG: f64 = 120_000.0;
const X_OEW_M: f64 = 32.0;

fn plane_and_config() -> (alas_geom::aircraft::airplane::Airplane, AlasConfig) {
    let preset = presets::get("B787-9").unwrap();
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": "B787-9" })).unwrap();
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .unwrap();
    (plane, config)
}

fn pallet(x: f64, range: [f64; 2], movable: bool) -> PointMassConfig {
    PointMassConfig {
        id: "slider".to_owned(),
        name: "Slider".to_owned(),
        x_m: x,
        y_m: 0.0,
        z_m: 0.5,
        mass_kg: 5_000.0,
        category: MassCategory::Payload,
        inertia_kgm2: None,
        releasable: false,
        drag_area_m2: None,
        movable,
        x_range_m: Some(range),
    }
}

/// The zero-fuel centre of gravity of operating empty plus the layout's payload.
fn zero_fuel_cg(layout: &alas_payload::PayloadLayout) -> f64 {
    (OEW_KG * X_OEW_M + layout.total_mass * layout.cg_x) / (OEW_KG + layout.total_mass)
}

fn slider_x(layout: &alas_payload::PayloadLayout) -> f64 {
    layout
        .items
        .iter()
        .find(|item| item.label == "Slider")
        .map(|item| item.x)
        .unwrap()
}

#[test]
fn a_movable_item_slides_to_the_target_and_a_locked_or_fixed_one_does_not() {
    let (plane, mut config) = plane_and_config();
    let g = alas_payload::build::product_cabin_geometry(&plane, &config).unwrap();
    let x0 = 25.0;
    config.cabin.payload_items.point_masses = vec![pallet(x0, [15.0, 45.0], false)];
    let fixed = build_payload_layout(&plane, &config, OEW_KG, X_OEW_M).unwrap();
    assert_eq!(slider_x(&fixed), x0, "not movable: where declared");
    let x_zf = zero_fuel_cg(&fixed);

    // Aim 0.2 m aft of where the declared layout balances.
    let target_x = x_zf + 0.2;
    config.mission.plan.balance_target_pct_mac = Some(100.0 * (target_x - g.x_lemac) / g.mac);
    config.cabin.payload_items.point_masses = vec![pallet(x0, [15.0, 45.0], true)];
    let balanced = build_payload_layout(&plane, &config, OEW_KG, X_OEW_M).unwrap();
    let expected_shift = 0.2 * (OEW_KG + fixed.total_mass) / 5_000.0;
    assert!(
        (slider_x(&balanced) - (x0 + expected_shift)).abs() < 1.0e-6,
        "moved {} m, expected {expected_shift} m",
        slider_x(&balanced) - x0
    );
    assert!((zero_fuel_cg(&balanced) - target_x).abs() < 1.0e-6);
    assert!(
        (balanced.total_mass - fixed.total_mass).abs() < 1.0e-9,
        "mass is not changed"
    );

    // Lock Layout: the same configuration is left as declared.
    config.mission.plan.lock_layout = true;
    let locked = build_payload_layout(&plane, &config, OEW_KG, X_OEW_M).unwrap();
    assert_eq!(slider_x(&locked), x0);
}

#[test]
fn a_target_beyond_the_range_leaves_the_item_at_the_end_of_it_and_says_so() {
    let (plane, mut config) = plane_and_config();
    let g = alas_payload::build::product_cabin_geometry(&plane, &config).unwrap();
    config.cabin.payload_items.point_masses = vec![pallet(25.0, [20.0, 27.0], true)];
    config.mission.plan.balance_target_pct_mac = Some(100.0 * (45.0 - g.x_lemac) / g.mac);
    let base = build_payload_layout(&plane, &config, OEW_KG, X_OEW_M).unwrap();
    assert!(
        (slider_x(&base) - 27.0).abs() < 1.0e-9,
        "aft end of its range"
    );
    // The preview reports the saturation. The layout before the declared items
    // is the base of the balance.
    let mut empty = base.clone();
    empty.items.retain(|item| item.label != "Slider");
    let (_, outcome) = auto_balance(&empty, &g, &config, OEW_KG, X_OEW_M).unwrap();
    assert!(outcome.saturated);
    assert!(outcome.reached_x_m < outcome.target_x_m);
}

#[test]
fn nothing_movable_is_untouched_bit_for_bit() {
    let (plane, mut config) = plane_and_config();
    config.cabin.payload_items.point_masses = vec![pallet(25.0, [15.0, 45.0], false)];
    let a = build_payload_layout(&plane, &config, 0.0, 0.0).unwrap();
    let b = build_payload_layout(&plane, &config, OEW_KG, X_OEW_M).unwrap();
    assert_eq!(slider_x(&a).to_bits(), slider_x(&b).to_bits());
}
