// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A failed unwrap in this test target is the assertion reporting malformed
// evidence, not a panic escaping from library code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Geometry, neutral point, tail sizing and trim (UAV-PLAN 3.4).
//!
//! Published verification cases, each re-read on the page of the PDF:
//!
//! * Raymer, Aircraft Design: A Conceptual Approach (6th ed.): Eq. 12.6 and
//!   the DR-1 wing (A = 6, eta = 0.95, unswept: 4.37, p. 880); Table 6.4 and
//!   the tail rules of p. 160; the DR-1 stick-fixed neutral point (pp. 888-889).
//! * Sadraey: Examples 6.1 and 6.2 (pp. 301, 331-335) for the tail arm, tail
//!   area, wing/fuselage Cm0, downwash and trim tail lift.

mod airframe_support;

use alas_uav::airframe::geometry::{
    planform_from_area, AirframeGeometry, Panel, Planform, TailRole, TailShape, TailSurfaceGeom,
};
use alas_uav::airframe::liftcurve::{
    control_effectiveness, downwash_angle, downwash_gradient, fuselage_lift_factor,
    lift_curve_slope,
};
use alas_uav::airframe::stability::{
    FuselageContribution, LongitudinalOptions, NeutralPointMethod, SimpleLiftingSurface,
};
use alas_uav::airframe::tail::{
    area_from_volume, optimum_tail_arm, size_from_volume, v_tail_dihedral_rad, AircraftClass,
    VTailMixer, VolumeCoefficients,
};
use alas_uav::airframe::trim::{
    size_horizontal_tail, trim_flying_wing, trim_tailed, SizingDriver, SizingRequirements,
    TrimCondition, TrimSetup,
};
use alas_uav::airframe::AirframeError;
use alas_uav::spec::master::EmpennageDriving;
use alas_uav::spec::topology::{TailKind, WingKind};

use airframe_support::{reference_spec, spec_with};

const FT: f64 = 0.3048;
const IN: f64 = 0.0254;

fn rect(span: f64, chord: f64, x_le: f64, z: f64, vertical: bool) -> Planform {
    Planform {
        panels: vec![Panel {
            span_m: if vertical { span } else { 0.5 * span },
            chord_root_m: chord,
            chord_tip_m: chord,
            root_le_m: [x_le, 0.0, z],
            sweep_le_rad: 0.0,
            dihedral_rad: 0.0,
        }],
        mirror: !vertical,
        vertical,
    }
}

fn tail_of(pf: Planform, role: TailRole, gamma: f64) -> TailSurfaceGeom {
    TailSurfaceGeom {
        role,
        planform: pf,
        v_dihedral_rad: gamma,
        incidence_rad: 0.0,
        count: 1,
    }
}

#[test]
fn trapezoid_mac_matches_the_textbook_formula() {
    // c_r = 1, taper 0.5, half span 1 (one panel, mirrored): the textbook
    // c = (2/3) c_r (1 + l + l^2)/(1 + l) = 0.7778 and S = 2 * 0.75 = 1.5.
    let pf = Planform {
        panels: vec![Panel {
            span_m: 1.0,
            chord_root_m: 1.0,
            chord_tip_m: 0.5,
            root_le_m: [0.0; 3],
            sweep_le_rad: 0.0,
            dihedral_rad: 0.0,
        }],
        mirror: true,
        vertical: false,
    };
    assert!((pf.projected_area_m2() - 1.5).abs() < 1e-12);
    assert!((pf.span_m() - 2.0).abs() < 1e-12);
    assert!((pf.mac_m() - 2.0 / 3.0 * 1.75 / 1.5).abs() < 1e-12);
    // Spanwise MAC station (2/S) integral y c dy = (b/6)(1 + 2 l)/(1 + l) = 0.4444.
    assert!((pf.s_mac_m() - (1.0 / 3.0) * (1.0 + 2.0 * 0.5) / (1.0 + 0.5)).abs() < 1e-12);
    // Swept: x_le of the MAC follows the same weighting.
    let swept = Planform {
        panels: vec![Panel {
            sweep_le_rad: 20.0_f64.to_radians(),
            ..pf.panels[0]
        }],
        ..pf
    };
    let y = swept.s_mac_m();
    assert!((swept.x_le_mac_m() - y * 20.0_f64.to_radians().tan()).abs() < 1e-12);
}

