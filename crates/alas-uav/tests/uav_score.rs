// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A failed unwrap in this test target is the assertion reporting malformed
// evidence, not a panic escaping from library code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Score expression language (units, dimensions, evaluation) and the rule
//! engine on the competition presets.
//!
//! Worked examples: none of the rulebooks prints a numeric example, so every
//! example applies the cited equations to chosen inputs. The expected value is
//! written in the test as plain arithmetic in the units of the rulebook (km,
//! Wh, ft, lb), independently of the engine's SI conversion, so a units or
//! transcription error shows up as a mismatch.

use std::collections::BTreeMap;

use alas_uav::score::eval::{eval, Env, Value};
use alas_uav::score::presets;
use alas_uav::score::{parse, Dim, RoundInputs, ScoreEngine, ScoreError, Series};

const LB: f64 = 0.453_592_37;
const FT: f64 = 0.3048;
const IN: f64 = 0.0254;

fn num(src: &str) -> f64 {
    scalar(src).0
}

fn scalar(src: &str) -> (f64, Dim) {
    match eval(&parse(src).unwrap(), &Env::new()).unwrap() {
        Value::Q(q) => (q.value, q.dim),
        Value::S(_) => panic!("series"),
    }
}

fn fails(src: &str) -> ScoreError {
    eval(&parse(src).unwrap(), &Env::new()).unwrap_err()
}

fn approx(a: f64, b: f64, rel: f64) {
    assert!(
        (a - b).abs() <= rel * b.abs().max(1e-12),
        "{a} differs from {b} by more than {rel} relative"
    );
}

#[test]
fn literals_carry_units_and_convert_to_si() {
    approx(num("350 g"), 0.350, 1e-12);
    approx(num("100 Wh"), 360_000.0, 1e-12);
    approx(num("55 lb"), 55.0 * LB, 1e-12);
    approx(num("1.5 ft"), 1.5 * FT, 1e-12);
    approx(num("2 Wh/km"), 7.2, 1e-12);
    approx(num("9.81 m/s^2"), 9.81, 1e-12);
    approx(num("2 m^2"), 2.0, 1e-12);
    approx(num("1e3 mm"), 1.0, 1e-12);
    approx(num("90 deg"), std::f64::consts::FRAC_PI_2, 1e-12);
    approx(num("2200 mAh"), 2200.0 * 3.6, 1e-12);
    let (_, d) = scalar("30 A");
    assert_eq!(d, Dim::CURRENT);
    let (_, d) = scalar("2 Wh/km");
    assert_eq!(d.to_string(), "m*kg*s^-2");
}

#[test]
fn precedence_and_associativity() {
    assert_eq!(num("1 + 2 * 3"), 7.0);
    assert_eq!(num("-2 ^ 2"), -4.0);
    assert_eq!(num("2 ^ 3 ^ 2"), 512.0);
    assert_eq!(num("10 - 4 - 3"), 3.0);
    assert_eq!(num("8 / 4 / 2"), 1.0);
    assert_eq!(num("1 < 2 && 2 < 3"), 1.0);
    assert_eq!(num("!(1 > 2) || 0"), 1.0);
}

#[test]
fn unit_binds_to_the_literal_only() {
    // (2 Wh) / km, not 2 (Wh/km): both agree here, but 1 A*s must be (1 A)*s
    // and a quotient by a unit literal is a quotient by the whole literal.
    approx(num("120 s / (60 s)"), 2.0, 1e-12);
    approx(num("120 s / 60 s"), 2.0, 1e-12);
    approx(num("(10 A * 3 s) / (1 A*s)"), 30.0, 1e-12);
}

#[test]
fn dimension_errors_are_reported_in_every_branch() {
    assert!(matches!(fails("1 m + 1 s"), ScoreError::Dimension(_)));
    assert!(matches!(fails("exp(1 s)"), ScoreError::Dimension(_)));
    assert!(matches!(fails("2 ^ (1 m)"), ScoreError::Dimension(_)));
    assert!(matches!(fails("if(1, 1 m, 2 s)"), ScoreError::Dimension(_)));
    assert!(matches!(fails("if(1 m, 1, 2)"), ScoreError::Dimension(_)));
    assert!(matches!(fails("1 m < 1 s"), ScoreError::Dimension(_)));
    assert!(matches!(fails("min(1 m, 2)"), ScoreError::Dimension(_)));
    // The untaken branch is still checked.
    assert!(matches!(
        fails("if(1, 1 m, 1 kg)"),
        ScoreError::Dimension(_)
    ));
    assert!(matches!(fails("nonsense + 1"), ScoreError::UnknownName(_)));
    assert!(matches!(fails("min()"), ScoreError::Arity(_)));
    assert!(matches!(parse("1 +"), Err(ScoreError::Parse { .. })));
    assert!(matches!(parse("1 $ 2"), Err(ScoreError::Parse { .. })));
}

