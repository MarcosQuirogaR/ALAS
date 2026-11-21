// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The cargo loader states when the structural payload limit, not the
//! positions, held the deck below the request.
//!
//! `requirements.max_structural_payload_kg` is gross (MZFW less OEW, unit
//! load devices included) and `cargo_payload_kg` is net, so a request equal
//! to the limit cannot be carried with its tare: the loader lowers the net
//! load until the gross fits, and the summary must say which limit did it so
//! the report reads the offload as a structural payload limit, not as a
//! shortfall of positions.

// Failed construction is a failed test assertion.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::{build_payload_layout, LayoutSummary};

/// The B747-400 freighter variant of the optimisation grid: 112,627 kg of
/// net cargo requested against a declared structural payload of the same
/// 112,627 kg (gross, so the requested freight and its tare cannot both fit).
fn b747f() -> (AlasConfig, alas_geom::aircraft::airplane::Airplane) {
    let preset = presets::get("B747-400").expect("registered B747-400 preset");
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "B747-400",
        "requirements": {
            "mtow_kg": 396_893.0,
            "aircraft_type": "cargo",
            "cabin_preset": "Custom",
            "num_passengers": 0,
            "cargo_payload_kg": 112_627.0,
            "max_structural_payload_kg": 112_627.0,
            "mlw_kg": 295_742.0
        }
    }))
    .expect("B747-400 freighter configuration");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("B747-400 geometry");
    (config, plane)
}

fn cargo_summary(layout: alas_payload::PayloadLayout) -> Box<alas_payload::layout::CargoSummary> {
    let LayoutSummary::Cargo(summary) = layout.summary else {
        panic!("the fixture is a freighter");
    };
    summary
}

#[test]
fn a_request_whose_tare_crosses_the_structural_limit_is_held_to_it_and_says_so() {
    let (config, plane) = b747f();
    let limit_kg = config.requirements.max_structural_payload_kg;
    assert!(config.requirements.cargo_payload_kg >= limit_kg);
    let summary = cargo_summary(build_payload_layout(&plane, &config, 0.0, 0.0).unwrap());
    assert_eq!(summary.held_to_structural_limit_kg, Some(limit_kg));
    assert!(summary.payload_t * 1_000.0 <= limit_kg * (1.0 + 1.0e-9));
    assert!(summary.loaded_net_payload_t < summary.requested_net_payload_t);
    // The positions are not what stopped the load.
    assert!(summary.capacity_t * 1_000.0 > config.requirements.cargo_payload_kg);
}

#[test]
fn a_request_under_the_limit_is_not_marked_as_held() {
    let (mut config, plane) = b747f();
    config.requirements.cargo_payload_kg = 60_000.0;
    let summary = cargo_summary(build_payload_layout(&plane, &config, 0.0, 0.0).unwrap());
    assert_eq!(summary.held_to_structural_limit_kg, None);
    assert!((summary.loaded_net_payload_t - 60.0).abs() < 1.0e-6);
}
