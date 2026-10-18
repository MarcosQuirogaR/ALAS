// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fairness diagnostics of a [`Loft`] (design 2.5).
//!
//! Four profile lines are examined: the crown `z = zc + e`, the keel
//! `z = zc - e`, the plan half-width `y = a`, and the shoulder line
//! `z = zc + k e`, each as a planar curve `f(x)` with signed curvature
//! `kappa = f'' / (1 + f'^2)^(3/2)`. The derivatives of `f` come analytically
//! from the spline jets; the metrics below are evaluated on 2000 stations
//! clustered towards both ends, where a point closure makes `f'` unbounded
//! (`kappa` stays finite). The change of curvature per unit arc length is
//! taken from consecutive samples, so the figures are diagnostics rather than
//! quadrature-grade energies.
//!
//! * `inflections`: sign changes of `kappa` between samples with
//!   `|kappa| * sqrt(a e)^(1/2) > 0.02` (radius of curvature below 50 local
//!   half-sizes; the constant cabin has `kappa = 0`).
//! * `tv_ratio`: total variation of `kappa` over the smallest total variation
//!   a profile with the same end values and the same extreme values can have
//!   (visit the two global extremes once each). 1 means at most two clean
//!   lobes; wiggles raise it.
//! * `energy`: the sampled `integral (d kappa / d s)^2 ds`, the quantity the
//!   fit penalty stands in for.
//! * `knot_jump`: the largest difference of `kappa` between the two sides of
//!   a knot, from the one-sided spline pieces. A G2 body has zero (to 1e-9).

use super::curves::{Jets, Side};
use super::Loft;

/// Stations used for the sampled metrics.
const SAMPLES: usize = 2000;
/// Dimensionless curvature (curvature times local half-size) below which a
/// sample has no sign: radius of curvature above 50 half-sizes.
const SIGN_THRESHOLD: f64 = 0.02;
/// Floor of the local half-size used for that scale, m.
const LOCAL_SCALE_FLOOR_M: f64 = 0.25;
/// Relative swing of the curvature below which an extremum is not counted.
const EXTREMUM_THRESHOLD: f64 = 2e-3;

#[derive(Clone, Copy)]
enum Line {
    Crown,
    Keel,
    Plan,
    Shoulder,
}

impl Line {
    const ALL: [Line; 4] = [Line::Crown, Line::Keel, Line::Plan, Line::Shoulder];

    fn name(self) -> &'static str {
        match self {
            Line::Crown => "crown",
            Line::Keel => "keel",
            Line::Plan => "plan",
            Line::Shoulder => "shoulder",
        }
    }
}

/// Fairness metrics of one profile line.
#[derive(Debug, Clone, PartialEq)]
pub struct LineFairness {
    /// `crown`, `keel`, `plan` or `shoulder`.
    pub name: &'static str,
    /// Sign changes of the curvature.
    pub inflections: usize,
    /// Turning points of the curvature (local extrema, ends excluded).
    pub curvature_extrema: usize,
    /// Total variation of the curvature, 1/m.
    pub total_variation: f64,
    /// Total variation over the minimum for the same range and end values.
    pub tv_ratio: f64,
    /// Sampled `integral (d kappa/ds)^2 ds`, 1/m^3.
    pub energy: f64,
    /// `max |d kappa/ds| * L^2`, dimensionless.
    pub max_dkappa_l2: f64,
    /// `max |kappa|`, 1/m.
    pub max_curvature: f64,
    /// Largest curvature difference across a knot, 1/m.
    pub knot_jump: f64,
}

/// The fairness report of a loft.
#[derive(Debug, Clone, PartialEq)]
pub struct FairnessReport {
    /// Crown, keel, plan and shoulder lines, in that order.
    pub lines: Vec<LineFairness>,
    /// Smallest exponent `n` over the body.
    pub min_exponent: f64,
    /// Largest `|k|` over the body.
    pub max_abs_shoulder: f64,
    /// Smallest `A` and `E` over the body, m^2 (non-negative for a valid body).
    pub min_squared_extent: f64,
    /// Local maxima of the section area `S(x)` (1 for a fuselage).
    pub area_maxima: usize,
    /// Violations of the budget stored with the loft, as readable strings.
    pub violations: Vec<String>,
}

impl FairnessReport {
    /// Whether the body meets its stored budget and the convexity limits.
    #[must_use]
    pub fn within_budget(&self) -> bool {
        self.violations.is_empty()
    }

    /// The metrics of the named line.
    #[must_use]
    pub fn line(&self, name: &str) -> Option<&LineFairness> {
        self.lines.iter().find(|l| l.name == name)
    }
}