#[test]
fn powers_roots_and_derived_dimensions() {
    let (v, d) = scalar("sqrt(4 s)");
    approx(v, 2.0, 1e-12);
    assert_eq!(d.to_string(), "s^0.5");
    let (_, d) = scalar("(3 m)^2 / (2 s)");
    assert_eq!(d.to_string(), "m^2*s^-1");
    // 1 m^0.5 is representable, 1 m^0.25 is not.
    assert!(matches!(fails("sqrt(sqrt(1 m))"), ScoreError::Dimension(_)));
    approx(num("pow(2, 0.5)"), 2_f64.sqrt(), 1e-12);
    let (v, d) = scalar("(1 W) * (1 s) / (1 J)");
    approx(v, 1.0, 1e-12);
    assert!(d.is_one());
}

#[test]
fn step_and_table() {
    // x <= edge picks the value, ascending edges, final argument is the default.
    assert_eq!(num("step(10 ft, 10 ft, 20, 25 ft, 15, 0)"), 20.0);
    assert_eq!(num("step(10.0001 ft, 10 ft, 20, 25 ft, 15, 0)"), 15.0);
    assert_eq!(num("step(30 ft, 10 ft, 20, 25 ft, 15, 0)"), 0.0);
    assert!(matches!(
        fails("step(1 m, 1 s, 2, 3)"),
        ScoreError::Dimension(_)
    ));
    // Piecewise-linear, clamped at both ends.
    assert_eq!(num("table(5, 0, 0, 10, 100)"), 50.0);
    assert_eq!(num("table(-1, 0, 0, 10, 100)"), 0.0);
    assert_eq!(num("table(99, 0, 0, 10, 100)"), 100.0);
}

fn series(t: &[f64], y: &[f64]) -> Series {
    Series {
        t: t.to_vec(),
        y: y.to_vec(),
        dim: Dim::CURRENT,
    }
}

#[test]
fn integral_above_is_exact_on_piecewise_linear_signals() {
    // 20 A to 40 A over 10 s crosses 30 A at 5 s: triangle 0.5 * 10 A * 5 s.
    let s = series(&[0.0, 10.0], &[20.0, 40.0]);
    approx(s.integral_above(30.0), 25.0, 1e-12);
    // Falling edge, symmetric.
    let s = series(&[0.0, 10.0], &[40.0, 20.0]);
    approx(s.integral_above(30.0), 25.0, 1e-12);
    // Entirely above: trapezoid; entirely below: zero.
    approx(
        series(&[0.0, 4.0], &[32.0, 36.0]).integral_above(30.0),
        16.0,
        1e-12,
    );
    assert_eq!(series(&[0.0, 4.0], &[10.0, 20.0]).integral_above(30.0), 0.0);
}

#[test]
fn block_mean_averages_half_second_blocks_and_exposes_short_peaks() {
    // A 0.25 s spike to 60 A inside one 0.5 s block averages with 30 A: the
    // measurement box compares block means with the limit, not raw samples.
    let t: Vec<f64> = (0..=20).map(|k| f64::from(k) * 0.1).collect();
    let mut y = vec![30.0; t.len()];
    y[10] = 60.0;
    let s = series(&t, &y);
    assert!(s.max() > 59.0);
    let b = s.block_mean(0.5);
    assert!(b.max() < 40.0, "block mean max {}", b.max());
    // A step edge written as two samples at one time is exact.
    let step = series(
        &[0.0, 10.0, 10.0, 30.0, 30.0, 60.0],
        &[25.0, 25.0, 35.0, 35.0, 25.0, 25.0],
    );
    approx(step.block_mean(0.5).integral_above(30.0), 100.0, 1e-12);
}

// ------------------------------------------------------------------ presets

fn engine(id: &str) -> ScoreEngine {
    ScoreEngine::new(&presets::load(id).unwrap(), None).unwrap()
}

fn inputs(values: &[(&str, f64)]) -> RoundInputs {
    RoundInputs {
        values: values.iter().map(|(k, v)| ((*k).to_string(), *v)).collect(),
        series: BTreeMap::new(),
    }
}

#[test]
fn every_embedded_preset_compiles_with_every_variant() {
    for id in presets::ids() {
        let preset = presets::load(id).unwrap();
        assert!(!preset.citation.is_empty(), "{id} lacks a citation");
        ScoreEngine::new(&preset, None).unwrap_or_else(|e| panic!("{id}: {e}"));
        for v in &preset.variants {
            ScoreEngine::new(&preset, Some(&v.id)).unwrap_or_else(|e| panic!("{id}/{}: {e}", v.id));
        }
        for g in &preset.gates {
            assert!(!g.citation.is_empty(), "{id}/{} lacks a citation", g.id);
        }
        // Parameter defaults lie inside their ranges.
        for p in &preset.parameters {
            assert!(p.min <= p.default && p.default <= p.max, "{id}/{}", p.name);
        }
        // Reference-field defaults are documented as assumptions.
        for r in &preset.score.reference_field {
            assert!(r.note.starts_with("ASSUMPTION"), "{id}/{}", r.name);
        }
    }
    assert_eq!(presets::ids().len(), 7);
}

