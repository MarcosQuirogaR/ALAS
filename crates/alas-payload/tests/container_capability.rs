// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The lower-deck container capability of a layout, distinct from its
//! as-configured installation. Numerical verification of the position grid
//! against the positions printed by the type's airport-planning document; not
//! a validation against a weight-and-balance manual.

#![allow(clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::cargo::CargoCapacity;
use alas_payload::LayoutSummary;

fn hold_capacity(name: &str) -> CargoCapacity {
    let preset = presets::get(name).expect("registered preset");
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).expect("config");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("preset geometry");
    let layout = alas_payload::build_payload_layout(&plane, &config, 0.0, 0.0).expect("payload");
    match layout.summary {
        LayoutSummary::Passenger(summary) => summary.hold_capacity,
        LayoutSummary::Cargo(summary) => summary.installed,
    }
}

/// Airbus A320 Aircraft Characteristics 2-6-0 pp2-3: the as-delivered hold of the
/// preset is loose-bulk (no installed container position); the container fit
/// takes three forward and four aft LD3-45W.
#[test]
fn the_bulk_a320_reports_seven_container_positions_as_capability_and_none_installed() {
    let capacity = hold_capacity("A320-200");
    assert_eq!(capacity.lower_deck_uld_positions, 0);
    assert_eq!(capacity.lower_deck_container_capable_positions, 7);
}

/// Loose-bulk types with no container option in their sources keep a zero
/// capability, and a container-loaded type's capability is its installation.
#[test]
fn capability_is_zero_without_a_container_option_and_the_installation_when_containerized() {
    for name in ["A220-300", "E195-E2", "LJ60"] {
        let capacity = hold_capacity(name);
        assert_eq!(capacity.lower_deck_container_capable_positions, 0, "{name}");
    }
    let capacity = hold_capacity("A340-300");
    assert!(capacity.lower_deck_uld_positions > 0);
    assert_eq!(
        capacity.lower_deck_container_capable_positions,
        capacity.lower_deck_uld_positions
    );
}
