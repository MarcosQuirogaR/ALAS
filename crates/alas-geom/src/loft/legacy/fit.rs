// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The adaptive penalised fit engine behind [`super::from_legacy`]: one or
//! more splines on a common knot vector are fitted together, knots are
//! inserted where the error of the physical outline is largest, and the
//! penalty is then raised as far as the deviation bound allows.

use alas_math::spline_fit::{fit_penalised, FitConstraint, FitOutcome, FitSample};

use super::LegacyFitOptions;
use crate::loft::LoftError;

/// One sampled station of the legacy laws.
pub(super) struct Sample {
    pub(super) u: f64,
    pub(super) a: f64,
    pub(super) e: f64,
    pub(super) zc: f64,
    pub(super) n: f64,
}

/// Cosine-clustered plus uniform stations, sorted, with `must` included.
pub(super) fn sample_positions(length: f64, count: usize, must: &[f64]) -> Vec<f64> {
    let half = count / 2;
    let mut xs: Vec<f64> = (0..=half)
        .map(|i| 0.5 * length * (1.0 - (std::f64::consts::PI * i as f64 / half as f64).cos()))
        .collect();
    xs.extend((0..=half).map(|i| length * i as f64 / half as f64));
    xs.extend_from_slice(must);
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|b, a| (*b - *a).abs() < 1e-9 * length);
    xs
}

/// A clamped knot vector with the given interior knots (repeats allowed).
pub(super) fn clamped_knots(degree: usize, interior: &[f64]) -> Vec<f64> {
    let mut sorted = interior.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mut k = vec![0.0; degree + 1];
    k.extend_from_slice(&sorted);
    k.extend(vec![1.0; degree + 1]);
    k
}

/// Distance from each fitted outline point `(xs[i], fitted[i])` to the
/// polyline through the sampled outline `(xs[j], meas[j])`, not to the nearest
/// sample, which overstates the gap where samples are far apart; a vertical gap
/// at a near-vertical tip is not a geometric deviation either.
pub(super) fn gap_series(xs: &[f64], meas: &[f64], fitted: &[f64]) -> Vec<f64> {
    let segment = |i: usize, j: usize| {
        let (px, py) = (xs[i], fitted[i]);
        let (ax, ay) = (xs[j], meas[j]);
        let (bx, by) = (xs[j + 1], meas[j + 1]);
        let (dx, dy) = (bx - ax, by - ay);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 {
            (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (px - ax - t * dx).hypot(py - ay - t * dy)
    };
    (0..xs.len())
        .map(|i| {
            let mut best = (fitted[i] - meas[i]).abs();
            let mut j = i;
            while j + 1 < xs.len() && xs[j] - xs[i] < best {
                best = best.min(segment(i, j));
                j += 1;
            }
            let mut j = i;
            while j > 0 && xs[i] - xs[j] < best {
                best = best.min(segment(i, j - 1));
                j -= 1;
            }
            best
        })
        .collect()
}

/// Penalty weight for a data set: `lambda_rel` times the summed weight times
/// the squared data range.
pub(super) fn lambda_for(rel: f64, samples: &[FitSample]) -> f64 {
    let weight_sum: f64 = samples.iter().map(|s| s.weight).sum();
    let (lo, hi) = samples
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), s| {
            (l.min(s.y), h.max(s.y))
        });
    rel * weight_sum * (hi - lo).max(1e-3).powi(2)
}

/// One penalised fit on the knots made from `interior`.
pub(super) fn fit_curve(
    name: &str,
    degree: usize,
    interior: &[f64],
    samples: &[FitSample],
    constraints: &[FitConstraint],
    lambda: f64,
) -> Result<FitOutcome, LoftError> {
    fit_penalised(
        clamped_knots(degree, interior),
        degree,
        samples,
        constraints,
        lambda,
        3,
    )
    .map_err(|e| LoftError::Fit(format!("{name}: {e}")))
}