/// `(f, f', f'')` of a line from the jets of the squared extents and `zc`.
fn derivatives(j: &Jets, line: Line) -> (f64, f64, f64) {
    let root = |s: &[f64; 4]| {
        let r = s[0].max(0.0).sqrt().max(1e-12);
        (
            r,
            s[1] / (2.0 * r),
            s[2] / (2.0 * r) - s[1] * s[1] / (4.0 * r * r * r),
        )
    };
    let (e, e1, e2) = root(&j.e2);
    match line {
        Line::Crown => (j.zc[0] + e, j.zc[1] + e1, j.zc[2] + e2),
        Line::Keel => (j.zc[0] - e, j.zc[1] - e1, j.zc[2] - e2),
        Line::Plan => root(&j.a2),
        Line::Shoulder => (
            j.zc[0] + j.k[0] * e,
            j.zc[1] + j.k[1] * e + j.k[0] * e1,
            j.zc[2] + j.k[2] * e + 2.0 * j.k[1] * e1 + j.k[0] * e2,
        ),
    }
}

fn curvature(j: &Jets, line: Line) -> f64 {
    let (_, f1, f2) = derivatives(j, line);
    f2 / (1.0 + f1 * f1).powf(1.5)
}

/// Smallest total variation of a profile with these end values and extremes.
fn minimal_variation(first: f64, last: f64, lo: f64, hi: f64) -> f64 {
    let up = (first - hi).abs() + (hi - lo).abs() + (lo - last).abs();
    let down = (first - lo).abs() + (lo - hi).abs() + (hi - last).abs();
    up.min(down)
}

/// Number of turning points of `values` whose swing exceeds `delta`.
fn turning_points(values: &[f64], delta: f64) -> usize {
    let mut count = 0usize;
    let mut direction = 0i8;
    let mut pivot = values[0];
    for &v in &values[1..] {
        match direction {
            0 => {
                if (v - pivot).abs() > delta {
                    direction = if v > pivot { 1 } else { -1 };
                    pivot = v;
                }
            }
            1 => {
                if v > pivot {
                    pivot = v;
                } else if pivot - v > delta {
                    count += 1;
                    direction = -1;
                    pivot = v;
                }
            }
            _ => {
                if v < pivot {
                    pivot = v;
                } else if v - pivot > delta {
                    count += 1;
                    direction = 1;
                    pivot = v;
                }
            }
        }
    }
    count
}

impl Loft {
    /// The sampled fairness report (see the module documentation), with the
    /// budget of the specification applied.
    #[must_use]
    pub fn fairness(&self) -> FairnessReport {
        let length = self.curves.length();
        let xs: Vec<f64> = (0..SAMPLES)
            .map(|i| {
                let t = (i as f64 + 0.5) / SAMPLES as f64;
                0.5 * length * (1.0 - (std::f64::consts::PI * t).cos())
            })
            .collect();
        let jets: Vec<Jets> = xs.iter().map(|&x| self.curves.jets(x, None)).collect();
        let knots: Vec<f64> = self
            .curves
            .breakpoints()
            .into_iter()
            .filter(|&x| x > 1e-9 * length && x < length * (1.0 - 1e-9))
            .collect();

        let mut lines = Vec::with_capacity(4);
        for line in Line::ALL {
            lines.push(self.line_metrics(line, &xs, &jets, &knots, length));
        }

        let mut min_n = f64::INFINITY;
        let mut max_k = 0.0_f64;
        let mut min_sq = f64::INFINITY;
        for j in &jets {
            min_n = min_n.min(j.n[0]);
            max_k = max_k.max(j.k[0].abs());
            min_sq = min_sq.min(j.a2[0]).min(j.e2[0]);
        }
        let mut area_maxima = 0usize;
        let rates: Vec<f64> = xs.iter().map(|&x| self.ds_dx(x)).collect();
        let scale = rates.iter().fold(0.0_f64, |m, r| m.max(r.abs()));
        let mut positive = false;
        for &r in &rates {
            if r > 1e-3 * scale {
                positive = true;
            } else if r < -1e-3 * scale && positive {
                area_maxima += 1;
                positive = false;
            }
        }
        if positive {
            // Still rising at the end: the maximum is the base face.
            area_maxima += 1;
        }

        let mut report = FairnessReport {
            lines,
            min_exponent: min_n,
            max_abs_shoulder: max_k,
            min_squared_extent: min_sq,
            area_maxima,
            violations: Vec::new(),
        };
        report.violations = self.budget_violations(&report);
        report
    }

