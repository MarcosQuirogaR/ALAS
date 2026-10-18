// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A failed unwrap or expect in a test is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::f64::consts::PI;
use std::sync::OnceLock;

use alas_config::engines::NacelleShapeV2;

use super::nacelle::{Body, NacelleBodyKind, NacelleError, NacelleLoft};
use super::nacelle_pieces::Piece;
use super::Section;

fn a320() -> NacelleShapeV2 {
    NacelleShapeV2 {
        highlight_radius_ratio: 0.93,
        cowl_cylinder_end_ratio: 0.5,
        fan_cowl_length_m: 3.2,
        fan_nozzle_radius_ratio: 0.8,
        core_cowl_length_m: 4.5,
        core_nozzle_radius_ratio: 0.37,
        lip_length_ratio: 0.4,
        lip_exponent: 2.2,
        boat_tail_angle_deg: 11.0,
        te_thickness_m: 0.02,
        plug_tip_radius_m: 0.0408,
        ellipticity: 1.0,
        droop_m: 0.0,
        provenance: Vec::new(),
    }
}

fn a320_loft() -> NacelleLoft {
    NacelleLoft::new(&a320(), 5.05, 1.02).unwrap()
}

/// A loft whose bodies are replaced by `pieces` (one body), for analytic checks.
fn custom(pieces: Vec<Piece>, ellipticity: f64) -> NacelleLoft {
    let mut shape = a320();
    shape.ellipticity = ellipticity;
    let mut loft = NacelleLoft::new(&shape, 5.05, 1.02).unwrap();
    loft.bodies = vec![Body {
        kind: NacelleBodyKind::FanCowl,
        pieces,
        rising: 0,
    }];
    loft.integrals = OnceLock::new();
    loft
}

#[test]
fn the_profile_is_g2_at_every_joint_and_g0_at_the_fan_nozzle_step() {
    for shape_length in [(a320(), 5.05, 1.02), (a320_other(), 8.5, 1.83)] {
        let loft = NacelleLoft::new(&shape_length.0, shape_length.1, shape_length.2).unwrap();
        let mut joints = 0;
        for body in 0..loft.body_count() {
            for (_, m0, m1, c0, c1) in loft.joints(body) {
                assert!((m0 - m1).abs() < 1e-9, "slope jump {m0} vs {m1}");
                assert!((c0 - c1).abs() < 1e-8, "curvature jump {c0} vs {c1}");
                joints += 1;
            }
            // Position continuity inside the body.
            let (x0, x1) = loft.body_x_range(body).unwrap();
            let pieces = &loft.bodies[body].pieces;
            for pair in pieces.windows(2) {
                let (a, b) = (pair[0].eval(1.0), pair[1].eval(0.0));
                assert!((a.x - b.x).abs() < 1e-12 && (a.r - b.r).abs() < 1e-12);
            }
            assert!(x1 > x0);
        }
        assert!(joints >= 4);
        // The step: same x, radius drops from the cowl trailing edge to the core cowl.
        assert_eq!(loft.body_count(), 2);
        let (_, fan_end) = loft.body_x_range(0).unwrap();
        let (core_start, _) = loft.body_x_range(1).unwrap();
        assert!((fan_end - core_start).abs() < 1e-12);
        let r_fan = loft.radius_at(0, fan_end).unwrap();
        let r_core = loft.radius_at(1, core_start).unwrap();
        assert!(r_core < r_fan - 0.1);
        assert!(loft.fan_exit_annulus_area_m2() > 0.0);
    }
}

fn a320_other() -> NacelleShapeV2 {
    NacelleShapeV2 {
        highlight_radius_ratio: 0.93,
        cowl_cylinder_end_ratio: 0.6,
        fan_cowl_length_m: 5.35,
        fan_nozzle_radius_ratio: 0.88,
        core_cowl_length_m: 6.1,
        core_nozzle_radius_ratio: 0.38,
        lip_exponent: 2.0,
        boat_tail_angle_deg: 9.0,
        plug_tip_radius_m: 0.073,
        te_thickness_m: 0.03,
        ellipticity: 1.0,
        ..a320()
    }
}

