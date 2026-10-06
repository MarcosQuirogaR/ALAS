// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The 747-400 seats business class on the upper deck and first class in the
//! nose of the main deck (ACAP D6-58326-1 Rev F sections 2.1.1 and 2.4.1),
//! not first class upstairs.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::{build_payload_layout, ItemKind};

#[test]
fn the_b747_upper_deck_is_business_and_first_is_in_the_main_deck_nose() {
    let preset = presets::get("B747-400").expect("B747 preset");
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset.name })).unwrap();
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .unwrap();
    let layout = build_payload_layout(&airplane, &config, 0.0, 0.0).unwrap();
    let rows: Vec<_> = layout
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::SeatRow)
        .collect();
    let upper: Vec<_> = rows.iter().filter(|r| r.deck == "upper").collect();
    assert!(!upper.is_empty(), "the upper deck is seated");
    assert!(
        upper.iter().all(|r| r.label == "Business"),
        "upper deck: business only"
    );
    let first_x: Vec<f64> = rows
        .iter()
        .filter(|r| r.label == "First")
        .map(|r| {
            assert_eq!(r.deck, "main", "first class sits on the main deck");
            r.x
        })
        .collect();
    assert!(!first_x.is_empty(), "first class is seated");
    let main_business_min_x = rows
        .iter()
        .filter(|r| r.label == "Business" && r.deck == "main")
        .map(|r| r.x)
        .fold(f64::INFINITY, f64::min);
    assert!(
        first_x.iter().all(|x| *x < main_business_min_x),
        "first is forward of main-deck business"
    );
}
