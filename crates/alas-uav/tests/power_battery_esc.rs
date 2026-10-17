// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Failed assertions identify broken fixtures in this test target.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Battery and ESC verification (analytic cases).
//!
//! The battery cases are closed-form integrals of the model equations; the
//! SOC resistance rise is checked against the 50-100 % band the plan quotes
//! from the published studies (component-data.md, EVS-24 reference).  No
//! manufacturer discharge sheet is in the curated kit yet, so the discharge
//! curve is VERIFIED against the analytic model, not validated.

use alas_uav::power::{BatteryModel, EscModel};

/// A pack with a linear Voc(SOC) = a + b SOC and constant resistance.
fn linear_pack(a: f64, b: f64, r: f64, capacity_c: f64) -> BatteryModel {
    let mut m = BatteryModel::new(capacity_c, 4, r)
        .unwrap()
        .with_voc_table(vec![0.0, 1.0], vec![a, a + b])
        .unwrap();
    m.r_soc_slope = 0.0;
    m
}

#[test]
fn constant_current_discharge_matches_the_closed_form() {
    let (a, b, r, q, i, t) = (12.0, 4.8, 0.02, 5.0 * 3600.0, 30.0, 400.0);
    let m = linear_pack(a, b, r, q);
    let mut s = m.start_state(1.0, 298.15).unwrap();
    let n = 40_000;
    for _ in 0..n {
        m.step(&mut s, i, 298.15, t / f64::from(n));
    }
    // SOC(t) = 1 - I t / Q; E = I [(a + b) t - b I t^2 / (2 Q)] - I^2 R t.
    let soc = 1.0 - i * t / q;
    let energy = i * ((a + b) * t - b * i * t * t / (2.0 * q)) - i * i * r * t;
    assert!((s.soc - soc).abs() < 1.0e-12);
    assert!(
        (s.energy_out_j / energy - 1.0).abs() < 1.0e-4,
        "{} vs {energy}",
        s.energy_out_j
    );
    assert!((s.energy_lost_j - i * i * r * t).abs() < 1.0e-6 * i * i * r * t);
    assert!((s.charge_out_c - i * t).abs() < 1.0e-6);
}

#[test]
fn sag_is_current_times_resistance() {
    let m = linear_pack(12.0, 4.8, 0.02, 18_000.0);
    assert!((m.v_terminal(0.5, 298.15, 30.0) - (14.4 - 30.0 * 0.02)).abs() < 1.0e-12);
    assert!((m.v_terminal(0.5, 298.15, 0.0) - m.voc(0.5)).abs() < 1.0e-15);
}

#[test]
fn resistance_rise_with_depth_of_discharge_is_in_the_published_band() {
    // Plan 3.2: published studies show a 50-100 % rise between 100 % and 15 % SOC.
    let m = BatteryModel::new(18_000.0, 4, 0.02).unwrap();
    let rise = m.r_pack(0.15, 298.15) / m.r_pack(1.0, 298.15) - 1.0;
    assert!((0.5..=1.0).contains(&rise), "rise {rise}");
}

#[test]
fn resistance_temperature_forms() {
    let mut m = BatteryModel::new(18_000.0, 4, 0.02).unwrap();
    assert!((m.r_pack(1.0, 273.15) - m.r_pack(1.0, 313.15)).abs() < 1.0e-15); // b = 0: off by default
    m.r_temp_b_k = 2000.0;
    assert!(
        m.r_pack(1.0, 273.15) > m.r_pack(1.0, 298.15)
            && m.r_pack(1.0, 313.15) < m.r_pack(1.0, 298.15)
    );
    // Measured curves take over: interpolate in SOC, then in T, clamped.
    let m = BatteryModel::new(18_000.0, 4, 0.02)
        .unwrap()
        .with_r_curve(273.15, vec![0.0, 1.0], vec![0.06, 0.04])
        .unwrap()
        .with_r_curve(298.15, vec![0.0, 1.0], vec![0.03, 0.02])
        .unwrap();
    assert!((m.r_pack(0.5, 273.15) - 0.05).abs() < 1.0e-12);
    assert!((m.r_pack(0.5, 285.65) - 0.0375).abs() < 1.0e-12);
    assert!((m.r_pack(0.5, 350.0) - 0.025).abs() < 1.0e-12);
}

#[test]
fn cutoff_triggers_on_loaded_voltage_and_on_soc() {
    let mut m = linear_pack(12.0, 4.8, 0.05, 18_000.0);
    m.v_cutoff_v = 12.0;
    let healthy = m.start_state(0.9, 298.15).unwrap();
    assert!(!m.is_cutoff(&healthy, 20.0)); // 16.32 V open, 15.32 V loaded
    let weak = m.start_state(0.1, 298.15).unwrap();
    assert!(m.is_cutoff(&weak, 20.0)); // 12.48 V open, 11.48 V loaded
    assert!(!m.is_cutoff(&weak, 0.0));
    let mut s = m.start_state(0.001, 298.15).unwrap();
    let step = m.step(&mut s, 10.0, 298.15, 10.0);
    assert!(step.cutoff && s.soc == 0.0);
}

#[test]
fn pack_heating_follows_i2r_when_adiabatic() {
    let mut m = linear_pack(12.0, 4.8, 0.02, 18_000.0);
    m.mass_kg = 0.5;
    let mut s = m.start_state(1.0, 298.15).unwrap();
    m.step(&mut s, 40.0, 298.15, 10.0);
    let expected = 40.0_f64.powi(2) * 0.02 * 10.0 / (0.5 * m.c_p_j_per_kg_k);
    assert!((s.t_k - 298.15 - expected).abs() < 1.0e-9);
}

#[test]
fn state_validation_rejects_nonphysical_input() {
    let m = BatteryModel::new(18_000.0, 4, 0.02).unwrap();
    assert!(m.start_state(1.2, 298.15).is_err());
    assert!(m.start_state(0.5, -1.0).is_err());
    assert!(BatteryModel::new(0.0, 4, 0.02).is_err());
    assert!(BatteryModel::new(18_000.0, 0, 0.02).is_err());
}

#[test]
fn ideal_esc_scales_voltage_and_current_by_duty() {
    let e = EscModel::ideal();
    assert!((e.v_motor(0.6, 16.0, 20.0) - 9.6).abs() < 1.0e-15);
    assert!((e.i_input(0.6, 20.0) - 12.0).abs() < 1.0e-15);
    assert!((e.efficiency(0.6, 16.0, 20.0) - 1.0).abs() < 1.0e-15);
}

#[test]
fn nominal_efficiency_is_reproduced_and_energy_is_conserved() {
    let e = EscModel::from_nominal_efficiency(0.96, 14.8, 40.0).unwrap();
    assert!((e.efficiency(1.0, 14.8, 40.0) - 0.96).abs() < 1.0e-12);
    for (d, v, i) in [(1.0, 14.8, 40.0), (0.5, 15.5, 12.0), (0.2, 16.0, 3.0)] {
        let p_in = v * e.i_input(d, i);
        let p_out = e.v_motor(d, v, i) * i;
        assert!((p_in - p_out - e.loss_w(v, i)).abs() < 1.0e-12 * p_in);
    }
    // Part-throttle battery current is below the motor current (plan 3.2).
    assert!(e.i_input(0.5, 12.0) < 12.0);
    assert!(EscModel::from_nominal_efficiency(1.2, 14.8, 40.0).is_err());
}
