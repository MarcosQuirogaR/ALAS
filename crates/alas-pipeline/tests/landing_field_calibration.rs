// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The landing-field-length model against the certified values of eight jets.
//!
//! `k_land` is a one-parameter calibration (mean of the per-aircraft implied
//! coefficients on these same eight jets), so every error below is an
//! IN-SAMPLE residual, not a validation. For a one-parameter mean the
//! "refit without the aircraft" error is only the in-sample residual inflated
//! by n/(n-1); it is reported as such and is not out-of-sample evidence.
//! Conditions: ISA, sea level, dry, MLW, zero wind; certified field length =
//! 25.125 demonstrated distance / 0.6.

// A test asserts on values it built here, so a failed unwrap is the assertion.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::airports::Airport;
use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_pipeline::field_performance::{calculate, dry_landing_distance_share};

const RHO0: f64 = 1.225;

/// (preset, certified LFL m at the preset MLW, Vref m/s at the preset MLW).
/// Sources are the manufacturers' airport-planning documents recorded in
/// `refs/*.json` (2026-10-07): A220 ACP 013 Fig 7, A320 AC Rev 46 3-4-1,
/// A340 3-4-1, A380 3-4-1, B747-400 ACAP, B787-9 ACAP, DC-10 ACAP 3.4/3.5,
/// C919 ACAP R9 DM 03-04/03-05. A340/A380 Vref is the published 138 kt at the
/// family MLW (192 t / 395 t) scaled by sqrt(m/m_source) to the preset MLW.
const CERTIFIED: [(&str, f64, f64); 8] = [
    ("A220-300", 1463.0, 67.55),
    ("A320-200", 1512.0, 69.96),
    ("A340-300", 1822.0, 70.25),
    ("A380-800", 1903.0, 70.18),
    ("B747-400", 2230.0, 78.70),
    ("B787-9", 1798.0, 78.70),
    ("DC-10", 1880.0, 74.80),
    ("C919", 1876.0, 72.90),
];

/// Model (LFL m, Vref m/s, MLW kg, area m^2) at ISA sea level.
fn model(name: &str) -> (f64, f64, f64, f64) {
    let preset = presets::get(name).unwrap();
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
    let area = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .unwrap()
        .s_ref;
    let mtow = config.requirements.mtow_kg;
    let mlw = config.landing_mass_limit_kg(mtow);
    let airport = Airport::custom("ISA sea level", 0.0, 4000.0, 4000.0, 0.0, 0.0, 0.0);
    let field = calculate(&config, &airport, area, mtow, mlw, 0.3, 0.02, 0.04).unwrap();
    let share = dry_landing_distance_share(&config);
    (
        field.ldr_m / share,
        config.performance.vapp_vstall_land_factor * field.v_speeds.v_stall_land_ms,
        mlw,
        area,
    )
}

#[test]
fn landing_field_length_in_sample_residuals_on_the_certified_jets() {
    let mut signed = Vec::new();
    for (name, lfl, _) in CERTIFIED {
        let (model_lfl, _, _, _) = model(name);
        let error = model_lfl / lfl - 1.0;
        assert!(
            error.abs() < 0.20,
            "{name}: LFL {model_lfl:.0} vs {lfl:.0} m"
        );
        signed.push(error);
    }
    let bias = signed.iter().sum::<f64>() / signed.len() as f64;
    // The previous 0.60/0.63 coefficients gave +23 % mean on these eight.
    assert!(bias.abs() < 0.03, "mean signed LFL error {bias:+.3}");
}

/// Independent of the fitted scale: the model must order the eight jets by
/// field length like the certified values do (Spearman rank correlation).
/// The coefficient is a common factor, so it cannot create this ordering.
#[test]
fn model_orders_the_certified_jets_by_landing_field_length() {
    let rank = |values: &[f64]| -> Vec<f64> {
        values
            .iter()
            .map(|v| values.iter().filter(|w| *w < v).count() as f64)
            .collect()
    };
    let certified: Vec<f64> = CERTIFIED.iter().map(|c| c.1).collect();
    let modelled: Vec<f64> = CERTIFIED.iter().map(|c| model(c.0).0).collect();
    let (a, b) = (rank(&certified), rank(&modelled));
    let n = a.len() as f64;
    let d2: f64 = a.iter().zip(&b).map(|(x, y)| (x - y).powi(2)).sum();
    let rho = 1.0 - 6.0 * d2 / (n * (n * n - 1.0));
    assert!(rho > 0.5, "Spearman rank correlation {rho:.2}");
}

