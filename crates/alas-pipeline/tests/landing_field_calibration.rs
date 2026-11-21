// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The landing-field-length model against the certified values of eight jets.
//!
//! The model is the certification segment model (FAA AC 25-7D 4.11 air distance
//! and transition) with the mean deceleration selected by the declared
//! main-gear truck (Torenbeek 2013 p. 274); no coefficient is fitted
//! to these aircraft. The requirement is +-10 % on all eight; a second test
//! bounds the residual of each jet from above only, so it can shrink freely.
//! Conditions: ISA, sea level, dry, MLW, zero wind; certified field length =
//! 25.125 demonstrated distance / 0.6.

// A test asserts on values it built here, so a failed unwrap is the assertion.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::airports::Airport;
use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_pipeline::field_performance::{calculate, dry_landing_distance_share};

/// (preset, certified LFL m at the preset MLW, Vref m/s at the preset MLW).
/// Sources are the manufacturers' airport-planning documents recorded in
/// `refs/*.json`: A220 ACP 013 Fig 7, A320 AC Rev 46 3-4-1,
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
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
    // The landing leg is under test; the jet take-off leg of the same call
    // needs an explicit asymmetric-trim assumption because no fin is built.
    config.performance.oei_asymmetric_trim_cd = Some(0.001);
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

/// Relative error of the model field length against the certified one.
fn residuals() -> Vec<(&'static str, f64)> {
    CERTIFIED
        .iter()
        .map(|&(name, lfl, _)| (name, model(name).0 / lfl - 1.0))
        .collect()
}

#[test]
fn landing_field_length_is_within_ten_percent_on_the_certified_jets() {
    let outside: Vec<String> = residuals()
        .into_iter()
        .filter(|(_, error)| error.is_nan() || error.abs() >= 0.10)
        .map(|(name, error)| format!("{name}: {error:+.3}"))
        .collect();
    assert!(
        outside.is_empty(),
        "landing field length outside +-10 % of the certified value: {outside:?}"
    );
}

/// The residual of a jet may shrink but not grow: 10 % for every jet, except
/// the two the model does not yet reproduce, held at 22 % (B787-9, model
/// long) and 20 % (C919, model short at its full-flap value).
#[test]
fn landing_field_length_residuals_do_not_grow() {
    for (name, error) in residuals() {
        let bound = match name {
            "B787-9" => 0.22,
            "C919" => 0.20,
            _ => 0.10,
        };
        assert!(error.abs() < bound, "{name}: {error:+.3} against {bound}");
    }
}

/// The model must order the eight jets by field length like the certified
/// values do: a Spearman rank correlation above 0.738, the two-sided 5 %
/// critical value for eight pairs, below which the agreement is not
/// distinguishable from chance.
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
    assert!(rho > 0.738, "Spearman rank correlation {rho:.2}");
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

/// The E195-E2 Vref is the FAA Aircraft Characteristics Database approach
/// speed (136 kt at 54,000 kg, no Embraer Vref is published) and its spec-sheet
/// length (1,290 m, MLW 54,000 kg, ISA, sea level) does not state its
/// factoring; it is read as demonstrated / 0.6 like the other charts. The
/// model is outside the +-10 % band: +15.8 % against the spec sheet and
/// +12.6 % against the APM factored distance (1,323 m). This test stays red as
/// the tracking test of the open landing-distance defect (a sourced
/// stopping discriminator beyond Vref and truck type, applied to all jets).
#[test]
fn the_e195_e2_is_within_ten_percent_of_its_spec_sheet() {
    let (lfl, _, mlw, _) = model("E195-E2");
    assert!((mlw - 54_000.0).abs() < 1.0, "{mlw}");
    let error = lfl / 1290.0 - 1.0;
    assert!(error.abs() < 0.10, "{error:+.3}");
}
