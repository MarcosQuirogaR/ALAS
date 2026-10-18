// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A failed unwrap in this test target is the assertion reporting malformed
// evidence, not a panic escaping from library code.
// The DRAG| lines are the reported evidence of this target.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

//! Trim drag and propeller-wash drag.
//!
//! Numerical verification against the closed forms written in the module
//! docs, plus the magnitudes on the three blind-test aircraft (reported, not
//! asserted as validation).  Cited sources: Roskam Part VI p. 104 Eq. 4.83 and
//! p. 69 (read on the page), Raymer 6th ed. Sec. 12.6 text (trim drag) and
//! Table 12.8 p. 431; the wash-velocity development formula is the one the
//! slipstream module flags as not read on a page.

#[path = "support/blind_aircraft.rs"]
mod blind;

use alas_uav::coupled::{AeroSettings, MotorStation, SlipstreamDrag, TrimDrag, Vehicle};
use alas_uav::perf::Environment;

#[test]
fn trim_drag_closed_forms() {
    let t = TrimDrag {
        cm0: -0.06,
        a: 0.13,
        d_w: 0.12,
        arm_ac: 2.8,
        k_tail: 1.0 / (std::f64::consts::PI * 4.0 * 0.5),
    };
    // The tail lift that zeroes the moment: CL_h a arm = Cm0 + CL d_w.
    for cl in [0.2, 0.6, 1.0] {
        let cl_h = t.cl_tail(cl);
        assert!((cl_h * t.a * t.arm_ac - (t.cm0 + cl * t.d_w)).abs() < 1e-12);
    }
    // At the CL where the tail carries no lift the increment vanishes.
    let cl0 = -t.cm0 / t.d_w;
    assert!(t.delta_cdi(cl0, 0.03).abs() < 1e-12);
    // No credit for a tail upload: the polar never goes below the wing-only one.
    assert!(t.delta_cdi(1.2, 0.03) >= 0.0);
    // A download (negative tail lift) costs drag; hand value at CL = 0.2.
    let (cl, k_w) = (0.2, 0.03);
    let cl_h = t.cl_tail(cl);
    assert!(cl_h < 0.0);
    let cl_w = cl - t.a * cl_h;
    let hand = k_w * (cl_w * cl_w - cl * cl) + t.a * t.k_tail * cl_h * cl_h;
    assert!((t.delta_cdi(cl, k_w) - hand).abs() < 1e-15);
    assert!(hand > 0.0);
    // Roskam Eq. 4.83 (tail term only, eta_h inside a) is the second addend.
    let roskam = cl_h * cl_h / (std::f64::consts::PI * 4.0 * 0.5) * t.a;
    assert!((roskam - t.a * t.k_tail * cl_h * cl_h).abs() < 1e-18);
}

#[test]
fn slipstream_drag_follows_momentum_theory() {
    let sd = SlipstreamDrag {
        radius_m: 0.14,
        motors: vec![
            MotorStation {
                y_m: -0.3,
                s_to_wing_m: 0.2,
            },
            MotorStation {
                y_m: 0.3,
                s_to_wing_m: 0.2,
            },
        ],
        wing_cd0: 0.012,
        chord_m: 0.27,
        other_cda_m2: 0.002,
        t_k: 288.15,
    };
    let (rho, v, thrust) = (1.17, 15.0, 10.0);
    let a = std::f64::consts::PI * 0.14_f64.powi(2);
    let per = thrust / 2.0;
    let vi = -0.5 * v + (0.25 * v * v + per / (2.0 * rho * a)).sqrt();
    let s: f64 = 0.2;
    let dv = vi * (1.0 + s / (0.14_f64.powi(2) + s * s).sqrt());
    let rs = 0.14 * ((v + vi) / (v + dv)).sqrt();
    let cda = 0.012 * 0.27 * 2.0 * rs + 0.001;
    let hand = 2.0 * 0.5 * rho * ((v + dv).powi(2) - v * v) * cda;
    let got = sd.drag_n(thrust, v, rho);
    println!(
        "DRAG|slipstream hand {hand:.4} N model {got:.4} N ({:.1} % of thrust)",
        100.0 * got / thrust
    );
    assert!((got - hand).abs() < 1e-9 * hand);
    assert!(got > 0.0 && got < 0.1 * thrust);
    assert_eq!(sd.drag_n(0.0, v, rho), 0.0);
    // At high speed the increment tends to a finite value (about 0.9 T_i CdA / A
    // per disc), not to zero: it stays within a factor 5 of the 15 m/s value.
    let far = sd.drag_n(thrust, 200.0, rho);
    assert!(far > 0.2 * got && far < 5.0 * got, "{far} vs {got}");
}

fn drag_at(v: &Vehicle<'_>, env: &Environment, cl: f64) -> (f64, f64) {
    let m = v.models(env).unwrap();
    m.airframe.polar.cd_parts(cl)
}

#[test]
fn trim_and_slipstream_magnitudes_on_the_blind_aircraft() {
    for t in [blind::trencalos(), blind::xtra2upv(), blind::uvigo()] {
        let env = Environment::new(t.rho, 0.0).unwrap();
        let mut with = blind::vehicle(&t, Some(30.0));
        with.aero = AeroSettings {
            strips: 12,
            ..AeroSettings::default()
        };
        let mut without = blind::vehicle(&t, Some(30.0));
        without.aero = AeroSettings {
            strips: 12,
            trim: None,
            ..AeroSettings::default()
        };
        let mut line = String::new();
        for cl in [0.3, 0.6, 0.9] {
            let (p1, i1) = drag_at(&with, &env, cl);
            let (p0, i0) = drag_at(&without, &env, cl);
            assert!((p1 - p0).abs() < 1e-12, "trim must not touch profile drag");
            line += &format!(" CL {cl}: dCDi {:+.5}", i1 - i0);
            // Magnitude guard: a trim increment is a small fraction of the polar.
            assert!((0.0..0.02).contains(&(i1 - i0)), "{}: {}", t.name, i1 - i0);
        }
        let m = with.models(&env).unwrap();
        let tr = m.airframe.polar.cd_parts(0.6);
        println!("DRAG|{}|trim{line}|CD at CL 0.6 {:.4}", t.name, tr.0 + tr.1);
        assert!(m.notes.iter().any(|n| n.starts_with("trim drag")));
        // Installed thrust is below the propeller thrust at 15 m/s, by less than 10 %.
        use alas_uav::perf::{BatteryState, Powertrain};
        let pt = m
            .powertrain
            .evaluate(1.0, 15.0, t.rho, &BatteryState::default());
        let raw = m.powertrain.operating_point(1.0, 15.0, 1.0).unwrap();
        let inst = pt.unwrap().thrust_n;
        println!(
            "DRAG|{}|slipstream thrust 15 m/s: propeller {:.2} N installed {:.2} N",
            t.name, raw.thrust_n, inst
        );
        assert!(inst < raw.thrust_n && inst > 0.9 * raw.thrust_n);
    }
}
