// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The ATR 72-600 has no under-floor hold, so its checked baggage is stowed in
//! the forward and aft main-deck compartments, ahead of and behind the seats.
//! The zero-fuel centre of gravity of the pipeline's mass statement must
//! therefore sit well forward of where it did when the whole overflow was a
//! single block at the aft end of the cabin.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig, DesignMode};
use alas_payload::{build_payload_layout, ItemKind, LayoutSummary};
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
/// the residual is the discrete container positions and the forward reach of
/// the forward hold relative to the wing [E]: the A220-300 wing datum sits
/// 0.66 m further forward with its drawn 29.5 deg leading-edge sweep, so its
/// empty-aircraft CG lies near the forward edge of what the holds can trim to.
const TARGET_CG_TOLERANCE_PCT_MAC: f64 = 1.5;

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
fn atr_bags_straddle_the_seats_and_keep_the_zero_fuel_cg_off_the_tail() {
    let ([oew_cg, zfw_cg, _], config, design) = balance("ATR72-600");

    let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), true)
        .unwrap();
    let layout = build_payload_layout(&plane, &config, 0.0, 0.0).unwrap();
    let rows = || {
        layout
            .items
            .iter()
            .filter(|item| item.kind == ItemKind::SeatRow)
    };
    let seat_start = rows()
        .map(|item| item.x - 0.5 * item.length)
        .fold(f64::INFINITY, f64::min);
    let seat_end = rows()
        .map(|item| item.x + 0.5 * item.length)
        .fold(f64::NEG_INFINITY, f64::max);
    let bags: Vec<f64> = layout
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::Bag)
        .map(|item| item.x)
        .collect();
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
    // The bags are stowed in the main-deck compartments ahead of and behind
    // the seats, never among them, and both compartments carry some.
    assert!(summary.hold_compartment_masses_kg.len() >= 2);
    assert!(
        bags.iter().any(|&x| x < seat_start),
        "no bag ahead of the seats"
    );
    assert!(
        bags.iter().any(|&x| x > seat_end),
        "no bag behind the seats"
    );
    assert!(bags.iter().all(|&x| x < seat_start || x > seat_end));
}

/// The route payload of the unchanged registered ATR 72-600 never takes the
/// zero-fuel mass above its published MZFW, 21,000 kg [S EASA TCDS A.084
/// Issue 14, III.13.b, Mod 6219]. When the laid-out seats and bags would, the
/// load is offloaded to `MZFW - modeled OEW` and the analysis says so.
#[test]
fn atr_route_payload_stays_within_the_published_zero_fuel_mass() {
    const ATR_MZFW_KG: f64 = 21_000.0;
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "ATR72-600" })).unwrap();
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    config.mission.enabled = false;
    config.mses.enabled = false;
    config.structures.enabled = false;
    let design = presets::get("ATR72-600").unwrap().design_vector;
    let report = FullAnalysis::new(config.clone())
        .run(&design, true)
        .unwrap();
    let masses = &report.component_masses;
    let zero_fuel_kg = masses.values().sum::<f64>() - masses["Fuel"];
    assert!(
        zero_fuel_kg <= ATR_MZFW_KG * (1.0 + 1e-9),
        "zero-fuel mass {zero_fuel_kg:.1} kg above the published MZFW"
    );
    let physical = assess_physical_feasibility(&config, &design, &report, None);
    let offloaded = report.geometry_summary.get("route_payload_offloaded_kg");
    let capped_finding = physical.findings.iter().any(|finding| {
        finding.code == alas_pipeline::feasibility::FindingCode::StructuralPayloadLimitViolation
    });
    assert_eq!(offloaded.is_some(), capped_finding);
}
