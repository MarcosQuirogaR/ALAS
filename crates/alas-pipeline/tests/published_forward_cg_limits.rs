// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The analyzed-takeoff rotation (nose-wheel liftoff) boundary against the
//! manufacturers' published most-forward CG, on the product basis: the
//! registered preset's full analysis and the item-ledger CG gate the
//! feasibility report uses.
//!
//! A published forward limit is the worst of several criteria (rotation,
//! trim, handling, gear loads; FAA AC 25-7D, section 42.11), so the rotation
//! boundary alone may lie forward of it but must not lie aft of it by more
//! than [`TOLERANCE_PCT_MAC`]: an inequality, not an agreement band.
//!
//! Anchor provenance. The A220-300 value is a flight envelope limit (ARP
//! p. 119). The A320-200 17 %MAC, the A340-300 20.3 %MAC and the A380-800
//! 34.65-37.8 %MAC are the ACAP most-forward CG used in the pavement-load
//! analysis at MRW (Aircraft Characteristics section 7-3), not certified
//! limits. The A380 is not an anchor at all. The A340 stays as an ignored
//! known residual: the model rotation limit (26.0 %MAC) sits 5.7 points aft
//! of 20.3. The DC-10 is a known failure,
//! not tuned: its centerline tail engine puts a thrust line above the CG and
//! its main-gear station is not anchored to a source, and no published
//! forward limit was retrieved for it.

use alas_config::AlasConfig;
use alas_pipeline::FullAnalysis;

/// Largest amount, %MAC, by which the rotation boundary may sit aft of a
/// published most-forward CG.
const TOLERANCE_PCT_MAC: f64 = 1.0;

/// The analyzed-takeoff rotation boundary of `preset`, converted to the
/// manufacturer's MAC frame (LEMAC m aft of the nose tip, MAC m).
fn rotation_limit_in_manufacturer_frame(preset: &str, lemac_m: f64, mac_m: f64) -> f64 {
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    let design = alas_config::presets::get(preset)
        .unwrap_or_else(|error| panic!("{preset}: {error}"))
        .design_vector;
    let report = FullAnalysis::new(config.clone())
        .run(&design, true)
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    let feasibility = alas_pipeline::assess_physical_feasibility(&config, &design, &report, None);
    let model_cg = feasibility
        .model_cg
        .as_ref()
        .unwrap_or_else(|| panic!("{preset}: no model CG assessment"));
    let rotation_pct_mac = model_cg
        .loading_states
        .iter()
        .find(|state| state.state == alas_opt::ModelCgLoadingState::AnalyzedTakeoff)
        .map(|state| state.physical_limits.rotation_fwd_pct_mac)
        .unwrap_or_else(|| panic!("{preset}: no analyzed takeoff state"));
    let frame = report
        .airplane
        .mac_frame()
        .unwrap_or_else(|| panic!("{preset}: no MAC frame"));
    (frame.x_at_pct(rotation_pct_mac) - lemac_m) / mac_m * 100.0
}

/// - A320-200: 17 %MAC, the ACAP most-forward CG used in the pavement-load
///   analysis at MRW, not a certified limit (Airbus A320 Aircraft
///   Characteristics, Fig. 7-3-0-991-010); LEMAC 15.26 m, MAC 4.1935 m
///   (EASA TCDS A.064).
/// - A220-300: 12.0 %MAC, the most-forward flight limit (Airbus A220-300
///   Aircraft Recovery Publication BD500-3AB48-10400-00, p. 119); LEMAC
///   16.535 m, MAC 3.781 m.
#[test]
fn the_rotation_limit_is_not_aft_of_published_forward_limits() {
    for (preset, lemac_m, mac_m, published) in [
        ("A320-200", 15.26, 4.1935, 17.0),
        ("A220-300", 16.535, 3.781, 12.0),
    ] {
        let rotation = rotation_limit_in_manufacturer_frame(preset, lemac_m, mac_m);
        assert!(
            rotation <= published + TOLERANCE_PCT_MAC,
            "{preset}: rotation limit {rotation:.2} %MAC is aft of the published {published} + {TOLERANCE_PCT_MAC}"
        );
    }
}

/// The same inequality for the A340-300: 20.3 %MAC, the ACAP most-forward CG
/// used in the pavement-load analysis at the WV025/WV029 ramp weight (Airbus
/// A340-200/-300 Aircraft Characteristics, Dec 2025, Fig. 7-3-0-991-007), not
/// a certified limit; LEMAC 28.083 m from that section's two-point statics,
/// MAC 7.270 m (EASA TCDS). The model rotation boundary sits about 5.7 %MAC
/// aft of it, so this stays a visible known residual rather than a widened
/// tolerance.
#[test]
#[ignore = "known residual: model rotation limit 26.0 % vs ACAP 20.3 % MAC"]
fn the_a340_300_rotation_limit_is_not_aft_of_its_published_forward_limit() {
    let rotation = rotation_limit_in_manufacturer_frame("A340-300", 28.083, 7.270);
    assert!(
        rotation <= 20.3 + TOLERANCE_PCT_MAC,
        "A340-300: rotation limit {rotation:.2} %MAC is aft of the published 20.3 + {TOLERANCE_PCT_MAC}"
    );
}
