// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Failed assertions identify broken fixtures in this test target.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Actuator-disk slipstream: verification against the closed forms of
//! momentum theory.  NOT validated against a slipstream experiment.
//!
//! Closed forms: T = rho A (V + v_i) 2 v_i (momentum), V_far = V sqrt(1 + 2 T
//! / (rho A V^2)), static v_i = sqrt(T / (2 rho A)), and continuity of the
//! contracted slipstream, R_s^2 (V + v(s)) = R^2 (V + v_i).

use alas_uav::aero::slipstream::{PropDisc, Slipstream, StripFlow};
use alas_uav::aero::AirState;
use alas_uav::power::{
    BatteryModel, BatteryState, BemtGeometry, Dynamics, EscModel, Flight, MotorModel, Powertrain,
    PropModel,
};

fn air(v: f64) -> AirState {
    AirState::from_isa(0.0, v).unwrap()
}

#[test]
fn induced_velocity_satisfies_momentum_theory() {
    let disc = PropDisc::new([0.0, 0.0, 0.0], 0.127, 8.0).unwrap();
    for v in [0.5, 5.0, 12.0, 25.0] {
        let a = air(v);
        let vi = disc.induced_velocity(&a);
        let rhs = disc.thrust_n / (2.0 * a.rho_kg_m3 * disc.area_m2());
        assert!((vi * vi + v * vi - rhs).abs() < 1e-12 * rhs.max(1.0));
        // Thrust from momentum: T = rho A (V + v_i) 2 v_i.
        let t = a.rho_kg_m3 * disc.area_m2() * (v + vi) * 2.0 * vi;
        assert!((t / disc.thrust_n - 1.0).abs() < 1e-12);
        // Far wake closed form.
        let far = v * (1.0 + 2.0 * disc.thrust_n / (a.rho_kg_m3 * disc.area_m2() * v * v)).sqrt();
        assert!((disc.far_wake_speed(&a) / far - 1.0).abs() < 1e-12);
    }
    // Static limit.
    let a = air(1e-9);
    let vi = disc.induced_velocity(&a);
    let st = (disc.thrust_n / (2.0 * a.rho_kg_m3 * disc.area_m2())).sqrt();
    assert!((vi / st - 1.0).abs() < 1e-6);
    assert!(disc.disk_loading_n_m2() > 100.0);
}

#[test]
fn slipstream_develops_from_v_i_to_2_v_i_and_contracts_by_continuity() {
    let disc = PropDisc::new([0.0, 0.0, 0.0], 0.127, 8.0).unwrap();
    let a = air(12.0);
    let vi = disc.induced_velocity(&a);
    assert!((disc.increment_at(&a, 0.0) - vi).abs() < 1e-12);
    assert!((disc.increment_at(&a, 1.0e6) / (2.0 * vi) - 1.0).abs() < 1e-6);
    let mut last = vi;
    for s in [0.05, 0.1, 0.2, 0.4, 0.8] {
        let inc = disc.increment_at(&a, s);
        assert!(inc > last && inc < 2.0 * vi);
        last = inc;
        let r = disc.radius_at(&a, s);
        let lhs = r * r * (a.v_m_s + inc);
        let rhs = disc.radius_m * disc.radius_m * (a.v_m_s + vi);
        assert!((lhs / rhs - 1.0).abs() < 1e-12);
        assert!(r < disc.radius_m);
    }
    // Far contraction: R_inf = R sqrt((V + v_i)/(V + 2 v_i)).
    let rinf = disc.radius_at(&a, 1.0e6);
    assert!((rinf / (disc.radius_m * ((12.0 + vi) / (12.0 + 2.0 * vi)).sqrt()) - 1.0).abs() < 1e-6);
}

