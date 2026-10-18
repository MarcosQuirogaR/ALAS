// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A declared freight hold (military transport or freighter): the payload
//! rides on pallets in a single row down the floor and the ramp, with no
//! underfloor containers.
//!
//! These are numerical-verification tests of the loading model against the
//! declared hold dimensions. They do not validate the loading against a
//! weight-and-balance manual.

// Failed construction or lookup is a failed test assertion.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use alas_config::{presets, AlasConfig, CargoDeckConfig, FreightHoldConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::layout::{ItemKind, MAIN};
use alas_payload::{build_payload_layout, CabinGeometry, CargoLoadManager};

fn built(name: &str) -> (AlasConfig, alas_geom::aircraft::airplane::Airplane) {
    let preset = presets::get(name).expect("registered preset");
    let config =
        AlasConfig::from_value(&serde_json::json!({ "preset": name })).expect("preset config");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("preset geometry");
    (config, plane)
}

fn hold(config: &AlasConfig) -> FreightHoldConfig {
    config
        .cabin
        .cargo
        .freight_hold
        .expect("the preset declares a freight hold")
}

#[test]
fn the_a400m_hold_takes_seven_pallets_on_the_floor_and_two_on_the_ramp() {
    let (config, plane) = built("A400M");
    let h = hold(&config);
    let g = CabinGeometry::new(
        &plane,
        &config.geometry,
        config.cabin.passenger.wall_thickness_m,
    )
    .expect("cabin frame");
    let manager = CargoLoadManager::new(&g, config.cabin.cargo.clone());
    let floor: Vec<_> = manager
        .slots
        .iter()
        .filter(|slot| slot.sid.starts_with("MD"))
        .collect();
    let ramp: Vec<_> = manager
        .slots
        .iter()
        .filter(|slot| slot.sid.starts_with("RAMP"))
        .collect();
    // Jane's AWA 2008-2009 p.297 and the Airbus brochure p.4: nine military
    // pallets; 17.71 m floor and 5.40 m ramp at an 88 in (2.235 m) pitch.
    assert_eq!((floor.len(), ramp.len()), (7, 2));
    assert_eq!(manager.slots.len(), 9, "no underfloor or bulk positions");
    for slot in &manager.slots {
        assert_eq!(slot.deck, MAIN);
        assert_eq!(slot.y, 0.0, "a 4.00 m hold takes one 2.743 m pallet across");
        let half = 0.5 * slot.uld.length;
        assert!(slot.x - half >= h.floor_start_x_m - 1.0e-9);
        assert!(slot.x + half <= h.aft_end_x_m() + 1.0e-9);
    }
    // No pallet straddles the ramp hinge.
    for slot in &floor {
        assert!(slot.x + 0.5 * slot.uld.length <= h.ramp_start_x_m() + 1.0e-9);
    }
    for slot in &ramp {
        assert!(slot.x - 0.5 * slot.uld.length >= h.ramp_start_x_m() - 1.0e-9);
    }
    // The ramp's declared 6 t is shared between its positions.
    let ramp_capacity: f64 = ramp.iter().map(|slot| slot.max_net()).sum();
    assert!((ramp_capacity - h.ramp_load_limit_kg.unwrap()).abs() < 1.0e-9);
}

#[test]
fn the_a400m_payload_is_carried_on_the_main_deck_and_never_below_it() {
    let (config, plane) = built("A400M");
    let layout = build_payload_layout(&plane, &config, 65_289.0, 18.178).expect("layout");
    let items: Vec<_> = layout
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::Uld)
        .collect();
    assert!(!items.is_empty());
    assert!(items.iter().all(|item| item.deck == MAIN));
    // The requested 20 t reaches the aircraft: the airliner lower-deck layout
    // topped out at 15.9 t on 14 LD3 containers.
    assert!((layout.total_mass - config.requirements.cargo_payload_kg).abs() < 1.0);
    // The load stands inside the hold, forward of the ramp hinge on average.
    let h = hold(&config);
    assert!(layout.cg_x > h.floor_start_x_m && layout.cg_x < h.ramp_start_x_m());
    // Floor height follows the declared clear height under the crown, not the
    // airliner main-deck floor at the cabin axis: the pallet stands below it.
    let g = CabinGeometry::new(
        &plane,
        &config.geometry,
        config.cabin.passenger.wall_thickness_m,
    )
    .expect("cabin frame");
    let axis_z = g.zc_at(items[0].x);
    assert!(items.iter().all(|item| item.z < axis_z));
}

#[test]
fn the_balance_target_moves_the_pallets_not_the_hold() {
    let (mut config, plane) = built("A400M");
    config.cabin.cargo.target_cg_pct_mac = 40.0;
    let aft = build_payload_layout(&plane, &config, 65_289.0, 18.178).expect("aft layout");
    config.cabin.cargo.target_cg_pct_mac = 10.0;
    let forward = build_payload_layout(&plane, &config, 65_289.0, 18.178).expect("fwd layout");
    assert!(aft.cg_x > forward.cg_x + 1.0);
    assert!((aft.total_mass - forward.total_mass).abs() < 1.0);
}

#[test]
fn a_body_without_a_declared_freight_hold_is_unchanged() {
    let (config, plane) = built("A320-200");
    assert!(config.cabin.cargo.freight_hold.is_none());
    let g = CabinGeometry::new(
        &plane,
        &config.geometry,
        config.cabin.passenger.wall_thickness_m,
    )
    .expect("cabin frame");
    let manager = CargoLoadManager::new(&g, CargoDeckConfig::default());
    assert!(manager.freight_deck().is_none());
    assert!(manager
        .slots
        .iter()
        .all(|slot| !slot.sid.starts_with("RAMP")));
}

#[test]
fn an_invalid_freight_hold_is_ignored_rather_than_laid_out() {
    let (config, plane) = built("A400M");
    let g = CabinGeometry::new(
        &plane,
        &config.geometry,
        config.cabin.passenger.wall_thickness_m,
    )
    .expect("cabin frame");
    let mut cargo = config.cabin.cargo.clone();
    cargo.freight_hold = Some(FreightHoldConfig {
        clear_width_m: f64::NAN,
        ..hold(&config)
    });
    let manager = CargoLoadManager::new(&g, cargo);
    assert!(manager.freight_deck().is_none());
    assert!(manager
        .slots
        .iter()
        .all(|slot| !slot.sid.starts_with("RAMP")));
}
