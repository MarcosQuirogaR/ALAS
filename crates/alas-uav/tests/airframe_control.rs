// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A failed unwrap in this test target is the assertion reporting malformed
// evidence, not a panic escaping from library code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Control-surface hinge moment and servo torque check (UAV-PLAN 3.4).
//!
//! Source: Sadraey, Aircraft Design: A Systems Engineering Approach,
//! Eq. 12.147-12.150 (p. 714, read on the page): `H = 1/2 rho V^2 S_cs c_cs Ch`,
//! `Ch = Ch0 + Ch_alpha alpha + Ch_delta delta`, typical `Ch_alpha = -0.1` and
//! `Ch_delta = -0.3` per radian.  The book has no worked numeric hinge-moment
//! example, so the verification is the equation evaluated by hand, limiting
//! cases and unit conversions; the derivatives themselves remain ESTIMATES
//! (+/-30 %).  The servo fixture is a hobby micro servo with a 2.0 kgf cm stall
//! torque at 6 V, a TEST FIXTURE and not a datasheet value.

mod airframe_support;

use alas_uav::airframe::control::{
    control_geometry, servo_check, worst_hinge_case, HingeCase, HingeMoment, ServoLoad,
    CH_ALPHA_TYPICAL, CH_DELTA_TYPICAL, CH_UNCERTAINTY,
};
use alas_uav::airframe::geometry::{Panel, Planform};
use alas_uav::airframe::AirframeError;
use alas_uav_db::parts::ServoSpec;
use alas_uav_db::{Curve, Method, Provenance, SourceId};

const KGF_CM: f64 = 0.980_665e-1; // N m per kgf cm (standard gravity)

fn servo(torque_4v8: f64, torque_6v: f64) -> ServoSpec {
    let prov = Provenance::new(SourceId::new("test-src").unwrap(), None, Method::User);
    ServoSpec {
        torque_vs_voltage: Some(Curve {
            x_name: "voltage".into(),
            x_unit_si: "V".into(),
            y_unit_si: "N m".into(),
            x: vec![4.8, 6.0],
            y: vec![torque_4v8, torque_6v],
            param: None,
            prov: prov.clone(),
        }),
        i_stall_vs_voltage: Some(Curve {
            x_name: "voltage".into(),
            x_unit_si: "V".into(),
            y_unit_si: "A".into(),
            x: vec![4.8, 6.0],
            y: vec![0.65, 0.8],
            param: None,
            prov,
        }),
        ..ServoSpec::default()
    }
}

fn rect_wing() -> Planform {
    Planform {
        panels: vec![Panel {
            span_m: 1.0,
            chord_root_m: 0.30,
            chord_tip_m: 0.30,
            root_le_m: [0.2, 0.0, 0.0],
            sweep_le_rad: 0.0,
            dihedral_rad: 0.0,
        }],
        mirror: true,
        vertical: false,
    }
}

#[test]
fn control_geometry_of_a_rectangular_aileron() {
    // Aileron from 55 % to 95 % of the half span, 25 % chord: span 0.4 m,
    // chord 0.075 m, area 0.03 m^2, centroid at the TE minus half its chord.
    let g = control_geometry(&rect_wing(), 0.55, 0.95, 0.25).unwrap();
    assert!((g.span_m - 0.4).abs() < 1e-12);
    assert!((g.mean_chord_m - 0.075).abs() < 1e-12);
    assert!((g.area_m2 - 0.03).abs() < 1e-12);
    assert!((g.centroid_x_m - (0.2 + 0.3 - 0.0375)).abs() < 1e-12);
    assert!(control_geometry(&rect_wing(), 0.9, 0.5, 0.25).is_err());
    assert!(control_geometry(&rect_wing(), 0.1, 0.5, 0.9).is_err());
    // Tapered wing: area equals the integral of the hinged chord fraction.
    let tapered = Planform {
        panels: vec![Panel {
            chord_tip_m: 0.15,
            ..rect_wing().panels[0]
        }],
        ..rect_wing()
    };
    let g = control_geometry(&tapered, 0.0, 1.0, 0.3).unwrap();
    assert!((g.area_m2 - 0.3 * 0.5 * (0.30 + 0.15) * 1.0).abs() < 1e-9);
}

