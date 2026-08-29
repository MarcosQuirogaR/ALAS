// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! W6.1 evidence: replay the real renderer inputs before comparing MLG output.
//!
//! The fixture is collected from the reference `FullAnalysis` report and the
//! public landing-gear-planform factory.  Assertions stay in causal order so
//! a future disagreement identifies the first intermediate that changed; this
//! test does not classify or repair a disagreement.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_config::LandingGearConfig;
use alas_perf::landing_gear::size_landing_gear;
use alas_testkit::{agrees, load_json, Tier};
use serde_json::Value;

fn number(object: &Value, key: &str) -> f64 {
    object[key].as_f64().unwrap()
}

#[test]
fn w61_fixture_replays_renderer_inputs_before_comparing_mlg_count() {
    let fixture = load_json("report", "w61_mlg_count");
    assert_eq!(fixture["schema"], "w61-mlg-count-evidence/v1");

    let sizing = &fixture["renderer_inputs"]["gear_sizing"];
    let first_call = &fixture["renderer_calls"][0];
    let second_call = &fixture["renderer_calls"][1];

    // Both theme renders must size the same physical gear.  A theme-dependent
    // call would make a screenshot comparison unable to identify the source.
    assert_eq!(first_call, second_call);
    for key in [
        "mtow_kg",
        "x_nlg_m",
        "x_mlg_m",
        "aero_fwd_lim_x_m",
        "aero_aft_lim_x_m",
        "fuselage_diameter_m",
        "cg_height_estimate_m",
    ] {
        assert_eq!(first_call[key], sizing[key], "renderer input {key}");
    }
    assert_eq!(
        first_call["landing_gear_config"], sizing["landing_gear_config"],
        "renderer effective landing-gear config"
    );

    let config: LandingGearConfig =
        serde_json::from_value(sizing["landing_gear_config"].clone()).unwrap();
    let layout = size_landing_gear(
        number(sizing, "mtow_kg"),
        number(sizing, "x_nlg_m"),
        number(sizing, "x_mlg_m"),
        number(sizing, "aero_fwd_lim_x_m"),
        number(sizing, "aero_aft_lim_x_m"),
        number(sizing, "fuselage_diameter_m"),
        number(sizing, "cg_height_estimate_m"),
        &config,
    );

    // Discrete decisions are checked before continuous telemetry, so a count
    // divergence is reported without being hidden by later arithmetic.
    let expected = &fixture["computed_gear"];
    assert_eq!(layout.n_nlg_wheels, expected["n_nlg_wheels"]);
    assert_eq!(layout.n_mlg_struts, expected["n_mlg_struts"]);
    assert_eq!(
        layout.wheels_per_mlg_strut,
        expected["wheels_per_mlg_strut"]
    );
    assert_eq!(layout.nlg_tire.code, expected["nlg_tire"]["code"]);
    assert_eq!(layout.mlg_tire.code, expected["mlg_tire"]["code"]);
    assert_eq!(
        layout.wheels.len(),
        expected["wheels"].as_array().unwrap().len()
    );

    // The loads, positions and derived capacities are the intermediate
    // evidence handed to the diagnosis owner; they are not new tolerances.
    for (field, actual) in [
        ("x_nlg", layout.x_nlg),
        ("x_mlg", layout.x_mlg),
        ("wheelbase_m", layout.wheelbase_m),
        ("track_width_m", layout.track_width_m),
        ("r_nlg_design_kg", layout.r_nlg_design_kg),
        ("r_mlg_total_design_kg", layout.r_mlg_total_design_kg),
        ("pct_load_nlg_max", layout.pct_load_nlg_max),
        ("pct_load_mlg_max", layout.pct_load_mlg_max),
        ("turnover_angle_deg", layout.turnover_angle_deg),
    ] {
        assert!(
            agrees(actual, number(expected, field), Tier::Closed),
            "first differing gear intermediate: {field}"
        );
    }

    for theme in ["light", "dark"] {
        let figure = &fixture["figures"][theme];
        assert_eq!(figure["panel_count"], 1);
        assert_eq!(figure["axes"]["xlabel"], "Y [m]");
        assert_eq!(figure["axes"]["ylabel"], "X [m] (fuselage station)");
        assert_eq!(figure["axes"]["legend"].as_array().unwrap().len(), 5);
        assert!(figure["suptitle"]
            .as_str()
            .unwrap()
            .contains("MLG: 4 strut(s)"));
    }
}
