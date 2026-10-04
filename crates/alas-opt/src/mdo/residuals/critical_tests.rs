// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use crate::mdo::ResidualRole;

use super::*;

#[test]
fn critical_stability_does_not_alias_the_fast_clean_neutral_point() {
    let mut config = AlasConfig::from_value(&serde_json::json!({
        "preset":"B787-9", "optimizer":{"design_space":{"mode":"reference_adaptation"}}
    }))
    .unwrap();
    config.analysis.chordwise_resolution = 4;
    let design = alas_config::presets::get("B787-9").unwrap().design_vector;
    let mut outcome = super::super::sizing::run_candidate(&config, &design.to_array())
        .unwrap_or_else(|failure| panic!("{}", failure.reason));
    let original = balance_residuals(&outcome, &config, ResidualRole::Constraint);
    let critical = original
        .iter()
        .find(|r| r.id == "static_margin_floor")
        .unwrap()
        .actual;
    assert!(critical.is_finite());
    // A contaminated clean-probe result must not move the independent
    // multi-condition boundary. The former duplicated argument did exactly so.
    outcome.x_np += 100.0;
    let changed = balance_residuals(&outcome, &config, ResidualRole::Constraint);
    assert_eq!(
        changed
            .iter()
            .find(|r| r.id == "static_margin_floor")
            .unwrap()
            .actual,
        critical
    );

    config.requirements.cruise_mach = f64::NAN;
    let unavailable = balance_residuals(&outcome, &config, ResidualRole::Constraint);
    assert!(unavailable
        .iter()
        .any(|r| r.id == "cg_model_error" && r.normalized_violation > 0.0));
}