#[test]
fn hinge_moment_equals_the_hand_evaluated_equation() {
    let g = control_geometry(&rect_wing(), 0.55, 0.95, 0.25).unwrap();
    let h = HingeMoment::typical(&g);
    assert_eq!(
        (h.ch_alpha, h.ch_delta),
        (CH_ALPHA_TYPICAL, CH_DELTA_TYPICAL)
    );
    // 25 m/s, sea level, alpha 0.1 rad, delta 0.3 rad:
    // q = 0.5 * 1.225 * 625 = 382.8125 Pa; S c = 0.03 * 0.075 = 2.25e-3 m^3;
    // Ch = -0.1 * 0.1 - 0.3 * 0.3 = -0.1 -> H = -0.08614 N m.
    let moment = h.moment_nm(1.225, 25.0, 0.1, 0.3);
    assert!((moment + 0.5 * 1.225 * 625.0 * 2.25e-3 * 0.1).abs() < 1e-12);
    assert!((moment + 0.086_132).abs() < 1e-5);
    // Limits: zero at zero speed, zero deflection and angle (symmetric section),
    // quadratic in speed, linear in deflection.
    assert_eq!(h.moment_nm(1.225, 0.0, 0.1, 0.3), 0.0);
    assert_eq!(h.moment_nm(1.225, 25.0, 0.0, 0.0), 0.0);
    assert!((h.moment_nm(1.225, 50.0, 0.1, 0.3) / moment - 4.0).abs() < 1e-12);
    assert!(
        (h.moment_nm(1.225, 25.0, 0.0, 0.6) / h.moment_nm(1.225, 25.0, 0.0, 0.3) - 2.0).abs()
            < 1e-12
    );
    // The conservative bound scales the derivatives by 1 + 30 %.
    assert!(
        (h.moment_upper_nm(1.225, 25.0, 0.1, 0.3) - (1.0 + CH_UNCERTAINTY) * moment.abs()).abs()
            < 1e-12
    );
}

#[test]
fn worst_case_selection_and_error_paths() {
    let g = control_geometry(&rect_wing(), 0.55, 0.95, 0.25).unwrap();
    let h = HingeMoment::typical(&g);
    let cases = vec![
        HingeCase {
            name: "cruise".into(),
            speed_ms: 22.0,
            rho: 1.225,
            alpha_rad: 0.05,
            delta_rad: 0.05,
        },
        HingeCase {
            name: "dive".into(),
            speed_ms: 38.0,
            rho: 1.225,
            alpha_rad: 0.0,
            delta_rad: 0.3,
        },
        HingeCase {
            name: "stall".into(),
            speed_ms: 12.0,
            rho: 1.225,
            alpha_rad: 0.25,
            delta_rad: 0.4,
        },
    ];
    let (i, w) = worst_hinge_case(&h, &cases).unwrap();
    assert_eq!(cases[i].name, "dive");
    assert!(w > 0.0);
    assert!(worst_hinge_case(&h, &[]).is_err());
}

fn load(hinge: f64, voltage: f64) -> ServoLoad {
    ServoLoad {
        hinge_moment_nm: hinge,
        servo_arm_m: 0.012,
        horn_m: 0.012,
        linkage_efficiency: 0.9,
        friction_nm: 0.005,
        margin_factor: 1.5,
        n_servos: 1,
        delta_max_rad: 25.0_f64.to_radians(),
        servo_travel_rad: 90.0_f64.to_radians(),
        voltage_v: voltage,
    }
}

#[test]
fn servo_check_applies_the_half_stall_rule_and_the_linkage() {
    // 2.0 kgf cm at 6 V = 0.196 N m stall; usable 0.098 N m.
    let s = servo(1.8 * KGF_CM, 2.0 * KGF_CM);
    let r = servo_check(&s, &load(0.0861, 6.0), "micro-servo").unwrap();
    assert!((r.stall_nm - 0.196_133).abs() < 1e-6);
    assert!((r.usable_nm - 0.5 * r.stall_nm).abs() < 1e-12);
    // Required = (H r_s/(r_h eta) + friction) * margin with r_s = r_h.
    let expect = (0.0861 / 0.9 + 0.005) * 1.5;
    assert!((r.required_nm - expect).abs() < 1e-12);
    assert!((r.ratio - expect / r.usable_nm).abs() < 1e-12);
}

#[test]
fn servo_check_flags_a_marginal_servo_and_interpolates_the_voltage() {
    let s = servo(1.8 * KGF_CM, 2.0 * KGF_CM);
    // A 0.0861 N m hinge moment with the 1.5 margin needs 0.151 N m against a
    // 0.098 N m usable torque: the check must FAIL.
    let r = servo_check(&s, &load(0.0861, 6.0), "micro-servo").unwrap();
    assert!(
        !r.torque_ok && r.ratio > 1.4 && r.ratio < 1.6,
        "{}",
        r.ratio
    );
    // Halved moment (two servos share it) passes.
    let two = ServoLoad {
        n_servos: 2,
        ..load(0.0861, 6.0)
    };
    assert!(servo_check(&s, &two, "micro-servo").unwrap().torque_ok);
    // 5.4 V is halfway between the tabulated points; stall current is interpolated too.
    let mid = servo_check(&s, &load(0.02, 5.4), "micro-servo").unwrap();
    assert!((mid.stall_nm - 1.9 * KGF_CM).abs() < 1e-12);
    assert!((mid.stall_current_a.unwrap() - 0.725).abs() < 1e-12);
    // A longer horn lowers the servo torque and raises the travel needed.
    let long_horn = ServoLoad {
        horn_m: 0.024,
        ..load(0.0861, 6.0)
    };
    let a = servo_check(&s, &load(0.0861, 6.0), "s").unwrap();
    let b = servo_check(&s, &long_horn, "s").unwrap();
    assert!(b.required_nm < a.required_nm);
    assert!((b.travel_needed_rad / a.travel_needed_rad - 2.0).abs() < 1e-12);
    // Travel: 25 deg at horn = arm needs 25 deg of servo rotation.
    assert!((a.travel_needed_rad - 25.0_f64.to_radians()).abs() < 1e-12 && a.travel_ok);
}

