// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The underfloor cargo floor is level aft of the constant section and the
//! lower-deck width follows the lining chord, so the published ULD loadings fit.
//! Numerical verification of the hold contour; not a validation against a
//! weight-and-balance manual.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::{CabinGeometry, CargoLoadManager};

fn manager_of(name: &str) -> (CabinGeometry, AlasConfig) {
    let preset = presets::get(name).expect("preset");
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).expect("config");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("geometry");
    let g = alas_payload::build::product_cabin_geometry(&plane, &config).expect("cabin");
    (g, config)
}

#[test]
fn the_747_hold_floor_stays_level_aft_of_the_middle_of_the_constant_section() {
    let (g, _) = manager_of("B747-400");
    let x_ref = 0.5 * (g.cabin_start_x + g.cabin_end_x);
    let level = g.floor_z(&g.lower_deck, x_ref);
    for x in [x_ref + 2.0, x_ref + 8.0, x_ref + 12.0] {
        assert!((g.floor_z(&g.lower_deck, x) - level).abs() < 1e-9, "x {x}");
    }
}

/// ACAP D6-58326-1 Rev F p.2-32: 16 half-width containers (LD1) forward and 16
/// aft. The loft's tail narrows the aft hold, so the model does not reach the
/// published aft count; it must reach all eight forward rows and a second hold.
#[test]
fn the_747_takes_ld1_pairs_forward_and_containers_aft() {
    let (g, config) = manager_of("B747-400");
    let m = CargoLoadManager::new(&g, config.cabin.cargo.clone());
    assert_eq!(m.lower_uld.key, "LD1");
    let fwd = m.installed_positions("FWD-", false);
    let aft = m.installed_positions("AFT-", false);
    assert_eq!(fwd, 16, "forward LD1 positions");
    assert!(aft >= 8, "aft LD1 positions {aft}");
}

/// A380 AC: 20 + 16 LD3 standard, 22 + 16 optional: no more than 38.
#[test]
fn the_a380_hold_does_not_exceed_the_published_ld3_count() {
    let (g, config) = manager_of("A380-800");
    let m = CargoLoadManager::new(&g, config.cabin.cargo.clone());
    let positions = m
        .slots
        .iter()
        .filter(|s| s.sid.starts_with("FWD-") || s.sid.starts_with("AFT-"))
        .count();
    assert!(positions <= 38, "{positions}");
}

/// Bodies whose drawing prints the hold's ceiling depth as well as its polygon
/// (AC A340 fig 2-5-0-991-002, AC A350 2-7-0 section A-A, the DC-10 and E195-E2
/// drawn holds): the hold stays as drawn, and the modelled lining, a superellipse
/// inset by one wall, is the approximation where the two disagree.
const PRINTED_HOLDS: [&str; 4] = ["A340-300", "A350-900", "DC-10", "E195-E2"];

/// Most a printed hold polygon may stand outside the modelled lining, m.
const PRINTED_HOLD_TOLERANCE_M: f64 = 0.07;

/// The hold polygon stands inside the lined interior, not only inside the outer
/// skin: the floor edge and the foot of the side wall of every registered body
/// with an underfloor hold clear the lining (the A220 forward hold's section
/// A-A, ACP BD500-3AB48-13800-00 Fig 8, draws them on it). A hold whose drawing
/// prints its own ceiling depth is left as drawn and held to a few centimetres.
#[test]
fn every_hold_polygon_stands_inside_the_lining() {
    let mut checked = 0;
    for name in presets::available() {
        let (g, _) = manager_of(name);
        let x_ref = 0.5 * (g.cabin_start_x + g.cabin_end_x);
        let vertices = g.hold_vertex_clearances(x_ref);
        if vertices.is_empty() {
            continue;
        }
        checked += 1;
        let limit = if PRINTED_HOLDS.contains(&name) {
            -PRINTED_HOLD_TOLERANCE_M
        } else {
            0.0
        };
        for vertex in vertices {
            assert!(
                vertex.clearance_m >= limit,
                "{name}: {} at y {:.3} m, z {:.3} m stands {:.3} m outside the lining",
                vertex.name,
                vertex.y_m,
                vertex.z_m,
                -vertex.clearance_m
            );
        }
    }
    assert!(checked >= 8, "{checked} bodies with a hold polygon");
}

/// The A220 section A-A keeps its printed dimensions after the hold is brought
/// inside the lining: 1.05 m from floor to ceiling, a 0.84 m floor, 2.24 m
/// between the walls and the 0.40 m drop of the lower corner arc.
#[test]
fn the_a220_hold_keeps_its_printed_section_inside_the_lining() {
    let (g, _) = manager_of("A220-300");
    let x_ref = 0.5 * (g.cabin_start_x + g.cabin_end_x);
    let hold = g.hold.expect("the A220 has a drawn hold");
    assert!((hold.height_m - 1.05).abs() < 1e-9, "{}", hold.height_m);
    assert!((2.0 * hold.floor_half_m - 0.84).abs() < 1e-9);
    assert!((2.0 * hold.side_half_m - 2.24).abs() < 1e-9);
    assert!((hold.chamfer_height_m - 0.40).abs() < 1e-9);
    let floor = g.floor_z(&g.lower_deck, x_ref);
    let ceiling = g.ceil_z(&g.lower_deck, x_ref);
    assert!((ceiling - floor - 1.05).abs() < 1e-6, "{}", ceiling - floor);
    // The floor beams stay clear of the hold ceiling.
    let main = g.floor_z(&g.passenger_decks[0], x_ref);
    assert!(main - ceiling >= 0.06 * g.internal_half_height(x_ref) - 1e-9);
}
