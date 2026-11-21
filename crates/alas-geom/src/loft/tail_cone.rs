// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Tail-cone smoothness diagnostics of a [`Loft`].
//!
//! The tail cone is the body aft of the end of the passenger cabin (`cabin.x_end_m`
//! of the data file). The report answers the
//! question "does the shape compress and expand there?":
//!
//! * `inflections`: curvature sign changes of the crown, keel and plan lines
//!   inside the tail cone (same sign rule as [`Loft::fairness`]). A fair cone has
//!   at most the one S-bend that joins the cylinder to the closure.
//! * `*_reversal`: the largest re-growth of the section area, perimeter,
//!   half-width and half-height after they have started to shrink,
//!   `max_x (v(x) - min_{x' <= x} v(x'))`, as a fraction of the cabin value.
//!   Zero means monotonic decay aft of the cabin.
//!
//! Units: metres; the fractions are dimensionless.

use super::body::area_of;
use super::fairness::{curvature, Line, LOCAL_SCALE_FLOOR_M, SIGN_THRESHOLD};
use super::Loft;

/// Relative rate of fall of the section area (per metre) that marks the cone: the
/// area has to lose more than this fraction per metre.
const CONE_RATE_PER_M: f64 = 2e-3;
/// Stations of the tail-cone scan.
const SAMPLES: usize = 1500;

/// Smoothness metrics of the tail cone of one body.
#[derive(Debug, Clone, PartialEq)]
pub struct TailConeReport {
    /// First station of the cone (end of the passenger cabin), m aft of the nose tip.
    pub x_start_m: f64,
    /// Length of the cone, m.
    pub length_m: f64,
    /// Curvature sign changes of the crown line inside the cone.
    pub crown_inflections: usize,
    /// Curvature sign changes of the keel line inside the cone.
    pub keel_inflections: usize,
    /// Curvature sign changes of the plan half-width line inside the cone.
    pub plan_inflections: usize,
    /// Largest re-growth of the section area, fraction of the cabin area.
    pub area_reversal: f64,
    /// Largest re-growth of the section perimeter, fraction of the cabin perimeter.
    pub perimeter_reversal: f64,
    /// Largest re-growth of the half-width, fraction of the cabin half-width.
    pub width_reversal: f64,
    /// Largest re-growth of the half-height, fraction of the cabin half-height.
    pub height_reversal: f64,
}

impl TailConeReport {
    /// Whether the cone decays monotonically (every reversal below `tol`, a
    /// fraction) with at most `allowed` curvature sign changes on each line.
    #[must_use]
    pub fn is_smooth(&self, tol: f64, allowed: usize) -> bool {
        self.area_reversal <= tol
            && self.perimeter_reversal <= tol
            && self.width_reversal <= tol
            && self.height_reversal <= tol
            && self.crown_inflections <= allowed
            && self.keel_inflections <= allowed
            && self.plan_inflections <= allowed
    }
}

/// `max_x (v - running minimum)` over `values`.
fn reversal(values: &[f64]) -> f64 {
    let mut low = f64::INFINITY;
    let mut worst = 0.0_f64;
    for &v in values {
        low = low.min(v);
        worst = worst.max(v - low);
    }
    worst
}

impl Loft {
    /// The tail-cone report (see the module documentation), over the body aft of
    /// the geometric start of the cone ([`Self::cone_start`]).
    #[must_use]
    pub fn tail_cone_report(&self) -> TailConeReport {
        self.tail_cone_report_from(self.cone_start())
    }

    /// The same report over the body aft of `x0` (built length, m aft of the nose tip).
    #[must_use]
    pub fn tail_cone_report_from(&self, x0: f64) -> TailConeReport {
        let length = self.curves.length();
        let span = (length - x0).max(0.0);
        let station = |i: usize| x0 + span * (i as f64 + 0.5) / SAMPLES as f64;
        let cabin = self.curves.jets(x0, None);
        let cabin_section = super::body::section_from(&cabin, 0.0);
        let (s_ref, p_ref) = (area_of(&cabin), cabin_section.perimeter());
        let (a_ref, e_ref) = (cabin_section.a, cabin_section.e);

        let jets: Vec<_> = (0..SAMPLES)
            .map(|i| self.curves.jets(station(i), None))
            .collect();
        let sections: Vec<_> = jets
            .iter()
            .map(|j| super::body::section_from(j, 0.0))
            .collect();
        let rel = |values: Vec<f64>, reference: f64| reversal(&values) / reference.max(1e-12);
        let signs = |line: Line| {
            let (mut count, mut last) = (0usize, 0i8);
            for j in &jets {
                let scale = (j.a2[0].max(0.0) * j.e2[0].max(0.0))
                    .sqrt()
                    .sqrt()
                    .max(LOCAL_SCALE_FLOOR_M);
                let k = curvature(j, line);
                if k.abs() * scale > SIGN_THRESHOLD {
                    let sign = if k > 0.0 { 1 } else { -1 };
                    if last != 0 && sign != last {
                        count += 1;
                    }
                    last = sign;
                }
            }
            count
        };
        TailConeReport {
            x_start_m: x0,
            length_m: span,
            crown_inflections: signs(Line::Crown),
            keel_inflections: signs(Line::Keel),
            plan_inflections: signs(Line::Plan),
            area_reversal: rel(jets.iter().map(area_of).collect(), s_ref),
            perimeter_reversal: rel(sections.iter().map(|s| s.perimeter()).collect(), p_ref),
            width_reversal: rel(sections.iter().map(|s| s.a).collect(), a_ref),
            height_reversal: rel(sections.iter().map(|s| s.e).collect(), e_ref),
        }
    }

    /// End of the passenger cabin of the data file (reference length mapped to the
    /// built length): payload semantics, not geometry.
    #[must_use]
    pub fn cabin_end(&self) -> f64 {
        self.curves
            .to_built(self.spec.cabin.x_end_m)
            .min(self.curves.length())
    }

    /// Geometric start of the tail cone: the aftmost station from which the section
    /// area falls monotonically with a relative rate above [`CONE_RATE_PER_M`]
    /// (the end of the constant section, or of the last plateau of the area).
    #[must_use]
    pub fn cone_start(&self) -> f64 {
        let length = self.curves.length();
        let n = 4000;
        let area = |x: f64| area_of(&self.curves.jets(x, None));
        let mut start = 0.0;
        for i in (1..n).rev() {
            let x = length * i as f64 / n as f64;
            let h = 0.5 * length / n as f64;
            let s = area(x);
            if s <= 0.0 {
                continue;
            }
            let rate = (area(x + h) - area((x - h).max(0.0))) / (2.0 * h) / s;
            if rate > -CONE_RATE_PER_M {
                start = x;
                break;
            }
        }
        start
    }
}
