// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Failed assertions identify broken fixtures in this test target.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Viscous VLM verification.
//!
//! * V0/V1: an ideal linear section (`cl = 2 pi alpha`) must reproduce the
//!   inviscid lattice; mesh refinement; strip consistency; determinism.
//! * Published theory (read on the PDF pages): Prandtl lifting line for the
//!   elliptic planform, `CDi = CL^2/(pi A)` (Torenbeek, Advanced Aircraft
//!   Design, Eq. 4.11, PDF p. 111) and `CLalpha = a0/(1 + a0/(pi A))`
//!   (Sadraey, Eq. 5.20, PDF p. 224), `a0 = 2 pi` (Raymer p. 398).
//! * Published data: NASA TM-4062 (McGhee, Walker, Millard 1988), Eppler 387
//!   in the Langley LTPT, R = 1e5, 2e5, 3e5 (cl, cd from the Fig. 17
//!   legends, pages 39-49 of the report).  These are 2-D section data: they
//!   verify the section model and the CSV import, not a finite-wing result.
//! * Not covered: a published low-Re FINITE-wing dataset was not retrieved;
//!   wing-level physical validation is open (M6).

#[path = "support/aero_wings.rs"]
mod wings;

use alas_geom::aircraft::wing::{Wing, WingXSec};
use alas_uav::aero::polar_csv::PolarTable;
use alas_uav::aero::section::SectionModel;
use alas_uav::aero::slipstream::{PropDisc, Slipstream};
use alas_uav::aero::viscous_vlm::{ViscousConfig, ViscousVlm};
use alas_uav::aero::{AeroError, AirState, Flag, Severity};

const PI: f64 = std::f64::consts::PI;

fn air(v: f64) -> AirState {
    AirState::from_isa(0.0, v).unwrap()
}

fn nf_pair(foil: &str) -> Vec<SectionModel> {
    vec![wings::neuralfoil(foil, 9.0), wings::neuralfoil(foil, 9.0)]
}

fn lin_pair() -> Vec<SectionModel> {
    vec![
        wings::linear_table("lin", 0.01),
        wings::linear_table("lin", 0.01),
    ]
}

/// (alpha deg, cl, cd) of the Eppler 387, NASA TM-4062, free-air LTPT data.
const E387_100K: [(f64, f64, f64); 9] = [
    (-2.98, 0.102, 0.0217),
    (-1.99, 0.210, 0.0162),
    (0.00, 0.390, 0.0167),
    (1.99, 0.587, 0.0211),
    (4.00, 0.778, 0.0230),
    (6.00, 0.974, 0.0224),
    (8.00, 1.172, 0.0210),
    (10.06, 1.200, 0.0413),
    (11.00, 1.201, 0.0525),
];
const E387_200K: [(f64, f64, f64); 9] = [
    (-2.84, 0.066, 0.0163),
    (-1.99, 0.156, 0.0133),
    (0.01, 0.352, 0.0105),
    (2.04, 0.574, 0.0118),
    (3.99, 0.785, 0.0133),
    (6.04, 0.999, 0.0144),
    (8.03, 1.182, 0.0174),
    (10.02, 1.231, 0.0357),
    (11.04, 1.214, 0.0570),
];
const E387_300K: [(f64, f64, f64); 9] = [
    (-2.93, 0.051, 0.0140),
    (-2.01, 0.146, 0.0118),
    (0.01, 0.352, 0.0089),
    (2.00, 0.573, 0.0099),
    (4.00, 0.792, 0.0109),
    (6.01, 1.009, 0.0118),
    (8.02, 1.180, 0.0168),
    (10.02, 1.251, 0.0316),
    (11.02, 1.241, 0.0621),
];

fn e387() -> alas_geom::aircraft::airfoil::Airfoil {
    let (_, coords) = alas_geom::selig::get("e387").expect("e387 in the Selig corpus");
    alas_geom::aircraft::airfoil::Airfoil::from_coordinates("e387", coords.to_vec())
}

