// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Failed assertions identify broken fixtures in this test target.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Ground-effect verification.
//!
//! * Published values: Raymer 6th ed. Eq. (12.60) (PDF p. 485) and the
//!   book's remark (p. 810) that phi is about 0.27 at h/b = 0.05;
//!   Wieselsberger, NACA TM-77 (1922): worked example sigma = 0.452 at
//!   wing-to-image distance h'/b = 0.242.
//! * Numerical truth: the exact image system.  The ground plane z = 0 is
//!   replaced by the mirror image of the wing (z -> -z, incidence reversed)
//!   in the production VLM; for a symmetric section the pair in a horizontal
//!   freestream is the free-air flow of two wings, one lifting down.  The
//!   Trefftz-plane drag of the pair is shared equally by the two wings.

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::VlmSystem;
use alas_atmo::Atmosphere;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::{Wing, WingXSec};
use alas_uav::aero::ground_effect::{induced_drag_factor, FreeAir, GroundEffect, GroundModel};

const SPAN: f64 = 2.0;
const CHORD: f64 = 0.25;

fn wing(z: f64, twist_deg: f64, name: &str) -> Wing {
    let foil = Airfoil::from_name("naca0012").unwrap();
    Wing::new(
        name,
        vec![
            WingXSec::new([0.0, 0.0, z], CHORD, twist_deg, foil.clone()),
            WingXSec::new([0.0, 0.5 * SPAN, z], CHORD, twist_deg, foil),
        ],
        true,
    )
}

fn airplane(wings: Vec<Wing>) -> Airplane {
    let s = wings[0].reference_area();
    Airplane {
        name: "ground".into(),
        xyz_ref: [0.0625, 0.0, 0.0],
        s_ref: s,
        c_ref: CHORD,
        b_ref: SPAN,
        wings,
        fuselages: Vec::new(),
    }
}