#[test]
fn a_dimension_error_in_a_preset_is_rejected_at_load() {
    let mut preset = presets::load("acc-2026").unwrap();
    preset.variants[1].overrides = vec![("s_raw".into(), "payload_mass + distance_120s".into())];
    let err = ScoreEngine::new(&preset, Some("legacy_additive"))
        .err()
        .unwrap();
    assert!(matches!(err, ScoreError::Dimension(_)), "{err}");
    preset.variants[1].overrides = vec![("s_raw".into(), "payload_mass * mystery".into())];
    assert!(matches!(
        ScoreEngine::new(&preset, Some("legacy_additive"))
            .err()
            .unwrap(),
        ScoreError::UnknownName(_)
    ));
    // A round score that is not dimensionless is rejected.
    preset.variants[1].overrides = vec![("round_expr".into(), "s_raw".into())];
    assert!(ScoreEngine::new(&preset, Some("legacy_additive")).is_err());
    assert!(ScoreEngine::new(&preset, Some("no-such-variant")).is_err());
}

fn acc2026_flight(current: Series) -> RoundInputs {
    let mut r = inputs(&[
        ("payload_count", 9.0),
        ("payload_mass", 9.0 * 0.350),
        ("distance_120s", 2700.0),
        ("takeoff_distance", 52.0),
        ("announced_takeoff", 60.0),
        ("declared_payload_count", 9.0),
        ("loading_time", 50.0),
        ("peak_voltage", 12.4),
        ("peak_current", 38.0),
        ("s_raw_best", 2.3e7),
    ]);
    r.series.insert("battery_current".into(), current);
    r
}

fn step_current() -> Series {
    // 25 A, a 35 A plateau from t = 10 s to 30 s, then 25 A, over 60 s.
    series(
        &[0.0, 10.0, 10.0, 30.0, 30.0, 60.0],
        &[25.0, 25.0, 35.0, 35.0, 25.0, 25.0],
    )
}

#[test]
fn acc_2026_worked_example_eq_2_to_7() {
    // ACC 2026 handbook v1.3 s4.7: 9 cans x 0.35 kg, 2700 m in 120 s, 60 m
    // announced and flown in 52 m, 100 A*s above 30 A, 50 s loading plus
    // unloading, 9 cans announced and carried, best raw score 2.3e7 kg*m^2.
    let r = engine("acc-2026")
        .evaluate_round(&acc2026_flight(step_current()))
        .unwrap();
    let p_current = 0.002 * 100.0; // eq. 6
    let s_raw = 1.0 * (9.0 * 0.350) * 2700.0_f64.powi(2) * (1.0 - p_current); // eq. 2
    let b_load = 60.0 * (1.0 - 50.0 / 120.0); // eq. 4
    let b_pred = 3.0 * 9.0; // eq. 5
    let expected = s_raw / 2.3e7 * 1000.0 + b_load + b_pred; // eq. 3
    approx(r.term("p_current").unwrap(), 0.2, 1e-9);
    approx(r.term("s_raw").unwrap(), 18_370_800.0, 1e-9);
    approx(r.score, expected, 1e-9);
    approx(r.score, 860.730_434, 1e-7);
    // Gates whose inputs are absent are reported as unevaluated, not satisfied.
    assert!(!r.unevaluated_gates.is_empty());
    assert!(!r.complete());
}

#[test]
fn acc_2026_penalty_cap_zero_flight_and_failed_takeoff() {
    let e = engine("acc-2026");
    // 0.002 * integral is capped at 1: a huge overshoot zeroes S_raw (not negative).
    let big = series(&[0.0, 60.0], &[60.0, 60.0]);
    let r = e.evaluate_round(&acc2026_flight(big)).unwrap();
    assert_eq!(r.term("p_current").unwrap(), 1.0);
    assert_eq!(r.term("s_raw").unwrap(), 0.0);
    // Voltage above 12.75 V anywhere: the whole round is zero (s4.7.4 item 2).
    let mut over = acc2026_flight(step_current());
    over.values.insert("peak_voltage".into(), 12.8);
    assert_eq!(e.evaluate_round(&over).unwrap().score, 0.0);
    // 70 A exceeded: zero (item 3).
    let mut amp = acc2026_flight(step_current());
    amp.values.insert("peak_current".into(), 71.0);
    assert_eq!(e.evaluate_round(&amp).unwrap().score, 0.0);
    // Take-off longer than announced: B_to = 0 and the flight is zero (eq. 1).
    let mut long = acc2026_flight(step_current());
    long.values.insert("takeoff_distance".into(), 61.0);
    let r = e.evaluate_round(&long).unwrap();
    assert_eq!(r.term("b_to").unwrap(), 0.0);
    assert_eq!(r.score, 0.0);
    // 40 m announced and flown: x1.15 on the raw score (eq. 1).
    let mut short = acc2026_flight(step_current());
    short.values.insert("announced_takeoff".into(), 40.0);
    short.values.insert("takeoff_distance".into(), 38.0);
    approx(
        e.evaluate_round(&short).unwrap().term("b_to").unwrap(),
        1.15,
        1e-12,
    );
}