#[test]
fn raymer_eq_12_6_reproduces_the_dr1_wing_slope() {
    // Page 880 (handwritten): A = 6, eta = 0.95, tan(L) = 0, beta = 1 gives
    // 2 pi 6 / (2 + sqrt(4 + 6^2/0.95^2)) = 4.37 per radian.
    let a = lift_curve_slope(6.0, 0.0, 0.0, 0.95).unwrap();
    assert!((a - 4.37).abs() < 0.005, "{a}");
    // Times the printed exposed-area/fuselage product 0.837 x 1.33 -> 4.85.
    assert!((a * 0.837 * 1.33 - 4.85).abs() < 0.03);
    // F = 1.07 (1 + d/b)^2 (Eq. 12.9).
    assert!((fuselage_lift_factor(0.1) - 1.07 * 1.21).abs() < 1e-12);
    // Limits: very high aspect ratio tends to the section slope eta 2 pi.
    let big = lift_curve_slope(1.0e4, 0.0, 0.0, 1.0).unwrap();
    assert!((big - 2.0 * std::f64::consts::PI).abs() < 1e-2);
    assert!(lift_curve_slope(6.0, 1.0, 0.0, 0.95).is_err());
}

#[test]
fn sadraey_example_6_2_downwash_and_wing_moment() {
    // Eq. 6.55-6.56: eps0 = 2 CL/(pi AR) = 0.0097 rad (CL = 0.428, AR = 28),
    // d eps/d alpha = 2 CLa/(pi AR) = 0.132 (CLa = 5.8).
    assert!((downwash_angle(0.428, 28.0) - 0.0097).abs() < 2e-4);
    assert!((downwash_gradient(5.8, 28.0) - 0.132).abs() < 1e-3);
    // Eq. 6.26: Cm0_wf = Cm_af AR cos^2(L)/(AR + 2 cos L) + 0.01 twist = -0.023.
    let l = 8.0_f64.to_radians();
    let cm0 = -0.013 * 28.0 * l.cos().powi(2) / (28.0 + 2.0 * l.cos()) + 0.01 * (-1.1);
    assert!((cm0 + 0.023).abs() < 5e-4, "{cm0}");
}

#[test]
fn raymer_table_6_4_and_the_topology_rules() {
    let h = VolumeCoefficients::raymer(AircraftClass::Homebuilt);
    assert_eq!((h.horizontal, h.vertical), (0.50, 0.04));
    let s = VolumeCoefficients::raymer(AircraftClass::Sailplane);
    assert_eq!((s.horizontal, s.vertical), (0.50, 0.02));
    let jt = VolumeCoefficients::raymer(AircraftClass::JetTransport);
    assert_eq!((jt.horizontal, jt.vertical), (1.00, 0.09));
    // T-tail: both coefficients reduced by 5 % (p. 160); all-moving 10-15 %.
    let t = h.for_topology(TailKind::T);
    assert!((t.horizontal - 0.475).abs() < 1e-12 && (t.vertical - 0.038).abs() < 1e-12);
    assert_eq!(h.for_topology(TailKind::Conventional), h);
    assert!((h.all_moving().horizontal - 0.4375).abs() < 1e-12);
}

#[test]
fn sadraey_examples_6_1_and_6_2_tail_arm_and_area() {
    // Example 6.1: S = 10, MAC = 1, V_H = 0.6, D_f = 1.17, Kc = 1.4:
    // l_opt = 3.577 m, S_h = 1.677 m^2.
    let l = optimum_tail_arm(1.4, 1.0, 10.0, 0.6, 1.17);
    assert!((l - 3.577).abs() < 1e-3, "{l}");
    let s_h = area_from_volume(0.6, 1.0, 10.0, l).unwrap();
    assert!((s_h - 1.677).abs() < 1e-3, "{s_h}");
    // Example 6.2: S = 18, MAC = 0.8, D_f = 1.1, Kc = 1.2: l = 3.795, S_h = 2.277.
    let l2 = optimum_tail_arm(1.2, 0.8, 18.0, 0.6, 1.1);
    assert!((l2 - 3.795).abs() < 1e-3, "{l2}");
    let s_h2 = area_from_volume(0.6, 0.8, 18.0, l2).unwrap();
    assert!((s_h2 - 2.277).abs() < 1e-3, "{s_h2}");
    assert!(area_from_volume(0.6, 0.8, 18.0, 0.0).is_err());
}