#[test]
fn an_ideal_linear_section_reproduces_the_inviscid_lattice() {
    let plane = wings::rectangular(8.0, 0.25, "naca0012");
    let v = ViscousVlm::new(&plane, lin_pair(), ViscousConfig::default()).unwrap();
    let a = air(15.0);
    for deg in [-3.0_f64, 2.0, 6.0] {
        let alpha = deg.to_radians();
        let inviscid = v.solve_inviscid(&a, alpha).unwrap();
        let s = v.solve(&a, alpha, None).unwrap();
        assert!(
            s.converged && s.iterations <= 2,
            "iterations {}",
            s.iterations
        );
        assert!((s.cl - inviscid.cl_lift).abs() <= 1e-6 * inviscid.cl_lift.abs().max(1e-3));
        // Constant section drag integrates to cd (the strips tile S_ref).
        assert!((s.cd_profile - 0.01).abs() < 1e-9, "{}", s.cd_profile);
        assert!(s.validity.severity() == Severity::Ok);
    }
}

#[test]
fn elliptic_wing_matches_prandtl_lifting_line() {
    let plane = wings::elliptic(8.0, 0.3, 24, "naca0012");
    let ar = plane.b_ref * plane.b_ref / plane.s_ref;
    assert!((ar - 8.0).abs() < 0.05);
    let n = plane.wings[0].xsecs.len();
    let sections = (0..n).map(|_| wings::linear_table("lin", 0.01)).collect();
    // 24 cross-sections already describe the planform: 4 strips each.
    let cfg = ViscousConfig {
        spanwise_strips: 96,
        ..ViscousConfig::default()
    };
    let v = ViscousVlm::new(&plane, sections, cfg).unwrap();
    let a = air(15.0);
    let prandtl_slope = 2.0 * PI / (1.0 + 2.0 * PI / (PI * ar));
    for deg in [2.0_f64, 5.0] {
        let alpha = deg.to_radians();
        let s = v.solve(&a, alpha, None).unwrap();
        // Induced drag: CDi = CL^2/(pi A) within 1 % (e = 1 for elliptic loading).
        let ideal = s.cl * s.cl / (PI * ar);
        assert!(
            (s.cd_induced / ideal - 1.0).abs() < 0.01,
            "{} vs {ideal}",
            s.cd_induced
        );
        let e = s.span_efficiency.unwrap();
        assert!((e - 1.0).abs() < 0.01, "e = {e}");
        // Lift slope: the lattice is a lifting SURFACE (finite chord), lower than the
        // lifting-line value by 4-5 % at A = 8 (c0/b = 0.16); never above it.
        let slope = s.cl / alpha;
        let ratio = slope / prandtl_slope;
        assert!(ratio < 1.0 && ratio > 0.93, "slope ratio {ratio}");
    }
}

#[test]
fn mesh_refinement_changes_cl_by_less_than_one_percent_and_cd_by_two() {
    // Plan 8, V1: < 1 % CL and < 2 % CD between refinement levels (24 -> 48 per half).
    let plane = wings::rectangular(8.0, 0.25, "naca2412");
    let a = air(15.0);
    let alpha = 6.0_f64.to_radians();
    let at = |spanwise, chordwise| {
        let cfg = ViscousConfig {
            spanwise_strips: spanwise,
            chordwise_resolution: chordwise,
            ..ViscousConfig::default()
        };
        ViscousVlm::new(&plane, nf_pair("naca2412"), cfg)
            .unwrap()
            .solve(&a, alpha, None)
            .unwrap()
    };
    let (coarse, fine) = (at(24, 4), at(48, 4));
    assert!(
        (coarse.cl / fine.cl - 1.0).abs() < 0.01,
        "{} vs {}",
        coarse.cl,
        fine.cl
    );
    let (cd_c, cd_f) = (
        coarse.cd_induced + coarse.cd_profile,
        fine.cd_induced + fine.cd_profile,
    );
    assert!((cd_c / cd_f - 1.0).abs() < 0.02, "{cd_c} vs {cd_f}");
    // Chordwise refinement 4 -> 8 panels.
    let deeper = at(24, 8);
    assert!((deeper.cl / coarse.cl - 1.0).abs() < 0.01);
}