#[test]
fn acc_2026_payload_prediction_staircase_and_loading_bonus() {
    let e = engine("acc-2026");
    let score = |carried: f64, loading: f64| {
        let mut r = acc2026_flight(step_current());
        r.values.insert("payload_count".into(), carried);
        r.values.insert("declared_payload_count".into(), 9.0);
        r.values.insert("loading_time".into(), loading);
        let out = e.evaluate_round(&r).unwrap();
        (out.term("b_pred").unwrap(), out.term("b_load").unwrap())
    };
    // 3 N / 2 N / 1 N / 0 for carried >= N, N-1, N-2, less (eq. 5), cap 60.
    assert_eq!(score(9.0, 50.0).0, 27.0);
    assert_eq!(score(10.0, 50.0).0, 27.0);
    assert_eq!(score(8.0, 50.0).0, 18.0);
    assert_eq!(score(7.0, 50.0).0, 9.0);
    assert_eq!(score(6.0, 50.0).0, 0.0);
    let mut big = acc2026_flight(step_current());
    big.values.insert("declared_payload_count".into(), 25.0);
    big.values.insert("payload_count".into(), 25.0);
    assert_eq!(
        e.evaluate_round(&big).unwrap().term("b_pred").unwrap(),
        60.0
    );
    // Loading bonus: 60 at 0 s, 0 at and beyond 120 s (eq. 4).
    assert_eq!(score(9.0, 0.0).1, 60.0);
    assert_eq!(score(9.0, 120.0).1, 0.0);
    assert_eq!(score(9.0, 200.0).1, 0.0);
}

#[test]
fn acc_2026_current_penalty_uses_half_second_block_means() {
    // A 0.25 s excursion to 90 A over a 20 A baseline averages down inside its
    // 0.5 s block (about 62 A), so the penalty is smaller than the raw-sample
    // integral: the measurement box compares block means with the limit.
    let t: Vec<f64> = (0..=1200).map(|k| f64::from(k) * 0.05).collect();
    let mut y = vec![20.0; t.len()];
    for v in &mut y[400..406] {
        *v = 90.0;
    }
    let spike = series(&t, &y);
    let raw = spike.integral_above(30.0);
    let r = engine("acc-2026")
        .evaluate_round(&acc2026_flight(spike))
        .unwrap();
    let blocked = r.term("i_over").unwrap();
    assert!(blocked > 0.0 && blocked < raw, "block {blocked} raw {raw}");
}

#[test]
fn acc_2026_legacy_additive_variant_is_selectable() {
    let preset = presets::load("acc-2026").unwrap();
    let e = ScoreEngine::new(&preset, Some("legacy_additive")).unwrap();
    let r = e.evaluate_round(&acc2026_flight(step_current())).unwrap();
    // 3.15 kg * 2700^2 m^2 - 2 * 100 A*s (in kg*m^2): the penalty is negligible.
    approx(
        r.term("s_raw").unwrap(),
        3.15 * 2700.0_f64.powi(2) - 200.0,
        1e-12,
    );
}

#[test]
fn acc_2026_rule_gates() {
    let e = engine("acc-2026");
    let mut r = acc2026_flight(step_current());
    for (k, v) in [
        ("mtow", 24.0),
        ("transport_box_sum", 1.4),
        ("transport_box_mass", 22.0),
        ("motor_count", 2.0),
        ("cell_count", 3.0),
        ("pack_voltage_max", 12.6),
        ("pack_energy_max", 99.0 * 3600.0),
        ("battery_continuous_rating", 45.0),
        ("bay_can_capacity", 4.0),
        ("static_margin_min", 0.08),
        ("static_margin_max", 0.30),
    ] {
        r.values.insert(k.into(), v);
    }
    let out = e.evaluate_round(&r).unwrap();
    for g in &out.gates {
        assert!(g.satisfied, "{} margin {}", g.id, g.margin);
    }
    assert!(out.feasible());
    // 25.5 kg violates A/C 5, a 55 % static margin violates Stab 2, a third
    // motor violates the motor rule, 13.1 V violates Bat 2.
    for (key, bad, gate) in [
        ("mtow", 25.5, "mtow"),
        ("static_margin_max", 0.55, "static_margin"),
        ("motor_count", 3.0, "motors"),
        ("pack_voltage_max", 13.1, "pack_voltage"),
        ("battery_continuous_rating", 35.0, "pack_rating"),
        ("transport_box_sum", 1.6, "box_sum"),
    ] {
        let mut b = r.clone();
        b.values.insert(key.into(), bad);
        let out = e.evaluate_round(&b).unwrap();
        let g = out.gates.iter().find(|g| g.id == gate).unwrap();
        assert!(!g.satisfied && g.margin < 0.0, "{gate} margin {}", g.margin);
        assert!(!out.feasible());
    }
    // Announcing 50 m is not an allowed option.
    let mut b = r.clone();
    b.values.insert("announced_takeoff".into(), 50.0);
    let out = e.evaluate_round(&b).unwrap();
    assert!(
        !out.gates
            .iter()
            .find(|g| g.id == "announcement")
            .unwrap()
            .satisfied
    );
}