#[test]
fn servo_check_refuses_to_extrapolate_or_invent() {
    let s = servo(1.8 * KGF_CM, 2.0 * KGF_CM);
    assert!(matches!(
        servo_check(&s, &load(0.05, 7.4), "s"),
        Err(AirframeError::Invalid(_))
    ));
    assert!(matches!(
        servo_check(&ServoSpec::default(), &load(0.05, 6.0), "bare"),
        Err(AirframeError::Missing {
            field: "torque_vs_voltage",
            ..
        })
    ));
    let bad = ServoLoad {
        servo_arm_m: 0.0,
        ..load(0.05, 6.0)
    };
    assert!(servo_check(&s, &bad, "s").is_err());
}

#[test]
fn reference_ruddervator_hinge_load_on_the_micro_servo_is_not_marginal_at_cruise() {
    // Plan 3.4 sanity case: micro servos (stall of order 2 kgf cm at 5-6 V) on
    // the reference aircraft's small surfaces.  A 0.1 m x 0.04 m ruddervator
    // half at 22 m/s cruise (alpha 3 deg, delta 5 deg) is far below the usable
    // torque, while the same surface at 38 m/s with 25 deg is not.
    let g = alas_uav::airframe::control::ControlGeometry {
        area_m2: 0.004,
        mean_chord_m: 0.04,
        span_m: 0.1,
        centroid_x_m: 0.0,
    };
    let h = HingeMoment::typical(&g);
    let s = servo(1.8 * KGF_CM, 2.0 * KGF_CM);
    let cruise = h.moment_upper_nm(1.225, 22.0, 3.0_f64.to_radians(), 5.0_f64.to_radians());
    let ok = servo_check(&s, &load(cruise, 6.0), "micro").unwrap();
    assert!(ok.torque_ok, "cruise ratio {}", ok.ratio);
    let dive = h.moment_upper_nm(1.225, 38.0, 0.0, 25.0_f64.to_radians());
    let hard = servo_check(&s, &load(dive, 6.0), "micro").unwrap();
    assert!(hard.ratio > ok.ratio);
}

#[test]
fn spec_control_surfaces_resolve_on_the_derived_geometry() {
    use alas_uav::airframe::control::geometry_of_control;
    use alas_uav::airframe::geometry::{AirframeGeometry, TailShape};
    use alas_uav::spec::master::{ControlKind, ControlSurface, HostSurface};
    let spec = airframe_support::reference_spec();
    let g =
        AirframeGeometry::from_master(&spec.master, &spec.topology, &TailShape::default()).unwrap();
    let aileron = ControlSurface {
        kind: ControlKind::Aileron,
        host: HostSurface::Wing,
        span_start: 0.55,
        span_end: 0.95,
        chord_fraction: 0.25,
        max_down_deg: 25.0,
        max_up_deg: 25.0,
    };
    let a = geometry_of_control(&g, &aileron).unwrap();
    // Half span 1.2 m: the surface spans 0.4 * 1.2 m and hinges 25 % of the local chord.
    assert!((a.span_m - 0.4 * 1.2).abs() < 1e-12);
    assert!(a.area_m2 > 0.0 && a.area_m2 < 0.25 * 0.28 * a.span_m);
    // Ruddervator on the V pair resolves; a fin on a V-tail aircraft does not exist.
    let ruddervator = ControlSurface {
        kind: ControlKind::Ruddervator,
        host: HostSurface::HorizontalTail,
        span_start: 0.1,
        span_end: 0.95,
        chord_fraction: 0.35,
        max_down_deg: 25.0,
        max_up_deg: 25.0,
    };
    assert!(geometry_of_control(&g, &ruddervator).is_ok());
    let rudder = ControlSurface {
        host: HostSurface::VerticalTail,
        kind: ControlKind::Rudder,
        ..ruddervator
    };
    assert!(matches!(
        geometry_of_control(&g, &rudder),
        Err(AirframeError::UnknownName(_))
    ));
}
