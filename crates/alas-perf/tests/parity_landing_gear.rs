// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-perf::landing_gear` against `alas.physics.landing_gear`, via
//! `golden/generators/gen_perf_landing_gear.py`.
//!
//! The buildup is closed-form arithmetic over a discrete tire ladder, so the
//! continuous quantities -- reaction loads, strength fractions, track width,
//! turnover angle and every wheel coordinate -- are checked at `Tier::Closed`,
//! matching `docs/PORTING.md`. The discrete outputs the same buildup produces
//! -- wheel/strut counts, the selected tire class and its published
//! dimensions, the strut-material label and the turnover verdict -- are
//! integers, strings and a bool, so they are checked for exact equality
//! rather than through a tolerance.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_config::LandingGearConfig;
use alas_perf::landing_gear::{size_landing_gear, LandingGearLayout, TireSpec};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;
use serde_json::{Map, Value};

#[derive(Debug, Deserialize)]
struct TireRecord {
    code: String,
    name: String,
    rated_load_kg: f64,
    diameter_m: f64,
    width_m: f64,
}

#[derive(Debug, Deserialize)]
struct WheelRecord {
    x: f64,
    y: f64,
    group: String,
    strut_label: String,
    diameter_m: f64,
    width_m: f64,
}

#[derive(Debug, Deserialize)]
struct LayoutRecord {
    n_nlg_wheels: i64,
    n_mlg_struts: i64,
    wheels_per_mlg_strut: i64,
    nlg_tire: TireRecord,
    mlg_tire: TireRecord,
    strut_material: String,
    x_nlg: f64,
    x_mlg: f64,
    track_width_m: f64,
    wheelbase_m: f64,
    wheels: Vec<WheelRecord>,
    r_nlg_design_kg: f64,
    r_mlg_total_design_kg: f64,
    pct_load_nlg_max: f64,
    pct_load_mlg_max: f64,
    turnover_angle_deg: f64,
    turnover_ok: bool,
}