#[test]
fn every_strip_satisfies_the_coupling_condition_at_convergence() {
    let plane = wings::rectangular(20.0, 0.1, "naca2412");
    let v = ViscousVlm::new(&plane, nf_pair("naca2412"), ViscousConfig::default()).unwrap();
    let a = air(25.0);
    let s = v.solve(&a, 5.0_f64.to_radians(), None).unwrap();
    assert!(s.converged && s.residual < 1e-3);
    for strip in &s.strips {
        assert!(
            (strip.cl_vlm - strip.q_ratio * strip.cl_section).abs() < 1.1e-3,
            "y {}: {} vs {}",
            strip.y_m,
            strip.cl_vlm,
            strip.cl_section
        );
        assert_eq!(strip.q_ratio, 1.0);
    }
    // Effective angle = geometric - induced: below the geometric angle by the
    // induced angle, positive and small at the centre of a high-AR wing.
    let mid = s
        .strips
        .iter()
        .min_by(|x, y| x.y_m.abs().total_cmp(&y.y_m.abs()))
        .unwrap();
    let induced_deg = 5.0 - mid.alpha_eff_rad.to_degrees();
    assert!(
        induced_deg > 0.0 && induced_deg < 1.0,
        "alpha_i = {induced_deg} deg"
    );
    // Prandtl: alpha_i,mid is of the order CL/(pi A); the lattice value is within a
    // factor 2 of it (rectangular loading, finite chord).
    let ll = (s.cl / (PI * 20.0)).to_degrees();
    assert!(
        induced_deg / ll > 0.5 && induced_deg / ll < 2.0,
        "{induced_deg} vs {ll}"
    );
    // The strip Reynolds number is rho V c / mu on the local chord.
    let re = a.reynolds(0.1);
    assert!((mid.re / re - 1.0).abs() < 0.01);
}

#[test]
fn solutions_are_deterministic_and_warm_start_agrees() {
    let plane = wings::rectangular(8.0, 0.25, "naca2412");
    let v = ViscousVlm::new(&plane, nf_pair("naca2412"), ViscousConfig::default()).unwrap();
    let a = air(15.0);
    let alpha = 7.0_f64.to_radians();
    let first = v.solve(&a, alpha, None).unwrap();
    let again = v.solve(&a, alpha, None).unwrap();
    assert_eq!(first.cl.to_bits(), again.cl.to_bits());
    assert_eq!(first.cd_profile.to_bits(), again.cd_profile.to_bits());
    // Warm start from a neighbouring angle gives the same answer within tolerance.
    let near = v.solve(&a, 6.0_f64.to_radians(), None).unwrap();
    let warm = v
        .solve_from(&a, alpha, None, Some(&near.offsets_rad))
        .unwrap();
    assert!((warm.cl / first.cl - 1.0).abs() < 2e-3);
    assert!(warm.iterations <= first.iterations);
}

#[test]
fn clmax_and_first_stall_location_follow_planform_physics() {
    let a = air(15.0);
    let step = 1.0_f64.to_radians();
    // Untapered wing: stall starts at the root.
    let rect = wings::rectangular(8.0, 0.25, "naca2412");
    let v = ViscousVlm::new(&rect, nf_pair("naca2412"), ViscousConfig::default()).unwrap();
    let m = v
        .cl_max(&a, None, 0.0, 22.0_f64.to_radians(), step)
        .unwrap();
    assert!(m.cl_max > 1.0 && m.cl_max < 1.5, "CLmax {}", m.cl_max);
    assert_eq!(m.band, 0.1);
    let first = m.first_stall.expect("a stall inside the sweep");
    assert!(!first.tip_stall && first.y_m.abs() < 0.35, "{first:?}");
    // CL rises monotonically up to the maximum.
    let up: Vec<f64> = m
        .curve
        .iter()
        .take_while(|(alpha, _, _)| *alpha <= m.alpha_rad + 1e-9)
        .map(|(_, cl, _)| *cl)
        .collect();
    assert!(up.windows(2).all(|w| w[1] > w[0]));
    // Tapered wing (0.4): the stall starts outboard.
    let (cr, ct, half) = (0.3, 0.12, 0.84);
    let foil = || wings::naca("naca2412");
    let taper = wings::airplane(Wing::new(
        "Main Wing",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], cr, 0.0, foil()),
            WingXSec::new([0.25 * (cr - ct), half, 0.0], ct, 0.0, foil()),
        ],
        true,
    ));
    let v = ViscousVlm::new(&taper, nf_pair("naca2412"), ViscousConfig::default()).unwrap();
    let m = v
        .cl_max(&a, None, 0.0, 22.0_f64.to_radians(), step)
        .unwrap();
    let first = m.first_stall.expect("a stall inside the sweep");
    assert!(first.tip_stall && first.y_m.abs() > 0.5 * half, "{first:?}");
}

