// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Tests of the optional upper-arc cross-section residual.

use std::f64::consts::PI;

use alas_config::loft::{ResidualSpec, SplineSpec};

use super::section::Section;
use super::tests::{mesh_area, rel, shaped_spec};
use super::{Loft, LoftError, LoftSpec, ResidualDefect};

/// Knot vector of `shaped_spec` (quintic, 8 coefficients).
fn shaped_knots() -> Vec<f64> {
    vec![
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.3, 0.6, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
    ]
}

/// A residual coefficient curve that is zero outside `u` in 0.2 .. 0.7: knots
/// every 0.05 there, the six coefficients 6..12 free.
fn rterm(c: [f64; 6]) -> SplineSpec {
    let mut knots = vec![0.0; 6];
    knots.extend((0..11).map(|i| 0.2 + 0.05 * f64::from(i)));
    knots.extend(vec![1.0; 6]);
    let mut coeffs = vec![0.0; 17];
    coeffs[6..12].copy_from_slice(&c);
    SplineSpec { knots, coeffs }
}

fn term(c: [f64; 8]) -> SplineSpec {
    SplineSpec {
        knots: shaped_knots(),
        coeffs: c.to_vec(),
    }
}

/// `shaped_spec` with a smooth residual that is zero for x < 4 m and x > 14 m.
fn residual_spec() -> LoftSpec {
    let mut s = shaped_spec();
    s.residual = Some(ResidualSpec {
        terms: vec![
            rterm([0.0, -0.1, -0.25, -0.2, -0.1, 0.0]),
            rterm([0.0, 0.05, 0.15, 0.1, 0.05, 0.0]),
            rterm([0.0, 0.0, -0.1, -0.1, 0.0, 0.0]),
            rterm([0.0, -0.1, -0.25, -0.25, -0.1, 0.0]),
        ],
    });
    s
}

fn plain_section() -> Section {
    Section::new(3.2, 3.6, 0.7, 2.3, 0.08)
}

fn residual_section() -> Section {
    plain_section().with_residual([-0.15, 0.1, -0.05, -0.25])
}

/// Area of the polygon through the boundary, shoelace.
fn polygon_area(s: &Section, m: usize) -> f64 {
    let pts: Vec<(f64, f64)> = (0..m)
        .map(|i| s.point_at_angle(2.0 * PI * i as f64 / m as f64))
        .collect();
    let mut twice = 0.0;
    for i in 0..m {
        let (y0, z0) = pts[i];
        let (y1, z1) = pts[(i + 1) % m];
        twice += y0 * z1 - y1 * z0;
    }
    0.5 * twice.abs()
}

#[test]
fn zero_residual_is_bit_identical_to_none() {
    let plain = Loft::from_reference(&shaped_spec()).unwrap();
    let mut spec = shaped_spec();
    spec.residual = Some(ResidualSpec {
        terms: vec![term([0.0; 8]), term([0.0; 8])],
    });
    let zero = Loft::from_reference(&spec).unwrap();
    assert_eq!(plain.wetted_area().to_bits(), zero.wetted_area().to_bits());
    assert_eq!(plain.volume().to_bits(), zero.volume().to_bits());
    for i in 0..=80 {
        let x = 20.0 * f64::from(i) / 80.0;
        assert_eq!(plain.section(x), zero.section(x));
        assert_eq!(plain.s_of_x(x).to_bits(), zero.s_of_x(x).to_bits());
        assert_eq!(plain.ds_dx(x).to_bits(), zero.ds_dx(x).to_bits());
        assert_eq!(
            plain.half_width_at(x, 1.5).to_bits(),
            zero.half_width_at(x, 1.5).to_bits()
        );
    }
    // A section with zero coefficients is the plain section, bit for bit.
    let s = plain_section();
    let z = s.with_residual([0.0; 4]);
    assert_eq!(s.area().to_bits(), z.area().to_bits());
    assert_eq!(s.perimeter().to_bits(), z.perimeter().to_bits());
}

#[test]
fn residual_area_agrees_with_a_dense_polygon_and_the_perimeter_with_its_length() {
    for (n, k) in [(2.0, 0.0), (2.3, 0.08), (2.6, -0.1)] {
        let s = Section::new(3.2, 3.6, 0.7, n, k).with_residual([-0.15, 0.1, -0.05, -0.25]);
        let poly = polygon_area(&s, 200_000);
        assert!(
            (s.area() - poly).abs() < 2e-5 * poly,
            "n {n} k {k}: {} vs {poly}",
            s.area()
        );
        // The perimeter against the polygon length.
        let m = 200_000;
        let pts: Vec<(f64, f64)> = (0..=m)
            .map(|i| s.point_at_angle(2.0 * PI * i as f64 / m as f64))
            .collect();
        let len: f64 = pts
            .windows(2)
            .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
            .sum();
        assert!(
            (s.perimeter() - len).abs() < 2e-5 * len,
            "n {n} k {k}: {} vs {len}",
            s.perimeter()
        );
    }
}

