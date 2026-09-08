// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Capacity and occupancy regressions for physical cargo layouts.

// Failed construction or lookup is a failed test assertion.
#![allow(clippy::expect_used)]

use alas_config::{presets, AlasConfig, CargoDeckConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::{CabinGeometry, CargoLoadManager};

fn geometry(name: &str) -> CabinGeometry {
    let preset = presets::get(name).expect("registered preset");
    let config =
        AlasConfig::from_value(&serde_json::json!({"preset": name})).expect("preset configuration");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("preset geometry");
    CabinGeometry::new(
        &plane,
        &config.geometry,
        config.cabin.passenger.wall_thickness_m,
    )
    .expect("preset cabin geometry")
}

#[test]
fn available_capacity_does_not_depend_on_dispatched_cargo_mass() {
    let g = geometry("B787-9");
    let mut manager = CargoLoadManager::new(
        &g,
        CargoDeckConfig {
            use_main_deck: false,
            ..Default::default()
        },
    );
    let empty = manager.capacity_summary();
    assert!(empty.uld_positions > 0);
    assert_eq!(manager.mass_props().0, 0.0);
    manager.solve(
        1_000.0,
        g.x_wing_ac,
        &|slot| (slot.x - g.x_wing_ac).abs(),
        true,
    );
    assert!(manager.mass_props().0 > 0.0);
    assert_eq!(empty, manager.capacity_summary());
    assert!(
        manager
            .slots
            .iter()
            .filter(|slot| slot.payload > 1.0)
            .count()
            < empty.uld_positions
    );
}

#[test]
fn bulk_capacity_is_not_reported_as_uld_capacity() {
    let g = geometry("B787-9");
    let manager = CargoLoadManager::new(
        &g,
        CargoDeckConfig {
            use_main_deck: false,
            lower_deck_uld: "BLK".to_owned(),
            ..Default::default()
        },
    );
    let capacity = manager.capacity_summary();
    assert!(capacity.bulk_positions > 0);
    assert_eq!(capacity.uld_positions, 0);
    assert_eq!(capacity.container_internal_volume_m3, 0.0);
    assert!(capacity.bulk_nominal_volume_m3 > 0.0);
}

#[test]
fn a_loaded_bulk_hold_has_mass_but_no_containers() {
    let g = geometry("B787-9");
    let config = CargoDeckConfig {
        use_main_deck: false,
        lower_deck_uld: "BLK".to_owned(),
        ..Default::default()
    };
    let req = alas_config::DesignRequirements {
        cargo_payload_kg: 1_000.0,
        ..Default::default()
    };
    let layout = alas_payload::build_cargo_layout(&g, &config, &req, 0.0, 0.0);
    assert!((layout.total_mass - 1_000.0).abs() < 1.0);
    assert!(layout
        .items
        .iter()
        .all(|item| item.meta == alas_payload::ItemMeta::BulkBag));
    let alas_payload::LayoutSummary::Cargo(summary) = layout.summary else {
        panic!("cargo load must have cargo summary");
    };
    assert_eq!(summary.n_ulds, 0);
    assert_eq!(summary.tare_mass_t, 0.0);
}

#[test]
fn physical_hold_positions_never_double_book_the_bulk_footprint() {
    for name in presets::available() {
        let g = geometry(name);
        let manager = CargoLoadManager::new(
            &g,
            CargoDeckConfig {
                use_main_deck: false,
                ..Default::default()
            },
        );
        for (index, first) in manager.slots.iter().enumerate() {
            for second in manager.slots.iter().skip(index + 1) {
                let x_overlap = (first.x - second.x).abs()
                    < (first.uld.length + second.uld.length) * 0.5 - 1e-9;
                let y_overlap =
                    (first.y - second.y).abs() < (first.uld.width + second.uld.width) * 0.5 - 1e-9;
                assert!(
                    first.deck != second.deck || !x_overlap || !y_overlap,
                    "{name}: {} overlaps {}",
                    first.sid,
                    second.sid
                );
            }
        }
    }
}

#[test]
fn a220_passenger_baggage_uses_the_configured_bulk_only_system() {
    let preset = presets::get("A220-300").expect("registered A220");
    let config = AlasConfig::from_value(&serde_json::json!({"preset": "A220-300"}))
        .expect("A220 configuration");
    assert_eq!(config.cabin.cargo.lower_deck_uld, "BLK");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("A220 geometry");
    let layout =
        alas_payload::build_payload_layout(&plane, &config, 0.0, 0.0).expect("A220 payload");
    assert!(!layout
        .items
        .iter()
        .any(|item| matches!(item.meta, alas_payload::ItemMeta::Container(_))));
    let bulk_mass: f64 = layout
        .items
        .iter()
        .filter(|item| item.meta == alas_payload::ItemMeta::BulkBag)
        .map(|item| item.mass)
        .sum();
    let alas_payload::LayoutSummary::Passenger(summary) = layout.summary else {
        panic!("passenger summary");
    };
    assert!(bulk_mass > 0.0);
    assert_eq!(summary.hold_ulds, 0);
    assert!((bulk_mass - 1000.0 * (summary.bag_mass_t + summary.belly_cargo_t)).abs() < 1.0);
    let g = geometry("A220-300");
    let manager = CargoLoadManager::new(
        &g,
        CargoDeckConfig {
            use_main_deck: false,
            ..config.cabin.cargo.clone()
        },
    );
    assert_eq!(manager.capacity_summary().uld_positions, 0);
    assert_eq!(manager.capacity_summary().container_internal_volume_m3, 0.0);
}

#[test]
fn a_shallow_bulk_only_hold_cannot_fall_back_to_a_container_system() {
    let mut g = geometry("B787-9");
    g.lower_deck.floor_frac = -0.55;
    let container_manager = CargoLoadManager::new(
        &g,
        CargoDeckConfig {
            use_main_deck: false,
            lower_deck_uld: "LD3-45".to_owned(),
            ..Default::default()
        },
    );
    assert!(container_manager.capacity_summary().uld_positions > 0);
    let bulk_manager = CargoLoadManager::new(
        &g,
        CargoDeckConfig {
            use_main_deck: false,
            lower_deck_uld: "BLK".to_owned(),
            ..Default::default()
        },
    );
    assert_eq!(bulk_manager.lower_uld.code, "BLK");
    assert_eq!(bulk_manager.capacity_summary().uld_positions, 0);
}