#[test]
fn v_tail_sizing_follows_raymer_and_reduces_to_the_conventional_limits() {
    let coeffs = VolumeCoefficients {
        horizontal: 0.5,
        vertical: 0.04,
    };
    let conv = size_from_volume(
        TailKind::Conventional,
        coeffs,
        0.7,
        0.28,
        2.4,
        0.9,
        0.9,
        None,
    )
    .unwrap();
    // Raymer p. 160: V surfaces carry the same total area as the separate
    // surfaces, with dihedral atan(sqrt(S_v/S_h)).
    let v = size_from_volume(
        TailKind::V { dihedral_deg: 0.0 },
        coeffs,
        0.7,
        0.28,
        2.4,
        0.9,
        0.9,
        None,
    )
    .unwrap();
    let g = v.v_dihedral_rad.unwrap();
    assert!((g - v_tail_dihedral_rad(conv.horizontal_eff_m2, conv.vertical_eff_m2)).abs() < 1e-12);
    let total = v.v_pair_area_m2.unwrap();
    assert!((total - (conv.horizontal_eff_m2 + conv.vertical_eff_m2)).abs() < 1e-12);
    assert!((v.horizontal_eff_m2 - conv.horizontal_eff_m2).abs() < 1e-12);
    assert!((v.vertical_eff_m2 - conv.vertical_eff_m2).abs() < 1e-12);
    // Equal requirements give 45 degrees.
    assert!((v_tail_dihedral_rad(1.0, 1.0) - std::f64::consts::FRAC_PI_4).abs() < 1e-15);
    // A fixed 35 deg dihedral enlarges the pair until both are met.
    let fixed = size_from_volume(
        TailKind::V { dihedral_deg: 35.0 },
        coeffs,
        0.7,
        0.28,
        2.4,
        0.9,
        0.9,
        Some(35.0_f64.to_radians()),
    )
    .unwrap();
    assert!(fixed.horizontal_eff_m2 >= conv.horizontal_eff_m2 - 1e-12);
    assert!(fixed.vertical_eff_m2 >= conv.vertical_eff_m2 - 1e-12);
    // A flying wing has no tail.
    let fw = size_from_volume(TailKind::None, coeffs, 0.7, 0.28, 2.4, 0.9, 0.9, None).unwrap();
    assert_eq!(fw.horizontal_eff_m2, 0.0);
}

/// Raymer DR-1 geometry in SI: S = 118 ft^2, MAC = 56 in, wing AC at 62 in,
/// tail area 25.5 ft^2 with its AC at 206 in (pp. 888-889).
fn dr1() -> AirframeGeometry {
    let s = 118.0 * FT * FT;
    let c = 56.0 * IN;
    let wing = rect(s / c, c, 62.0 * IN - 0.25 * c, 0.0, false);
    let tail_c = 0.5;
    let s_t = 25.5 * FT * FT;
    let tail = rect(s_t / tail_c, tail_c, 206.0 * IN - 0.25 * tail_c, 0.0, false);
    AirframeGeometry {
        wing,
        horizontal: Some(tail_of(tail, TailRole::Horizontal, 0.0)),
        vertical: None,
        tail_kind: TailKind::Conventional,
    }
}

#[test]
fn raymer_dr1_neutral_point_formula_on_the_printed_inputs() {
    let g = dr1();
    let o = LongitudinalOptions {
        wing_cl_alpha: Some(4.85),
        tail_cl_alpha: Some(3.77),
        downwash_gradient: Some(0.38),
        tail_dynamic_pressure_ratio: 0.9,
        // Printed fuselage term: Cm_a,fus = +0.12 per rad (destabilising).
        fuselage: FuselageContribution {
            cm_alpha_per_rad: 0.12,
        },
        ..LongitudinalOptions::default()
    };
    let np = SimpleLiftingSurface.neutral_point(&g, &o).unwrap();
    // Hand evaluation of Eq. 16.9 with the printed numbers, in MAC units:
    // x_acw = 62/56, x_ach = 206/56, tail term 0.9 (25.5/118) 3.77 0.62.
    let tail = 0.9 * (25.5 / 118.0) * 3.77 * 0.62;
    let x = (4.85 * (62.0 / 56.0) - 0.12 + tail * (206.0 / 56.0)) / (4.85 + tail);
    assert!(
        (np.x_np_m / (56.0 * IN) - x).abs() < 2e-3,
        "{} vs {x}",
        np.x_np_m / (56.0 * IN)
    );
    assert!((np.tail_term - tail).abs() < 1e-9);
    // The printed result is X_np = 1.27 (71.2 in): its own arithmetic gives
    // 1.305 (73.1 in); the library follows the equation, 2.7 % above the print.
    assert!((x - 1.305).abs() < 1e-3);
    assert!(((x - 1.27) / 1.27).abs() < 0.03);
    // Static margin and Cm_alpha at the printed most-aft CG 64.5 in:
    // SM = (x_np - x_cg)/c and Cm_a = -CLa_total SM (Eq. 16.10-16.11; the
    // page prints SM 0.12 and Cm_a -0.58 from its 1.27).
    let sm = np.static_margin(64.5 * IN);
    assert!((sm - (x - 64.5 / 56.0)).abs() < 2e-3);
    assert!((np.cm_alpha_per_rad(64.5 * IN) + np.cl_alpha_total() * sm).abs() < 1e-12);
    // x-aft sign convention: moving the CG aft by dx lowers SM by dx/c exactly.
    assert!((np.static_margin(64.5 * IN + 0.1) - (sm - 0.1 / np.mac_m)).abs() < 1e-12);
    assert_eq!(np.method, "simple-lifting-surface");
}