/// Result of [`Engine::grow`].
pub(super) struct Grown {
    pub(super) curves: Vec<FitOutcome>,
    pub(super) errors: Vec<f64>,
    /// Interior knots inserted by the error-driven growth.
    pub(super) inserted: usize,
}

/// Knot growth and smoothing over a set of splines fitted together.
pub(super) struct Engine<'a> {
    pub(super) options: &'a LegacyFitOptions,
    /// Sample abscissae, `u` in `[0, 1]`.
    pub(super) u: &'a [f64],
    /// Per-sample factor by which the tolerance and deviation bound are
    /// relaxed (the pole region, where a regular tip cannot follow the law).
    pub(super) relax: &'a [f64],
    /// Open interval in which no knot may be inserted (the pinned cabin).
    pub(super) keep_clear: Option<(f64, f64)>,
}

type FitFn<'f> = dyn Fn(&[f64], f64) -> Result<Vec<FitOutcome>, LoftError> + 'f;
type ErrFn<'f> = dyn Fn(&[FitOutcome]) -> Vec<f64> + 'f;

impl Engine<'_> {
    /// Fit with `fit(interior_knots, penalty_scale)`, inserting a simple knot
    /// at the sample of largest `error(curves)` until it is below
    /// `tolerance` or the knot budget is spent; then raise the penalty scale
    /// (bisection in log over eight decades) to the largest value whose worst
    /// error stays within `max_deviation`. The data are the constraint and the
    /// fairness energy the objective, as in design 4.2.
    pub(super) fn grow(
        &self,
        mut interior: Vec<f64>,
        fit: &FitFn<'_>,
        error: &ErrFn<'_>,
        tolerance: f64,
        max_deviation: f64,
    ) -> Result<Grown, LoftError> {
        let min_gap = 1.5e-3;
        let mut inserted = 0usize;
        let (mut curves, mut errors) = loop {
            let curves = fit(&interior, 1.0)?;
            let errors = error(&curves);
            let relaxed: Vec<f64> = errors.iter().zip(self.relax).map(|(e, r)| e / r).collect();
            let worst = relaxed.iter().fold(0.0_f64, |m, e| m.max(*e));
            if worst <= tolerance || inserted >= self.options.max_interior_knots {
                break (curves, errors);
            }
            let mut order: Vec<usize> = (0..errors.len()).collect();
            order.sort_by(|&p, &q| relaxed[q].total_cmp(&relaxed[p]));
            let new_knot = order.iter().find_map(|&i| {
                let u = self.u[i];
                let clear = self.keep_clear.is_none_or(|(lo, hi)| u <= lo || u >= hi)
                    && u > min_gap
                    && u < 1.0 - min_gap
                    && interior.iter().all(|k| (k - u).abs() > min_gap);
                clear.then_some(u)
            });
            match new_knot {
                Some(u) => {
                    interior.push(u);
                    interior.sort_by(f64::total_cmp);
                    inserted += 1;
                }
                None => break (curves, errors),
            }
        };
        let trial = |scale: f64| -> Option<(Vec<FitOutcome>, Vec<f64>)> {
            let c = fit(&interior, scale).ok()?;
            let errs = error(&c);
            let worst = errs
                .iter()
                .zip(self.relax)
                .fold(0.0_f64, |m, (e, r)| m.max(e / r));
            (worst <= max_deviation).then_some((c, errs))
        };
        if let Some(best) = trial(1e8) {
            (curves, errors) = best;
        } else if trial(1.0).is_some() {
            let (mut good, mut bad) = (0.0_f64, 8.0_f64);
            for _ in 0..16 {
                let mid = 0.5 * (good + bad);
                if trial(10f64.powf(mid)).is_some() {
                    good = mid;
                } else {
                    bad = mid;
                }
            }
            if let Some(best) = trial(10f64.powf(good)) {
                (curves, errors) = best;
            }
        }
        Ok(Grown {
            curves,
            errors,
            inserted,
        })
    }
}
