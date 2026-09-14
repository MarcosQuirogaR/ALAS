// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `belly_cargo_kg` is the only source of revenue freight in a passenger
//! baseline: a request of zero must carry zero, no matter how much
//! structural or hold capacity the airframe leaves free once seats and bags
//! are aboard. These regressions pin that contract for the product entry
//! point (`build_payload_layout`), and pin its one deliberate exception --
//! the frozen `*_reference_compatibility` replay used by the Python parity
//! fixtures, which auto-fills the belly to the structural cap and must keep
//! doing so.

// Failed construction is a failed test assertion.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::{
    build_payload_layout, build_payload_layout_reference_compatibility, LayoutSummary,
};

/// Loads the B787-9 preset config/plane pair, so callers only vary
/// `cabin.passenger.belly_cargo_kg`.
fn b787_config() -> (AlasConfig, alas_geom::aircraft::airplane::Airplane) {
    let preset = presets::get("B787-9").expect("registered B787-9 preset");
    let config = AlasConfig::from_value(&serde_json::json!({"preset": "B787-9"}))
        .expect("B787-9 configuration");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("B787-9 geometry");
    (config, plane)
}

/// Loads the unregistered clean-sheet configuration and its default aircraft.
///
/// No study -- clean-sheet included -- treats `requirements.num_passengers`
/// as a fixed load-case target; capacity is always resolved from the cabin
/// class-mix percentages and the candidate's actual geometry. Keeping this
/// fixture separate from [`b787_config`] still matters: a clean-sheet study
/// has no registered-aircraft source cap or certified exit layout to apply,
/// which is a real (if now identically-resolved) difference from a
/// registered aircraft.
fn clean_sheet_config() -> (AlasConfig, alas_geom::aircraft::airplane::Airplane) {
    let mut config = AlasConfig::default();
    config.requirements.cabin_preset = "Custom".to_owned();
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(None, true)
        .expect("clean-sheet geometry");
    (config, plane)
}

fn passenger_summary(
    layout: alas_payload::PayloadLayout,
) -> Box<alas_payload::layout::PassengerSummary> {
    let LayoutSummary::Passenger(summary) = layout.summary else {
        panic!("the fixture is a passenger baseline");
    };
    summary
}

#[test]
fn zero_belly_cargo_request_carries_no_revenue_freight_even_with_large_structural_headroom() {
    let (config, plane) = b787_config();
    assert_eq!(config.cabin.passenger.belly_cargo_kg, 0.0);
    // The preset's own structural cap is tens of tonnes above seats + bags,
    // which is exactly the headroom the bug used to auto-fill.
    assert!(config.requirements.max_structural_payload_kg > 40_000.0);

    let layout = build_payload_layout(&plane, &config, 0.0, 0.0).expect("B787-9 layout builds");
    let summary = passenger_summary(layout);

    assert_eq!(
        summary.belly_cargo_t, 0.0,
        "belly_cargo_kg == 0 must not auto-fill revenue freight up to the structural cap"
    );
    assert!(
        summary.payload_t * 1_000.0 < config.requirements.max_structural_payload_kg,
        "an unrequested baseline must not carry payload anywhere near the structural cap"
    );
}

#[test]
fn positive_belly_cargo_request_is_carried_up_to_the_request() {
    let (mut config, plane) = b787_config();
    config.cabin.passenger.belly_cargo_kg = 3_000.0;

    let layout = build_payload_layout(&plane, &config, 0.0, 0.0).expect("B787-9 layout builds");
    let summary = passenger_summary(layout);

    assert!(
        (summary.belly_cargo_t * 1_000.0 - 3_000.0).abs() < 1.0,
        "requested revenue freight well inside hold capacity should be delivered in full, got {} kg",
        summary.belly_cargo_t * 1_000.0
    );
}

#[test]
fn belly_cargo_request_beyond_hold_capacity_is_clamped_and_reported_as_a_shortfall() {
    let (mut config, plane) = b787_config();
    let requested_kg = 10_000_000.0;
    config.cabin.passenger.belly_cargo_kg = requested_kg;

    let layout = build_payload_layout(&plane, &config, 0.0, 0.0).expect("B787-9 layout builds");
    let summary = passenger_summary(layout);

    let carried_kg = summary.belly_cargo_t * 1_000.0;
    assert!(
        carried_kg < requested_kg,
        "an impossible request must not be silently reported as fulfilled"
    );
    assert!(
        carried_kg <= summary.hold_capacity_t * 1_000.0 + 1.0,
        "carried freight must not exceed the physical hold capacity: carried={carried_kg}, capacity={}",
        summary.hold_capacity_t * 1_000.0
    );
    let shortfall_kg = requested_kg - carried_kg;
    assert!(
        shortfall_kg > 1.0,
        "the unaccommodated remainder must be visible as requested-minus-carried, not hidden"
    );
}