#[test]
fn sadraey_example_6_2_stability_derivative() {
    // Printed inputs: S = 18, MAC 0.8, wing/fuselage AC at 23 % MAC, CLa_wf =
    // 5.7, tail S_h = 2.277 at l = 3.795 behind the AC, CLa_h = 6.1,
    // eta = 0.98, d eps/d alpha = 0.132, CG at 14.2 % MAC.
    let c = 0.8;
    let wing = rect(18.0 / c, c, -0.016, 0.0, false); // AC at 0.25 c - 0.016 = 0.184 = 0.23 c
    let tail_c = 0.4;
    let ac_t = 0.184 + 3.795;
    let tail = rect(2.277 / tail_c, tail_c, ac_t - 0.25 * tail_c, 0.0, false);
    let g = AirframeGeometry {
        wing,
        horizontal: Some(tail_of(tail, TailRole::Horizontal, 0.0)),
        vertical: None,
        tail_kind: TailKind::Conventional,
    };
    let o = LongitudinalOptions {
        wing_cl_alpha: Some(5.7),
        tail_cl_alpha: Some(6.1),
        downwash_gradient: Some(0.132),
        tail_dynamic_pressure_ratio: 0.98,
        ..LongitudinalOptions::default()
    };
    let np = SimpleLiftingSurface.neutral_point(&g, &o).unwrap();
    let term = 0.98 * (2.277 / 18.0) * 6.1 * 0.868;
    let h_np = (5.7 * 0.23 + term * (0.23 + 3.795 / 0.8)) / (5.7 + term);
    assert!((np.x_np_m / c - h_np).abs() < 1e-9);
    // Datum = the book's MAC leading edge (wing AC at h0 = 0.23 MAC, tail AC 3.795 m aft);
    // CG at h = 0.142 MAC.
    let x_cg = 0.142 * c;
    let cm_a = np.cm_alpha_per_rad(x_cg);
    // Cm_alpha = -CLa_total (h_np - h).
    let consistent = -(5.7 + term) * (h_np - 0.142);
    assert!((cm_a - consistent).abs() < 1e-9);
    // Printed Cm_alpha = -3.7 per rad (its CG entry mixes 0.114 m with 0.142 MAC):
    // the consistent evaluation of the same equation is within 6 %.
    assert!(
        ((cm_a + 3.7) / 3.7).abs() < 0.06,
        "Cm_alpha {cm_a} vs printed -3.7"
    );
}

#[test]
fn v_tail_at_90_degrees_is_a_pure_fin_and_at_zero_a_pure_stabiliser() {
    let tail_pf = planform_from_area(0.2, 4.0, 0.7, 0.0, 0.0, [1.1, 0.0, 0.05], false).unwrap();
    let mk = |gamma: f64| TailSurfaceGeom {
        role: TailRole::VPair,
        planform: tail_pf.clone(),
        v_dihedral_rad: gamma,
        incidence_rad: 0.0,
        count: 1,
    };
    let s = tail_pf.true_area_m2();
    assert!((s - 0.2).abs() < 1e-12);
    let flat = mk(0.0);
    assert!((flat.effective_horizontal_area_m2() - s).abs() < 1e-12);
    assert!(flat.effective_vertical_area_m2().abs() < 1e-15);
    let fin = mk(std::f64::consts::FRAC_PI_2);
    assert!(fin.effective_horizontal_area_m2().abs() < 1e-15);
    assert!((fin.effective_vertical_area_m2() - s).abs() < 1e-12);
    let g45 = mk(std::f64::consts::FRAC_PI_4);
    assert!(
        (g45.effective_horizontal_area_m2() + g45.effective_vertical_area_m2() - s).abs() < 1e-12
    );
    // The neutral point of the pair at Gamma = 0 equals that of a conventional tail.
    let wing = rect(2.0, 0.28, 0.25, 0.0, false);
    let geo = |t: TailSurfaceGeom| AirframeGeometry {
        wing: wing.clone(),
        horizontal: Some(t),
        vertical: None,
        tail_kind: TailKind::Conventional,
    };
    let o = LongitudinalOptions::default();
    let conv = SimpleLiftingSurface
        .neutral_point(
            &geo(tail_of(tail_pf.clone(), TailRole::Horizontal, 0.0)),
            &o,
        )
        .unwrap();
    let v0 = SimpleLiftingSurface
        .neutral_point(&geo(mk(0.0)), &o)
        .unwrap();
    assert!((conv.x_np_m - v0.x_np_m).abs() < 1e-12);
    // At 90 deg the tail adds no pitch stiffness: NP is the wing alone.
    let v90 = SimpleLiftingSurface
        .neutral_point(&geo(mk(std::f64::consts::FRAC_PI_2)), &o)
        .unwrap();
    assert!(v90.tail_term.abs() < 1e-12);
    assert!((v90.x_np_m - v90.x_ac_wing_m).abs() < 1e-12);
}

