// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Shared tank inventory: geometric scaling, custom declarations and closure.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_mass::tanks::{resolve_product_layout, FuelTankLayout, TankLayoutError};

#[test]
fn all_presets_resolve_scaled_inventory_and_reject_overfill() {
    for preset in presets::registry() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset":preset.name})).unwrap();
        let builder = AircraftBuilder::new(Some(config.geometry.clone()));
        let reference = builder.build(Some(&preset.design_vector), false).unwrap();
        let baseline = resolve_product_layout(&config, &preset.design_vector, &reference).unwrap();
        let mut design = preset.design_vector;
        design.span_m *= 0.95;
        // Root and kink chords shrink together: the ATR 72-600 centre
        // section is constant-chord, so a smaller root alone would taper
        // the wing inward and is not a buildable planform.
        design.root_chord_m *= 0.96;
        design.break_chord_m *= 0.96;
        let plane = builder.build(Some(&design), false).unwrap();
        let tanks = resolve_product_layout(&config, &design, &plane).unwrap();
        let capacity = tanks.usable_capacity_kg();
        assert!(capacity.is_finite() && capacity > 0.0, "{}", preset.name);
        assert!(capacity < baseline.usable_capacity_kg(), "{}", preset.name);
        let fill = tanks.distribute(0.8 * capacity).unwrap();
        let sum: f64 = fill
            .mass_items(&tanks)
            .iter()
            .map(|item| item.mass_kg)
            .sum();
        assert!((sum - 0.8 * capacity).abs() < 1e-7);
        assert!(matches!(
            tanks.distribute(capacity + 1.0),
            Err(TankLayoutError::Overflow { .. })
        ));
        for tank in tanks.tanks().iter().filter(|t| t.id == "auxiliary") {
            let original = baseline.tanks().iter().find(|t| t.id == tank.id).unwrap();
            assert_eq!(tank.usable_capacity_kg, original.usable_capacity_kg);
        }
    }
}

#[test]
fn custom_auxiliary_disable_does_not_restore_the_reference_total() {
    let preset = presets::get("DC-10").unwrap();
    let mut config = AlasConfig::from_value(&serde_json::json!({"preset":"DC-10"})).unwrap();
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), false)
        .unwrap();
    let installed = resolve_product_layout(&config, &preset.design_vector, &plane).unwrap();
    let auxiliary = installed
        .tanks()
        .iter()
        .find(|t| t.id == "auxiliary")
        .unwrap()
        .usable_capacity_kg;
    // The registered arrangement is reconciled onto the published inventory;
    // a custom one keeps its own cells. Removing the auxiliary tank leaves
    // exactly the remaining cells, never the reference total.
    let cells = FuelTankLayout::resolve(
        &plane,
        &config.geometry,
        &config.structures,
        &config.fuel_tanks,
        &config.fuel_policy,
        installed.density_kg_m3,
        preset.reference.usable_fuel_volume_l,
    )
    .unwrap();
    config.fuel_tanks.auxiliary.enabled = false;
    let removed = resolve_product_layout(&config, &preset.design_vector, &plane).unwrap();
    assert!((cells.usable_capacity_kg() - removed.usable_capacity_kg() - auxiliary).abs() < 1e-7);
    assert!(removed.distribute(installed.usable_capacity_kg()).is_err());
}
