// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The landing-mass limit a report is judged against is the design landing
//! mass the sized run designed its gear for, including the `ZFW + reserves`
//! floor of the MTOW band and payload-adjusted modes.

// A test asserts on values it constructed here directly, so a failed unwrap
// is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig, DesignMode, MtowSizing};
use alas_pipeline::feasibility::landing_mass_limit_kg;
use alas_pipeline::FullAnalysis;

#[test]
fn a_floored_sized_report_reads_the_floored_design_landing_mass() {
    let preset = presets::get("A320-200").unwrap();
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
    config.optimizer.design_space.mode = DesignMode::ReferenceAdaptation;
    config.optimizer.objective.mtow_sizing = MtowSizing::MtowBand;
    let closure_kg = 0.8 * config.requirements.mtow_kg;
    let ratio_kg = config.design_landing_mass_at_closure(closure_kg);
    let floor_kg = 0.95 * closure_kg;
    assert!(floor_kg > ratio_kg, "the floor must bind for the scenario");

    let floored = FullAnalysis::new(config.clone())
        .run_at_sized_design_weights(&preset.design_vector, true, closure_kg, Some(floor_kg))
        .unwrap();
    assert_eq!(floored.design_landing_mass_kg(), Some(floor_kg));
    assert_eq!(landing_mass_limit_kg(&config, &floored), floor_kg);

    let unfloored = FullAnalysis::new(config.clone())
        .run_at_sized_design_weights(&preset.design_vector, true, closure_kg, None)
        .unwrap();
    assert_eq!(landing_mass_limit_kg(&config, &unfloored), ratio_kg);

    let baseline = FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .unwrap();
    assert_eq!(baseline.design_landing_mass_kg(), None);
    assert_eq!(
        landing_mass_limit_kg(&config, &baseline),
        config.landing_mass_limit_kg(config.requirements.mtow_kg)
    );
}
