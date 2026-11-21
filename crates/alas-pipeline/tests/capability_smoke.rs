// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Capability smoke test, family `pipeline`: a registered preset runs the full
//! aircraft analysis at its baseline design, with no search, and returns
//! component masses and a closed geometry summary of plausible size.
//!
//! The bounds are plausibility floors for a transport near 78 t, not a
//! calibration target: the point is that the path runs end to end.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig, DesignMode};
use alas_pipeline::quick_analysis::report_oew_kg;
use alas_pipeline::FullAnalysis;

#[test]
fn capability_smoke_pipeline_baseline_analysis_runs_for_a_preset() {
    let preset = presets::get("A320-200").unwrap();
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    let mtow_kg = config.requirements.mtow_kg;

    let report = FullAnalysis::new(config)
        .run(&preset.design_vector, true)
        .unwrap();
    let oew_kg = report_oew_kg(&report);
    assert!(
        oew_kg > 0.35 * mtow_kg && oew_kg < 0.70 * mtow_kg,
        "OEW {oew_kg} kg for MTOW {mtow_kg} kg"
    );
}
