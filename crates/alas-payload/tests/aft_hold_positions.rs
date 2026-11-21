// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The container hold keeps a level structural floor to the aft end of its last
//! declared compartment, so the belly upsweep of the loft does not lift the
//! floor under the last container position. Numerical verification of the hold
//! contour; not a validation against a weight-and-balance manual.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::{CabinGeometry, CargoLoadManager};

fn built(name: &str, lower_deck_uld: &str) -> (CabinGeometry, AlasConfig) {
    let preset = presets::get(name).expect("preset");
    let mut config =
        AlasConfig::from_value(&serde_json::json!({ "preset": name })).expect("config");
    config.cabin.cargo.lower_deck_uld = lower_deck_uld.to_owned();
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("geometry");
    let g = alas_payload::build::product_cabin_geometry(&plane, &config).expect("cabin");
    (g, config)
}

/// Airbus A320 Aircraft Characteristics 2-6-0 pp2-3: three LD3-45W positions
/// forward and four aft.
#[test]
fn the_a320_takes_three_forward_and_four_aft_ld3_45w() {
    let (g, config) = built("A320-200", "LD3-45");
    let m = CargoLoadManager::new(&g, config.cabin.cargo.clone());
    let count = |prefix: &str| m.slots.iter().filter(|s| s.sid.starts_with(prefix)).count();
    assert_eq!(count("FWD-"), 3, "forward LD3-45 positions");
    assert_eq!(count("AFT-"), 4, "aft LD3-45 positions");
}

/// The floor under the last aft container row is the level floor of the rest of
/// the hold, not the belly-lifted one.
#[test]
fn the_a320_hold_floor_is_level_to_the_end_of_the_aft_container_hold() {
    let (g, config) = built("A320-200", "LD3-45");
    let aft = config
        .cabin
        .cargo
        .hold_compartments
        .iter()
        .find(|hold| hold.name == "Aft hold")
        .expect("aft hold");
    let level = g.floor_z(&g.lower_deck, aft.x_start_m);
    for step in 0..=20 {
        let x = aft.x_start_m + (aft.x_end_m - aft.x_start_m) * f64::from(step) / 20.0;
        assert!((g.floor_z(&g.lower_deck, x) - level).abs() < 1e-3, "x {x}");
    }
}

/// Declaring where the container holds end must not change the passenger deck:
/// a seat row is contained by the outer skin, not by the hold's flat keel. The
/// B747-400 carried 299 tourist seats before its lower holds were declared and
/// 302 after, because the keel-holding interior widened the armrest chord aft.
#[test]
fn a_lower_hold_declaration_does_not_move_the_passenger_deck_width() {
    let with_hold = built("B747-400", "LD1");
    assert!(
        with_hold
            .1
            .geometry
            .fuselage
            .hold_interior_end_x_m
            .is_some(),
        "the B747-400 declares its hold end"
    );
    let preset = presets::get("B747-400").expect("preset");
    let mut config = with_hold.1.clone();
    config.geometry.fuselage.hold_interior_end_x_m = None;
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("geometry");
    let without = alas_payload::build::product_cabin_geometry(&plane, &config).expect("cabin");
    let g = &with_hold.0;
    let (x0, x1) = (g.cabin_start_x, g.cabin_end_x);
    for deck in &g.passenger_decks {
        for step in 0..=200 {
            let x = x0 + (x1 - x0) * f64::from(step) / 200.0;
            let (a, b) = (g.usable_width(deck, x), without.usable_width(deck, x));
            assert!((a - b).abs() < 1e-9, "{} x {x}: {a} vs {b}", deck.name);
            let (a, b) = (
                g.row_chord_over(deck, x, x + 0.8, 0.5),
                without.row_chord_over(deck, x, x + 0.8, 0.5),
            );
            assert!((a - b).abs() < 1e-9, "{} row x {x}: {a} vs {b}", deck.name);
        }
    }
}
