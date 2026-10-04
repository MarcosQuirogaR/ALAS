// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The logarithmic-kernel integrals of the Trefftz-plane energy over straight
//! wake segments: the inner integral in closed form, the outer one by
//! Gauss-Legendre quadrature (see the parent module doc).

use super::{length, lerp, Segment};

/// Eight-point Gauss-Legendre abscissae on `[-1, 1]` (positive half;
/// Abramowitz and Stegun, *Handbook of Mathematical Functions*, 1964,
/// Table 25.4).
const GAUSS_ABSCISSAE: [f64; 4] = [
    0.183_434_642_495_649_8,
    0.525_532_409_916_329,
    0.796_666_477_413_626_7,
    0.960_289_856_497_536_2,
];

/// Weights of [`GAUSS_ABSCISSAE`] (same table).
const GAUSS_WEIGHTS: [f64; 4] = [
    0.362_683_783_378_362,
    0.313_706_645_877_887_3,
    0.222_381_034_453_374_5,
    0.101_228_536_290_376_3,
];

/// Sub-interval boundaries, as fractions of a segment, that grade the outer
/// quadrature geometrically (ratio 4) towards an end where the inner
/// integral has a logarithmic derivative singularity.
const GRADED_FRACTIONS: [f64; 5] = [0.0, 1.0 / 64.0, 1.0 / 16.0, 1.0 / 4.0, 1.0];

/// `int_0^L int_0^L ln|s - s'| ds ds' = L^2 (ln L - 3/2)`.
pub(super) fn self_log_integral(segment_length: f64) -> f64 {
    if segment_length <= 0.0 {
        return 0.0;
    }
    segment_length * segment_length * (segment_length.ln() - 1.5)
}

/// `int_a int_b ln|r - r'| ds' ds` for two distinct segments: the inner
/// integral over `b` in closed form, the outer over `a` by graded
/// Gauss-Legendre quadrature.
pub(super) fn pair_log_integral(a: &Segment, b: &Segment) -> f64 {
    let length_a = length(a.start, a.end);
    if length_a <= 0.0 {
        return 0.0;
    }
    let touches_start = point_near_segment(a.start, b, length_a);
    let touches_end = point_near_segment(a.end, b, length_a);
    let mut total = 0.0;
    let mut integrate = |t0: f64, t1: f64| {
        let half = 0.5 * (t1 - t0);
        let centre = 0.5 * (t1 + t0);
        for (&x, &w) in GAUSS_ABSCISSAE.iter().zip(&GAUSS_WEIGHTS) {
            for t in [centre - half * x, centre + half * x] {
                let p = lerp(a.start, a.end, t);
                total += w * half * length_a * point_log_integral(p, b);
            }
        }
    };
    // Grade towards whichever end of `a` lies on or beside `b`; the inner
    // integral's derivative is logarithmically singular there.
    match (touches_start, touches_end) {
        (false, false) => integrate(0.0, 1.0),
        (true, false) => graded(&mut integrate, false),
        (false, true) => graded(&mut integrate, true),
        (true, true) => {
            let mut halves = |t0: f64, t1: f64| integrate(0.5 * t0, 0.5 * t1);
            graded(&mut halves, false);
            let mut halves = |t0: f64, t1: f64| integrate(0.5 + 0.5 * t0, 0.5 + 0.5 * t1);
            graded(&mut halves, true);
        }
    }
    total
}

/// Apply `integrate` over `[0, 1]` split at [`GRADED_FRACTIONS`], refined
/// towards `t = 0`, or towards `t = 1` when `towards_end`.
fn graded(integrate: &mut impl FnMut(f64, f64), towards_end: bool) {
    for pair in GRADED_FRACTIONS.windows(2) {
        if towards_end {
            integrate(1.0 - pair[1], 1.0 - pair[0]);
        } else {
            integrate(pair[0], pair[1]);
        }
    }
}

/// Whether `p` lies within a quarter of `reference` of segment `b`.
fn point_near_segment(p: [f64; 2], b: &Segment, reference: f64) -> bool {
    let d = [b.end[0] - b.start[0], b.end[1] - b.start[1]];
    let length_sq = d[0] * d[0] + d[1] * d[1];
    let t = if length_sq > 0.0 {
        (((p[0] - b.start[0]) * d[0] + (p[1] - b.start[1]) * d[1]) / length_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    length(p, lerp(b.start, b.end, t)) <= 0.25 * reference
}

/// `int_b ln|p - r'| ds'` in closed form: in the segment's own frame, with
/// `u` the along-segment and `h` the normal coordinate of `p`,
/// `F(x) = x ln sqrt(x^2 + h^2) - x + h atan(x / h)` between `x = -u` and
/// `x = L - u`.
fn point_log_integral(p: [f64; 2], b: &Segment) -> f64 {
    let segment_length = length(b.start, b.end);
    if segment_length <= 0.0 {
        return 0.0;
    }
    let tangent = [
        (b.end[0] - b.start[0]) / segment_length,
        (b.end[1] - b.start[1]) / segment_length,
    ];
    let offset = [p[0] - b.start[0], p[1] - b.start[1]];
    let u = offset[0] * tangent[0] + offset[1] * tangent[1];
    let h = offset[1] * tangent[0] - offset[0] * tangent[1];
    antiderivative(segment_length - u, h) - antiderivative(-u, h)
}

/// The antiderivative `F(x)` of [`point_log_integral`], continuous through
/// `x = 0` and `h = 0` (`x ln|x| -> 0`, `h atan(x / h) -> 0`).
fn antiderivative(x: f64, h: f64) -> f64 {
    let radius_sq = x * x + h * h;
    let log_term = if x == 0.0 || radius_sq == 0.0 {
        0.0
    } else {
        0.5 * x * radius_sq.ln()
    };
    let angle_term = if h == 0.0 { 0.0 } else { h * (x / h).atan() };
    log_term - x + angle_term
}
