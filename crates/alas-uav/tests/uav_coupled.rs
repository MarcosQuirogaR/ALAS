// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A failed unwrap in this test target is the assertion reporting malformed
// evidence, not a panic escaping from library code.
// The BLIND|/E2E| lines are the reported evidence of this target.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

//! Integrated chain: `UavSpec` -> viscous VLM + drag build-up -> powertrain ->
//! mission -> score, for ACC 2026 and one preset each of SAE, DBF and XC.
//!
//! Numerical verification of the glue (the models themselves are verified in
//! their own targets): trait adapters agree with the wrapped solver, energy
//! closes, the score engine consumes the flown mission.  The aircraft is the
//! Xtra2UPV build of the blind-prediction test; no number here is a
//! validation.

#[path = "support/blind_aircraft.rs"]
mod blind;

use alas_uav::coupled::assess::mission_config_for;
use alas_uav::coupled::{AeroSettings, ElectricPowertrain, ElectricSettings};
use alas_uav::perf::{BatteryState, Environment, Powertrain};
use alas_uav::score::presets;
use alas_uav::spec::mission::PresetRef;

fn fast_vehicle<'a>(
    t: &'a blind::Team,
    id: &str,
    cap: Option<f64>,
) -> alas_uav::coupled::Vehicle<'a> {
    let mut v = blind::vehicle(t, cap);
    v.aero = AeroSettings {
        strips: 12,
        ..AeroSettings::default()
    };
    v.spec = &t.spec;
    let _ = id;
    v
}

#[test]
fn electric_adapter_matches_the_wrapped_solver_and_handles_windmilling() {
    let t = blind::xtra2upv();
    let pt = ElectricPowertrain::new(
        t.chain.clone(),
        ElectricSettings {
            battery_current_cap_a: Some(30.0),
            ..ElectricSettings::default()
        },
        t.rho,
    )
    .unwrap();
    let fresh = BatteryState::default();
    for (th, v) in [(1.0, 0.0), (1.0, 15.0), (0.6, 20.0)] {
        let p = pt.evaluate(th, v, t.rho, &fresh).unwrap();
        let op = pt.operating_point(th, v, 1.0).unwrap();
        assert!((p.thrust_n - op.thrust_n).abs() < 1e-9);
        assert!((p.battery_current_a - op.i_battery_a).abs() < 1e-9);
        assert!(
            p.battery_current_a <= 30.0 + 1e-6,
            "cap {}",
            p.battery_current_a
        );
        assert!(op.balance_residual.abs() < 1e-6);
    }
    // Charge drawn lowers the pack voltage.
    let used = BatteryState {
        charge_used_c: 0.7 * 2.6 * 3600.0,
        energy_used_j: 0.0,
    };
    let full = pt.evaluate(1.0, 10.0, t.rho, &fresh).unwrap();
    let low = pt.evaluate(1.0, 10.0, t.rho, &used).unwrap();
    assert!(low.battery_voltage_v < full.battery_voltage_v);
    // Far above the free-running speed: no powered equilibrium, no thrust.
    let free = pt.evaluate(0.2, 60.0, t.rho, &fresh).unwrap();
    assert_eq!(free.thrust_n, 0.0);
    assert_eq!(free.battery_current_a, 0.0);
    // A mismatched density is refused rather than silently reused.
    assert!(pt.evaluate(1.0, 10.0, 0.9, &fresh).is_err());
}

#[test]
fn polar_is_monotone_and_induced_drag_is_parabolic() {
    let t = blind::xtra2upv();
    let env = Environment::new(t.rho, 0.0).unwrap();
    let v = fast_vehicle(&t, "acc-2026", Some(30.0));
    let m = v.models(&env).unwrap();
    let p = &m.airframe.polar;
    assert!(p.cl_max() > 1.0 && p.cl_max() < 1.8, "{}", p.cl_max());
    let mut last = 0.0;
    for k in 1..=10 {
        let cl = p.cl_max() * f64::from(k) / 11.0;
        let (cdp, cdi) = p.cd_parts(cl);
        assert!(cdp > 0.005 && cdp < 0.08 && cdi > 0.0);
        assert!(cdi > last, "induced drag must grow with CL");
        last = cdi;
    }
    // CDi/CL^2 is nearly constant (e between 0.8 and 1.05 for this wing).
    let (_, cdi) = p.cd_parts(0.8);
    let e = 0.8 * 0.8 / (std::f64::consts::PI * m.polar.aspect_ratio * cdi);
    assert!((0.8..1.05).contains(&e), "e = {e}");
    // Ground effect default is Raymer Eq. 12.60.
    let phi = m.airframe.ground_effect_factor();
    let h_b = m.airframe.wing_height_m / m.airframe.span_m;
    let x = 33.0 * h_b.powf(1.5);
    assert!((phi - x / (1.0 + x)).abs() < 1e-12);
}

