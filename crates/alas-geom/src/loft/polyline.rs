// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Station polylines of a [`Loft`] with a stated accuracy.
//!
//! [`Loft::sample_xsecs`] takes a station count. Consumers that still read the
//! body as linearly interpolated stations (the fin seat, the perimeter
//! integrals, the cabin fit) want the opposite: a tolerance, and the count
//! that meets it. [`Loft::sample_xsecs_within`] sizes the count from the
//! curvature monitor, then measures the polyline against the exact crown, keel
//! and plan half-width and grows it until the measured deviation is within the
//! tolerance or the cap is reached.

use crate::aircraft::fuselage::FuselageXSec;

use super::Loft;

/// Points per axis at which the deviation is measured.
const CHECK_POINTS: usize = 3000;
/// Growth of the station count between attempts.
const GROWTH: f64 = 1.25;

/// A station polyline and how far it is from the lofted surface.
#[derive(Debug, Clone, PartialEq)]
pub struct SampledPolyline {
    /// Stations, nose to tail, in the output frame.
    pub xsecs: Vec<FuselageXSec>,
    /// Largest distance, m, between the polyline and the exact crown, keel or
    /// plan half-width, measured in `z` (crown, keel) or `y` (half-width).
    pub max_deviation_m: f64,
    /// Whether `max_deviation_m` is within the requested tolerance.
    pub within_tolerance: bool,
}

impl Loft {
    /// Largest distance, m, between the linear interpolation of `xsecs`
    /// (centre height and width/height, as the legacy consumers read them) and
    /// the exact crown `zc + e`, keel `zc - e` and half-width `a`, over the
    /// whole body.
    #[must_use]
    pub fn polyline_deviation(&self, xsecs: &[FuselageXSec]) -> f64 {
        if xsecs.len() < 2 {
            return f64::INFINITY;
        }
        let (x0, x1) = self.x_range();
        let mut worst = 0.0_f64;
        let mut segment = 0usize;
        for i in 0..=CHECK_POINTS {
            let x = x0 + (x1 - x0) * i as f64 / CHECK_POINTS as f64;
            while segment + 2 < xsecs.len() && xsecs[segment + 1].xyz_c[0] < x {
                segment += 1;
            }
            let (p, q) = (&xsecs[segment], &xsecs[segment + 1]);
            let span = q.xyz_c[0] - p.xyz_c[0];
            let t = if span > 0.0 {
                ((x - p.xyz_c[0]) / span).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let lerp = |u: f64, v: f64| u + t * (v - u);
            let zc = lerp(p.xyz_c[2], q.xyz_c[2]);
            let half_h = 0.5 * lerp(p.height, q.height);
            let half_w = 0.5 * lerp(p.width, q.width);
            let s = self.section(x);
            worst = worst
                .max((zc + half_h - (s.zc + s.e)).abs())
                .max((zc - half_h - (s.zc - s.e)).abs())
                .max((half_w - s.a).abs());
        }
        worst
    }

    /// A station polyline within `tolerance_m` of the exact crown, keel and
    /// plan half-width, using between `min_count` and `max_count` stations.
    /// When the cap is reached first, `within_tolerance` is false and the
    /// polyline is the best one found.
    #[must_use]
    pub fn sample_xsecs_within(
        &self,
        tolerance_m: f64,
        min_count: usize,
        max_count: usize,
    ) -> SampledPolyline {
        let min_count = min_count.max(4);
        let max_count = max_count.max(min_count);
        // A chord of length h on a curve of curvature k has sagitta k h^2 / 8:
        // the monitor's sagitta target is the deviation we accept, with margin
        // for the three curves adding up.
        let wanted = 1.0 + self.intervals_for_sagitta(0.5 * tolerance_m).ceil();
        let mut count = (wanted.max(min_count as f64) as usize).clamp(min_count, max_count);
        loop {
            let xs = self.station_positions_for(count, 0.5 * tolerance_m);
            let xsecs = self.xsecs_at(&xs);
            let max_deviation_m = self.polyline_deviation(&xsecs);
            let within_tolerance = max_deviation_m <= tolerance_m;
            if within_tolerance || count >= max_count {
                return SampledPolyline {
                    xsecs,
                    max_deviation_m,
                    within_tolerance,
                };
            }
            count = ((count as f64 * GROWTH).ceil() as usize).min(max_count);
        }
    }
}