#[test]
fn residual_area_matches_the_closed_form_for_a_pure_quadratic_in_the_circle_limit() {
    // n = 2, k = 0: the section is a circle of radius r; with b3 only,
    // R(t) = b3 t^6 and the added area is 2 r^2 b3 int_0^1 sqrt(1 - t^2) t^6 dt
    // = 2 r^2 b3 (5 pi / 256).
    let r = 2.0;
    let b3 = -0.3;
    let s = Section::new(r, r, 0.0, 2.0, 0.0).with_residual([0.0, 0.0, 0.0, b3]);
    let want = PI * r * r + 2.0 * r * r * b3 * 5.0 * PI / 256.0;
    assert!(rel(s.area(), want) < 1e-11, "{} vs {want}", s.area());
}

#[test]
fn residual_leaves_crown_keel_widest_point_and_the_lower_arc_unchanged() {
    let (p, r) = (plain_section(), residual_section());
    assert_eq!(p.crown_z(), r.crown_z());
    assert_eq!(p.keel_z(), r.keel_z());
    assert_eq!(p.shoulder_z(), r.shoulder_z());
    assert!((r.half_width_at(r.shoulder_z()) - r.a).abs() < 1e-12);
    for i in 0..=20 {
        let z = r.keel_z() + (r.shoulder_z() - r.keel_z()) * f64::from(i) / 20.0;
        assert_eq!(p.half_width_at(z).to_bits(), r.half_width_at(z).to_bits());
    }
    // Above the shoulder the arc differs, the factor is 1 + R.
    let z = r.shoulder_z() + 0.8 * (r.crown_z() - r.shoulder_z());
    assert!(r.half_width_at(z) < p.half_width_at(z));
    // The inverse half-width reproduces boundary points of the angle map.
    for phi in [0.2, 0.7, 1.2, 1.5, 2.0, 2.9, 3.6, 5.0] {
        let (y, z) = r.point_at_angle(phi);
        assert!((r.half_width_at(z) - y.abs()).abs() < 1e-9, "phi {phi}");
    }
    assert!(r.check_residual().is_ok());
}

#[test]
fn residual_keeps_the_circumferential_curvature_continuous_at_the_shoulder() {
    // Curvature of the outline just above and below the widest point, from
    // the angle map: the jump across the shoulder must vanish with the
    // distance to it (R, R', R'' are zero there), here by a factor of
    // about three per quarter of the distance.
    let curvature = |s: &Section, phi: f64| {
        let h = 1e-5;
        let p = |t: f64| s.point_at_angle(t);
        let (a, b, c) = (p(phi - h), p(phi), p(phi + h));
        let d1 = ((c.0 - a.0) / (2.0 * h), (c.1 - a.1) / (2.0 * h));
        let d2 = (
            (c.0 - 2.0 * b.0 + a.0) / (h * h),
            (c.1 - 2.0 * b.1 + a.1) / (h * h),
        );
        (d1.0 * d2.1 - d1.1 * d2.0) / d1.0.hypot(d1.1).powi(3)
    };
    let r = Section::new(3.2, 3.6, 0.7, 2.3, 0.08).with_residual([-0.3, 0.15, 0.1, -0.25]);
    let jump = |eps: f64| (curvature(&r, eps) - curvature(&r, -eps)).abs();
    let (j1, j2, j3) = (jump(4e-3), jump(1e-3), jump(2.5e-4));
    // zeta ~ phi^(2/n): a quarter of the distance gives a factor 4^(2/n) = 3.3.
    assert!(j2 < j1 / 2.8 && j3 < j2 / 2.8, "jumps {j1} {j2} {j3}");
}

#[test]
fn residual_check_rejects_a_pinched_or_overwide_upper_arc() {
    // Strong narrowing in the middle of the arc and a bulge on top: concave.
    let dent = plain_section().with_residual([-0.6, 0.0, 1.5, -0.5]);
    assert!(matches!(
        dent.check_residual(),
        Err(ResidualDefect::NotConvex { .. } | ResidualDefect::WiderThanShoulder { .. })
    ));
    // Wider than the shoulder.
    let wide = Section::new(3.0, 3.0, 0.0, 2.0, 0.0).with_residual([0.5, 0.5, 0.0, 0.0]);
    assert!(matches!(
        wide.check_residual(),
        Err(ResidualDefect::WiderThanShoulder { .. })
    ));
    // A concave notch: narrow lower part of the arc, wide top.
    let notch = Section::new(3.0, 3.0, 0.0, 2.0, 0.0).with_residual([-0.9, -0.8, 0.0, 0.0]);
    assert!(matches!(
        notch.check_residual(),
        Err(ResidualDefect::NotConvex { .. })
    ));
}