#[test]
fn acc_2026_flight_competition_is_the_mean_of_the_two_best_of_three() {
    let e = engine("acc-2026");
    let make = |dist: f64| {
        let mut r = acc2026_flight(step_current());
        r.values.insert("distance_120s".into(), dist);
        e.evaluate_round(&r).unwrap()
    };
    let (a, b, c) = (make(2000.0), make(2700.0), make(2400.0));
    let best_two = (b.score + c.score) / 2.0;
    approx(
        e.flight_score(&[a.clone(), b.clone(), c]).unwrap(),
        best_two,
        1e-12,
    );
    // Fewer than three rounds: the single best (eq. 7).
    approx(e.flight_score(&[a, b.clone()]).unwrap(), b.score, 1e-12);
}

#[test]
fn unknown_or_out_of_range_inputs_are_rejected() {
    let e = engine("acc-2026");
    let mut r = acc2026_flight(step_current());
    r.values.insert("payload_mas".into(), 1.0);
    assert!(matches!(e.evaluate_round(&r), Err(ScoreError::Input(_))));
    let mut r = acc2026_flight(step_current());
    r.values.insert("flight_valid".into(), 2.0);
    assert!(matches!(e.evaluate_round(&r), Err(ScoreError::Input(_))));
    let mut r = acc2026_flight(step_current());
    r.values.insert("distance_120s".into(), f64::NAN);
    assert!(matches!(e.evaluate_round(&r), Err(ScoreError::Input(_))));
    // A missing output is named in the error.
    let mut r = acc2026_flight(step_current());
    r.values.remove("payload_mass");
    assert!(matches!(
        e.evaluate_round(&r),
        Err(ScoreError::UnknownName(_))
    ));
    // A reference of zero gives a non-finite score, reported not propagated.
    let mut r = acc2026_flight(step_current());
    r.values.insert("s_raw_best".into(), 0.0);
    assert!(matches!(
        e.evaluate_round(&r),
        Err(ScoreError::NonFinite(_))
    ));
}

#[test]
fn acc_2024_worked_example_s3_7() {
    // ACC 2024 handbook v1.3 s3.7.3-3.7.11: 12 balls (field best 14), 1.8 km
    // and 5 Wh in the efficiency window (field best 0.4 km^2/Wh), 2400 m in the
    // distance window (field best 2700 m), 40 s loading, 40 A*s above 30 A,
    // 60 m announced and flown in 60 m.
    let mut r = inputs(&[
        ("payload_count", 12.0),
        ("distance_eff", 1800.0),
        ("energy_eff", 5.0 * 3600.0),
        ("distance_90s", 2400.0),
        ("loading_time", 40.0),
        ("takeoff_distance", 60.0),
        ("announced_takeoff", 60.0),
        ("peak_voltage", 12.5),
        ("n_best", 14.0),
        ("e_max", 0.4e6 / 3600.0), // 0.4 km^2/Wh in m^2/J
        ("d_max", 2700.0),
    ]);
    r.series.insert(
        "battery_current".into(),
        series(
            &[0.0, 10.0, 10.0, 18.0, 18.0, 60.0],
            &[28.0, 28.0, 35.0, 35.0, 28.0, 28.0],
        ),
    );
    let out = engine("acc-2024").evaluate_round(&r).unwrap();
    let s_payload = 1000.0 * 12.0 / 14.0;
    let e_team = 1.8_f64.powi(2) / (2.0 * 1.8 + 5.0); // km^2/Wh, s3.7.4
    let s_eff = 1000.0 * e_team / 0.4;
    let s_dist = 1000.0 * 2400.0 / 2700.0;
    let b_load = 60.0 * (1.0 - 40.0 / 120.0);
    let p_cur = 0.5 * 5.0 * 8.0; // 0.5 pt per A*s, 5 A over for 8 s
    let expected = (s_payload + s_eff + s_dist) / 3.0 + b_load - p_cur;
    approx(out.score, expected, 1e-9);
    approx(out.score, 915.964_07, 1e-7);
    // Landing damage halves the round (s3.7.9).
    r.values.insert("landing_damaged".into(), 1.0);
    approx(
        engine("acc-2024").evaluate_round(&r).unwrap().score,
        expected / 2.0,
        1e-9,
    );
}

#[test]
fn acc_2024_weights_worst_25_second_worst_50_rest_25() {
    let e = engine("acc-2024");
    let mut r = inputs(&[
        ("payload_count", 12.0),
        ("distance_eff", 1800.0),
        ("energy_eff", 18_000.0),
        ("distance_90s", 2400.0),
        ("loading_time", 40.0),
        ("takeoff_distance", 60.0),
        ("announced_takeoff", 60.0),
        ("peak_voltage", 12.5),
    ]);
    r.series.insert(
        "battery_current".into(),
        series(&[0.0, 60.0], &[20.0, 20.0]),
    );
    let base = e.evaluate_round(&r).unwrap();
    let with = |score: f64| {
        let mut x = base.clone();
        x.score = score;
        x
    };
    let three = e
        .flight_score(&[with(900.0), with(600.0), with(800.0)])
        .unwrap();
    approx(three, 0.25 * 600.0 + 0.5 * 800.0 + 0.25 * 900.0, 1e-12);
    // Two rounds: the worse counts 50 % (s3.7.11).
    approx(
        e.flight_score(&[with(900.0), with(600.0)]).unwrap(),
        750.0,
        1e-12,
    );
    // Four rounds: 25 / 50 / then the remaining 25 % shared.
    let four = e
        .flight_score(&[with(700.0), with(600.0), with(800.0), with(900.0)])
        .unwrap();
    approx(
        four,
        0.25 * 600.0 + 0.5 * 700.0 + 0.125 * 800.0 + 0.125 * 900.0,
        1e-12,
    );
}