#[test]
fn v_tail_mixer_algebra_and_limits() {
    let (s, a, tau, q) = (0.2, 4.2, 0.5, 1.0);
    let _ = q;
    for gamma in [0.0_f64, 0.35, 0.61, std::f64::consts::FRAC_PI_2, -0.61] {
        let m = VTailMixer { gamma_rad: gamma };
        // Round trip of the commands.
        let (dl, dr) = m.surface_deflections(0.1, -0.04);
        let (p, y) = m.commands(dl, dr);
        assert!((p - 0.1).abs() < 1e-15 && (y + 0.04).abs() < 1e-15);
        // Pure pitch has no side force; pure yaw has no vertical force.
        let (fz, fy) = m.forces_per_q(s, a, tau, 0.1, 0.1);
        assert!(fy.abs() < 1e-15);
        assert!((fz - s * a * tau * gamma.cos() * 0.1).abs() < 1e-12);
        let (dl, dr) = m.surface_deflections(0.0, 0.1);
        let (fz, fy) = m.forces_per_q(s, a, tau, dl, dr);
        assert!(fz.abs() < 1e-15);
        assert!((fy - s * a * tau * gamma.sin() * 0.1).abs() < 1e-12);
        // Force demand round trip where both axes exist.
        if gamma.cos().abs() > 1e-6 && gamma.sin().abs() > 1e-6 {
            let (dl, dr) = m.deflections_for_forces(s, a, tau, 0.03, -0.01).unwrap();
            let (fz, fy) = m.forces_per_q(s, a, tau, dl, dr);
            assert!((fz - 0.03).abs() < 1e-12 && (fy + 0.01).abs() < 1e-12);
        }
    }
    // Gamma = 90 deg equals a conventional rudder of the same area: F_y/q = S CLa tau d.
    let fin = VTailMixer {
        gamma_rad: std::f64::consts::FRAC_PI_2,
    };
    let (dl, dr) = fin.surface_deflections(0.0, 0.2);
    let (_, fy) = fin.forces_per_q(s, a, tau, dl, dr);
    assert!((fy - s * a * tau * 0.2).abs() < 1e-12);
    // Inverted V flips the side-force sign for the same differential command.
    let up = VTailMixer { gamma_rad: 0.6 };
    let down = VTailMixer { gamma_rad: -0.6 };
    let (dl, dr) = up.surface_deflections(0.0, 0.1);
    assert!(
        (up.forces_per_q(s, a, tau, dl, dr).1 + down.forces_per_q(s, a, tau, dl, dr).1).abs()
            < 1e-15
    );
    // Impossible demands are rejected; the worst surface carries |p| + |y|.
    assert!(matches!(
        fin.deflections_for_forces(s, a, tau, 0.1, 0.0),
        Err(AirframeError::NoSolution(_))
    ));
    assert!((VTailMixer::worst_surface_deflection(0.1, -0.05) - 0.15).abs() < 1e-15);
}