#[test]
fn refit_without_one_jet_is_an_in_sample_residual_within_twenty_two_percent() {
    // In-sample residual (n/(n-1) inflated), not an out-of-sample test.
    // Implied factored coefficient per aircraft: LFL / (0.5 rho0 Vs^2) in
    // m/Pa, Vs = Vref / 1.23 at the preset MLW.
    let implied: Vec<f64> = CERTIFIED
        .iter()
        .map(|&(_, lfl, vref)| lfl / (0.5 * RHO0 * (vref / 1.23_f64).powi(2)))
        .collect();
    // Worst case is the B787-9 (+19.8 %): its short certified LFL for its Vref is type-specific braking, not representable by one coefficient.
    for (held_out, &(name, lfl, vref)) in CERTIFIED.iter().enumerate() {
        let others: Vec<f64> = implied
            .iter()
            .enumerate()
            .filter(|&(index, _)| index != held_out)
            .map(|(_, &value)| value)
            .collect();
        let refit = others.iter().sum::<f64>() / others.len() as f64;
        let predicted = refit * 0.5 * RHO0 * (vref / 1.23_f64).powi(2);
        assert!(
            (predicted / lfl - 1.0).abs() < 0.22,
            "{name}: refit-without-one LFL {predicted:.0} vs {lfl:.0} m"
        );
    }
    // The shipped coefficient is the (rounded) mean of the same eight.
    let mean = implied.iter().sum::<f64>() / implied.len() as f64;
    let shipped = AlasConfig::default().performance.k_land / 0.6;
    assert!((shipped / mean - 1.0).abs() < 0.01, "{shipped} vs {mean}");
}

/// Model against the chart value for all twelve presets, with the chart's own
/// definition printed next to the number:
/// `cargo test -p alas-pipeline --release --test landing_field_calibration lfl_table -- --ignored --nocapture`
#[test]
#[ignore = "diagnostic table; prints"]
// The table is the output of this diagnostic, so it prints.
#[allow(clippy::print_stderr)]
fn lfl_table() {
    // (preset, chart LFL m at the chart MLW, chart MLW kg, chart definition). Jets only: the
    // turboprop landing path (Torenbeek, 70 % planning share) needs an airframe for its OEI drag.
    let charts: [(&str, f64, f64, &str); 9] = [
        (
            "A220-300",
            1463.0,
            0.0,
            "ACP 013 Fig 6, dry, SL (basis factor not printed)",
        ),
        (
            "A320-200",
            1512.0,
            66_000.0,
            "AC Fig 3-4-1: actual distance / 0.6",
        ),
        ("A340-300", 1822.0, 0.0, "AC 3-4-1 ISA SL"),
        ("A380-800", 1903.0, 0.0, "AC 3-4-1 ISA SL"),
        (
            "B747-400",
            2230.0,
            285_763.0,
            "ACAP FAR landing runway length, flaps 25",
        ),
        ("B787-9", 1798.0, 192_776.0, "ACAP nomogram dry SL"),
        ("DC-10", 1880.0, 190_962.0, "ACAP FAR landing runway length"),
        (
            "C919",
            1900.0,
            67_800.0,
            "ACAP DM 03-04: demonstrated / 0.6",
        ),
        (
            "E195-E2",
            1290.0,
            54_000.0,
            "spec sheet MLW ISA SL, factoring not stated",
        ),
    ];
    for (name, chart, chart_mlw, definition) in charts {
        let (lfl, vref, mlw, area) = model(name);
        eprintln!(
            "LFL {name:10} model {lfl:6.0} chart {chart:6.0} err {:+5.1} % | MLW model {mlw:8.0} chart {chart_mlw:8.0} | Vref {vref:5.1} m/s S {area:6.1} | {definition}",
            100.0 * (lfl / chart - 1.0)
        );
    }
}

/// The E195-E2 is not in the calibration set: its spec-sheet length (1,290 m,
/// MLW 54,000 kg, ISA, sea level) does not state its factoring, so it is an
/// out-of-sample check under the assumption that it is the 25.125 distance
/// over 0.6 like the other charts. If the sheet were the demonstrated distance
/// the model actual distance (0.6 x LFL) would sit about 30 % below it; the factored
/// reading gives +16 %, the closer of the two (not proof of the sheet basis).
#[test]
fn the_e195_e2_out_of_sample_error_is_inside_the_one_coefficient_scatter() {
    let (lfl, _, mlw, _) = model("E195-E2");
    assert!((mlw - 54_000.0).abs() < 1.0, "{mlw}");
    let error = lfl / 1290.0 - 1.0;
    assert!(error > 0.0 && error < 0.20, "{error:+.3}");
}
