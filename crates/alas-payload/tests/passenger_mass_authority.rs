// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `requirements.passenger_mass_kg` is the single product load-case
//! authority for what one seated passenger costs the zero-fuel mass, of any
//! class: occupant plus checked baggage combined, standard masses that do
//! not vary by class (FAA AC 120-27E / EASA). These regressions pin that
//! contract at the product entry point every report/GUI/pipeline/export
//! consumer calls (`build_payload_layout`), for a mixed-class cabin, for a
//! heavier declared bag mass, for a non-default combined mass, and for a
//! cabin declared by seat count rather than by class-length share.
//!
//! The frozen `*_reference_compatibility` replay is untouched by this
//! authority and is not exercised here; `parity_cabin_sizing.rs` already pins
//! its historical per-class masses.

// Failed construction is a failed test assertion.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::{build_payload_layout, LayoutSummary};

fn passenger_summary(
    layout: alas_payload::PayloadLayout,
) -> Box<alas_payload::layout::PassengerSummary> {
    let LayoutSummary::Passenger(summary) = layout.summary else {
        panic!("the fixture is a passenger baseline");
    };
    summary
}

/// Builds the B787-9 preset config/plane pair, so callers only vary
/// `requirements.passenger_mass_kg` and `cabin.passenger.checked_bag_mass_kg`.
fn b787_config() -> (AlasConfig, alas_geom::aircraft::airplane::Airplane) {
    let preset = presets::get("B787-9").expect("registered B787-9 preset");
    let config = AlasConfig::from_value(&serde_json::json!({"preset": "B787-9"}))
        .expect("B787-9 configuration");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("B787-9 geometry");
    (config, plane)
}

#[test]
fn a_mixed_class_cabin_prices_every_seat_at_the_same_combined_mass() {
    // The B787-9 preset seeds a two-class (business/economy) percent-mode
    // cabin from `PassengerCabinConfig::default`, whose per-class seed masses
    // (90 kg business, 84 kg economy) differ before the authority is applied.
    // After it, every seated passenger -- whichever class -- costs the same
    // combined `passenger_mass_kg`.
    let (config, plane) = b787_config();
    assert_eq!(config.requirements.passenger_mass_kg, 100.0);
    assert_eq!(config.cabin.passenger.checked_bag_mass_kg, 16.0);

    let layout = build_payload_layout(&plane, &config, 0.0, 0.0).expect("B787-9 layout builds");
    let summary = passenger_summary(layout);

    let business_seats = summary
        .classes
        .iter()
        .find(|(name, _)| *name == "Business")
        .map_or(0, |(_, seats)| *seats);
    assert!(
        business_seats > 0,
        "the fixture must exercise a mixed cabin with a business slot"
    );
    assert!(
        summary.seated_pax > business_seats,
        "the fixture must also exercise an economy slot"
    );

    let occupant_kg =
        config.requirements.passenger_mass_kg - config.cabin.passenger.checked_bag_mass_kg;
    let expected_seat_mass_kg = summary.seated_pax as f64 * occupant_kg;
    let expected_bag_mass_kg =
        summary.seated_pax as f64 * config.cabin.passenger.checked_bag_mass_kg;

    assert!(
        (summary.seat_mass_t * 1_000.0 - expected_seat_mass_kg).abs() < 1e-6,
        "seat mass must be seated_pax * (passenger_mass_kg - checked_bag), independent of class: \
         got {}, expected {expected_seat_mass_kg}",
        summary.seat_mass_t * 1_000.0
    );
    assert!(
        (summary.bag_mass_t * 1_000.0 - expected_bag_mass_kg).abs() < 1e-6,
        "bag mass must be seated_pax * checked_bag_mass_kg: got {}, expected {expected_bag_mass_kg}",
        summary.bag_mass_t * 1_000.0
    );
}