#[test]
fn sadraey_example_6_2_trim_tail_lift_and_closure() {
    // Eq. 6.29: CL_h = (Cm0 + CL (h - h0)) / (eta V_H) with Cm0 = -0.023,
    // CL = 0.428, V_H = 0.6.  The printed -0.121 uses h = 0.114 (a length in
    // metres, not 0.142 MAC) and drops eta; reproduced here with those inputs.
    let c = 0.8;
    let wing = rect(18.0 / c, c, 0.0, 0.0, false);
    let x_acw = 0.25 * c;
    let tail_c = 0.4;
    let l = 3.795;
    let tail = rect(
        2.277 / tail_c,
        tail_c,
        x_acw + l - 0.25 * tail_c,
        0.0,
        false,
    );
    let g = AirframeGeometry {
        wing,
        horizontal: Some(tail_of(tail, TailRole::Horizontal, 0.0)),
        vertical: None,
        tail_kind: TailKind::Conventional,
    };
    let mk = |eta: f64| LongitudinalOptions {
        wing_cl_alpha: Some(5.8),
        tail_cl_alpha: Some(6.1),
        downwash_gradient: Some(0.132),
        tail_dynamic_pressure_ratio: eta,
        ..LongitudinalOptions::default()
    };
    let setup = TrimSetup {
        cm0: -0.023,
        tail_incidence_rad: 0.0,
        tau: 0.5,
        max_down_rad: 0.4,
        max_up_rad: 0.4,
    };
    // V_H as printed: S_h l / (c S) = 2.277 * 3.795 / (0.8 * 18) = 0.6001.
    let v_h = 2.277 * l / (c * 18.0);
    assert!((v_h - 0.6).abs() < 2e-3);
    let np1 = SimpleLiftingSurface.neutral_point(&g, &mk(1.0)).unwrap();
    let book = TrimCondition {
        name: "book (h = 0.114 slip, eta dropped)".into(),
        cl: 0.428,
        x_cg_m: x_acw + (0.114 - 0.23) * c,
    };
    let r = trim_tailed(&np1, &g, &mk(1.0), &setup, &book).unwrap();
    assert!((r.cl_tail + 0.121).abs() < 2e-3, "{}", r.cl_tail);
    // Consistent inputs (h = 0.142 MAC, eta = 0.98) give a different, closed-form value.
    let np = SimpleLiftingSurface.neutral_point(&g, &mk(0.98)).unwrap();
    let cons = TrimCondition {
        name: "consistent".into(),
        cl: 0.428,
        x_cg_m: x_acw + (0.142 - 0.23) * c,
    };
    let r2 = trim_tailed(&np, &g, &mk(0.98), &setup, &cons).unwrap();
    let expect = (-0.023 + 0.428 * (0.142 - 0.23)) / (0.98 * v_h);
    assert!((r2.cl_tail - expect).abs() < 2e-3);
    // Closure: the moment about the CG and the tail force law are satisfied.
    let a = 0.98 * 2.277 / 18.0;
    let cl_w = 0.428 - a * r2.cl_tail;
    let cm = -0.023 + cl_w * (cons.x_cg_m - x_acw) / c
        - a * r2.cl_tail * (np.x_ac_tail_m - cons.x_cg_m) / c;
    assert!(cm.abs() < 1e-12, "{cm}");
    let alpha_t = r2.alpha_wing_rad * (1.0 - 0.132);
    assert!((6.1 * (alpha_t + 0.5 * r2.delta_rad) - r2.cl_tail).abs() < 1e-12);
    // Control power Cm_delta = -eta (S_h/S) CLa_h tau (x_ach - x_cg)/c  (Eq. 12.51).
    assert!(r2.cm_delta_per_rad < 0.0);
    assert!(r2.within_stops && r2.authority_used < 1.0);
}

#[test]
fn control_effectiveness_matches_the_figure_and_text() {
    // Example 12.4 step 8: chord ratio 0.2 gives tau = 0.41.
    assert!((control_effectiveness(0.2).unwrap() - 0.41).abs() < 1e-12);
    assert!((control_effectiveness(0.4).unwrap() - 0.60).abs() < 0.02);
    assert!(control_effectiveness(0.8).is_err());
}

fn driven_geometry(kind: TailKind) -> AirframeGeometry {
    let spec = spec_with(
        kind,
        WingKind::Straight,
        EmpennageDriving::Driven {
            horizontal_volume: 0.55,
            vertical_volume: 0.04,
            arm_m: 0.9,
        },
    );
    AirframeGeometry::from_master(&spec.master, &spec.topology, &TailShape::default()).unwrap()
}

#[test]
fn driven_geometry_reproduces_the_volume_coefficients_per_topology() {
    for kind in [
        TailKind::Conventional,
        TailKind::T,
        TailKind::V { dihedral_deg: 40.0 },
        TailKind::InvV { dihedral_deg: 40.0 },
        TailKind::TwinBoom,
    ] {
        let g = driven_geometry(kind);
        let (s, c, b) = (g.wing.projected_area_m2(), g.wing.mac_m(), g.wing.span_m());
        let x_acw = g.wing.x_le_mac_m() + 0.25 * c;
        let h = g.horizontal.as_ref().unwrap();
        let arm = h.planform.aerodynamic_centre_m()[0] - x_acw;
        assert!((arm - 0.9).abs() < 1e-9, "{kind:?}: arm {arm}");
        let v_h = h.effective_horizontal_area_m2() * arm / (c * s);
        let vertical = h.effective_vertical_area_m2()
            + g.vertical
                .as_ref()
                .map_or(0.0, TailSurfaceGeom::effective_vertical_area_m2);
        let v_v = vertical * arm / (b * s);
        assert!(v_h >= 0.55 - 1e-9, "{kind:?}: V_H {v_h}");
        assert!(v_v >= 0.04 - 1e-9, "{kind:?}: V_V {v_v}");
        if matches!(
            kind,
            TailKind::Conventional | TailKind::T | TailKind::TwinBoom
        ) {
            assert!((v_h - 0.55).abs() < 1e-9 && (v_v - 0.04).abs() < 1e-9);
        }
    }
    // Flying wing: no tail surfaces at all.
    let fw = spec_with(
        TailKind::None,
        WingKind::Flying {
            sweep_deg: 25.0,
            washout_deg: 2.0,
            reflex_airfoil: true,
        },
        EmpennageDriving::Explicit {
            horizontal: None,
            vertical: None,
        },
    );
    let g = AirframeGeometry::from_master(&fw.master, &fw.topology, &TailShape::default()).unwrap();
    assert!(g.horizontal.is_none() && g.vertical.is_none());
    // The reference spec builds too.
    let spec = reference_spec();
    assert!(
        AirframeGeometry::from_master(&spec.master, &spec.topology, &TailShape::default()).is_ok()
    );
}

