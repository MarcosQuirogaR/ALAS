// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Who owns which cargo mass (clarified ledger App Features 2, decision D10).
//!
//! Three different quantities share the word "cargo" in a freighter study and
//! the requirement is that they stay three:
//!
//! * `requirements.cargo_objective_kg` - the mass the *user asked for*. Only
//!   the user writes it, so it survives cabin-preset application and
//!   save/load, and it is only ever read as a scoring target.
//! * `requirements.cargo_payload_kg` - the *capacity* the deck configuration
//!   is sized for, which a cabin preset computes and overwrites, and which
//!   the load case asks the hold for.
//! * the layout's loaded payload - what the hold *actually took*, capped by
//!   the physical slots.
//!
//! These tests hold the boundaries between them. The scoring that consumes
//! the target lives in `alas-opt` (`tests/cargo_target_objective.rs`).

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap or expect there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};

/// A requested objective no computed capacity can coincide with to the
/// kilogram, so "the preset did not touch it" is a real assertion.
const REQUESTED_OBJECTIVE_KG: f64 = 61_234.5;

fn freighter() -> AlasConfig {
    AlasConfig::from_value(&serde_json::json!({
        "preset": "AVE",
        "requirements": {
            "aircraft_type": "cargo",
            "cabin_preset": "Max payload",
            "cargo_objective_kg": REQUESTED_OBJECTIVE_KG,
        },
    }))
    .unwrap_or_else(|error| panic!("freighter configuration: {error}"))
}

#[test]
fn a_cabin_preset_rewrites_the_capacity_and_never_the_requested_objective() {
    let mut config = freighter();
    let design = presets::get("AVE")
        .expect("registered preset")
        .design_vector;
    let configured_capacity_kg = config.requirements.cargo_payload_kg;

    alas_payload::apply_cabin_preset(&mut config, Some(&design))
        .unwrap_or_else(|error| panic!("cabin preset: {error}"));

    // The preset owns the capacity and did its job on it.
    let preset_capacity_kg = config.requirements.cargo_payload_kg;
    assert!(
        preset_capacity_kg.is_finite() && preset_capacity_kg > 0.0,
        "the Max payload preset sized the hold at {preset_capacity_kg} kg"
    );
    assert_ne!(
        preset_capacity_kg, configured_capacity_kg,
        "the preset is expected to overwrite the capacity; that is the whole \
         reason the requested objective cannot live in that field"
    );

    // The request is untouched, and it is the mass the objective scores.
    assert_eq!(
        config.requirements.cargo_objective_kg,
        REQUESTED_OBJECTIVE_KG
    );
    assert_eq!(
        config.requirements.cargo_target_kg(),
        REQUESTED_OBJECTIVE_KG
    );
    assert!(
        (preset_capacity_kg - REQUESTED_OBJECTIVE_KG).abs() > 1.0,
        "capacity {preset_capacity_kg} kg and request {REQUESTED_OBJECTIVE_KG} kg \
         must remain distinguishable for this test to mean anything"
    );
}

#[test]
fn the_requested_objective_survives_a_save_and_load_round_trip() {
    let mut config = freighter();
    let design = presets::get("AVE")
        .expect("registered preset")
        .design_vector;
    alas_payload::apply_cabin_preset(&mut config, Some(&design))
        .unwrap_or_else(|error| panic!("cabin preset: {error}"));

    let saved = serde_json::to_value(&config).unwrap();
    assert_eq!(
        saved["requirements"]["cargo_objective_kg"],
        REQUESTED_OBJECTIVE_KG
    );
    let loaded = AlasConfig::from_value(&saved).expect("the saved study loads");
    assert_eq!(
        loaded.requirements.cargo_objective_kg,
        REQUESTED_OBJECTIVE_KG
    );
    assert_eq!(
        loaded.requirements.cargo_target_kg(),
        REQUESTED_OBJECTIVE_KG
    );
    assert_eq!(
        loaded.requirements.cargo_payload_kg,
        config.requirements.cargo_payload_kg
    );
}

#[test]
fn the_load_case_asks_the_hold_for_the_capacity_not_the_requested_objective() {
    // The request must never be treated as achieved capacity: what the hold
    // takes is what the load case asked of it, capped by physical slots, and
    // the objective is not part of that chain at all.
    let preset = presets::get("AVE").expect("registered preset");
    let mut config = freighter();
    alas_payload::apply_cabin_preset(&mut config, Some(&preset.design_vector))
        .unwrap_or_else(|error| panic!("cabin preset: {error}"));

    let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("preset geometry");
    let geometry = alas_payload::CabinGeometry::new(
        &plane,
        &config.geometry,
        config.cabin.passenger.wall_thickness_m,
    )
    .expect("preset cabin geometry");

    let asked_of_the_hold_kg = 1_000.0;
    let requirements = alas_config::DesignRequirements {
        cargo_payload_kg: asked_of_the_hold_kg,
        cargo_objective_kg: REQUESTED_OBJECTIVE_KG,
        ..config.requirements.clone()
    };
    let layout =
        alas_payload::build_cargo_layout(&geometry, &config.cabin.cargo, &requirements, 0.0, 0.0);
    let alas_payload::LayoutSummary::Cargo(summary) = layout.summary else {
        panic!("a freighter load case produces a cargo summary");
    };

    let loaded_kg = summary.loaded_net_payload_t * 1_000.0;
    let capacity_kg = summary.capacity_t * 1_000.0;
    assert!(
        capacity_kg > REQUESTED_OBJECTIVE_KG,
        "this hold ({capacity_kg} kg) must be able to take more than the \
         {REQUESTED_OBJECTIVE_KG} kg request for the test to be about the \
         request rather than about the slots"
    );
    assert!(
        (loaded_kg - asked_of_the_hold_kg).abs() < 1.0,
        "the hold took {loaded_kg} kg; it was asked for {asked_of_the_hold_kg} kg \
         and the {REQUESTED_OBJECTIVE_KG} kg objective must not have loaded it"
    );
}
