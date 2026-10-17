// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Failed assertions identify broken fixtures in this test target.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Motor model verification.
//!
//! Source for the equations: M. Drela, "First-Order DC Electric Motor Model",
//! MIT Aero & Astro, February 2007 (https://web.mit.edu/drela/Public/web/qprop/motor1_theory.pdf),
//! read 2026-10-12: eq. (1) Q = (i - i0)/KQ, eq. (3) v = omega/KV + i R,
//! eq. (4) i(omega, v), eq. (7) eta, eq. (8) eta = KV/KQ for i0 = R = 0,
//! eq. (12) KQ = KV.  The manufacturer figures are the Hobbywing Skywalker
//! 2814 SL 1000KV values stored in the curated kit (page read 2026-10-12).

use alas_uav::power::from_spec::motor_from_spec;
use alas_uav::power::{MotorModel, MotorThermal};
use alas_uav_db::parts::MotorSpec;
use alas_uav_db::{kit, Body, ComponentId};

const KV_RPM_V: f64 = 1000.0;

fn kv_si(rpm_per_v: f64) -> f64 {
    rpm_per_v * std::f64::consts::TAU / 60.0
}

fn motor() -> MotorModel {
    MotorModel::new(kv_si(KV_RPM_V), 0.029, 2.0, 14.8).unwrap()
}

#[test]
fn zero_loss_motor_is_ideal_drela_eq8() {
    // i0 = 0 and R -> 0: eta = KV/KQ = 1 (eq. 8 with KQ = KV, eq. 12).
    let m = MotorModel::new(kv_si(1000.0), 1.0e-9, 0.0, 10.0).unwrap();
    for (omega, q) in [(300.0, 0.05), (900.0, 0.3), (50.0, 1.0)] {
        let s = m.state(omega, q, 293.15);
        assert!(
            (s.efficiency - 1.0).abs() < 1.0e-6,
            "eta = {}",
            s.efficiency
        );
        // v = omega / KV exactly, eq. (2)/(3) with i R -> 0.
        assert!((s.voltage_v - omega / kv_si(1000.0)).abs() < 1.0e-6);
    }
}

#[test]
fn matches_drela_eqs_1_to_7_at_a_terminal_condition() {
    let m = motor();
    let (v, omega) = (14.8, 1200.0);
    let r = 0.029;
    let i_eq4 = (v - omega / kv_si(KV_RPM_V)) / r; // eq. (4)
    let q_eq5 = (i_eq4 - 2.0 * (omega / (kv_si(KV_RPM_V) * 14.8))) * (1.0 / kv_si(KV_RPM_V)); // eq. (1), i0 at this speed
    let s = m.state(omega, q_eq5, 293.15);
    assert!((s.current_a - i_eq4).abs() < 1.0e-9 * i_eq4);
    assert!((s.voltage_v - v).abs() < 1.0e-9);
    // eq. (7): eta = (1 - i0/i) / (1 + i R KV / omega) with KQ = KV.
    let i0 = m.i0(omega, 293.15);
    let eta_eq7 = (1.0 - i0 / i_eq4) / (1.0 + i_eq4 * r * kv_si(KV_RPM_V) / omega);
    assert!((s.efficiency - eta_eq7).abs() < 1.0e-12);
}

#[test]
fn power_balance_is_exact_at_all_temperatures() {
    let m = motor();
    for t in [263.15, 293.15, 340.0, 390.0] {
        for (omega, q) in [(100.0, 0.02), (700.0, 0.4), (1400.0, 0.9)] {
            let s = m.state(omega, q, t);
            let rhs = s.p_copper_w + s.p_iron_w + s.p_shaft_w;
            assert!((s.p_electric_w - rhs).abs() < 1.0e-12 * s.p_electric_w.max(1.0));
        }
    }
}

#[test]
fn maximum_efficiency_current_is_sqrt_i0_v_over_r() {
    // d eta / d i = 0 for eta = (1 - i0/i)(1 - i R / v) gives i = sqrt(i0 v / R).
    let m = MotorModel::new(kv_si(1000.0), 0.029, 2.0, 14.8).unwrap();
    let v = 14.8;
    let eta = |i: f64| (1.0 - 2.0 / i) * (1.0 - i * 0.029 / v);
    let i_star = m.i_max_efficiency(v);
    assert!((i_star - (2.0_f64 * v / 0.029).sqrt()).abs() < 1.0e-12);
    assert!(eta(i_star) > eta(0.97 * i_star) && eta(i_star) > eta(1.03 * i_star));
    let (mut lo, mut hi) = (2.1, 500.0);
    for _ in 0..200 {
        let (a, b) = (lo + (hi - lo) / 3.0, hi - (hi - lo) / 3.0);
        if eta(a) < eta(b) {
            lo = a;
        } else {
            hi = b;
        }
    }
    assert!((0.5 * (lo + hi) - i_star).abs() < 1.0e-6 * i_star);
}