#[test]
fn a_heavier_checked_bag_still_splits_out_of_the_same_combined_mass() {
    let (mut config, plane) = b787_config();
    config.cabin.passenger.checked_bag_mass_kg = 20.0;
    config.requirements.passenger_mass_kg = 100.0;

    let layout = build_payload_layout(&plane, &config, 0.0, 0.0).expect("B787-9 layout builds");
    let summary = passenger_summary(layout);

    let occupant_kg = summary.seat_mass_t * 1_000.0 / summary.seated_pax as f64;
    let bag_kg = summary.bag_mass_t * 1_000.0 / summary.seated_pax as f64;
    assert!(
        (occupant_kg - 80.0).abs() < 1e-6,
        "occupant share must be 100 - 20 = 80 kg, got {occupant_kg}"
    );
    assert!(
        (bag_kg - 20.0).abs() < 1e-6,
        "bag share must be the declared 20 kg, got {bag_kg}"
    );
    assert!(
        (occupant_kg + bag_kg - 100.0).abs() < 1e-6,
        "occupant plus bag must still combine to the 100 kg authority"
    );
}

#[test]
fn a_non_default_combined_mass_is_carried_through_unchanged() {
    let (mut config, plane) = b787_config();
    config.requirements.passenger_mass_kg = 95.0;

    let layout = build_payload_layout(&plane, &config, 0.0, 0.0).expect("B787-9 layout builds");
    let summary = passenger_summary(layout);

    let combined_kg =
        (summary.seat_mass_t + summary.bag_mass_t) * 1_000.0 / summary.seated_pax as f64;
    assert!(
        (combined_kg - 95.0).abs() < 1e-6,
        "combined per-seat mass must track a non-default passenger_mass_kg: got {combined_kg}"
    );
}

#[test]
fn a_declared_count_cabin_still_prices_every_seat_at_the_combined_authority() {
    // The Airbus typical two-class 150-seat A320 cabin, declared by count
    // exactly as `fixed_aircraft_mass_basis.rs` reconstructs it: the layout
    // seats exactly the declared classes, and every one of the 150 seats,
    // first or economy, is still priced at the 100 kg combined authority.
    let preset = presets::get("A320-200").expect("registered A320-200 preset");
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" }))
        .expect("A320-200 configuration");
    let cabin = &mut config.cabin.passenger;
    cabin.class_mix_mode = "count".to_owned();
    cabin.first.count = 12;
    cabin.first.pitch_m = 0.9144;
    cabin.first.width_m = 0.55;
    cabin.business.count = 0;
    cabin.premium.count = 0;
    cabin.economy.count = 138;
    cabin.economy.pitch_m = 0.7874;
    cabin.economy.width_m = 0.46;
    config.requirements.num_passengers = 150;

    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("A320-200 geometry");
    let layout = build_payload_layout(&plane, &config, 0.0, 0.0).expect("A320-200 layout builds");
    let summary = passenger_summary(layout);

    // The declared counts are the input; how many of them fit is the layout
    // engine's finding (it seats 12 first and as many of the 138 economy
    // seats as the pitch allows). This contract only requires that no seat
    // of either class is priced differently from the authority.
    let seated = summary.seated_pax;
    assert!(seated > 0, "the declared cabin must seat some passengers");
    assert!(
        seated <= 150 && summary.seated_pax + summary.unseated_pax == summary.total_pax,
        "the layout must account for every declared passenger as seated or unseated"
    );
    assert!(
        summary
            .classes
            .iter()
            .any(|(name, seats)| *name == "First" && *seats > 0),
        "the declared first-class cabin must be seated so a class premium would be visible"
    );

    let occupant_kg = summary.seat_mass_t * 1_000.0 / seated as f64;
    let bag_kg = summary.bag_mass_t * 1_000.0 / seated as f64;
    assert!(
        (occupant_kg - 84.0).abs() < 1e-6,
        "declared-count first class must not carry a per-class premium: got {occupant_kg}"
    );
    assert!(
        (bag_kg - 16.0).abs() < 1e-6,
        "declared-count cabin must still charge the standard 16 kg checked bag: got {bag_kg}"
    );
    assert!(
        (occupant_kg + bag_kg - 100.0).abs() < 1e-6,
        "every seated passenger, of any class, must combine to the 100 kg authority"
    );
}
