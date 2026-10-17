// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! C2 interpolation of the spanwise profiles of the main wing: dihedral
//! height, twist and thickness multiplier.
//!
//! The planform (leading edge, trailing edge, chord, kinks) stays piecewise
//! linear: the Yehudi and trailing-edge kinks are real design discontinuities
//! and the reference area is dimensioned on them. Only the three *profiles*
//! may be smoothed, because they are sampled data (a handful of stations read
//! from drawings), not dimensioned straight panels.
//!
//! [`WingProfileInterpolation::C2Spline`] replaces the polyline between the
//! stations by a **natural cubic spline that interpolates every station
//! exactly** (zero second derivative at the two ends, Burden & Faires): the
//! pinned ground clearances and the pinned A380/A340 stations do not move, the
//! curve is C2 across every station, and with two stations it degenerates to
//! the straight line.
//!
//! # Shape-preservation guard
//!
//! A natural spline overshoots where the data bend abruptly (measured: the
//! A380 thickness stations, falling steeply then nearly flat, gave an
//! undershoot of 4 % of the local value below the tip station). Where the
//! data are locally monotone (three consecutive secants of one sign) the
//! spline must stay within the interval's end values to within
//! [`OVERSHOOT_TOLERANCE`] of the profile's range. If any interval violates
//! this, that one profile falls back to the monotone cubic Hermite
//! interpolant of Fritsch and Carlson (1980), which still interpolates every
//! station exactly and never overshoots, but is only C1. The fallback is
//! reported by [`ProfileSpline::is_c2`].
//!
//! Frame: `eta` is the span fraction (distance from the centreline over the
//! semispan, dimensionless, `[0, 1]`); outside the stations the value is held.

use alas_math::{Boundary, CubicSpline};
use serde::{Deserialize, Serialize};

use super::config::WingConfig;
use super::planform::TransportPlanform;

/// Largest tolerated overshoot of a natural spline beyond the end values of a
/// locally monotone interval, as a fraction of the profile's value range.
pub const OVERSHOOT_TOLERANCE: f64 = 0.01;

/// How the dihedral height, twist and thickness stations are interpolated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WingProfileInterpolation {
    /// Piecewise linear between stations (the established behaviour).
    #[default]
    Linear,
    /// Natural cubic spline through every station (C2, interpolating).
    C2Spline,
}

impl WingProfileInterpolation {
    /// `true` for the default; used to skip serialisation.
    pub fn is_linear(&self) -> bool {
        *self == Self::Linear
    }
}

/// A scalar profile `value(eta)` through ascending stations, held constant
/// outside the first and last station.
#[derive(Debug, Clone)]
pub struct ProfileSpline {
    nodes: Vec<(f64, f64)>,
    spline: Option<CubicSpline>,
    /// Hermite slopes at the nodes when the monotone fallback is active.
    slopes: Option<Vec<f64>>,
}

/// Fritsch-Carlson / Fritsch-Butland node slopes (as in SciPy `PchipInterpolator`).
fn pchip_slopes(nodes: &[(f64, f64)]) -> Vec<f64> {
    let n = nodes.len();
    let h: Vec<f64> = nodes.windows(2).map(|w| w[1].0 - w[0].0).collect();
    let d: Vec<f64> = nodes
        .windows(2)
        .zip(&h)
        .map(|(w, h)| (w[1].1 - w[0].1) / h)
        .collect();
    let mut m = vec![0.0; n];
    if n == 2 {
        m.fill(d.first().copied().unwrap_or(0.0));
        return m;
    }
    for k in 1..n - 1 {
        if d[k - 1] * d[k] > 0.0 {
            let (w1, w2) = (2.0 * h[k] + h[k - 1], h[k] + 2.0 * h[k - 1]);
            m[k] = (w1 + w2) / (w1 / d[k - 1] + w2 / d[k]);
        }
    }
    let end = |h0: f64, h1: f64, d0: f64, d1: f64| -> f64 {
        let s = ((2.0 * h0 + h1) * d0 - h0 * d1) / (h0 + h1);
        if s * d0 <= 0.0 {
            0.0
        } else if d0 * d1 <= 0.0 && s.abs() > 3.0 * d0.abs() {
            3.0 * d0
        } else {
            s
        }
    };
    m[0] = end(h[0], h[1], d[0], d[1]);
    m[n - 1] = end(h[n - 2], h[n - 3], d[n - 2], d[n - 3]);
    m
}

impl ProfileSpline {
    /// Interpolant through `nodes` (`(eta, value)`): the natural cubic spline
    /// unless the shape-preservation guard rejects it (module docs). Nodes
    /// are sorted; a node within `1e-9` of its predecessor is dropped. `None`
    /// for no nodes.
    pub fn natural(nodes: &[(f64, f64)]) -> Option<Self> {
        let mut sorted: Vec<(f64, f64)> = nodes.to_vec();
        sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
        sorted.dedup_by(|b, a| (b.0 - a.0).abs() <= 1.0e-9);
        if sorted.is_empty() {
            return None;
        }
        let spline = (sorted.len() >= 2)
            .then(|| {
                let x: Vec<f64> = sorted.iter().map(|n| n.0).collect();
                let y: Vec<Vec<f64>> = sorted.iter().map(|n| vec![n.1]).collect();
                let zero = [0.0];
                CubicSpline::new(
                    &x,
                    &y,
                    Boundary::SecondDerivative(&zero),
                    Boundary::SecondDerivative(&zero),
                )
                .ok()
            })
            .flatten();
        let mut profile = Self {
            nodes: sorted,
            spline,
            slopes: None,
        };
        if profile.overshoots() {
            profile.slopes = Some(pchip_slopes(&profile.nodes));
        }
        Some(profile)
    }