fn xc25_round() -> RoundInputs {
    inputs(&[
        ("payload_volume", 0.003),
        ("time_1km", 50.0),
        ("glide_time", 90.0),
        ("altitude_at_60s", 80.0),
        ("loading_time", 25.0),
        ("takeoff_distance", 35.0),
        ("announced_takeoff", 40.0),
    ])
}

#[test]
fn xc_2025_clubs_worked_example_eq_1_to_10() {
    // XC 2025 Clubs v1.20 s8: 3 L (field best 4 L), 50 s for 1 km (best 40 s),
    // 90 s glide (best 120 s), 80 m at 60 s, 25 s loading (best C/sqrt(t) of
    // 6e-4 L-equivalents), 40 m declared and flown in 35 m.
    let out = engine("xc-2025-clubs")
        .evaluate_round(&xc25_round())
        .unwrap();
    let a: f64 = 80.0;
    let p_alt = 6.5455e-6 * a.powi(4) - 0.001822 * a.powi(3) + 0.1436 * a.powi(2) - 1.3011 * a; // eq. 4
    let ct = 3.0e-3 / 25.0_f64.sqrt(); // m^3/s^0.5; eq. 5
    let expected = ((200.0 * 3.0 / 4.0 + 200.0 * 40.0 / 50.0 + 150.0 * 90.0 / 120.0 + p_alt)
        + 100.0 * ct / 6.0e-4)
        * 1.125;
    approx(out.score, expected, 1e-9);
    // The published polynomial peaks at 150.19 at 80 m, not exactly 150.
    approx(p_alt, 150.191_68, 1e-6);
    // Violating the declared take-off zeroes the round; an external pilot x0.75.
    let mut long = xc25_round();
    long.values.insert("takeoff_distance".into(), 45.0);
    assert_eq!(
        engine("xc-2025-clubs").evaluate_round(&long).unwrap().score,
        0.0
    );
    let mut ext = xc25_round();
    ext.values.insert("pilot_external".into(), 1.0);
    approx(
        engine("xc-2025-clubs").evaluate_round(&ext).unwrap().score,
        0.75 * expected,
        1e-9,
    );
    // Flight score: mean of all rounds except the worst (eq. 10).
    let e = engine("xc-2025-clubs");
    let r = e.evaluate_round(&xc25_round()).unwrap();
    let mut a1 = r.clone();
    a1.score = 700.0;
    let mut a2 = r.clone();
    a2.score = 500.0;
    approx(
        e.flight_score(&[a1, a2, r.clone()]).unwrap(),
        (700.0 + r.score) / 2.0,
        1e-12,
    );
}

#[test]
fn xc_2023_worked_example_eq_1_to_10() {
    // XC 2023 v3.00 s9: 2.4 kg (best 3.0 kg), 45 s for 1 km (best 40 s), rho
    // 1.15 kg/m3 with PL_pred = 2.092 rho + 0.7468, a loop (3 pts), 30 s
    // loading, 25 m declared and flown in 22 m.
    let mut r = inputs(&[
        ("payload_mass", 2.4),
        ("time_1km", 45.0),
        ("air_density", 1.15),
        ("acro_points", 3.0),
        ("loading_time", 30.0),
        ("takeoff_distance", 22.0),
        ("announced_takeoff", 25.0),
    ]);
    let e = engine("xc-2023");
    let out = e.evaluate_round(&r).unwrap();
    let pl_pred: f64 = 2.092 * 1.15 + 0.7468;
    let b_pred = 50.0 * (1.0 - (2.4 / pl_pred - 1.0).abs());
    let b_load = 0.021 * 30.0_f64.powi(2) - 2.59 * 30.0 + 79.8; // eq. 3, 10 <= t < 60
    let expected = ((275.0 * 40.0 / 45.0 + 200.0 * 2.4 / 3.0 + b_pred + 3.0) + b_load) * 1.1;
    approx(out.score, expected, 1e-9);
    approx(b_load, 21.0, 1e-12);
    // Loading bonus branches: 60(1 - t/150) below 10 s (continuous at 10 s),
    // zero from 60 s.
    for (t, want) in [
        (5.0, 60.0 * (1.0 - 5.0 / 150.0)),
        (10.0, 56.0),
        (59.999, 0.0),
        (60.0, 0.0),
        (90.0, 0.0),
    ] {
        r.values.insert("loading_time".into(), t);
        let b = e.evaluate_round(&r).unwrap().term("b_load").unwrap();
        assert!((b - want).abs() < 0.05, "t = {t}: {b} vs {want}");
    }
    // The prediction bonus is never negative (s9.4).
    r.values.insert("payload_mass".into(), 8.0);
    assert_eq!(e.evaluate_round(&r).unwrap().term("b_pred").unwrap(), 0.0);
    // The worst of more than three flights is dropped, otherwise all count.
    let base = e.evaluate_round(&r).unwrap();
    let with = |s: f64| {
        let mut x = base.clone();
        x.score = s;
        x
    };
    approx(
        e.flight_score(&[with(400.0), with(500.0), with(600.0)])
            .unwrap(),
        500.0,
        1e-12,
    );
    approx(
        e.flight_score(&[with(100.0), with(400.0), with(500.0), with(600.0)])
            .unwrap(),
        500.0,
        1e-12,
    );
}