#[test]
fn temperature_dependence_has_the_documented_signs() {
    let m = motor();
    let (cold, hot) = (293.15, 393.15);
    // Copper: +0.393 %/K x 100 K = +39.3 %.
    assert!((m.rm(hot) / m.rm(cold) - 1.393).abs() < 1.0e-12);
    // NdFeB flux falls when hot: Kt falls, Kv rises (plan 3.2 sign check).
    assert!(m.kt(hot) < m.kt(cold) && m.kv(hot) > m.kv(cold));
    assert!((m.kv(hot) * m.kt(hot) - 1.0).abs() < 1.0e-12);
    // Loss torque is held, so the no-load current rises as Kt falls.
    assert!((m.i0(500.0, hot) * m.kt(hot) - m.i0(500.0, cold) * m.kt(cold)).abs() < 1.0e-12);
}

#[test]
fn i0_is_normalised_to_the_test_voltage_and_scales_with_speed() {
    let m = motor();
    let w_test = kv_si(KV_RPM_V) * 14.8;
    assert!((m.i0(w_test, 293.15) - 2.0).abs() < 1.0e-12); // exact at the quoted point
    assert!((m.i0(0.5 * w_test, 293.15) - 1.0).abs() < 1.0e-12); // a = 1
    let mut flat = m.clone();
    flat.i0_exponent = 0.0;
    assert!((flat.i0(0.2 * w_test, 293.15) - 2.0).abs() < 1.0e-12);
    let table = motor()
        .with_i0_table(vec![100.0, 1000.0], vec![0.5, 2.5])
        .unwrap();
    assert!((table.i0(550.0, 293.15) - 1.5).abs() < 1.0e-12);
}

#[test]
fn lumped_thermal_model_matches_the_closed_form() {
    let mut m = motor();
    m.thermal = Some(MotorThermal {
        r_th_k_per_w: 2.0,
        c_th_j_per_k: 50.0,
    });
    let (t0, amb, p) = (300.0, 300.0, 40.0);
    // Steady state is T_amb + R_th P.
    assert!((m.t_steady(amb, p) - 380.0).abs() < 1.0e-12);
    // Explicit-Euler integration of C dT/dt = P - (T - T_amb)/R_th agrees with the exact step.
    let (mut t, dt) = (t0, 1.0e-3);
    for _ in 0..60_000 {
        t += dt * m.dt_dt(t, amb, p);
    }
    let exact = m.advance_temperature(t0, amb, p, 60.0);
    assert!((t - exact).abs() < 1.0e-3, "euler {t} vs exact {exact}");
    // time_to_temperature inverts advance_temperature.
    let t_hit = m.time_to_temperature(t0, amb, p, 350.0).unwrap();
    assert!((m.advance_temperature(t0, amb, p, t_hit) - 350.0).abs() < 1.0e-9);
    assert!(m.time_to_temperature(t0, amb, p, 381.0).is_none());
    assert_eq!(m.time_to_temperature(400.0, amb, p, 350.0), Some(0.0));
}

fn kit_motor(id: &str) -> MotorSpec {
    let db = kit::kit().expect("kit loads");
    let id = ComponentId::new(id).unwrap();
    let component = db.current(&id).expect("kit motor");
    match &component.body {
        Body::Motor(spec) => spec.clone(),
        other => panic!("not a motor: {other:?}"),
    }
}

#[test]
fn hobbywing_2814_page_values_are_internally_consistent() {
    // Manufacturer figures (Hobbywing Skywalker SL 2814 1000KV, kit, read 2026-10-12):
    // 1000 KV, Rm 29.0 mOhm, I0 2.0 A at 14.8 V, 55.2 A and 815.8 W for 43 s.
    // This checks the data path (spec -> model), NOT the physics: the page's
    // "maximum power" equals nominal voltage x maximum current to 0.2 %.
    let spec = kit_motor("hobbywing-skywalker-2814-sl-1000kv");
    let m = motor_from_spec(&spec, "hobbywing-2814").unwrap();
    assert!((m.kv_ref() - kv_si(1000.0)).abs() < 1.0e-9);
    assert!((m.rm_ref() - 0.029).abs() < 1.0e-12);
    let lim = m.limits;
    assert!((lim.i_burst_a.unwrap() - 55.2).abs() < 1.0e-12);
    let p_nominal = 14.8 * lim.i_burst_a.unwrap();
    assert!((lim.p_max_w.unwrap() / p_nominal - 1.0).abs() < 2.0e-3);
    // Quoted no-load point: i0 = 2.0 A at the speed Kv x 14.8 V.
    assert!((m.i0(m.kv_ref() * 14.8, 293.15) - 2.0).abs() < 1.0e-12);
}

#[test]
fn burst_rating_is_consistent_with_adiabatic_heating_of_the_default_heat_capacity() {
    // PLAUSIBILITY only: 55.2 A through 29 mOhm for the page's 43 s, heating
    // C_th = 450 J/(kg K) x 0.115 kg adiabatically, raises the winding ~73 K,
    // the order of a 100 C limit from a 20 C start.  Not a validation.
    let spec = kit_motor("hobbywing-skywalker-2814-sl-1000kv");
    let c = alas_uav::power::from_spec::MOTOR_SPECIFIC_HEAT * spec.mass_kg.as_ref().unwrap().value;
    let rise = 55.2_f64.powi(2) * 0.029 * 43.0 / c;
    assert!((60.0..90.0).contains(&rise), "adiabatic rise {rise} K");
}

#[test]
fn missing_database_fields_are_reported_not_defaulted() {
    let spec = MotorSpec::default();
    let err = motor_from_spec(&spec, "empty").unwrap_err();
    assert!(err.to_string().contains("kv_rad_s_per_v"), "{err}");
}