/// `(CL, CDi)` of the wing alone in free air and of the wing at height `h`.
fn lattice_pair(h: Option<f64>, incidence_deg: f64) -> (f64, f64) {
    let op = OperatingPoint::new(Atmosphere::new(0.0), 20.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let mut wings = vec![wing(h.unwrap_or(0.0), incidence_deg, "wing")];
    if let Some(h) = h {
        wings.push(wing(-h, -incidence_deg, "image"));
    }
    let plane = airplane(wings);
    let system = VlmSystem::assemble(&plane, 8, 4).unwrap();
    let result = system.solve(&op).unwrap();
    let cd = system
        .trefftz_induced_drag_coefficient(&result, &op)
        .unwrap();
    if h.is_some() {
        // Lift of the real wing = first wing's panels; drag shared equally.
        let n = result.panels.iter().filter(|p| p.wing_index == 0).count();
        let lift: f64 = result.panel_forces_geometry[..n].iter().map(|f| f[2]).sum();
        let q = op.dynamic_pressure();
        (lift / (q * plane.s_ref), 0.5 * cd)
    } else {
        (result.cl_lift, cd)
    }
}

#[test]
fn raymer_and_wieselsberger_values_match_the_published_numbers() {
    // Raymer p. 810: phi ~ 0.27 at h/b = 0.05.
    let r = induced_drag_factor(GroundModel::Raymer, 0.05).unwrap();
    assert!((r - 0.27).abs() < 0.005, "{r}");
    // Wieselsberger worked example: sigma = 0.452 read from the report's
    // chart at h'/b = 0.242 (h' = wing-to-image distance = 2 h): the
    // Prandtl formula in the report gives sigma = 0.432 (4.5 % lower).
    let phi = induced_drag_factor(GroundModel::Wieselsberger, 0.121).unwrap();
    assert!((1.0 - phi - 0.452).abs() < 0.025, "sigma {}", 1.0 - phi);
    // McCormick (16 h/b)^2/(1 + (16 h/b)^2) at 0.05 is 0.39.
    let m = induced_drag_factor(GroundModel::McCormick, 0.05).unwrap();
    assert!((m - 0.64 / 1.64).abs() < 1e-12);
    // Limits: 0 on the ground, 1 far away; monotone.
    for model in [GroundModel::Raymer, GroundModel::McCormick] {
        assert_eq!(induced_drag_factor(model, 0.0).unwrap(), 0.0);
        assert!(induced_drag_factor(model, 50.0).unwrap() > 0.999);
        let mut last = 0.0;
        for k in 1..40 {
            let v = induced_drag_factor(model, 0.025 * k as f64).unwrap();
            assert!(v >= last && v < 1.0);
            last = v;
        }
    }
    assert!(induced_drag_factor(GroundModel::McCormick, -0.1).is_err());
}

/// Image-system results for the rectangular AR 8 wing (c = 0.25 m) at 3 deg:
/// free-air `CL`, `CDi` and the free-air state the correlations act on.
fn free_state() -> (f64, f64, FreeAir) {
    let (cl0, cdi0) = lattice_pair(None, 3.0);
    let aspect = SPAN * SPAN / (SPAN * CHORD);
    let free = FreeAir {
        cl: cl0,
        cd_induced: cdi0,
        cl_alpha_per_rad: cl0 / 3.0_f64.to_radians(),
        aspect_ratio: aspect,
        span_efficiency: cl0 * cl0 / (std::f64::consts::PI * aspect * cdi0),
    };
    (cl0, cdi0, free)
}

#[test]
fn k_factor_ratio_matches_the_image_system_vlm() {
    // phi_VLM = (CDi_g/CL_g^2) / (CDi/CL^2): the induced-drag factor at equal CL.
    // Image-VLM values (8x4 mesh): 0.443 (h/b 0.05), 0.594 (0.1), 0.690 (0.15),
    // 0.805 (0.25), 0.921 (0.5), 0.975 (1.0).  Raymer Eq. 12.60 tracks them to
    // 5 % for h/b >= 0.15; below that the finite chord (h/c < 1) dominates and
    // every lifting-line correlation is low (phi_Raymer 0.27 at h/b 0.05).
    let (cl0, cdi0, _) = free_state();
    for (hb, tol_raymer, tol_wies, tol_mccormick) in [
        (0.15, 0.05, 0.10, 0.30),
        (0.25, 0.02, 0.06, 0.20),
        (0.5, 0.02, 0.02, 0.10),
        (1.0, 0.02, 0.04, 0.03),
    ] {
        let (cl, cdi) = lattice_pair(Some(hb * SPAN), 3.0);
        let phi_vlm = (cdi / cdi0) / (cl / cl0).powi(2);
        for (model, tol) in [
            (GroundModel::Raymer, tol_raymer),
            (GroundModel::Wieselsberger, tol_wies),
            (GroundModel::McCormick, tol_mccormick),
        ] {
            let phi = induced_drag_factor(model, hb).unwrap();
            assert!(
                (phi / phi_vlm - 1.0).abs() < tol,
                "{model:?} at h/b {hb}: {phi:.4} vs image VLM {phi_vlm:.4}"
            );
        }
    }
    // The McCormick form overestimates phi (underestimates the ground effect)
    // by 20 % around h/b = 0.1..0.25: pinned so the finding is not lost.
    let (cl, cdi) = lattice_pair(Some(0.1 * SPAN), 3.0);
    let phi_vlm = (cdi / cdi0) / (cl / cl0).powi(2);
    let mc = induced_drag_factor(GroundModel::McCormick, 0.1).unwrap();
    assert!(mc / phi_vlm > 1.15, "{}", mc / phi_vlm);
}

#[test]
fn lift_and_drag_at_fixed_alpha_follow_the_image_system_above_one_tenth_span() {
    let (cl0, cdi0, free) = free_state();
    for (hb, tol_cl, tol_cdi) in [(0.15, 0.02, 0.08), (0.25, 0.02, 0.05), (0.5, 0.01, 0.03)] {
        let (cl, cdi) = lattice_pair(Some(hb * SPAN), 3.0);
        let ge = GroundEffect {
            model: GroundModel::Raymer,
            height_m: hb * SPAN,
            span_m: SPAN,
        };
        let p = ge.at_fixed_alpha(&free).unwrap();
        assert!(
            (p.cl / cl - 1.0).abs() < tol_cl,
            "CL at h/b {hb}: {} vs {cl}",
            p.cl
        );
        assert!(
            (p.cd_induced / cdi - 1.0).abs() < tol_cdi,
            "CDi at h/b {hb}: {} vs {cdi}",
            p.cd_induced
        );
        assert!(p.cl > cl0 && p.cd_induced < cdi0 * (p.cl / cl0).powi(2));
        assert!(p.cl_alpha_per_rad > free.cl_alpha_per_rad);
    }
    // KNOWN LIMIT (documented in the module): at h/b = 0.05 (h/c = 0.4) the
    // image-bound-vortex term raises CL by 38 % in the VLM; the lifting-line
    // correction predicts about 12 %.
    let (cl, _) = lattice_pair(Some(0.05 * SPAN), 3.0);
    let ge = GroundEffect {
        model: GroundModel::Raymer,
        height_m: 0.05 * SPAN,
        span_m: SPAN,
    };
    let p = ge.at_fixed_alpha(&free).unwrap();
    assert!(p.cl / cl < 0.9);
}

#[test]
fn fixed_cl_drag_is_phi_times_free_and_far_away_is_free_air() {
    let ge = GroundEffect {
        model: GroundModel::Raymer,
        height_m: 0.1 * SPAN,
        span_m: SPAN,
    };
    let phi = ge.phi().unwrap();
    assert!((ge.cd_induced_at_fixed_cl(0.01).unwrap() - 0.01 * phi).abs() < 1e-15);
    let (_, _, free) = free_state();
    let far = GroundEffect {
        model: GroundModel::Raymer,
        height_m: 1.0e4,
        span_m: SPAN,
    };
    let p = far.at_fixed_alpha(&free).unwrap();
    assert!(
        (p.cl / free.cl - 1.0).abs() < 1e-6 && (p.cd_induced / free.cd_induced - 1.0).abs() < 1e-6
    );
    // A slope at or above the lifting-line bound pi A e is refused.
    let mut bad = free;
    bad.cl_alpha_per_rad = 100.0;
    assert!(ge.at_fixed_alpha(&bad).is_err());
}