#[test]
fn flying_wing_neutral_point_is_the_wing_and_trim_uses_elevons() {
    let fw = spec_with(
        TailKind::None,
        WingKind::Straight,
        EmpennageDriving::Explicit {
            horizontal: None,
            vertical: None,
        },
    );
    let g = AirframeGeometry::from_master(&fw.master, &fw.topology, &TailShape::default()).unwrap();
    let np = SimpleLiftingSurface
        .neutral_point(&g, &LongitudinalOptions::default())
        .unwrap();
    assert_eq!(np.tail_term, 0.0);
    assert!((np.x_np_m - np.x_ac_wing_m).abs() < 1e-12);
    // Fuselage destabilising term moves the NP forward by c Cm_a / CLa.
    let o = LongitudinalOptions {
        fuselage: FuselageContribution {
            cm_alpha_per_rad: 0.2,
        },
        ..LongitudinalOptions::default()
    };
    let npf = SimpleLiftingSurface.neutral_point(&g, &o).unwrap();
    assert!((npf.x_np_m - (np.x_ac_wing_m - np.mac_m * 0.2 / np.cl_alpha_wing)).abs() < 1e-12);
    // Elevon trim: closure of Cm0 + CL (x_cg - x_ac)/c + Cm_delta delta = 0.
    let setup = TrimSetup {
        cm0: 0.02,
        tail_incidence_rad: 0.0,
        tau: 0.5,
        max_down_rad: 0.35,
        max_up_rad: 0.35,
    };
    let cond = TrimCondition {
        name: "cruise".into(),
        cl: 0.4,
        x_cg_m: np.x_np_m - 0.12 * np.mac_m,
    };
    let x_e = np.x_np_m + 0.4 * np.mac_m;
    let r = trim_flying_wing(&np, &setup, 0.05, x_e, &cond).unwrap();
    let moment = setup.cm0
        + 0.4 * (cond.x_cg_m - np.x_ac_wing_m) / np.mac_m
        + r.cm_delta_per_rad * r.delta_rad;
    assert!(moment.abs() < 1e-12);
    // Elevons forward of the CG have no nose-down authority.
    assert!(trim_flying_wing(&np, &setup, 0.05, cond.x_cg_m - 0.1, &cond).is_err());
}

#[test]
fn tail_sizing_hits_the_margin_exactly_and_reports_the_driver() {
    let start = driven_geometry(TailKind::Conventional);
    let np0 = SimpleLiftingSurface
        .neutral_point(&start, &LongitudinalOptions::default())
        .unwrap();
    let o = LongitudinalOptions::default();
    let x_aft = np0.x_np_m - 0.10 * np0.mac_m; // 10 % margin with the volume-coefficient tail
    let setup = TrimSetup {
        cm0: -0.02,
        tail_incidence_rad: 0.0,
        tau: 0.5,
        max_down_rad: 0.4,
        max_up_rad: 0.4,
    };
    let req = SizingRequirements {
        sm_min_aft: 0.15,
        sm_max_fwd: 0.5,
        x_cg_aft_m: x_aft,
        x_cg_fwd_m: x_aft - 0.04,
        trim_conditions: vec![TrimCondition {
            name: "cruise".into(),
            cl: 0.5,
            x_cg_m: x_aft - 0.04,
        }],
        authority_fraction: 0.8,
        setup,
    };
    let r = size_horizontal_tail(&SimpleLiftingSurface, &start, &o, &req).unwrap();
    assert!((r.sm_aft - 0.15).abs() < 1e-6, "SM {}", r.sm_aft);
    assert!(r.scale > 1.0 && r.driver == SizingDriver::Margin);
    // Tail AC stayed put; area scaled by r.scale.
    let (h0, h1) = (
        start.horizontal.as_ref().unwrap(),
        r.geometry.horizontal.as_ref().unwrap(),
    );
    assert!(
        (h1.planform.aerodynamic_centre_m()[0] - h0.planform.aerodynamic_centre_m()[0]).abs()
            < 1e-9
    );
    assert!(
        (h1.planform.projected_area_m2() / h0.planform.projected_area_m2() - r.scale).abs() < 1e-9
    );
    // A smaller required margin gives a smaller tail (monotone), and a tight
    // authority limit turns the driver to authority.
    let loose = SizingRequirements {
        sm_min_aft: 0.12,
        ..req.clone()
    };
    let rl = size_horizontal_tail(&SimpleLiftingSurface, &start, &o, &loose).unwrap();
    assert!(rl.scale < r.scale);
    let tight = SizingRequirements {
        sm_min_aft: 0.05,
        setup: TrimSetup {
            cm0: -0.20,
            ..setup
        },
        ..req
    };
    let rt = size_horizontal_tail(&SimpleLiftingSurface, &start, &o, &tight).unwrap();
    assert_eq!(rt.driver, SizingDriver::Authority);
    for cond in &tight.trim_conditions {
        let t = trim_tailed(&rt.np, &rt.geometry, &o, &tight.setup, cond).unwrap();
        assert!(t.authority_used <= 0.8 + 1e-6);
    }
    // An unreachable margin is reported, not clamped.
    let absurd = SizingRequirements {
        sm_min_aft: 40.0,
        ..loose
    };
    assert!(matches!(
        size_horizontal_tail(&SimpleLiftingSurface, &start, &o, &absurd),
        Err(AirframeError::NoSolution(_))
    ));
}

