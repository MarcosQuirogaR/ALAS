// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The ATR 72-600 has no under-floor hold, so its checked baggage is stowed in
//! the forward and aft main-deck compartments. The zero-fuel centre of gravity
//! of the pipeline's mass statement must therefore sit between the empty
//! aircraft and the seat centre of gravity, not behind the seats as it did
//! when the whole overflow was a single block at the aft end of the cabin.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig, DesignMode};
use alas_payload::{build_payload_layout, CabinGeometry, ItemKind, LayoutSummary};
use alas_pipeline::{assess_physical_feasibility, FullAnalysis};

/// Zero-fuel CG the frozen single aft overflow block produced, % MAC,
/// recorded from the pipeline before the compartment model.
const ZERO_FUEL_CG_WITH_SINGLE_AFT_BLOCK_PCT_MAC: f64 = 54.01;

/// Baseline OEW, zero-fuel and flown-takeoff CG in % MAC of a preset, and the
/// configuration and design they came from.
fn balance(name: &str) -> ([f64; 3], AlasConfig, alas_config::DesignVector) {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    config.mission.enabled = false;
    config.mses.enabled = false;
    config.structures.enabled = false;
    let design = presets::get(name).unwrap().design_vector;
    let report = FullAnalysis::new(config.clone())
        .run(&design, true)
        .unwrap();
    let physical = assess_physical_feasibility(&config, &design, &report, None);
    let statement = physical.mass_balance.as_ref().expect("mass statement");
    let cg = |label: &str| {
        statement
            .states
            .iter()
            .find(|state| state.label == label)
            .unwrap_or_else(|| panic!("state {label}"))
            .cg_pct_mac
    };
    (
        [cg("operating empty"), cg("zero fuel"), cg("flown takeoff")],
        config,
        design,
    )
}

/// Published A320 centre-of-gravity range for pavement analysis, % MAC
/// [S Airbus A320 Aircraft Characteristics (ACAP), pavement-analysis CG range
/// 17 % to 40 % MAC].
const A320_ACAP_CG_BAND_PCT_MAC: [f64; 2] = [17.0, 40.0];

/// Tolerance of the ZFW-to-OEW CG equality, % MAC. The TargetCg baggage policy
/// places the hold baggage so the zero-fuel CG follows the empty-aircraft CG;
/// the residual is the discrete container positions [E].
const TARGET_CG_TOLERANCE_PCT_MAC: f64 = 1.0;

/// Under the default TargetCg baggage policy, aircraft whose holds have
/// container positions stow the baggage so the zero-fuel CG stays at the
/// operating-empty CG. The flown takeoff CG of the A320 stays inside the
/// published ACAP band.
#[test]
fn aircraft_with_lower_holds_keep_their_zero_fuel_and_takeoff_cg() {
    for name in ["A320-200", "A220-300"] {
        let ([oew_cg, zfw_cg, tow_cg], _, _) = balance(name);
        assert!(
            (zfw_cg - oew_cg).abs() < TARGET_CG_TOLERANCE_PCT_MAC,
            "{name} ZFW CG {zfw_cg:.2} vs OEW CG {oew_cg:.2} %MAC"
        );
        if name == "A320-200" {
            let [lo, hi] = A320_ACAP_CG_BAND_PCT_MAC;
            assert!(
                (lo..=hi).contains(&tow_cg),
                "{name} TOW CG {tow_cg:.2} %MAC outside ACAP [{lo}, {hi}]"
            );
        }
    }
}

#[test]
fn atr_zero_fuel_cg_lies_between_the_empty_aircraft_and_the_seat_cg() {
    let ([oew_cg, zfw_cg, _], config, design) = balance("ATR72-600");

    // Seat centre of gravity of the same layout, % MAC: the payload cannot
    // sit further aft than its seats once the bags straddle them.
    let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), true)
        .unwrap();
    let g = CabinGeometry::new(
        &plane,
        &config.geometry,
        config.cabin.passenger.wall_thickness_m,
    )
    .unwrap();
    let layout = build_payload_layout(&plane, &config, 0.0, 0.0).unwrap();
    let seats: Vec<_> = layout
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::SeatRow)
        .collect();
    let seat_mass: f64 = seats.iter().map(|item| item.mass).sum();
    let seat_cg =
        g.x_to_pct_mac(seats.iter().map(|item| item.mass * item.x).sum::<f64>() / seat_mass);
    let LayoutSummary::Passenger(summary) = &layout.summary else {
        panic!("passenger layout");
    };

    assert!(
        zfw_cg > oew_cg,
        "payload moves the CG aft of the empty aircraft"
    );
    assert!(
        zfw_cg < ZERO_FUEL_CG_WITH_SINGLE_AFT_BLOCK_PCT_MAC,
        "ZFW CG {zfw_cg:.2} %MAC is not forward of the single-block result"
    );
    assert!(
        zfw_cg < seat_cg,
        "ZFW CG {zfw_cg:.2} %MAC must lie forward of the seat CG {seat_cg:.2} %MAC"
    );
    assert!(summary.hold_compartment_masses_kg.len() >= 2);
}
