// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Declared floor zones and point masses.
//!
//! A B787-9 shell gets a furnished office amidships and a point-mass drone:
//! the zone must displace seats (no seat row inside it), its furnishing must
//! reach the mass ledger's furnishings line and not the payload, its occupants
//! and the drone must reach the payload and move its centre of gravity, and a
//! configuration that declares nothing must be untouched.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{
    presets, AlasConfig, FloorZoneConfig, MassCategory, PointMassConfig, ZoneDeck, ZoneKind,
};
use alas_geom::builder::AircraftBuilder;
use alas_payload::{build_payload_layout, ItemKind, ItemMeta, LayoutSummary, PayloadLayout};

const PRESET: &str = "B787-9";

fn plane_and_config() -> (alas_geom::aircraft::airplane::Airplane, AlasConfig) {
    let preset = presets::get(PRESET).unwrap();
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": PRESET })).unwrap();
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .unwrap();
    (plane, config)
}

fn office(x0: f64, x1: f64) -> FloorZoneConfig {
    FloorZoneConfig {
        name: "Office".to_owned(),
        kind: ZoneKind::Office,
        deck: ZoneDeck::Main,
        x_start_m: x0,
        x_end_m: x1,
        width_m: None,
        y_center_m: 0.0,
        furnishing_kg_per_m2: 180.0,
        total_mass_kg: None,
        occupants: 4,
        carve_seat_floor: true,
        movable: false,
        x_range_m: None,
    }
}

fn drone() -> PointMassConfig {
    PointMassConfig {
        id: "drone".to_owned(),
        name: "Drone".to_owned(),
        x_m: 30.0,
        y_m: 0.0,
        z_m: -1.0,
        mass_kg: 1_000.0,
        category: MassCategory::Payload,
        inertia_kgm2: None,
        releasable: true,
        drag_area_m2: Some(0.15),
        movable: false,
        x_range_m: None,
    }
}

fn seated(layout: &PayloadLayout) -> i64 {
    match &layout.summary {
        LayoutSummary::Passenger(summary) => summary.seated_pax,
        LayoutSummary::Cargo(_) => panic!("passenger aircraft"),
    }
}

#[test]
fn nothing_declared_leaves_the_layout_and_the_serialized_config_unchanged() {
    let (plane, config) = plane_and_config();
    let base = build_payload_layout(&plane, &config, 0.0, 0.0).unwrap();
    assert_eq!(base.equipment_mass(), 0.0);
    let value = serde_json::to_value(&config).unwrap();
    assert!(value["cabin"].get("payload_items").is_none());
}

#[test]
fn a_zone_displaces_seats_and_its_masses_reach_the_right_ledger_line() {
    let (plane, mut config) = plane_and_config();
    let base = build_payload_layout(&plane, &config, 0.0, 0.0).unwrap();
    let base_seats = seated(&base);

    // Twelve metres of full-width main deck near mid-cabin.
    config.cabin.payload_items.zones = vec![office(24.0, 36.0)];
    config.cabin.payload_items.point_masses = vec![drone()];
    let layout = build_payload_layout(&plane, &config, 0.0, 0.0).unwrap();

    let rows: Vec<_> = layout
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::SeatRow && item.deck == "main")
        .collect();
    assert!(!rows.is_empty());
    for row in &rows {
        let (a, b) = (row.x - 0.5 * row.length, row.x + 0.5 * row.length);
        assert!(
            b <= 24.0 + 1e-6 || a >= 36.0 - 1e-6,
            "seat row at {a:.2}..{b:.2} m lies inside the reserved zone"
        );
    }
    assert!(
        seated(&layout) < base_seats,
        "the office must displace seats"
    );

    // Equipment: area-based furnishing, kept out of the payload.
    let area = 12.0 * 5.0..12.0 * 6.5; // wide-body main-deck width, m
    let equipment = layout.equipment_mass();
    assert!(
        area.contains(&(equipment / 180.0)),
        "furnished area {} m2",
        equipment / 180.0
    );
    let cg = layout.equipment_cg();
    assert!((24.0..36.0).contains(&cg[0]));

    // Payload: seats lost + four occupants at the load-case mass + the drone.
    let occupants =
        4.0 * (config.requirements.passenger_mass_kg - config.cabin.passenger.checked_bag_mass_kg);
    let declared_payload: f64 = layout
        .items
        .iter()
        .filter(|item| matches!(&item.meta, ItemMeta::Declared(m) if m.category == MassCategory::Payload))
        .map(|item| item.mass)
        .sum();
    assert!((declared_payload - (occupants + 1_000.0)).abs() < 1e-9);
    assert!(layout
        .items
        .iter()
        .any(|item| item.kind == ItemKind::PointMass));
}

#[test]
fn an_overlapping_declaration_is_refused() {
    let (plane, mut config) = plane_and_config();
    config.cabin.payload_items.zones = vec![office(24.0, 36.0), office(30.0, 40.0)];
    assert!(build_payload_layout(&plane, &config, 0.0, 0.0).is_err());
    assert!(alas_config::validate(&config)
        .iter()
        .any(|issue| issue.field_path == "cabin.payload_items"));
}