    fn line_metrics(
        &self,
        line: Line,
        xs: &[f64],
        jets: &[Jets],
        knots: &[f64],
        length: f64,
    ) -> LineFairness {
        let kappa: Vec<f64> = jets.iter().map(|j| curvature(j, line)).collect();
        let heights: Vec<f64> = jets.iter().map(|j| derivatives(j, line).0).collect();
        let max_curvature = kappa.iter().fold(0.0_f64, |m, k| m.max(k.abs()));
        let lo = kappa.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = kappa.iter().copied().fold(f64::NEG_INFINITY, f64::max);

        // A curvature counts as a sign only when the radius of curvature is
        // below 50 local half-sizes; flatter than that is straight for any
        // practical purpose and its sign is rounding of the fit.
        let mut inflections = 0usize;
        let mut last_sign = 0i8;
        for (&k, j) in kappa.iter().zip(jets) {
            let scale = (j.a2[0].max(0.0) * j.e2[0].max(0.0))
                .sqrt()
                .sqrt()
                .max(LOCAL_SCALE_FLOOR_M);
            if k.abs() * scale > SIGN_THRESHOLD {
                let sign = if k > 0.0 { 1 } else { -1 };
                if last_sign != 0 && sign != last_sign {
                    inflections += 1;
                }
                last_sign = sign;
            }
        }

        let (mut tv, mut energy, mut max_rate) = (0.0, 0.0, 0.0_f64);
        for i in 1..kappa.len() {
            let dk = kappa[i] - kappa[i - 1];
            let ds = (xs[i] - xs[i - 1]).hypot(heights[i] - heights[i - 1]);
            tv += dk.abs();
            if ds > 0.0 {
                energy += dk * dk / ds;
                max_rate = max_rate.max(dk.abs() / ds);
            }
        }
        let reference = minimal_variation(kappa[0], kappa[kappa.len() - 1], lo, hi);
        let tv_ratio = if reference > 1e-12 * (1.0 + max_curvature) {
            (tv / reference).max(1.0)
        } else {
            1.0
        };
        let extrema = turning_points(&kappa, EXTREMUM_THRESHOLD * (hi - lo));

        let knot_jump = knots.iter().fold(0.0_f64, |m, &x| {
            let left = curvature(&self.curves.jets(x, Some(Side::Left)), line);
            let right = curvature(&self.curves.jets(x, Some(Side::Right)), line);
            m.max((left - right).abs())
        });
        LineFairness {
            name: line.name(),
            inflections,
            curvature_extrema: extrema,
            total_variation: tv,
            tv_ratio,
            energy,
            max_dkappa_l2: max_rate * length * length,
            max_curvature,
            knot_jump,
        }
    }

    fn budget_violations(&self, report: &FairnessReport) -> Vec<String> {
        let budget = &self.spec.fairness;
        let mut out = Vec::new();
        for (name, allowed) in [
            ("crown", budget.inflections.crown),
            ("keel", budget.inflections.keel),
            ("plan", budget.inflections.plan),
        ] {
            let Some(l) = report.line(name) else { continue };
            if l.inflections > allowed as usize {
                out.push(format!(
                    "{name}: {} inflections exceed the allowed {allowed}",
                    l.inflections
                ));
            }
            if l.tv_ratio > budget.tv_ratio_max {
                out.push(format!(
                    "{name}: curvature variation ratio {:.3} exceeds {:.3}",
                    l.tv_ratio, budget.tv_ratio_max
                ));
            }
        }
        if report.min_exponent < alas_config::loft::MIN_EXPONENT - 1e-9 {
            out.push(format!(
                "exponent n = {:.3} below the convexity limit",
                report.min_exponent
            ));
        }
        if report.max_abs_shoulder >= alas_config::loft::SHOULDER_LIMIT {
            out.push(format!(
                "shoulder offset |k| = {:.3} reaches 0.5",
                report.max_abs_shoulder
            ));
        }
        if report.min_squared_extent < -1e-9 {
            out.push(format!(
                "squared extent {:.3e} is negative",
                report.min_squared_extent
            ));
        }
        for l in &report.lines {
            if l.knot_jump > 1e-9 {
                out.push(format!(
                    "{}: curvature jumps {:.3e} across a knot",
                    l.name, l.knot_jump
                ));
            }
        }
        out
    }
}

/// Inflections of a sampled planar line `(xs, f)` with the same sign rule as
/// [`Loft::fairness`]: curvature from three-point differences, significant when
/// `|kappa| * scale > 0.02` with `scale` the local half-size (floored at 0.25 m).
pub(super) fn sampled_inflections(xs: &[f64], f: &[f64], scale: &[f64]) -> usize {
    let mut count = 0usize;
    let mut last = 0i8;
    for i in 1..xs.len().saturating_sub(1) {
        let (h0, h1) = (xs[i] - xs[i - 1], xs[i + 1] - xs[i]);
        if h0 <= 0.0 || h1 <= 0.0 {
            continue;
        }
        let (s0, s1) = ((f[i] - f[i - 1]) / h0, (f[i + 1] - f[i]) / h1);
        let f2 = 2.0 * (s1 - s0) / (h0 + h1);
        let f1 = (s0 * h1 + s1 * h0) / (h0 + h1);
        let kappa = f2 / (1.0 + f1 * f1).powf(1.5);
        if kappa.abs() * scale[i].max(LOCAL_SCALE_FLOOR_M) > SIGN_THRESHOLD {
            let sign = if kappa > 0.0 { 1 } else { -1 };
            if last != 0 && sign != last {
                count += 1;
            }
            last = sign;
        }
    }
    count
}
