// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-prop::cycle` against `alas.physics.propulsion`, via
//! `golden/generators/gen_prop_cycle.py`.
//!
//! The cycle is closed-form compressible-flow arithmetic whose only external
//! input is the ambient state -- read here through `Atmosphere::new`, the same
//! fitted model upstream's `asb.Atmosphere(...)` selects -- so every continuous
//! output (specific thrust, TSFC, the fuel-air ratio, the efficiency
//! decomposition, every station temperature and both exit velocities) is
//! checked at `Tier::Closed`, matching `docs/PORTING.md`. The discrete outputs
//! the same buildup produces -- the feasibility flag, the reason text and the
//! bypass-ratio classification label -- are a bool and strings, so they are
//! checked for exact equality.
//!
//! A value the reference leaves `NaN` (every output of an infeasible cycle, and
//! every masked-off sweep cell) is recorded as JSON `null`; it maps to a `NaN`
//! here, and a `NaN` is defined to agree with a `NaN`.

// This file is itself a test binary, so an unwrap or expect that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_config::PropulsionCycleConfig;
use alas_prop::cycle::sweeps::{
    compute_altitude_sweep, compute_bpr_sensitivity, compute_carpet_plot,
    compute_efficiency_decomposition,
};
use alas_prop::cycle::{
    anchor_mass_flow_kg_s, classify_engine_by_bpr, compute_turbofan_cycle, TurbofanCycleInputs,
    TurbofanCycleResult,
};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

