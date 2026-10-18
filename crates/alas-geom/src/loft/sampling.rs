// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The legacy station view of a [`Loft`]: a polyline of
//! [`FuselageXSec`] stations with curvature-adaptive spacing.
//!
//! Unmigrated consumers still read the body as linearly interpolated
//! stations. Their spacing equidistributes a monitor built from the second
//! derivatives of the half-width, half-height and centre height (the sagitta
//! of a chord of length `h` is `h^2 |f''| / 8`), plus a uniform floor, so the
//! stations cluster where the body bends: the nose and the aft body. The ends
//! of the constant cabin cylinder are kept as stations, which keeps the cabin
//! sections identical to the legacy ones.

use crate::aircraft::fuselage::FuselageXSec;

use super::curves::Jets;
use super::section::unit_area;
use super::Loft;

/// Chord sagitta the monitor aims at, m.
const TARGET_SAGITTA_M: f64 = 2e-3;
/// Grid used to integrate the monitor.
const GRID: usize = 4000;
/// Share of the stations allocated uniformly regardless of curvature.
const UNIFORM_SHARE: f64 = 0.25;

impl Loft {
    /// `count` stations from the nose tip to the tail end (at least two) in
    /// the output frame, spaced by curvature.
    ///
    /// Each station has width `2 a`, height `2 e`, centre `(x, 0, zc)` and
    /// shape exponent `n`: the legacy superellipse area `4 a e g(shape)` then
    /// equals the loft section area exactly (the shoulder offset `k` does not
    /// change the area). The legacy perimeter fit knows nothing of the shoulder
    /// warp, so its perimeter is only approximate where `k` is not zero.
    #[must_use]
    pub fn sample_xsecs(&self, count: usize) -> Vec<FuselageXSec> {
        let count = count.max(2);
        self.xsecs_at(&self.station_positions(count))
    }

    /// Stations at built local `x` positions.
    pub(super) fn xsecs_at(&self, xs: &[f64]) -> Vec<FuselageXSec> {
        let o = self.params.origin;
        xs.iter()
            .map(|&x| {
                let (_, j) = self.local_state(x + o[0]);
                FuselageXSec {
                    xyz_c: [x + o[0], o[1], j.zc[0] + o[2]],
                    width: 2.0 * j.a2[0].max(0.0).sqrt(),
                    height: 2.0 * j.e2[0].max(0.0).sqrt(),
                    shape: self.equivalent_exponent(&j),
                }
            })
            .collect()
    }

    /// The legacy shape exponent of a station: `n`, or, where the upper-arc
    /// residual removes area, the superellipse exponent with the same area
    /// `4 a e g(shape)` (bisection on the monotone `unit_area`, bounded to
    /// `[1, 8]`), so the legacy polyline view keeps the section area.
    fn equivalent_exponent(&self, j: &Jets) -> f64 {
        let n = j.n[0].max(1.0);
        if !super::body::has_residual(j) {
            return n;
        }
        let (a, e) = (j.a2[0].max(0.0).sqrt(), j.e2[0].max(0.0).sqrt());
        if a <= 0.0 || e <= 0.0 {
            return n;
        }
        let target = super::body::area_of(j) / (a * e);
        let (mut lo, mut hi) = (1.0_f64, 8.0_f64);
        if target <= unit_area(lo) {
            return lo;
        }
        if target >= unit_area(hi) {
            return hi;
        }
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if unit_area(mid) < target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    }

    /// Built local `x` of the stations (ascending, first 0, last the length).
    pub(super) fn station_positions(&self, count: usize) -> Vec<f64> {
        self.station_positions_for(count, TARGET_SAGITTA_M)
    }

    /// Axis nodes of the monitor integration: cosine-spaced, so the nose and
    /// the tail (where the bending is) are resolved.
    fn monitor_grid(&self) -> Vec<f64> {
        let length = self.curves.length();
        (0..=GRID)
            .map(|i| 0.5 * length * (1.0 - (std::f64::consts::PI * i as f64 / GRID as f64).cos()))
            .collect()
    }

    /// Cumulative monitor at the grid nodes (midpoint rule per cell) for chord
    /// sagitta `sagitta_m`, plus a uniform `floor` density.
    fn monitor_cumulative(&self, grid: &[f64], sagitta_m: f64, floor: f64) -> Vec<f64> {
        let mut cumulative = vec![0.0; GRID + 1];
        for i in 0..GRID {
            let xm = 0.5 * (grid[i] + grid[i + 1]);
            let j = self.curves.jets(xm, None);
            let a = j.a2[0].max(1e-24).sqrt();
            let e = j.e2[0].max(1e-24).sqrt();
            let second = |r: f64, s: &[f64; 4]| s[2] / (2.0 * r) - s[1] * s[1] / (4.0 * r * r * r);
            let curvature_sum = second(a, &j.a2).abs() + second(e, &j.e2).abs() + j.zc[2].abs();
            let density = (curvature_sum / (8.0 * sagitta_m)).sqrt() + floor;
            cumulative[i + 1] = cumulative[i] + density * (grid[i + 1] - grid[i]);
        }
        cumulative
    }

    /// Number of intervals that holds the chord sagitta of the half-width,
    /// half-height and centre-height curves at `sagitta_m` when the stations
    /// are equidistributed on the monitor (no uniform floor).
    pub(super) fn intervals_for_sagitta(&self, sagitta_m: f64) -> f64 {
        let grid = self.monitor_grid();
        self.monitor_cumulative(&grid, sagitta_m, 0.0)[GRID]
    }

    /// [`Self::station_positions`] aiming at chord sagitta `sagitta_m`.
    pub(super) fn station_positions_for(&self, count: usize, sagitta_m: f64) -> Vec<f64> {
        let length = self.curves.length();
        let grid = self.monitor_grid();
        let floor = UNIFORM_SHARE * count as f64 / length;
        let cumulative = self.monitor_cumulative(&grid, sagitta_m, floor);
        let total = cumulative[GRID];
        let mut out = Vec::with_capacity(count);
        let mut cell = 0usize;
        for k in 0..count {
            let target = total * k as f64 / (count - 1) as f64;
            while cell + 1 < GRID && cumulative[cell + 1] < target {
                cell += 1;
            }
            let span = cumulative[cell + 1] - cumulative[cell];
            let t = if span > 0.0 {
                ((target - cumulative[cell]) / span).clamp(0.0, 1.0)
            } else {
                0.0
            };
            out.push(grid[cell] + t * (grid[cell + 1] - grid[cell]));
        }
        out[0] = 0.0;
        out[count - 1] = length;

        // Keep the cabin cylinder ends as stations.
        if let Some((xa, xb)) = self.curves.flat_zone_built() {
            let mut anchors = vec![xa];
            if xb > xa {
                anchors.push(xb);
            }
            let mut used: Vec<usize> = Vec::new();
            for anchor in anchors {
                if anchor <= 0.0 || anchor >= length || count < 4 {
                    continue;
                }
                let nearest = (1..count - 1)
                    .filter(|i| !used.contains(i))
                    .min_by(|&p, &q| (out[p] - anchor).abs().total_cmp(&(out[q] - anchor).abs()));
                if let Some(i) = nearest {
                    out[i] = anchor;
                    used.push(i);
                }
            }
            out.sort_by(f64::total_cmp);
        }
        out
    }
}
