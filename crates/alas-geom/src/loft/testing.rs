// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Builders of analytic test bodies (sphere, spheroid, cylinder, cone, frustum).

use alas_config::loft::{
    BodyCurves, CabinStations, Closure, FairnessBudget, FitRecord, InflectionBudget, SplineSpec,
    LOFT_SCHEMA_VERSION,
};
use alas_config::LoftSpec;
use alas_math::spline_fit::{fit_penalised, FitSample};

/// A single-span spline of `degree` equal to the polynomial `sum c_i u^i`
/// (degree at most `degree`), exact to rounding.
pub(super) fn poly(coeffs: &[f64], degree: usize) -> SplineSpec {
    assert!(coeffs.len() <= degree + 1);
    let mut knots = vec![0.0; degree + 1];
    knots.extend(vec![1.0; degree + 1]);
    let samples: Vec<FitSample> = (0..=40)
        .map(|i| {
            let u = f64::from(i) / 40.0;
            let y = coeffs.iter().rev().fold(0.0, |acc, c| acc * u + c);
            FitSample {
                x: u,
                y,
                weight: 1.0,
            }
        })
        .collect();
    let out = fit_penalised(knots, degree, &samples, &[], 0.0, 3).unwrap();
    SplineSpec {
        knots: out.spline.knots().to_vec(),
        coeffs: out.spline.coeffs().to_vec(),
    }
}

/// Polynomial inputs of a body, ascending powers of `u = x / length`.
pub(super) struct Polys<'a> {
    pub a: &'a [f64],
    pub e: &'a [f64],
    pub zc: &'a [f64],
    pub n: &'a [f64],
    pub k: &'a [f64],
}

/// A specification of the given polynomial curves.
pub(super) fn spec(
    length: f64,
    degree: u8,
    p: &Polys<'_>,
    front: Closure,
    rear: Closure,
) -> LoftSpec {
    let d = usize::from(degree);
    LoftSpec {
        schema: LOFT_SCHEMA_VERSION,
        preset: "test".to_owned(),
        body: "fuselage".to_owned(),
        reference_length_m: length,
        degree,
        frame: String::new(),
        curves: BodyCurves {
            a: poly(p.a, d),
            e: poly(p.e, d),
            zc: poly(p.zc, d),
            n: poly(p.n, d),
            k: poly(p.k, d),
        },
        front,
        rear,
        cabin: CabinStations {
            x_start_m: 0.0,
            x_end_m: length,
        },
        fairness: FairnessBudget {
            inflections: InflectionBudget {
                crown: 4,
                keel: 4,
                plan: 4,
            },
            tv_ratio_max: 50.0,
        },
        fit: FitRecord::default(),
        provenance: Vec::new(),
        residual: None,
    }
}

/// Sphere of radius `r`: `A = E = 4 r^2 u (1 - u)`, length `2 r`.
pub(super) fn sphere(r: f64, degree: u8) -> LoftSpec {
    let s = [0.0, 4.0 * r * r, -4.0 * r * r];
    spec(
        2.0 * r,
        degree,
        &Polys {
            a: &s,
            e: &s,
            zc: &[r],
            n: &[2.0],
            k: &[0.0],
        },
        Closure::Point { tip_radius_m: r },
        Closure::Point { tip_radius_m: r },
    )
}

/// Spheroid with equatorial radius `b` and polar semi-axis `c` along x.
pub(super) fn spheroid(b: f64, c: f64) -> LoftSpec {
    let s = [0.0, 4.0 * b * b, -4.0 * b * b];
    spec(
        2.0 * c,
        3,
        &Polys {
            a: &s,
            e: &s,
            zc: &[b],
            n: &[2.0],
            k: &[0.0],
        },
        Closure::Point {
            tip_radius_m: b * b / c,
        },
        Closure::Point {
            tip_radius_m: b * b / c,
        },
    )
}

/// Constant section of half-extents `a`, `e`, exponent `n`, shoulder `k`.
pub(super) fn cylinder(length: f64, a: f64, e: f64, n: f64, k: f64) -> LoftSpec {
    spec(
        length,
        3,
        &Polys {
            a: &[a * a],
            e: &[e * e],
            zc: &[e],
            n: &[n],
            k: &[k],
        },
        Closure::Base,
        Closure::Base,
    )
}

/// Cone or frustum of radius `r0` at the nose and `r1` at the tail.
pub(super) fn frustum(length: f64, r0: f64, r1: f64) -> LoftSpec {
    let d = r1 - r0;
    let q = [r0 * r0, 2.0 * r0 * d, d * d];
    spec(
        length,
        3,
        &Polys {
            a: &q,
            e: &q,
            zc: &[r1.max(r0)],
            n: &[2.0],
            k: &[0.0],
        },
        Closure::Base,
        Closure::Base,
    )
}