/// A recorded cycle result: the numeric fields are `Option<f64>` because an
/// infeasible cycle writes each of them as `null`.
#[derive(Debug, Deserialize)]
struct CycleResultRecord {
    cycle_feasible: bool,
    infeasibility_reason: String,
    specific_thrust_ms: Option<f64>,
    tsfc_mg_ns: Option<f64>,
    fuel_air_ratio: Option<f64>,
    thermal_efficiency: Option<f64>,
    propulsive_efficiency: Option<f64>,
    overall_efficiency: Option<f64>,
    temperature_t0_k: Option<f64>,
    temperature_t13_k: Option<f64>,
    temperature_t25_k: Option<f64>,
    temperature_t3_k: Option<f64>,
    temperature_t4_k: Option<f64>,
    temperature_t45_k: Option<f64>,
    temperature_t5_k: Option<f64>,
    temperature_t6_k: Option<f64>,
    exit_velocity_core_ms: Option<f64>,
    exit_velocity_fan_ms: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct CycleInputs {
    mach: f64,
    altitude_m: f64,
    bypass_ratio: f64,
    overall_pressure_ratio: f64,
    fan_pressure_ratio: f64,
    turbine_inlet_temperature_k: f64,
}

#[derive(Debug, Deserialize)]
struct CycleCase {
    name: String,
    inputs: CycleInputs,
    result: CycleResultRecord,
}

#[derive(Debug, Deserialize)]
struct AnchorInputs {
    thrust_kn: f64,
    overall_pressure_ratio: f64,
    fan_pressure_ratio: f64,
    bypass_ratio: f64,
    turbine_inlet_temperature_k: f64,
}

#[derive(Debug, Deserialize)]
struct AnchorCase {
    inputs: AnchorInputs,
    mdot_total_kg_s: Option<f64>,
    static_result: CycleResultRecord,
}

#[derive(Debug, Deserialize)]
struct CarpetInputs {
    compressor_pressure_ratio_vector: Vec<f64>,
    tit_vector_k: Vec<f64>,
    mach: f64,
    altitude_m: f64,
    fan_pressure_ratio: f64,
    bypass_ratio: f64,
}

#[derive(Debug, Deserialize)]
struct CarpetCase {
    inputs: CarpetInputs,
    compressor_pressure_ratio_vector: Vec<f64>,
    tit_vector_k: Vec<f64>,
    specific_thrust_ms: Vec<Vec<Option<f64>>>,
    tsfc_mg_ns: Vec<Vec<Option<f64>>>,
    feasible_mask: Vec<Vec<bool>>,
}

#[derive(Debug, Deserialize)]
struct BprInputs {
    bpr_vector: Vec<f64>,
    overall_pressure_ratio: f64,
    turbine_inlet_temperature_k: f64,
    fan_pressure_ratio: f64,
    mach: f64,
    altitude_m: f64,
}

#[derive(Debug, Deserialize)]
struct BprCase {
    inputs: BprInputs,
    bypass_ratio_vector: Vec<f64>,
    specific_thrust_ms: Vec<Option<f64>>,
    tsfc_mg_ns: Vec<Option<f64>>,
    feasible_mask: Vec<bool>,
}

#[derive(Debug, Deserialize)]
struct EffInputs {
    pi_c_vector: Vec<f64>,
    turbine_inlet_temperature_k: f64,
    bypass_ratio: f64,
    fan_pressure_ratio: f64,
    mach: f64,
    altitude_m: f64,
}

#[derive(Debug, Deserialize)]
struct EffCase {
    inputs: EffInputs,
    compressor_pressure_ratio_vector: Vec<f64>,
    thermal_efficiency: Vec<Option<f64>>,
    propulsive_efficiency: Vec<Option<f64>>,
    overall_efficiency: Vec<Option<f64>>,
    feasible_mask: Vec<bool>,
}

#[derive(Debug, Deserialize)]
struct AltInputs {
    altitude_vector_m: Vec<f64>,
    mach_values: Vec<f64>,
    bypass_ratio: f64,
    overall_pressure_ratio: f64,
    fan_pressure_ratio: f64,
    turbine_inlet_temperature_k: f64,
    mdot_total_kg_s: f64,
}

#[derive(Debug, Deserialize)]
struct AltCase {
    inputs: AltInputs,
    altitude_m: Vec<f64>,
    mach_values: Vec<f64>,
    specific_thrust_ms: Vec<Vec<Option<f64>>>,
    tsfc_mg_ns: Vec<Vec<Option<f64>>>,
    dimensional_thrust_kn: Vec<Vec<Option<f64>>>,
    feasible_mask: Vec<Vec<bool>>,
}

#[derive(Debug, Deserialize)]
struct ClassifyCase {
    bypass_ratio: f64,
    label: String,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    cycle_cases: Vec<CycleCase>,
    anchor: AnchorCase,
    carpet_plot: CarpetCase,
    bpr_sensitivity: BprCase,
    efficiency_decomposition: EffCase,
    altitude_sweep: AltCase,
    classify: Vec<ClassifyCase>,
}

/// A recorded value maps `null` to a `NaN`, which `agrees` treats as a match
/// for a `NaN`.
fn n(value: Option<f64>) -> f64 {
    value.unwrap_or(f64::NAN)
}

fn compare_result(
    comparison: &mut Comparison,
    name: &str,
    got: &TurbofanCycleResult,
    want: &CycleResultRecord,
) {
    comparison.exact(
        &format!("{name}: cycle_feasible"),
        &got.cycle_feasible,
        &want.cycle_feasible,
    );
    comparison.exact(
        &format!("{name}: infeasibility_reason"),
        &got.infeasibility_reason,
        &want.infeasibility_reason,
    );
    for (field, got_v, want_v) in [
        (
            "specific_thrust_ms",
            got.specific_thrust_ms,
            want.specific_thrust_ms,
        ),
        ("tsfc_mg_ns", got.tsfc_mg_ns, want.tsfc_mg_ns),
        ("fuel_air_ratio", got.fuel_air_ratio, want.fuel_air_ratio),
        (
            "thermal_efficiency",
            got.thermal_efficiency,
            want.thermal_efficiency,
        ),
        (
            "propulsive_efficiency",
            got.propulsive_efficiency,
            want.propulsive_efficiency,
        ),
        (
            "overall_efficiency",
            got.overall_efficiency,
            want.overall_efficiency,
        ),
        (
            "temperature_t0_k",
            got.temperature_t0_k,
            want.temperature_t0_k,
        ),
        (
            "temperature_t13_k",
            got.temperature_t13_k,
            want.temperature_t13_k,
        ),
        (
            "temperature_t25_k",
            got.temperature_t25_k,
            want.temperature_t25_k,
        ),
        (
            "temperature_t3_k",
            got.temperature_t3_k,
            want.temperature_t3_k,
        ),
        (
            "temperature_t4_k",
            got.temperature_t4_k,
            want.temperature_t4_k,
        ),
        (
            "temperature_t45_k",
            got.temperature_t45_k,
            want.temperature_t45_k,
        ),
        (
            "temperature_t5_k",
            got.temperature_t5_k,
            want.temperature_t5_k,
        ),
        (
            "temperature_t6_k",
            got.temperature_t6_k,
            want.temperature_t6_k,
        ),
        (
            "exit_velocity_core_ms",
            got.exit_velocity_core_ms,
            want.exit_velocity_core_ms,
        ),
        (
            "exit_velocity_fan_ms",
            got.exit_velocity_fan_ms,
            want.exit_velocity_fan_ms,
        ),
    ] {
        comparison.scalar(&format!("{name}: {field}"), got_v, n(want_v));
    }
}

fn compare_vec_opt(comparison: &mut Comparison, name: &str, got: &[f64], want: &[Option<f64>]) {
    let expected: Vec<f64> = want.iter().map(|v| n(*v)).collect();
    comparison.slice(name, got, &expected);
}

fn compare_grid(
    comparison: &mut Comparison,
    name: &str,
    got: &[Vec<f64>],
    want: &[Vec<Option<f64>>],
) {
    assert_eq!(got.len(), want.len(), "{name}: row count");
    for (i, (got_row, want_row)) in got.iter().zip(want).enumerate() {
        compare_vec_opt(comparison, &format!("{name}[{i}]"), got_row, want_row);
    }
}

#[test]
fn turbofan_cycle_matches_python_across_flight_and_feasibility_cases() {
    let fixture: Fixture = alas_testkit::load("prop", "cycle");
    let cfg = PropulsionCycleConfig::default();
    let mut comparison = Comparison::new("alas-prop::cycle", Tier::Closed);

    for case in &fixture.cycle_cases {
        let inputs = TurbofanCycleInputs {
            mach: case.inputs.mach,
            altitude_m: case.inputs.altitude_m,
            bypass_ratio: case.inputs.bypass_ratio,
            overall_pressure_ratio: case.inputs.overall_pressure_ratio,
            fan_pressure_ratio: case.inputs.fan_pressure_ratio,
            turbine_inlet_temperature_k: case.inputs.turbine_inlet_temperature_k,
        };
        let got = compute_turbofan_cycle(&inputs, &cfg);
        compare_result(&mut comparison, &case.name, &got, &case.result);
    }

    // -- anchor_mass_flow_kg_s -----------------------------------------------
    let anchor = &fixture.anchor;
    let (mdot, static_result) = anchor_mass_flow_kg_s(
        anchor.inputs.thrust_kn,
        anchor.inputs.overall_pressure_ratio,
        anchor.inputs.fan_pressure_ratio,
        anchor.inputs.bypass_ratio,
        anchor.inputs.turbine_inlet_temperature_k,
        &cfg,
    );
    comparison.scalar("anchor: mdot_total_kg_s", mdot, n(anchor.mdot_total_kg_s));
    compare_result(
        &mut comparison,
        "anchor: static_result",
        &static_result,
        &anchor.static_result,
    );

    // -- compute_carpet_plot -------------------------------------------------
    let c = &fixture.carpet_plot;
    let carpet = compute_carpet_plot(
        &c.inputs.compressor_pressure_ratio_vector,
        &c.inputs.tit_vector_k,
        c.inputs.mach,
        c.inputs.altitude_m,
        c.inputs.fan_pressure_ratio,
        c.inputs.bypass_ratio,
        &cfg,
    );
    comparison.slice(
        "carpet: compressor_pressure_ratio_vector",
        &carpet.compressor_pressure_ratio_vector,
        &c.compressor_pressure_ratio_vector,
    );
    comparison.slice(
        "carpet: tit_vector_k",
        &carpet.tit_vector_k,
        &c.tit_vector_k,
    );
    compare_grid(
        &mut comparison,
        "carpet: specific_thrust_ms",
        &carpet.specific_thrust_ms,
        &c.specific_thrust_ms,
    );
    compare_grid(
        &mut comparison,
        "carpet: tsfc_mg_ns",
        &carpet.tsfc_mg_ns,
        &c.tsfc_mg_ns,
    );
    comparison.exact(
        "carpet: feasible_mask",
        &carpet.feasible_mask,
        &c.feasible_mask,
    );

    // -- compute_bpr_sensitivity ---------------------------------------------
    let b = &fixture.bpr_sensitivity;
    let bpr = compute_bpr_sensitivity(
        &b.inputs.bpr_vector,
        b.inputs.overall_pressure_ratio,
        b.inputs.turbine_inlet_temperature_k,
        b.inputs.fan_pressure_ratio,
        b.inputs.mach,
        b.inputs.altitude_m,
        &cfg,
    );
    comparison.slice(
        "bpr: bypass_ratio_vector",
        &bpr.bypass_ratio_vector,
        &b.bypass_ratio_vector,
    );
    compare_vec_opt(
        &mut comparison,
        "bpr: specific_thrust_ms",
        &bpr.specific_thrust_ms,
        &b.specific_thrust_ms,
    );
    compare_vec_opt(
        &mut comparison,
        "bpr: tsfc_mg_ns",
        &bpr.tsfc_mg_ns,
        &b.tsfc_mg_ns,
    );
    comparison.exact("bpr: feasible_mask", &bpr.feasible_mask, &b.feasible_mask);

    // -- compute_efficiency_decomposition ------------------------------------
    let e = &fixture.efficiency_decomposition;
    let eff = compute_efficiency_decomposition(
        &e.inputs.pi_c_vector,
        e.inputs.turbine_inlet_temperature_k,
        e.inputs.bypass_ratio,
        e.inputs.fan_pressure_ratio,
        e.inputs.mach,
        e.inputs.altitude_m,
        &cfg,
    );
    comparison.slice(
        "eff: compressor_pressure_ratio_vector",
        &eff.compressor_pressure_ratio_vector,
        &e.compressor_pressure_ratio_vector,
    );
    compare_vec_opt(
        &mut comparison,
        "eff: thermal_efficiency",
        &eff.thermal_efficiency,
        &e.thermal_efficiency,
    );
    compare_vec_opt(
        &mut comparison,
        "eff: propulsive_efficiency",
        &eff.propulsive_efficiency,
        &e.propulsive_efficiency,
    );
    compare_vec_opt(
        &mut comparison,
        "eff: overall_efficiency",
        &eff.overall_efficiency,
        &e.overall_efficiency,
    );
    comparison.exact("eff: feasible_mask", &eff.feasible_mask, &e.feasible_mask);

    // -- compute_altitude_sweep ----------------------------------------------
    let a = &fixture.altitude_sweep;
    let alt = compute_altitude_sweep(
        &a.inputs.altitude_vector_m,
        &a.inputs.mach_values,
        a.inputs.bypass_ratio,
        a.inputs.overall_pressure_ratio,
        a.inputs.fan_pressure_ratio,
        a.inputs.turbine_inlet_temperature_k,
        a.inputs.mdot_total_kg_s,
        &cfg,
    );
    comparison.slice("alt: altitude_m", &alt.altitude_m, &a.altitude_m);
    comparison.slice("alt: mach_values", &alt.mach_values, &a.mach_values);
    compare_grid(
        &mut comparison,
        "alt: specific_thrust_ms",
        &alt.specific_thrust_ms,
        &a.specific_thrust_ms,
    );
    compare_grid(
        &mut comparison,
        "alt: tsfc_mg_ns",
        &alt.tsfc_mg_ns,
        &a.tsfc_mg_ns,
    );
    compare_grid(
        &mut comparison,
        "alt: dimensional_thrust_kn",
        &alt.dimensional_thrust_kn,
        &a.dimensional_thrust_kn,
    );
    comparison.exact("alt: feasible_mask", &alt.feasible_mask, &a.feasible_mask);

    // -- classify_engine_by_bpr ----------------------------------------------
    for case in &fixture.classify {
        let got = classify_engine_by_bpr(case.bypass_ratio);
        comparison.exact(
            &format!("classify({})", case.bypass_ratio),
            &got.to_owned(),
            &case.label,
        );
    }

    comparison.finish();
}