#[test]
fn acc_2026_end_to_end_scores_the_flown_mission() {
    let t = blind::xtra2upv();
    let env = Environment::new(t.rho, 0.0).unwrap();
    let v = fast_vehicle(&t, "acc-2026", Some(30.0));
    let preset = presets::load("acc-2026").unwrap();
    let cfg = mission_config_for(&preset);
    let a = v.assess(env, &cfg).unwrap();
    let d = a.inputs.values["distance_120s"];
    assert!(d > 1000.0 && d < 5000.0, "{d}");
    assert!(a.inputs.values["peak_current"] <= 30.5, "limiter holds");
    assert!(a.round.score.is_finite() && a.round.score > 0.0);
    // Energy closure of the integrated powertrain over the whole mission.
    let (mut e, mut q) = (0.0, 0.0);
    for w in a.mission.samples.windows(2) {
        let dt = w[1].t - w[0].t;
        e += 0.5 * (w[0].power_w + w[1].power_w) * dt;
        q += 0.5 * (w[0].current_a + w[1].current_a) * dt;
    }
    let last = a.mission.samples.last().unwrap();
    assert!((e / last.energy_j - 1.0).abs() < 5e-3);
    assert!((q / last.charge_c - 1.0).abs() < 5e-3);
    // Lift-off before the announced 60 m and above the stall speed.
    assert!(a.inputs.values["takeoff_distance"] < 60.0);
}

fn other_preset(id: &str, extra: &[(&str, f64)]) -> f64 {
    let mut t = blind::xtra2upv();
    let preset = presets::load(id).unwrap();
    t.spec.mission.preset = PresetRef {
        id: id.into(),
        version: preset.version,
        variant: None,
    };
    t.spec.mission.segments = preset.segments.clone();
    let env = Environment::new(1.2, 0.0).unwrap();
    let mut v = fast_vehicle(&t, id, Some(30.0));
    for (k, val) in extra {
        v.extra_inputs.insert((*k).to_owned(), *val);
    }
    let mut cfg = mission_config_for(&preset);
    cfg.climb_speed_m_s = Some(13.0);
    let a = v.assess(env, &cfg).unwrap();
    assert!(a.round.score.is_finite(), "{id}");
    println!(
        "E2E|{id}|score {:.3}|{} terms",
        a.round.score,
        a.round.terms.len()
    );
    a.round.score
}

#[test]
fn sae_regular_end_to_end() {
    let s = other_preset(
        "sae-regular-2026",
        &[
            ("empty_bottles", 2.0),
            ("filled_bottles", 2.0),
            ("predicted_score", 40.0),
        ],
    );
    assert!(s > 0.0);
}

#[test]
fn dbf_end_to_end() {
    let s = other_preset(
        "dbf-2026",
        &[
            ("pax_count", 4.0),
            ("cargo_count", 2.0),
            // The template flies the charter mission only; the banner mission
            // (M3) is not simulated, so its flag and lap count are declared.
            ("m2_flown", 1.0),
            ("m3_flown", 0.0),
            ("laps_m3", 0.0),
            ("banner_length", 1.0),
            ("ground_mission_time", 80.0),
            ("net_income_best", 100.0),
            ("banner_metric_best", 5.0),
            ("ground_time_min", 40.0),
            ("report_score", 100.0),
            ("battery_energy", 100.0 * 3600.0),
        ],
    );
    assert!(s > 0.0);
}

#[test]
fn xc_2025_end_to_end() {
    let s = other_preset("xc-2025-clubs", &[("payload_volume", 0.003)]);
    assert!(s > 0.0);
}