#[test]
fn leaving_the_section_model_domain_is_refused_or_flagged() {
    let plane = wings::rectangular(8.0, 0.25, "naca2412");
    let v = ViscousVlm::new(&plane, nf_pair("naca2412"), ViscousConfig::default()).unwrap();
    // 0.6 m/s: section Re 1e4, refused.
    let e = v.solve(&air(0.6), 0.05, None);
    assert!(matches!(e, Err(AeroError::OutsideDomain(_))), "{e:?}");
    // 3 m/s: Re 5e4, warned.
    let s = v.solve(&air(3.0), 0.05, None).unwrap();
    assert_eq!(s.validity.severity(), Severity::Warn);
    assert!(s
        .validity
        .flags
        .iter()
        .any(|(_, f)| matches!(f, Flag::LowReynolds(_))));
    // 60 m/s: Mach 0.18 above the incompressible domain, warned.
    let s = v.solve(&air(60.0), 0.03, None).unwrap();
    assert!(s
        .validity
        .flags
        .iter()
        .any(|(_, f)| matches!(f, Flag::Mach(_))));
    // The same refused case with refusal switched off returns the flag instead.
    let lenient = ViscousVlm::new(
        &plane,
        nf_pair("naca2412"),
        ViscousConfig {
            refuse_outside_domain: false,
            ..ViscousConfig::default()
        },
    )
    .unwrap();
    let s = lenient.solve(&air(0.6), 0.05, None).unwrap();
    assert_eq!(s.validity.severity(), Severity::Refused);
}

#[test]
fn constructor_rejects_inconsistent_inputs() {
    let plane = wings::rectangular(8.0, 0.25, "naca0012");
    // One section model for two cross-sections.
    assert!(ViscousVlm::new(
        &plane,
        vec![wings::neuralfoil("naca0012", 9.0)],
        ViscousConfig::default()
    )
    .is_err());
    let bad = ViscousConfig {
        relaxation: 0.0,
        ..ViscousConfig::default()
    };
    assert!(ViscousVlm::new(&plane, nf_pair("naca0012"), bad).is_err());
    let v = ViscousVlm::new(&plane, nf_pair("naca0012"), ViscousConfig::default()).unwrap();
    assert!(v.solve(&air(15.0), f64::NAN, None).is_err());
    assert!(v.cl_max(&air(15.0), None, 0.2, 0.1, 0.01).is_err());
}

#[test]
fn neuralfoil_matches_the_nasa_eppler_387_data_inside_the_plan_thresholds() {
    // Plan 8 V2 thresholds (ESTIMATES): cl +-0.08 below stall, cd +-20 % in the
    // laminar bucket.  n_crit = 9 (average tunnel).  Mach 0.05.
    let section = SectionModel::neuralfoil(&e387(), 9.0).unwrap();
    for (re, data) in [(1.0e5, E387_100K), (2.0e5, E387_200K), (3.0e5, E387_300K)] {
        for (alpha, cl, cd) in data {
            let p = section.eval(alpha.to_radians(), re, 0.05).unwrap();
            if alpha <= 6.1 {
                assert!(
                    (p.cl - cl).abs() < 0.08,
                    "Re {re} a {alpha}: cl {} vs {cl}",
                    p.cl
                );
                assert!(
                    (p.cd / cd - 1.0).abs() < 0.20,
                    "Re {re} a {alpha}: cd {} vs {cd}",
                    p.cd
                );
            } else {
                // Bubble-burst region (8 deg) and post-stall: looser, documented.
                assert!(
                    (p.cl - cl).abs() < 0.10,
                    "Re {re} a {alpha}: cl {} vs {cl}",
                    p.cl
                );
            }
            assert_eq!(p.validity.severity(), Severity::Ok, "{:?}", p.validity);
        }
    }
    // KNOWN WEAK SPOT: at 8 deg the network predicts the bubble drag rise 20-40 %
    // too early (UAV-PLAN 3.1: bubble-dominated drag is the largest model risk).
    let p = section.eval(8.03_f64.to_radians(), 2.0e5, 0.05).unwrap();
    assert!(p.cd / 0.0174 > 1.2);
}