#[test]
fn sae_regular_worked_example_ffs_with_prediction_bonus() {
    // SAE 2026 s7.6: FS = 4 EB + 15 FB, FFS = mean(top 3) + PPB with
    // PPB = max(10 - (FS - PS)^2, 0) taken at the best of the three rounds.
    let e = engine("sae-regular-2026");
    let round = |eb: f64, fb: f64| {
        e.evaluate_round(&inputs(&[
            ("empty_bottles", eb),
            ("filled_bottles", fb),
            ("predicted_score", 40.0),
        ]))
        .unwrap()
    };
    let (r1, r2, r3, r4) = (
        round(3.0, 2.0),
        round(2.0, 2.0),
        round(0.0, 2.0),
        round(0.0, 1.0),
    );
    assert_eq!(r1.score, 42.0);
    assert_eq!(r2.score, 38.0);
    assert_eq!(r3.score, 30.0);
    // PPB: FS 42 -> 10 - 4 = 6; FS 38 -> 10 - 4 = 6; FS 30 -> 0. Best is 6.
    let ffs = e
        .flight_score(&[r1.clone(), r2.clone(), r3.clone(), r4])
        .unwrap();
    approx(ffs, (42.0 + 38.0 + 30.0) / 3.0 + 6.0, 1e-12);
    // A far-off prediction earns no bonus (a negative PPB defaults to zero).
    let far = e
        .evaluate_round(&inputs(&[
            ("empty_bottles", 3.0),
            ("filled_bottles", 2.0),
            ("predicted_score", 80.0),
        ]))
        .unwrap();
    approx(e.flight_score(&[far]).unwrap(), 42.0, 1e-12);
    // A perfect prediction earns the full 10.
    let exact = e
        .evaluate_round(&inputs(&[
            ("empty_bottles", 3.0),
            ("filled_bottles", 2.0),
            ("predicted_score", 42.0),
        ]))
        .unwrap();
    approx(e.flight_score(&[exact]).unwrap(), 42.0 + 10.0, 1e-12);
}

#[test]
fn sae_regular_gates() {
    let e = engine("sae-regular-2026");
    let mut r = inputs(&[
        ("span", 100.0 * IN),
        ("chord_min", 6.0 * IN),
        ("body_length", 90.0 * IN),
        ("frp_used", 0.0),
        ("motor_count", 2.0),
        ("cell_count", 4.0),
        ("battery_capacity", 2200.0 * 3.6),
        ("prop_diameter", 12.0 * IN),
    ]);
    assert!(e
        .evaluate_round(&r)
        .unwrap()
        .gates
        .iter()
        .all(|g| g.satisfied));
    for (k, v, gate) in [
        ("span", 70.0 * IN, "span_band"),
        ("span", 121.0 * IN, "span_band"),
        ("chord_min", 3.0 * IN, "chord_min"),
        ("frp_used", 1.0, "no_frp"),
        ("motor_count", 3.0, "motors"),
        ("battery_capacity", 2400.0 * 3.6, "capacity"),
        ("prop_diameter", 10.0 * IN, "prop"), // 4 motors below
    ] {
        let mut b = r.clone();
        b.values.insert(k.into(), v);
        if gate == "prop" {
            b.values.insert("motor_count".into(), 4.0);
        }
        let out = e.evaluate_round(&b).unwrap();
        assert!(
            !out.gates.iter().find(|g| g.id == gate).unwrap().satisfied,
            "{gate} {k}"
        );
    }
    r.values.insert("prop_diameter".into(), 9.0 * IN);
    r.values.insert("motor_count".into(), 4.0);
    assert!(e
        .evaluate_round(&r)
        .unwrap()
        .gates
        .iter()
        .all(|g| g.satisfied));
}