#[test]
fn strip_coverage_dynamic_pressure_and_flow_angle_follow_the_geometry() {
    let disc = PropDisc::new([-0.2, 0.3, 0.0], 0.127, 8.0).unwrap();
    let a = air(12.0);
    let slip = Slipstream { discs: vec![disc] };
    let alpha = 6.0_f64.to_radians();
    let s = 0.2 + 0.0; // wing quarter chord 0.2 m behind the disc plane at x = 0
    let r = disc.radius_at(&a, s);
    let vs = a.v_m_s + disc.increment_at(&a, s);

    // Strip well inside the disc: full coverage.
    let inside = slip.strip_flow(&a, 0.0, 0.3, 0.0, 0.02, alpha);
    assert!((inside.coverage - 1.0).abs() < 1e-12);
    assert!((inside.q_ratio - (vs / a.v_m_s).powi(2)).abs() < 1e-12);
    let alpha_loc = (alpha.tan() * a.v_m_s / vs).atan();
    assert!((inside.d_alpha_rad - (alpha - alpha_loc)).abs() < 1e-12);
    assert!(inside.d_alpha_rad > 0.0 && inside.q_ratio > 1.0);

    // Strip straddling the slipstream edge: half covered.
    let edge = slip.strip_flow(&a, 0.0, 0.3 + r, 0.0, 0.02, alpha);
    assert!((edge.coverage - 0.5).abs() < 1e-9, "{}", edge.coverage);
    assert!(edge.q_ratio > 1.0 && edge.q_ratio < inside.q_ratio);

    // Outside in span, above the slipstream, or upstream of the disc.
    assert_eq!(
        slip.strip_flow(&a, 0.0, 0.3 + r + 0.05, 0.0, 0.02, alpha),
        StripFlow::NONE
    );
    assert_eq!(
        slip.strip_flow(&a, 0.0, 0.3, r + 0.01, 0.02, alpha),
        StripFlow::NONE
    );
    assert_eq!(
        slip.strip_flow(&a, -0.5, 0.3, 0.0, 0.02, alpha),
        StripFlow::NONE
    );
    // No thrust, no slipstream.
    let idle = Slipstream {
        discs: vec![PropDisc::new([-0.2, 0.3, 0.0], 0.127, 0.0).unwrap()],
    };
    assert_eq!(
        idle.strip_flow(&a, 0.0, 0.3, 0.0, 0.02, alpha),
        StripFlow::NONE
    );
    // Vertical offset shortens the covered chord of the circle but keeps it positive.
    let off = slip.strip_flow(&a, 0.0, 0.3, 0.5 * r, 0.02, alpha);
    assert!((off.coverage - 1.0).abs() < 1e-12);
}

#[test]
fn bad_discs_are_rejected() {
    assert!(PropDisc::new([0.0; 3], 0.0, 1.0).is_err());
    assert!(PropDisc::new([0.0; 3], 0.1, -1.0).is_err());
    assert!(PropDisc::new([f64::NAN, 0.0, 0.0], 0.1, 1.0).is_err());
}

#[test]
fn disc_built_from_the_powertrain_uses_its_thrust_and_diameter() {
    let mut motor =
        MotorModel::new(1000.0 * std::f64::consts::TAU / 60.0, 0.029, 2.0, 14.8).unwrap();
    motor.thermal = None;
    let battery = BatteryModel::new(5.0 * 3600.0, 4, 0.02).unwrap();
    let chain = Powertrain {
        motor,
        esc: EscModel::from_nominal_efficiency(0.96, 14.8, 60.0).unwrap(),
        battery,
        prop: PropModel::from_bemt(BemtGeometry::generic(0.254, 0.127, 2, 0.15).unwrap()),
        n_motors: 2,
    };
    let fl = Flight::new(12.0, 1.225, 293.15);
    let dy = Dynamics {
        battery: BatteryState::new(1.0, 298.15),
        t_winding_k: 293.15,
    };
    let op = chain.solve_duty(&fl, &dy, 0.8).unwrap();
    let disc = PropDisc::from_powertrain(&chain, &op, [-0.1, 0.4, 0.0]).unwrap();
    assert!((disc.radius_m - 0.5 * chain.prop.diameter_m()).abs() < 1e-15);
    assert!(
        (disc.radius_m / 0.127 - 1.0).abs() < 0.01,
        "radius {}",
        disc.radius_m
    );
    assert!((disc.thrust_n * 2.0 - op.thrust_n).abs() < 1e-9);
    assert!(disc.thrust_n > 0.0);
    // Consistency with the propeller's own advance: induced velocity is real and finite.
    let a = AirState::from_rho_t(12.0, 1.225, 293.15).unwrap();
    assert!(disc.induced_velocity(&a).is_finite() && disc.induced_velocity(&a) > 0.0);
}