#[test]
fn the_lip_has_a_vertical_tangent_at_the_highlight_and_leaves_it_at_the_highlight_radius() {
    for n in [2.0, 2.2, 3.0] {
        let mut shape = a320();
        shape.lip_exponent = n;
        let loft = NacelleLoft::new(&shape, 5.05, 1.02).unwrap();
        let lip = loft.bodies[0].pieces[0];
        let p0 = lip.eval(0.0);
        assert_eq!(p0.x, 0.0);
        assert!((p0.r - 0.93 * 1.02).abs() < 1e-15);
        assert_eq!(p0.dx, 0.0, "dx/dt must vanish at the highlight (n = {n})");
        assert!(p0.dr > 0.0);
        let p = lip.eval(1.0e-6);
        assert!(p.dr / p.dx > 1.0e3, "slope {} at n = {n}", p.dr / p.dx);
        assert!((loft.radius_at(0, 0.0).unwrap() - p0.r).abs() < 1e-6);
    }
}

#[test]
fn a_cylinder_has_the_analytic_area_and_volume() {
    let (r, l) = (1.5, 7.0);
    let loft = custom(vec![Piece::cylinder(0.0, l, r)], 1.0);
    assert!((loft.wetted_area_m2() / (2.0 * PI * r * l) - 1.0).abs() < 1e-13);
    assert!((loft.volume_m3() / (PI * r * r * l) - 1.0).abs() < 1e-13);
    // Elliptic cylinder: perimeter of the section family times length.
    let loft = custom(vec![Piece::cylinder(0.0, l, r)], 1.2);
    let perimeter = Section::new(r, 1.2 * r, 0.0, 2.0, 0.0).perimeter();
    assert!((loft.wetted_area_m2() / (perimeter * l) - 1.0).abs() < 1e-10);
    assert!((loft.volume_m3() / (PI * r * 1.2 * r * l) - 1.0).abs() < 1e-13);
}

#[test]
fn a_cone_has_the_analytic_area_and_volume() {
    let (r0, r1, l) = (2.0, 0.5, 6.0);
    let slope = (r1 - r0) / l;
    let loft = custom(
        vec![Piece::hermite(1.0, l, r0, slope, 0.0, r1, slope, 0.0)],
        1.0,
    );
    let area = PI * (r0 + r1) * (l * l + (r0 - r1).powi(2)).sqrt();
    let volume = PI * l * (r0 * r0 + r0 * r1 + r1 * r1) / 3.0;
    assert!((loft.wetted_area_m2() / area - 1.0).abs() < 1e-13);
    assert!((loft.volume_m3() / volume - 1.0).abs() < 1e-13);
}

#[test]
fn the_exact_skin_matches_a_fine_frustum_sum_of_the_same_profile() {
    let loft = a320_loft();
    let mut frustum = 0.0;
    for body in 0..loft.body_count() {
        let pts = loft.body_sections(body, 40);
        for pair in pts.windows(2) {
            let (x0, s0) = pair[0];
            let (x1, s1) = pair[1];
            frustum += PI * (s0.a + s1.a) * ((x1 - x0).powi(2) + (s1.a - s0.a).powi(2)).sqrt();
        }
    }
    assert!(
        (loft.wetted_area_m2() / frustum - 1.0).abs() < 2e-5,
        "{} vs {frustum}",
        loft.wetted_area_m2()
    );
    assert!(
        loft.wetted_area_m2() > 20.0 && loft.wetted_area_m2() < 32.0,
        "{}",
        loft.wetted_area_m2()
    );
    assert!(loft.volume_m3() > 0.0);
    assert!(loft.trailing_edge_ring_area_m2() > 0.0 && loft.plug_tip_area_m2() > 0.0);
}