    /// A profile with one value everywhere.
    pub fn constant(value: f64) -> Self {
        Self {
            nodes: vec![(0.0, value)],
            spline: None,
            slopes: None,
        }
    }

    /// `true` when the profile is the C2 natural spline, `false` when the
    /// monotone C1 fallback is active.
    pub fn is_c2(&self) -> bool {
        self.slopes.is_none()
    }

    /// Does the natural spline leave the end values of a locally monotone
    /// interval by more than [`OVERSHOOT_TOLERANCE`] of the value range?
    fn overshoots(&self) -> bool {
        let n = self.nodes.len();
        if n < 3 {
            return false;
        }
        let (lo, hi) = self
            .nodes
            .iter()
            .fold((f64::MAX, f64::MIN), |(l, h), p| (l.min(p.1), h.max(p.1)));
        let tol = OVERSHOOT_TOLERANCE * (hi - lo);
        let secant: Vec<f64> = self
            .nodes
            .windows(2)
            .map(|w| (w[1].1 - w[0].1) / (w[1].0 - w[0].0))
            .collect();
        for k in 0..n - 1 {
            let lo_k = k.saturating_sub(1);
            let hi_k = (k + 1).min(n - 2);
            let slice = &secant[lo_k..=hi_k];
            let monotone = slice.iter().all(|s| *s > 0.0) || slice.iter().all(|s| *s < 0.0);
            if !monotone {
                continue;
            }
            let (a, b) = (self.nodes[k], self.nodes[k + 1]);
            let (vmin, vmax) = (a.1.min(b.1), a.1.max(b.1));
            for i in 1..40 {
                let eta = a.0 + (b.0 - a.0) * f64::from(i) / 40.0;
                let v = self.eval_spline(eta);
                if v < vmin - tol || v > vmax + tol {
                    return true;
                }
            }
        }
        false
    }

    fn eval_spline(&self, eta: f64) -> f64 {
        self.spline.as_ref().map_or_else(
            || self.nodes.first().map_or(0.0, |p| p.1),
            |s| s.evaluate(eta)[0],
        )
    }

    fn eval_hermite(&self, slopes: &[f64], eta: f64) -> f64 {
        let k = self
            .nodes
            .partition_point(|p| p.0 <= eta)
            .saturating_sub(1)
            .min(self.nodes.len().saturating_sub(2));
        let (a, b) = (self.nodes[k], self.nodes[k + 1]);
        let h = b.0 - a.0;
        let t = (eta - a.0) / h;
        let (t2, t3) = (t * t, t * t * t);
        (2.0 * t3 - 3.0 * t2 + 1.0) * a.1
            + (t3 - 2.0 * t2 + t) * h * slopes[k]
            + (-2.0 * t3 + 3.0 * t2) * b.1
            + (t3 - t2) * h * slopes[k + 1]
    }

    /// Value at `eta`; held at the end values outside the stations.
    pub fn eval(&self, eta: f64) -> f64 {
        let (Some(first), Some(last)) = (self.nodes.first(), self.nodes.last()) else {
            return 0.0;
        };
        if eta <= first.0 {
            return first.1;
        }
        if eta >= last.0 {
            return last.1;
        }
        match &self.slopes {
            Some(slopes) => self.eval_hermite(slopes, eta),
            None => self.eval_spline(eta),
        }
    }
}

impl WingConfig {
    /// Twist profile through the root, kink and tip twists (degrees).
    pub fn twist_spline(&self, planform: &TransportPlanform, tip_twist_deg: f64) -> ProfileSpline {
        ProfileSpline::natural(&[
            (0.0, self.root_twist_deg),
            (planform.kink.span_fraction, self.break_twist_deg),
            (1.0, tip_twist_deg),
        ])
        .unwrap_or_else(|| ProfileSpline::constant(0.0))
    }

    /// Profile of the section thickness ratio t/c through the thickness
    /// stations, `None` with no stations. The stations are multipliers on the
    /// root-kink-tip blend, whose own t/c is `base_tc(eta)`; the drawings the
    /// stations were read from give t/c itself (monotone root to tip), so the
    /// spline runs through `scale * base_tc` and the multiplier is recovered
    /// as `profile(eta) / base_tc(eta)`. Smoothing the multiplier instead let
    /// the product dip below the tip value (A380: 12 % under the polyline
    /// mid-outboard), because that multiplier is not monotone.
    pub fn thickness_spline(&self, base_tc: impl Fn(f64) -> f64) -> Option<ProfileSpline> {
        let nodes: Vec<(f64, f64)> = self
            .thickness_stations
            .iter()
            .map(|s| {
                (
                    s.span_fraction,
                    s.thickness_scale * base_tc(s.span_fraction),
                )
            })
            .collect();
        ProfileSpline::natural(&nodes)
    }
}