#[derive(Debug, Deserialize)]
struct InputsRecord {
    mtow_kg: f64,
    x_nlg: f64,
    x_mlg: f64,
    aero_fwd_lim_x: f64,
    aero_aft_lim_x: f64,
    fuselage_diameter_m: f64,
    cg_height_estimate_m: f64,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    inputs: InputsRecord,
    config: Map<String, Value>,
    layout: LayoutRecord,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

/// Rebuild a `LandingGearConfig` from a default plus the recorded overrides,
/// the same `setattr`-after-default the generator does. An unhandled key is a
/// fixture the test cannot reproduce, so it panics.
fn config_for(overrides: &Map<String, Value>) -> LandingGearConfig {
    let mut cfg = LandingGearConfig::default();
    for (key, value) in overrides {
        match key.as_str() {
            "tire_safety_factor" => cfg.tire_safety_factor = value.as_f64().unwrap(),
            "n_nlg_wheels" => cfg.n_nlg_wheels = value.as_i64().unwrap(),
            "nlg_dual_wheel_mtow_kg" => cfg.nlg_dual_wheel_mtow_kg = value.as_f64().unwrap(),
            "n_mlg_struts" => cfg.n_mlg_struts = value.as_i64().unwrap(),
            "mlg_body_gear_mtow_kg" => cfg.mlg_body_gear_mtow_kg = value.as_f64().unwrap(),
            "wheels_per_mlg_strut" => cfg.wheels_per_mlg_strut = value.as_i64().unwrap(),
            "track_diameter_factor" => cfg.track_diameter_factor = value.as_f64().unwrap(),
            "tire_class" => cfg.tire_class = value.as_str().unwrap().to_owned(),
            "strut_material" => cfg.strut_material = value.as_str().unwrap().to_owned(),
            "turnover_angle_limit_deg" => cfg.turnover_angle_limit_deg = value.as_f64().unwrap(),
            other => panic!("fixture set an unhandled LandingGearConfig field: {other}"),
        }
    }
    cfg
}

fn compare_tire(comparison: &mut Comparison, label: &str, actual: TireSpec, expected: &TireRecord) {
    assert_eq!(actual.code, expected.code, "{label}: tire code");
    assert_eq!(actual.name, expected.name, "{label}: tire name");
    comparison.scalar(
        &format!("{label}: rated_load_kg"),
        actual.rated_load_kg,
        expected.rated_load_kg,
    );
    comparison.scalar(
        &format!("{label}: diameter_m"),
        actual.diameter_m,
        expected.diameter_m,
    );
    comparison.scalar(
        &format!("{label}: width_m"),
        actual.width_m,
        expected.width_m,
    );
}

fn compare_layout(
    comparison: &mut Comparison,
    name: &str,
    got: &LandingGearLayout,
    want: &LayoutRecord,
) {
    // Discrete outputs: exact equality.
    assert_eq!(got.n_nlg_wheels, want.n_nlg_wheels, "{name}: n_nlg_wheels");
    assert_eq!(got.n_mlg_struts, want.n_mlg_struts, "{name}: n_mlg_struts");
    assert_eq!(
        got.wheels_per_mlg_strut, want.wheels_per_mlg_strut,
        "{name}: wheels_per_mlg_strut"
    );
    assert_eq!(
        got.strut_material, want.strut_material,
        "{name}: strut_material"
    );
    assert_eq!(got.turnover_ok, want.turnover_ok, "{name}: turnover_ok");
    assert_eq!(got.wheels.len(), want.wheels.len(), "{name}: wheel count");

    compare_tire(
        comparison,
        &format!("{name}: nlg_tire"),
        got.nlg_tire,
        &want.nlg_tire,
    );
    compare_tire(
        comparison,
        &format!("{name}: mlg_tire"),
        got.mlg_tire,
        &want.mlg_tire,
    );

    // Continuous outputs: closed tier.
    comparison.scalar(&format!("{name}: x_nlg"), got.x_nlg, want.x_nlg);
    comparison.scalar(&format!("{name}: x_mlg"), got.x_mlg, want.x_mlg);
    comparison.scalar(
        &format!("{name}: track_width_m"),
        got.track_width_m,
        want.track_width_m,
    );
    comparison.scalar(
        &format!("{name}: wheelbase_m"),
        got.wheelbase_m,
        want.wheelbase_m,
    );
    comparison.scalar(
        &format!("{name}: r_nlg_design_kg"),
        got.r_nlg_design_kg,
        want.r_nlg_design_kg,
    );
    comparison.scalar(
        &format!("{name}: r_mlg_total_design_kg"),
        got.r_mlg_total_design_kg,
        want.r_mlg_total_design_kg,
    );
    comparison.scalar(
        &format!("{name}: pct_load_nlg_max"),
        got.pct_load_nlg_max,
        want.pct_load_nlg_max,
    );
    comparison.scalar(
        &format!("{name}: pct_load_mlg_max"),
        got.pct_load_mlg_max,
        want.pct_load_mlg_max,
    );
    comparison.scalar(
        &format!("{name}: turnover_angle_deg"),
        got.turnover_angle_deg,
        want.turnover_angle_deg,
    );

    for (i, (got_w, want_w)) in got.wheels.iter().zip(&want.wheels).enumerate() {
        assert_eq!(got_w.group, want_w.group, "{name}: wheel[{i}] group");
        assert_eq!(
            got_w.strut_label, want_w.strut_label,
            "{name}: wheel[{i}] strut_label"
        );
        comparison.scalar(&format!("{name}: wheel[{i}].x"), got_w.x, want_w.x);
        comparison.scalar(&format!("{name}: wheel[{i}].y"), got_w.y, want_w.y);
        comparison.scalar(
            &format!("{name}: wheel[{i}].diameter_m"),
            got_w.diameter_m,
            want_w.diameter_m,
        );
        comparison.scalar(
            &format!("{name}: wheel[{i}].width_m"),
            got_w.width_m,
            want_w.width_m,
        );
    }
}

#[test]
fn size_landing_gear_matches_python_across_weight_and_config_cases() {
    let fixture: Fixture = alas_testkit::load("perf", "landing_gear");

    let mut comparison =
        Comparison::new("alas-perf::landing_gear::size_landing_gear", Tier::Closed);
    for case in &fixture.cases {
        let cfg = config_for(&case.config);
        let inputs = &case.inputs;
        let layout = size_landing_gear(
            inputs.mtow_kg,
            inputs.x_nlg,
            inputs.x_mlg,
            inputs.aero_fwd_lim_x,
            inputs.aero_aft_lim_x,
            inputs.fuselage_diameter_m,
            inputs.cg_height_estimate_m,
            &cfg,
        );
        compare_layout(&mut comparison, &case.name, &layout, &case.layout);
    }
    comparison.finish();
}
