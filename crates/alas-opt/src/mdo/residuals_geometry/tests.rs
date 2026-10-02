// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A test asserts on values it built here, so a failed unwrap is the assertion
// failing rather than a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::mdo::sizing::run_candidate;
use alas_config::AerodromeReferenceCode;

fn config(preset: &str, mode: &str) -> AlasConfig {
    AlasConfig::from_value(&serde_json::json!({
        "preset": preset,
        "optimizer": {"design_space": {"mode": mode}}
    }))
    .unwrap()
}

fn nominal(preset: &str) -> DesignVector {
    alas_config::presets::get(preset).unwrap().design_vector
}

/// The geometry residuals of `dv` under `config`, keyed by identifier.
fn residuals(config: &AlasConfig, dv: &DesignVector) -> Vec<ConstraintResidual> {
    let outcome = run_candidate(config, &dv.to_array()).unwrap_or_else(|f| panic!("{}", f.reason));
    geometry_residuals(
        &outcome,
        config,
        &config.optimizer.weights,
        ConstraintPolicy::Hard,
        0,
        0.0,
    )
}

fn violated(residuals: &[ConstraintResidual], id: &str) -> bool {
    residuals
        .iter()
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("no {id} residual"))
        .normalized_violation
        > 0.0
}

#[test]
fn every_nominal_preset_meets_its_own_span_and_washout_requirements() {
    for preset in ["A320-200", "B787-9", "ATR72-600", "A380-800"] {
        let table = residuals(&config(preset, "reference_adaptation"), &nominal(preset));
        assert!(!violated(&table, "span"), "{preset}");
        assert!(!violated(&table, "panel_washout_max"), "{preset}");
        assert!(!violated(&table, "tip_washout_max"), "{preset}");
    }
}

#[test]
fn the_a320_product_winner_of_the_audit_is_rejected_for_span_and_outboard_wash_in() {
    // Winner of the Phase 4 matrix run (seed 20260922), which left the code C
    // band (39.09 m) and turned the outboard panel to +0.47 deg of wash-in.
    let mut winner = nominal("A320-200");
    winner.span_m = 39.093_631_058_533_695;
    winner.root_chord_m = 6.608_167_387_797_838;
    winner.break_chord_m = 3.113_932_051_994_131;
    winner.tip_chord_m = 1.279_436_160_182_117_4;
    winner.sweep_deg = 28.927_608_974_082_76;
    winner.tip_twist_deg = 1.474_186_243_917_152_5;
    winner.wing_x_shift_m = -0.685_372_217_876_921_9;
    winner.airfoil_thickness_scale = 0.921_552_300_687_120_6;
    winner.airfoil_camber_scale = 0.912_545_167_546_212_5;
    let table = residuals(&config("A320-200", "reference_adaptation"), &winner);
    assert!(violated(&table, "span"));
    let panel = table.iter().find(|r| r.id == "panel_washout_max").unwrap();
    assert!(panel.normalized_violation > 0.0);
    // Tip incidence 1.474 deg over the break's preset 1.0 deg.
    assert!((panel.actual - 0.474).abs() < 1e-3, "{}", panel.actual);
    // The root-to-tip window alone passes it: the wing is still washed out.
    assert!(!violated(&table, "tip_washout_max"));
}

#[test]
fn a_tip_set_above_the_break_is_wash_in_even_when_the_wing_is_washed_out() {
    let preset = "A320-200";
    let break_twist_deg = alas_config::presets::get(preset)
        .unwrap()
        .geometry
        .wing
        .break_twist_deg;
    let mut dv = nominal(preset);
    dv.tip_twist_deg = break_twist_deg + 0.3;
    let table = residuals(&config(preset, "reference_adaptation"), &dv);
    assert!(violated(&table, "panel_washout_max"));
    assert!(!violated(&table, "tip_washout_max"));
    dv.tip_twist_deg = break_twist_deg;
    let table = residuals(&config(preset, "reference_adaptation"), &dv);
    assert!(!violated(&table, "panel_washout_max"));
}

#[test]
fn the_span_band_is_open_at_the_top() {
    // ICAO Annex 14 Table 1-1: a wingspan of exactly 36 m is code D.
    let preset = "A320-200";
    let config = config(preset, "reference_adaptation");
    let mut dv = nominal(preset);
    dv.span_m = 35.98;
    assert!(!violated(&residuals(&config, &dv), "span"));
    dv.span_m = 36.0;
    assert!(violated(&residuals(&config, &dv), "span"));
}

#[test]
fn a_clean_sheet_is_limited_by_the_users_letter() {
    let preset = "A320-200";
    let mut config = config(preset, "clean_sheet");
    let dv = nominal(preset);
    config.optimizer.objective.aerodrome_reference_code = AerodromeReferenceCode::C;
    assert!(!violated(&residuals(&config, &dv), "span"));
    config.optimizer.objective.aerodrome_reference_code = AerodromeReferenceCode::B;
    assert!(violated(&residuals(&config, &dv), "span"));
    config.optimizer.objective.aerodrome_reference_code = AerodromeReferenceCode::Unrestricted;
    assert!(residuals(&config, &dv).iter().all(|r| r.id != "span"));
}