#[test]
fn static_margin_moves_one_to_one_with_the_cg() {
    let g = driven_geometry(TailKind::Conventional);
    let np = SimpleLiftingSurface
        .neutral_point(&g, &LongitudinalOptions::default())
        .unwrap();
    let x = np.x_cg_for_margin_m(0.12);
    assert!((np.static_margin(x) - 0.12).abs() < 1e-12);
    assert!((np.cm_alpha_per_rad(x) + 0.12 * np.cl_alpha_total()).abs() < 1e-12);
    // Aft of the neutral point the aircraft is unstable (Cm_alpha > 0).
    assert!(np.cm_alpha_per_rad(np.x_np_m + 0.01) > 0.0);
}

#[test]
fn t_tail_wake_screen_follows_the_wake_line_geometry() {
    use alas_uav::airframe::tail::t_tail_wake_screen;
    // Trailing edge at the origin, tail AC 1.0 m aft and 0.25 m up, c = 0.3 m,
    // d eps/d alpha = 0.4.  At alpha = 15 deg the wake line has slope
    // tan(15 deg * 0.6) = tan(9 deg): z_wake = 0.1584 m, so the tail is
    // 0.0916 m above it, inside one chord (0.3 m): flagged.
    let a = 15.0_f64.to_radians();
    let s = t_tail_wake_screen([0.0; 3], [1.0, 0.0, 0.25], a, 0.4, 0.3, 1.0).unwrap();
    assert!((s.height_above_wake_m - (0.25 - (a * 0.6).tan())).abs() < 1e-12);
    assert!((s.height_above_wake_m - 0.0916).abs() < 1e-4);
    assert!(s.in_wake);
    // Cruise (alpha 2 deg): the wake line is almost level and the tail clears it.
    let cruise = t_tail_wake_screen(
        [0.0; 3],
        [1.0, 0.0, 0.45],
        2.0_f64.to_radians(),
        0.4,
        0.3,
        1.0,
    )
    .unwrap();
    assert!(!cruise.in_wake);
    // Level-with-the-wing tail at zero alpha lies exactly on the wake line.
    let level = t_tail_wake_screen([0.0; 3], [1.0, 0.0, 0.0], 0.0, 0.4, 0.3, 1.0).unwrap();
    assert!(level.height_above_wake_m.abs() < 1e-15 && level.in_wake);
    assert!(t_tail_wake_screen([1.0, 0.0, 0.0], [0.5, 0.0, 0.1], 0.1, 0.4, 0.3, 1.0).is_err());
}

#[test]
fn load_case_static_margins_follow_the_cg_envelope() {
    use alas_uav::airframe::stability::StabilityPoint;
    let g = driven_geometry(TailKind::Conventional);
    let np = SimpleLiftingSurface
        .neutral_point(&g, &LongitudinalOptions::default())
        .unwrap();
    let cases = vec![
        ("empty".to_owned(), 2.0, np.x_cg_for_margin_m(0.25)),
        ("half".to_owned(), 2.5, np.x_cg_for_margin_m(0.15)),
        ("full".to_owned(), 3.0, np.x_cg_for_margin_m(0.05)),
    ];
    let pts = StabilityPoint::evaluate(&np, &cases);
    for (p, want) in pts.iter().zip([0.25, 0.15, 0.05]) {
        assert!((p.static_margin - want).abs() < 1e-12, "{}", p.case);
        assert!(p.cm_alpha_per_rad < 0.0);
    }
}
