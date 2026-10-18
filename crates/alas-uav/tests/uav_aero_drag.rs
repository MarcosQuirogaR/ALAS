// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Failed assertions identify broken fixtures in this test target.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Low-Re drag build-up verification.
//!
//! References (read on the PDF pages 2026-10-12): Raymer, Aircraft Design: A
//! Conceptual Approach, 6th ed., Eq. 12.26 (laminar Cf = 1.328/sqrt(Re)),
//! Eq. 12.27 (turbulent Cf), Eq. 12.30 (wing form factor), Table 12.6 (misc.
//! drag coefficients), p. 425 (interference Q), p. 430 (gear x1.2), Table
//! 12.8 (leakage 5-10 % for propeller aircraft).  The Prandtl-Schlichting
//! transition constant is derived from momentum-thickness matching.

use alas_uav::aero::drag_lowre::{
    build_up, cf_laminar, cf_transitional, cf_turbulent, form_factor_body, form_factor_surface,
    table_12_6, Component, Junction, Shape, Transition,
};
use alas_uav::aero::AirState;

fn air(v: f64) -> AirState {
    AirState::from_isa(0.0, v).unwrap()
}

#[test]
fn laminar_and_turbulent_cf_match_the_raymer_equations() {
    // Eq. 12.26 and 12.27 at Re = 1e6, M = 0: hand values.
    assert!((cf_laminar(1.0e6) - 1.328e-3).abs() < 1e-12);
    let hand = 0.455 / 6.0_f64.powf(2.58);
    assert!((cf_turbulent(1.0e6, 0.0) - hand).abs() < 1e-12);
    // Torenbeek Eq. 4.6 is the incompressible form; Mach 0.1 changes it by < 0.1 %.
    let ratio = cf_turbulent(1.0e6, 0.1) / cf_turbulent(1.0e6, 0.0);
    assert!((ratio - 1.0).abs() < 1.0e-3 && ratio < 1.0);
}

#[test]
fn transitional_cf_is_continuous_and_between_the_laminar_and_turbulent_limits() {
    let tr = Transition::AtReynolds(5.0e5);
    // Continuity at the transition Reynolds number.
    let below = cf_transitional(5.0e5 * (1.0 - 1e-9), 0.0, tr);
    let above = cf_transitional(5.0e5 * (1.0 + 1e-9), 0.0, tr);
    assert!((below - above).abs() < 1e-8, "{below} vs {above}");
    assert!((below - cf_laminar(5.0e5)).abs() < 1e-8);
    // Between the limits and approaching the turbulent value as Re grows.
    for re in [6.0e5, 1.0e6, 3.0e6, 1.0e7] {
        let c = cf_transitional(re, 0.0, tr);
        assert!(c < cf_turbulent(re, 0.0) && c > cf_laminar(re));
    }
    let far = cf_transitional(1.0e8, 0.0, tr) / cf_turbulent(1.0e8, 0.0);
    assert!(far > 0.99);
    // Fully turbulent option is the turbulent law.
    assert_eq!(
        cf_transitional(2.0e6, 0.0, Transition::Turbulent),
        cf_turbulent(2.0e6, 0.0)
    );
}

#[test]
fn transition_constant_is_1611_for_the_log_law_and_near_schlichting_1700() {
    // A = Re_tr (Cf_turb - Cf_lam) at Re_tr = 5e5.  Schlichting's 1700 belongs to
    // the 0.074 Re^-0.2 law; the 0.455/log10 law gives about 1611 (5 % lower).
    let a = 5.0e5 * (cf_turbulent(5.0e5, 0.0) - cf_laminar(5.0e5));
    assert!((a - 1611.0).abs() < 5.0, "A = {a}");
    assert!((a / 1700.0 - 1.0).abs() < 0.06);
    // At Re = 2e6 the laminar run lowers Cf by A/Re = 0.0008 (about 19 %).
    let drop =
        cf_turbulent(2.0e6, 0.0) - cf_transitional(2.0e6, 0.0, Transition::AtReynolds(5.0e5));
    assert!((drop - a / 2.0e6).abs() < 1e-12);
    // Plan check: at Re 5e5 the turbulent value is about 0.0051, laminar 0.0019.
    assert!((cf_turbulent(5.0e5, 0.0) - 0.0051).abs() < 2e-4);
    assert!((cf_laminar(5.0e5) - 0.0019).abs() < 1e-4);
}