#[test]
fn sae_micro_worked_example_s9_6() {
    // SAE 2026 s9.6: 5 lb payload, 2.5 lb empty, 4 ft span, take-off in 20 ft.
    // M = 11/((2.5-1)^4 + 8.9), Z = 15 - 4^1.5, FS = 3*5*M + Z.
    let e = engine("sae-micro-2026");
    let round = |payload_lb: f64, empty_lb: f64, span_ft: f64, to_ft: f64| {
        e.evaluate_round(&inputs(&[
            ("payload_mass", payload_lb * LB),
            ("empty_mass", empty_lb * LB),
            ("span", span_ft * FT),
            ("takeoff_distance", to_ft * FT),
        ]))
        .unwrap()
    };
    let m = 11.0 / ((2.5_f64 - 1.0).powi(4) + 8.9);
    let r = round(5.0, 2.5, 4.0, 20.0);
    approx(r.score, 3.0 * 5.0 * m + (15.0 - 4.0_f64.powf(1.5)), 1e-9);
    approx(r.score, 18.817_36, 1e-6);
    // Take-off bins: <= 10 ft 20, <= 25 ft 15, <= 50 ft 9, <= 100 ft 0.
    for (ft, b) in [
        (5.0, 20.0),
        (10.0, 20.0),
        (10.5, 15.0),
        (25.0, 15.0),
        (40.0, 9.0),
        (75.0, 0.0),
    ] {
        approx(
            round(5.0, 2.5, 4.0, ft).term("b_takeoff").unwrap(),
            b,
            1e-12,
        );
    }
    // Beyond 100 ft the attempt is disqualified: zero.
    assert_eq!(round(5.0, 2.5, 4.0, 120.0).score, 0.0);
    // Sum of the top three flight scores.
    let (a, b, c, d) = (
        round(5.0, 2.5, 4.0, 20.0),
        round(4.0, 2.5, 4.0, 20.0),
        round(3.0, 2.5, 4.0, 20.0),
        round(1.0, 2.5, 4.0, 20.0),
    );
    approx(
        e.flight_score(&[a.clone(), b.clone(), c.clone(), d])
            .unwrap(),
        a.score + b.score + c.score,
        1e-12,
    );
    // The 450 W limiter is a soft margin.
    let mut over = inputs(&[
        ("payload_mass", 2.0),
        ("empty_mass", 1.5),
        ("span", 1.2),
        ("takeoff_distance", 5.0),
        ("peak_power", 500.0),
        ("cell_count", 4.0),
        ("container_volume", 67.0 * 2.957_352_956_25e-5),
    ]);
    let out = e.evaluate_round(&over).unwrap();
    let lim = out.gates.iter().find(|g| g.id == "limiter").unwrap();
    assert!(!lim.satisfied && !lim.hard && out.feasible());
    over.values
        .insert("container_volume".into(), 60.0 * 2.957_352_956_25e-5);
    assert!(!e.evaluate_round(&over).unwrap().feasible());
}

#[test]
fn dbf_worked_example_s3_3_3_and_s6_3() {
    // DBF 2025-26 s3.3.3: 60 in span (5 ft) -> RAC 1.00; 100 Wh pack -> EF 1;
    // M2: 4 pax + 2 cargo, 3 laps: income 4(6+6) + 2(10+24) = 116, cost
    // 3(10 + 2 + 4)*1 = 48, net 68 of a field best 100 -> M2 1.68; M3: 4 laps,
    // 1 m banner (field best 5): 2 + 4/1/5 = 2.8; GM 40 s/80 s = 0.5.
    let e = engine("dbf-2026");
    let r = inputs(&[
        ("span", 60.0 * IN),
        ("battery_energy", 100.0 * 3600.0),
        ("pax_count", 4.0),
        ("cargo_count", 2.0),
        ("laps_m2", 3.0),
        ("laps_m3", 4.0),
        ("banner_length", 1.0),
        ("ground_mission_time", 80.0),
        ("net_income_best", 100.0),
        ("banner_metric_best", 5.0),
        ("ground_time_min", 40.0),
        ("report_score", 100.0),
    ]);
    let out = e.evaluate_round(&r).unwrap();
    approx(out.term("rac").unwrap(), 1.0, 1e-12);
    approx(out.term("net_income").unwrap(), 68.0, 1e-12);
    approx(out.term("m2").unwrap(), 1.68, 1e-12);
    approx(out.term("m3").unwrap(), 2.8, 1e-12);
    approx(out.term("gm").unwrap(), 0.5, 1e-12);
    approx(out.score, 100.0 * (1.0 + 1.68 + 2.8 + 0.5), 1e-12);
    // RAC rounding: 3 ft span is the minimum 0.90, 5 ft is 1.00, and a span
    // of 4.4 ft rounds to the nearest inch (53 in = 4.42 ft -> 0.971).
    let rac = |span_in: f64| {
        let mut x = r.clone();
        x.values.insert("span".into(), span_in * IN);
        e.evaluate_round(&x).unwrap().term("rac").unwrap()
    };
    approx(rac(30.0), 0.90, 1e-12);
    approx(rac(53.0), 0.05 * 4.42 + 0.75, 1e-12);
    // A bigger pack raises the cost per lap (EF = Wh/100).
    let mut big = r.clone();
    big.values.insert("battery_energy".into(), 200.0 * 3600.0);
    assert!(e.evaluate_round(&big).unwrap().term("net_income").unwrap() < 68.0);
    // Gates.
    let mut g = r.clone();
    g.values.insert("span".into(), 1.6);
    g.values.insert("mtow".into(), 20.0);
    g.values.insert("pack_energy_max".into(), 99.0 * 3600.0);
    g.values.insert("fuse_rating".into(), 100.0);
    let out = e.evaluate_round(&g).unwrap();
    assert!(!out.gates.iter().find(|x| x.id == "span").unwrap().satisfied);
    assert!(out
        .gates
        .iter()
        .filter(|x| x.id != "span")
        .all(|x| x.satisfied));
}
