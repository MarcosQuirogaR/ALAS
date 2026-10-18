// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The mission planner holds its cruise levels under the max-cruise-thrust
//! ceiling (owner decision 2026-10-14), by the band rule of the
//! `cruise_thrust` check (`ThrustAssessment::flyable`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::AlasConfig;
use alas_opt::assess_product_candidate;
use alas_opt::mdo::candidate_mission_model;

/// A thrust margin demanded on top of the drag (negative band): the ceiling
/// then binds well below the level the 300 ft/min rule alone allows.
const DEMANDED_MARGIN_BAND: f64 = -0.25;

#[test]
fn a_tighter_ceiling_lowers_the_planned_level_and_the_nominal_one_does_not_bind() {
    let config = AlasConfig::from_value(&serde_json::json!({"preset": "DC-10"})).unwrap();
    let design = alas_config::presets::get("DC-10").unwrap().design_vector;
    let assessment = assess_product_candidate(&config, &design).unwrap();
    let artifacts = &assessment.sized.fuel_artifacts;
    let plan = artifacts.frozen_plan.as_ref().unwrap();
    let mass_kg = assessment.sized.takeoff_mass_kg;
    let freeze = |band: Option<f64>| {
        candidate_mission_model(&config, artifacts)
            .unwrap()
            .with_cruise_ceiling_band(band)
            .freeze_plan(mass_kg, plan.range_m, &plan.policy)
            .unwrap()
            .cruise_altitude_m
    };
    let (off, nominal, tight) = (
        freeze(None),
        freeze(Some(0.0)),
        freeze(Some(DEMANDED_MARGIN_BAND)),
    );
    // The DC-10 flies below its ceiling at the nominal band (the check passes
    // at every planned level), so the ceiling changes nothing; demanding 25 %
    // thrust margin lowers the level.
    assert!((nominal - off).abs() < 1.0, "{nominal} against {off} m");
    assert!(tight < nominal - 300.0, "{tight} against {nominal} m");
}

#[test]
fn the_check_holds_at_every_level_the_planner_flies() {
    for name in ["DC-10", "B747-400", "A380-800", "B787-9", "A340-300"] {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": name})).unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let assessment = assess_product_candidate(&config, &design).unwrap();
        let row = assessment
            .residuals
            .iter()
            .find(|row| row.id == "cruise_thrust")
            .unwrap();
        assert!(!row.violated(), "{name}: {row:?}");
        assert!(
            row.detail.as_deref().unwrap().contains("the plan's"),
            "{name}"
        );
    }
}