#[test]
fn a_polar_imported_from_the_nasa_table_drives_the_wing_like_neuralfoil() {
    // The published R = 2e5 points as CSV: real data through the import path.
    let mut text = String::from("alpha_deg,cl,cd\n");
    for (a, cl, cd) in E387_200K {
        text.push_str(&format!("{a},{cl},{cd}\n"));
    }
    let table = PolarTable::parse("E387 NASA TM-4062 R=2e5", &text, Some(2.0e5)).unwrap();
    let imported = SectionModel::from_table(table);
    let neural = SectionModel::neuralfoil(&e387(), 9.0).unwrap();
    // Straight, untwisted wing of the same airfoil; chord such that Re ~ 2e5.
    let foil = e387();
    let half = 0.5 * 8.0 * 0.25;
    let plane = wings::airplane(Wing::new(
        "Main Wing",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 0.25, 0.0, foil.clone()),
            WingXSec::new([0.0, half, 0.0], 0.25, 0.0, foil),
        ],
        true,
    ));
    let a = AirState::from_rho_t(12.0, 1.225, 288.15).unwrap();
    let re = a.reynolds(0.25);
    assert!(re > 1.8e5 && re < 2.2e5, "Re {re}");
    let v_imp = ViscousVlm::new(
        &plane,
        vec![imported.clone(), imported],
        ViscousConfig::default(),
    )
    .unwrap();
    let v_nf = ViscousVlm::new(
        &plane,
        vec![neural.clone(), neural],
        ViscousConfig::default(),
    )
    .unwrap();
    for deg in [2.0_f64, 4.0, 6.0] {
        let alpha = deg.to_radians();
        let s_imp = v_imp.solve(&a, alpha, None).unwrap();
        let s_nf = v_nf.solve(&a, alpha, None).unwrap();
        assert!(s_imp.converged && s_nf.converged);
        // The two section sources differ by <= 0.06 in cl (see the test above): the wing CL
        // follows within 8 %, and the profile drag within 25 %.
        assert!(
            (s_imp.cl / s_nf.cl - 1.0).abs() < 0.08,
            "a {deg}: {} vs {}",
            s_imp.cl,
            s_nf.cl
        );
        assert!((s_imp.cd_profile / s_nf.cd_profile - 1.0).abs() < 0.25);
        assert_eq!(
            s_imp.validity.severity(),
            Severity::Ok,
            "{:?}",
            s_imp.validity
        );
    }
}

#[test]
fn slipstream_raises_local_dynamic_pressure_lift_and_profile_drag() {
    let plane = wings::rectangular(8.0, 0.25, "naca2412");
    let v = ViscousVlm::new(&plane, nf_pair("naca2412"), ViscousConfig::default()).unwrap();
    let a = air(15.0);
    let alpha = 4.0_f64.to_radians();
    let none = v.solve(&a, alpha, None).unwrap();
    // Zero thrust: bit-identical to no slipstream.
    let idle = Slipstream {
        discs: vec![PropDisc::new([-0.2, 0.5, 0.0], 0.12, 0.0).unwrap()],
    };
    let zero = v.solve(&a, alpha, Some(&idle)).unwrap();
    assert_eq!(none.cl.to_bits(), zero.cl.to_bits());
    // Two tractor discs of 0.12 m radius, 6 N each, 0.2 m ahead of the leading edge.
    let slip = Slipstream {
        discs: vec![
            PropDisc::new([-0.2, 0.5, 0.0], 0.12, 6.0).unwrap(),
            PropDisc::new([-0.2, -0.5, 0.0], 0.12, 6.0).unwrap(),
        ],
    };
    let s = v.solve(&a, alpha, Some(&slip)).unwrap();
    assert!(s.converged);
    assert!(s.cl > none.cl * 1.02, "{} vs {}", s.cl, none.cl);
    assert!(s.cd_profile > none.cd_profile * 1.02);
    let q_max = s.strips.iter().map(|x| x.q_ratio).fold(1.0, f64::max);
    assert!(q_max > 1.2);
    // The lift gain cannot exceed the largest dynamic-pressure increase.
    assert!(s.cl / none.cl < q_max);
    // Coverage is confined to the strips behind the discs and covered strips
    // have q_ratio > 1.
    for strip in &s.strips {
        let behind_disc = (strip.y_m.abs() - 0.5).abs() < 0.12 + 0.5 * strip.width_m;
        if strip.coverage > 0.0 {
            assert!(behind_disc && strip.q_ratio > 1.0, "y {}", strip.y_m);
        } else {
            assert_eq!(strip.q_ratio, 1.0);
        }
    }
    assert!(s.strips.iter().filter(|x| x.coverage > 0.0).count() >= 2);
    assert!(s.cl.is_finite() && s.cd_induced > none.cd_induced);
}
