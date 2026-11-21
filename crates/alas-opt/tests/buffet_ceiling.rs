// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The mission planner holds its cruise levels, initial and step, under the
//! wing's 1.3 g buffet ceiling at the current mass (Airbus, *Getting to Grips
//! with Aircraft Performance*, 2002, "Maximum altitude"), on top of the
//! 300 ft/min residual climb and the thrust ceiling.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::AlasConfig;
use alas_opt::assess_product_candidate;
use alas_opt::mdo::candidate_mission_model;

/// Load-factor floor of the buffet margin, g.
const FLOOR_G: f64 = 1.3;

#[test]
fn a_lower_onset_lift_lowers_the_planned_level_to_its_buffet_ceiling() {
    let config = AlasConfig::from_value(&serde_json::json!({"preset": "B787-9"})).unwrap();
    let design = alas_config::presets::get("B787-9").unwrap().design_vector;
    let assessment = assess_product_candidate(&config, &design).unwrap();
    let artifacts = &assessment.sized.fuel_artifacts;
    let cl_b = artifacts
        .buffet_onset_cl
        .expect("the closure carries the wing's onset lift at the cruise Mach");
    let plan = artifacts.frozen_plan.as_ref().unwrap();
    let mass_kg = assessment.sized.takeoff_mass_kg;
    let model = candidate_mission_model(&config, artifacts).unwrap();
    assert_eq!(model.buffet_onset_cl, Some(cl_b));
    let level = |cl: Option<f64>| {
        model
            .clone()
            .with_buffet_onset_cl(cl)
            .freeze_plan(mass_kg, plan.range_m, &plan.policy)
            .unwrap()
            .cruise_altitude_m
    };
    let (carried, low) = (level(Some(cl_b)), level(Some(0.6 * cl_b)));
    assert!(
        low < carried - 300.0,
        "a 40 % lower onset lift: {low} m against {carried} m"
    );
    // At the planner's top-of-climb mass estimate (0.985 of brake release,
    // Raymer 6th ed. Sec. 6.3) the lowered level keeps the 1.3 g margin.
    let lowered = model.clone().with_buffet_onset_cl(Some(0.6 * cl_b));
    let n = lowered
        .buffet_load_factor(0.985 * mass_kg, low, lowered.cruise_tas_m_s)
        .unwrap();
    assert!(n >= FLOOR_G - 1.0e-3, "{n} g at {low} m");
    // No bound at all never flies lower than the carried one.
    assert!(level(None) >= carried - 1.0);
    // The reported initial cruise altitude capability of the same trip is
    // held under the same buffet level, read at the cruise speed: with the
    // margin read at the climb speed below the crossover no level qualified
    // and the capability was missing (DC-10, joint run 9).
    let capability = lowered
        .initial_cruise_capability_m(mass_kg, plan.range_m, 13_000.0)
        .unwrap()
        .expect("a level holds both the 300 ft/min and the 1.3 g criteria");
    // The planner estimates the top-of-climb mass as 0.985 of brake release;
    // the capability burns the integrated climb fuel, less to a low level, so
    // its heavier top-of-climb mass holds 1.3 g a little lower, never higher
    // than the planner's continuous level. The planned level is the legal
    // flight level at or below that (ICAO Annex 2, App. 3: 1,000 ft apart
    // below FL410, 2,000 ft from it up), so the capability may lie up to one
    // level spacing above it.
    let level_spacing_m = 2_000.0 * 0.3048;
    assert!(
        capability <= low + level_spacing_m + 1.0 && capability > low - 1_000.0,
        "capability {capability} m against the planned {low} m"
    );
}

#[test]
fn the_flown_start_of_cruise_keeps_the_buffet_margin() {
    for name in ["B787-9", "A340-300"] {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": name})).unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let assessment = assess_product_candidate(&config, &design).unwrap();
        let row = assessment
            .residuals
            .iter()
            .find(|row| row.id == "buffet_margin")
            .unwrap();
        assert!(row.actual >= FLOOR_G - 5.0e-3, "{name}: {row:?}");
    }
}