#[test]
fn loft_with_a_residual_changes_only_the_hump_region() {
    let plain = Loft::from_reference(&shaped_spec()).unwrap();
    let res = Loft::from_reference(&residual_spec()).unwrap();
    for i in 0..=100 {
        let x = 20.0 * f64::from(i) / 100.0;
        // Side and plan silhouettes (crown, keel, half-width a) are untouched.
        assert_eq!(plain.crown_z(x).to_bits(), res.crown_z(x).to_bits());
        assert_eq!(plain.keel_z(x).to_bits(), res.keel_z(x).to_bits());
        assert_eq!(plain.section(x).a.to_bits(), res.section(x).a.to_bits());
        // Outside the residual region the section area is bit-identical.
        if !(4.0..=14.0).contains(&x) {
            assert_eq!(plain.s_of_x(x).to_bits(), res.s_of_x(x).to_bits(), "x {x}");
        }
    }
    // The upper arc is narrower in the middle of the region.
    let x = 9.0;
    let s = res.section(x);
    let z = s.shoulder_z() + 0.85 * (s.crown_z() - s.shoulder_z());
    assert!(res.half_width_at(x, z) < plain.half_width_at(x, z) - 0.01);
}

#[test]
fn loft_residual_wetted_area_volume_and_area_ruling_are_exact() {
    let loft = Loft::from_reference(&residual_spec()).unwrap();
    let exact = loft.wetted_area();
    let fine = loft.lateral_area_with(24, 48);
    assert!(rel(exact, fine) < 1e-8, "12x24 {exact} vs 24x48 {fine}");
    let mesh = mesh_area(&loft, 1500, 720);
    assert!(rel(exact, mesh) < 3e-4, "quadrature {exact} vs mesh {mesh}");
    // Volume against a trapezoid of S(x) from the dense polygon area.
    let m = 4_000;
    let (x0, x1) = loft.x_range();
    let mut v = 0.0;
    for i in 0..m {
        let xa = x0 + (x1 - x0) * i as f64 / m as f64;
        let xb = x0 + (x1 - x0) * (i + 1) as f64 / m as f64;
        v += 0.5 * (loft.s_of_x(xa) + loft.s_of_x(xb)) * (xb - xa);
    }
    assert!(rel(loft.volume(), v) < 1e-5, "{} vs {v}", loft.volume());
    // S(x) of a station against the shoelace area of its polygon.
    for x in [4.5, 7.0, 9.0, 12.5] {
        let poly = polygon_area(&loft.section(x), 100_000);
        assert!(rel(loft.s_of_x(x), poly) < 2e-5, "x {x}");
    }
    // dS/dx against a central difference.
    for x in [1.0, 3.0, 6.2, 9.9, 12.5, 17.5] {
        let h = 1e-5;
        let fd = (loft.s_of_x(x + h) - loft.s_of_x(x - h)) / (2.0 * h);
        let an = loft.ds_dx(x);
        assert!(
            (fd - an).abs() < 2e-6 * (1.0 + an.abs()),
            "x {x}: {fd} vs {an}"
        );
    }
    // The residual removes volume and skin relative to the plain body.
    let plain = Loft::from_reference(&shaped_spec()).unwrap();
    assert!(loft.volume() < plain.volume() * (1.0 - 1e-3));
    assert!(loft.wetted_area() < plain.wetted_area() * (1.0 - 1e-4));
}

#[test]
fn loft_refuses_a_non_convex_or_collapsing_residual() {
    let mut spec = residual_spec();
    spec.residual = Some(ResidualSpec {
        terms: vec![
            rterm([0.0, -0.9, -0.9, -0.9, -0.9, 0.0]),
            rterm([0.0, -0.8, -0.8, -0.8, -0.8, 0.0]),
        ],
    });
    assert!(matches!(
        Loft::from_reference(&spec),
        Err(LoftError::Residual { .. })
    ));
    // Too many coefficient curves are a specification error.
    let mut spec = residual_spec();
    if let Some(r) = spec.residual.as_mut() {
        r.terms.push(rterm([0.0; 6]));
    }
    assert!(matches!(
        Loft::from_reference(&spec),
        Err(LoftError::Spec(_))
    ));
}

#[test]
fn legacy_view_keeps_the_section_area_with_a_residual() {
    let loft = Loft::from_reference(&residual_spec()).unwrap();
    for st in loft.sample_xsecs(41) {
        let s = Section::new(0.5 * st.width, 0.5 * st.height, st.xyz_c[2], st.shape, 0.0);
        if s.a > 0.0 && s.e > 0.0 {
            let want = loft.s_of_x(st.xyz_c[0]);
            assert!(rel(s.area(), want) < 1e-9, "x {}", st.xyz_c[0]);
        }
    }
}