#[test]
fn ellipticity_scales_the_section_height_and_leaves_the_plan_width() {
    let mut shape = a320();
    shape.ellipticity = 1.1;
    let loft = NacelleLoft::new(&shape, 5.05, 1.02).unwrap();
    let s = loft.section_at(0, 1.0).unwrap();
    assert!((s.a - 1.02).abs() < 1e-12 && (s.e - 1.1 * 1.02).abs() < 1e-12);
    assert!(loft.wetted_area_m2() > a320_loft().wetted_area_m2());
}

#[test]
fn an_impossible_boat_tail_is_rejected() {
    let mut shape = a320();
    shape.boat_tail_angle_deg = 44.0;
    assert!(matches!(
        NacelleLoft::new(&shape, 5.05, 1.02),
        Err(NacelleError::NotMonotone(_))
    ));
    shape.boat_tail_angle_deg = 11.0;
    shape.highlight_radius_ratio = 1.0;
    assert!(matches!(
        NacelleLoft::new(&shape, 5.05, 1.02),
        Err(NacelleError::Invalid { .. })
    ));
}

#[test]
fn every_catalogue_v2_nacelle_builds_and_matches_its_legacy_numbers() {
    let mut seen = 0;
    for engine in alas_config::engines::database() {
        let Some(v2) = engine.nacelle_shape_v2.as_ref() else {
            continue;
        };
        seen += 1;
        if let Some(legacy) = engine.nacelle_shape.as_ref() {
            let l = v2.legacy();
            assert!(
                (l.highlight_radius_ratio - legacy.highlight_radius_ratio).abs() <= 0.06,
                "{}",
                engine.name
            );
            let same = |a: &alas_config::engines::NacelleShape| {
                (
                    a.cowl_cylinder_end_ratio,
                    a.fan_cowl_length_m,
                    a.fan_nozzle_radius_ratio,
                    a.core_cowl_length_m,
                    a.core_nozzle_radius_ratio,
                )
            };
            assert_eq!(same(&l), same(legacy), "{}", engine.name);
        }
        assert!(!v2.provenance.is_empty(), "{}", engine.name);
        let loft = NacelleLoft::new(v2, engine.nacelle_length_m, engine.nacelle_max_radius_m)
            .unwrap_or_else(|e| panic!("{}: {e}", engine.name));
        let (_, end) = loft.body_x_range(loft.body_count() - 1).unwrap();
        assert!(
            (end - engine.nacelle_length_m).abs() < 1e-9,
            "{}",
            engine.name
        );
        let widest = loft
            .outline(2)
            .iter()
            .flatten()
            .map(|p| p.1)
            .fold(0.0, f64::max);
        assert!(
            (widest - engine.nacelle_max_radius_m).abs() < 1e-9,
            "{}",
            engine.name
        );
        assert!(loft.wetted_area_m2().is_finite());
    }
    assert!(seen >= 10);
}

#[test]
fn the_dc10_pod_is_a_single_body_of_the_stated_size_in_the_same_family() {
    let pod = alas_config::CentreEngineInstallation {
        axis_z_m: 4.85,
        axis_pitch_deg: 4.0,
        width_m: 2.85,
        height_m: 3.0,
        profile: vec![
            (0.0, 0.93),
            (0.35, 0.99),
            (0.8, 1.0),
            (11.9, 1.0),
            (14.15, 0.25),
        ],
    };
    let loft = crate::builder::centre_pod_nacelle_loft(&pod).expect("pod in the family");
    assert_eq!(loft.body_count(), 1);
    let mid = loft.section_at(0, 6.0).unwrap();
    assert!((2.0 * mid.a - 2.85).abs() < 1e-12);
    assert!((2.0 * mid.e - 3.0).abs() < 1e-12);
    let (_, end) = loft.body_x_range(0).unwrap();
    assert!((end - 14.15).abs() < 1e-9);
    let tail = loft.section_at(0, 14.15).unwrap();
    assert!((tail.a - 0.25 * 1.425).abs() < 1e-6);
}