#[test]
fn surface_form_factor_agrees_with_raymer_eq_12_30_without_the_mach_term() {
    for tc in [0.08_f64, 0.10, 0.12, 0.15] {
        let raymer = 1.0 + 0.6 / 0.3 * tc + 100.0 * tc.powi(4);
        let hoerner = form_factor_surface(tc);
        assert!(
            (hoerner / raymer - 1.0).abs() < 0.02,
            "t/c {tc}: {hoerner} vs {raymer}"
        );
        assert!(hoerner > 1.0, "the transport Mach term must not appear");
    }
    // Slender bodies tend to 1, fat ones grow.
    assert!(form_factor_body(0.02) < 1.01);
    assert!(form_factor_body(0.2) > form_factor_body(0.1));
}

#[test]
fn component_line_equals_the_hand_calculation() {
    // Fuselage L = 0.9 m, d = 0.1 m, S_wet = 0.25 m2 at 15 m/s sea level, S_ref = 0.4 m2.
    let a = air(15.0);
    let comp = Component::new(
        "fuselage",
        Shape::Body {
            length_m: 0.9,
            diameter_m: 0.1,
            wetted_m2: 0.25,
        },
    );
    let table = build_up(&[comp], &a, 0.4, 0.0).unwrap();
    let re = a.reynolds(0.9);
    let cf = cf_transitional(re, a.mach(), Transition::AtReynolds(5.0e5));
    let ff = form_factor_body(0.1 / 0.9);
    let hand = cf * ff * 0.25 / 0.4;
    assert!((table.lines[0].cd0 - hand).abs() < 1e-15);
    assert!((table.lines[0].re - re).abs() < 1e-6 * re);
    assert_eq!(table.excrescence.cd0, 0.0);
    assert!((table.cd0_total - hand).abs() < 1e-15);
}

#[test]
fn interference_gear_cylinder_and_excrescence_follow_the_cited_values() {
    let a = air(18.0);
    let tail = Component::new(
        "v-tail",
        Shape::Surface {
            chord_m: 0.12,
            wetted_m2: 0.06,
            t_over_c: 0.09,
        },
    )
    .with_junction(Junction::VTail);
    assert!((Junction::VTail.q() - 1.03).abs() < 1e-12);
    assert!((Junction::ConventionalTail.q() - 1.045).abs() < 1e-12);
    assert!((Junction::NacelleOnBody.q() - 1.5).abs() < 1e-12);
    let wheel = Component::new(
        "main wheel",
        Shape::Item {
            cd: table_12_6::WHEEL_REGULAR,
            frontal_m2: 0.004,
        },
    )
    .with_count(2)
    .as_gear();
    let wire = Component::new(
        "wire",
        Shape::Cylinder {
            diameter_m: 0.002,
            length_m: 0.3,
        },
    );
    let s_ref = 0.5;
    let t = build_up(&[tail, wheel, wire], &a, s_ref, 0.05).unwrap();
    // Wheel pair: 0.25 * 0.004 * 2 * 1.2 (gear interference).
    assert!((t.lines[1].cda_m2 - 0.25 * 0.004 * 2.0 * 1.2).abs() < 1e-15);
    assert!((t.lines[1].q - 1.2).abs() < 1e-12);
    // The wire is subcritical: Cd 1.17 on d*l.
    assert!((t.lines[2].cda_m2 - 1.17 * 0.002 * 0.3).abs() < 1e-15);
    // Explicit 5 % allowance line on the component sum.
    let sum_cda: f64 = t.lines.iter().map(|l| l.cda_m2).sum();
    assert!((t.excrescence.cda_m2 - 0.05 * sum_cda).abs() < 1e-15);
    assert!((t.cd0_total - 1.05 * sum_cda / s_ref).abs() < 1e-12);
    // Turbulent-vs-transitional ordering for the tail.
    let turb = Component::new(
        "v-tail",
        Shape::Surface {
            chord_m: 0.12,
            wetted_m2: 0.06,
            t_over_c: 0.09,
        },
    )
    .with_transition(Transition::Turbulent);
    let tt = build_up(&[turb], &a, s_ref, 0.0).unwrap();
    assert!(tt.lines[0].cd0 > t.lines[0].cd0 / Junction::VTail.q());
}

#[test]
fn bad_geometry_and_references_are_rejected() {
    let a = air(15.0);
    let bad = Component::new(
        "x",
        Shape::Body {
            length_m: -1.0,
            diameter_m: 0.1,
            wetted_m2: 0.1,
        },
    );
    assert!(build_up(&[bad], &a, 0.4, 0.05).is_err());
    assert!(build_up(&[], &a, 0.0, 0.05).is_err());
    assert!(build_up(&[], &a, 0.4, -0.1).is_err());
    assert!(AirState::from_isa(0.0, 0.0).is_err());
}