#[test]
fn cabin_density_and_baggage_changes_move_the_actual_payload() {
    // No study -- clean-sheet included -- carries a fixed/exact
    // passenger-count target any more: capacity is always resolved from the
    // cabin class-mix percentages and the candidate's actual geometry,
    // exactly like the registered-aircraft contract covered by the test
    // below. A denser economy pitch seats more passengers in the same cabin
    // floor, which is the lever that actually moves capacity now that
    // `requirements.num_passengers` is a pure output.
    let (low_config, plane) = clean_sheet_config();
    assert!(low_config.preset.is_empty());
    assert_eq!(low_config.requirements.cabin_preset, "Custom");

    let mut high_config = low_config.clone();
    high_config.cabin.passenger.economy.pitch_m = 0.7112;

    let low = passenger_summary(
        build_payload_layout(&plane, &low_config, 0.0, 0.0).expect("low pax layout builds"),
    );
    let high = passenger_summary(
        build_payload_layout(&plane, &high_config, 0.0, 0.0).expect("high pax layout builds"),
    );

    assert!(
        high.seated_pax > low.seated_pax,
        "a denser economy pitch should seat more passengers: low={}, high={}",
        low.seated_pax,
        high.seated_pax
    );
    assert!(
        high.bag_mass_t > low.bag_mass_t,
        "more seated passengers must carry proportionally more checked baggage"
    );
    assert!(
        high.payload_t > low.payload_t,
        "the higher passenger count must increase total payload"
    );

    // The retired `requirements.num_passengers` target has no effect: a
    // clean-sheet study's saved passenger count must not change the
    // geometry-resolved capacity, exactly like a registered aircraft.
    let mut overridden = low_config.clone();
    overridden.requirements.num_passengers = 1;
    let overridden_summary = passenger_summary(
        build_payload_layout(&plane, &overridden, 0.0, 0.0).expect("overridden layout builds"),
    );
    assert_eq!(
        overridden_summary.seated_pax, low.seated_pax,
        "a clean-sheet study's saved passenger target must not change the resolved capacity"
    );

    // Heavier checked bags, same passengers, must also move the payload.
    let mut heavier_bags_config = low_config.clone();
    heavier_bags_config.cabin.passenger.checked_bag_mass_kg *= 2.0;
    let heavier_bags = passenger_summary(
        build_payload_layout(&plane, &heavier_bags_config, 0.0, 0.0)
            .expect("heavier-bag layout builds"),
    );
    assert!(
        heavier_bags.bag_mass_t > low.bag_mass_t,
        "doubling checked-bag mass per passenger must increase total bag mass"
    );
    assert!(
        heavier_bags.payload_t > low.payload_t,
        "heavier checked bags must increase total payload"
    );
}

#[test]
fn registered_aircraft_uses_cabin_percentages_instead_of_saved_passenger_target() {
    let (mut low_config, plane) = b787_config();
    low_config.requirements.num_passengers = 1;

    let low = passenger_summary(
        build_payload_layout(&plane, &low_config, 0.0, 0.0)
            .expect("registered low-target layout builds"),
    );

    // The B787 aircraft preset carries a Custom percentage-mode cabin seed.
    // Changing only the saved passenger target must not alter its geometry-
    // derived capacity, while changing the class percentages must alter the
    // class allocation that produces that capacity.
    let mut high_target = low_config.clone();
    high_target.requirements.num_passengers = 999;
    let high_target_summary = passenger_summary(
        build_payload_layout(&plane, &high_target, 0.0, 0.0)
            .expect("registered high-target layout builds"),
    );
    assert_eq!(
        high_target_summary.seated_pax, low.seated_pax,
        "a registered aircraft derives seats from its cabin percentage mix"
    );
    assert_eq!(
        high_target_summary.payload_t, low.payload_t,
        "a registered aircraft's saved passenger target must not change payload"
    );

    let mut shifted_mix = low_config;
    shifted_mix.cabin.passenger.business.share_pct = 60.0;
    shifted_mix.cabin.passenger.economy.share_pct = 40.0;
    let shifted = passenger_summary(
        build_payload_layout(&plane, &shifted_mix, 0.0, 0.0)
            .expect("registered shifted-mix layout builds"),
    );
    assert_ne!(
        shifted.classes, low.classes,
        "editing the Custom cabin percentages must change class allocation"
    );
}

#[test]
fn container_tare_is_counted_once_in_the_loaded_hold_mass() {
    let (mut config, plane) = b787_config();
    config.cabin.passenger.belly_cargo_kg = 2_000.0;

    let layout = build_payload_layout(&plane, &config, 0.0, 0.0).expect("B787-9 layout builds");
    let summary = passenger_summary(layout);

    assert!(
        summary.hold_ulds > 0,
        "the request should fill at least one ULD"
    );
    let net_requested_kg = summary.bag_mass_t * 1_000.0 + summary.belly_cargo_t * 1_000.0;
    let hold_used_kg = summary.hold_used_t * 1_000.0;
    let implied_tare_kg = hold_used_kg - net_requested_kg;
    // One LD3-class container's tare is order-80 kg; a `hold_ulds` count times
    // that, generously bounded, catches a tare that is being summed twice
    // while still allowing for the loose-bulk overflow item's zero tare.
    let max_plausible_tare_kg = summary.hold_ulds as f64 * 300.0;
    assert!(
        implied_tare_kg >= 0.0 && implied_tare_kg <= max_plausible_tare_kg,
        "container tare should be added exactly once: net={net_requested_kg}, hold_used={hold_used_kg}, ulds={}",
        summary.hold_ulds
    );
}

#[test]
fn reference_compatibility_replay_still_auto_fills_to_the_structural_cap() {
    let (config, plane) = b787_config();
    assert_eq!(config.cabin.passenger.belly_cargo_kg, 0.0);

    let layout = build_payload_layout_reference_compatibility(&plane, &config, 0.0, 0.0)
        .expect("B787-9 reference-compatibility layout builds");
    let summary = passenger_summary(layout);

    assert!(
        summary.belly_cargo_t * 1_000.0 > 1_000.0,
        "the frozen parity replay must keep auto-filling the belly even with belly_cargo_kg == 0"
    );
}
