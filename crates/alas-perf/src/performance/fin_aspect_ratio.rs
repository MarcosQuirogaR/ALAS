// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The effective aspect ratio of a single vertical tail on the plane of
//! symmetry, USAF DATCOM (1978), Sec. 5.3.1.1, Method 1, Step 1,
//! Eq. 5.3.1.1-a:
//!
//! `A_eff = (A_v(B) / A_v) A_v [1 + K_H (A_v(HB) / A_v(B) - 1)]`
//!
//! `A_v` is the geometric aspect ratio of the isolated panel with its span
//! and area measured to the body centreline; `A_v(B) / A_v` the end-plate
//! gain of the body (Fig. 5.3.1.1-22a, against the span over the body depth
//! at the tail and the panel taper ratio); `A_v(HB) / A_v(B)` that of the
//! horizontal tail (Fig. 5.3.1.1-22b, against its height on the panel and
//! its chordwise place); `K_H` the size of the horizontal tail against the
//! panel (Fig. 5.3.1.1-22c). The lift-curve slope of the panel follows from
//! `A_eff` (Step 2), and so does its induced drag: the effective span is
//! `b_eff^2 = A_eff S_v` on the panel area measured to the centreline.
//!
//! The chart curves are read at the printed grid; a value beyond a chart's
//! edge is held at that edge. The sample problem of the section (printed
//! p. 5.3.1.1-6) is reproduced in the tests.

/// Fig. 5.3.1.1-22a, taper ratio 0.6 and below: `(b_v / 2 r_1, A_v(B)/A_v)`.
const BODY_GAIN_TAPERED: [(f64, f64); 17] = [
    (0.0, 0.0),
    (0.25, 0.93),
    (0.5, 1.16),
    (1.0, 1.446),
    (1.5, 1.594),
    (1.75, 1.627),
    (2.0, 1.627),
    (2.25, 1.60),
    (2.5, 1.526),
    (2.75, 1.439),
    (3.0, 1.348),
    (3.5, 1.218),
    (4.0, 1.138),
    (4.5, 1.096),
    (5.0, 1.057),
    (6.0, 1.033),
    (7.0, 1.024),
];

/// Fig. 5.3.1.1-22a, taper ratio 1.0.
const BODY_GAIN_UNTAPERED: [(f64, f64); 14] = [
    (0.0, 0.0),
    (0.25, 0.74),
    (0.5, 0.998),
    (1.0, 1.311),
    (1.5, 1.459),
    (2.0, 1.508),
    (2.5, 1.428),
    (3.0, 1.286),
    (3.5, 1.163),
    (4.0, 1.102),
    (4.5, 1.065),
    (5.0, 1.044),
    (6.0, 1.033),
    (7.0, 1.024),
];

/// Fig. 5.3.1.1-22b: `A_v(HB)/A_v(B)` at the tail heights `0, 0.1, .. 1.0`
/// of the panel span above the centreline, one row per `x / c_v` of
/// [`TAIL_POSITIONS`].
const TAIL_GAIN: [[f64; 11]; 4] = [
    [
        1.061, 0.996, 0.945, 0.908, 0.872, 0.862, 0.872, 0.939, 1.073, 1.294, 1.697,
    ],
    [
        1.141, 1.079, 1.013, 0.958, 0.917, 0.899, 0.903, 0.963, 1.086, 1.303, 1.697,
    ],
    [
        1.211, 1.152, 1.073, 1.000, 0.945, 0.917, 0.921, 0.982, 1.100, 1.317, 1.697,
    ],
    [
        1.284, 1.189, 1.101, 1.015, 0.963, 0.932, 0.936, 1.000, 1.119, 1.330, 1.697,
    ],
];

/// The `x / c_v` of the [`TAIL_GAIN`] rows: horizontal-tail aerodynamic
/// centre aft of the panel leading edge, over the panel chord, both at the
/// height of the horizontal tail.
const TAIL_POSITIONS: [f64; 4] = [0.5, 0.6, 0.7, 0.8];

/// Fig. 5.3.1.1-22c: `(S_H / S_v, K_H)`.
const TAIL_SIZE: [(f64, f64); 11] = [
    (0.0, 0.0),
    (0.2, 0.285),
    (0.4, 0.506),
    (0.6, 0.700),
    (0.8, 0.823),
    (1.0, 0.912),
    (1.2, 0.980),
    (1.4, 1.035),
    (1.6, 1.072),
    (1.8, 1.101),
    (2.0, 1.127),
];

/// The vertical panel and what surrounds it, as Method 1 measures them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FinPanel {
    /// Panel span from the body centreline to the tip, `b_v`, m.
    pub span_m: f64,
    /// Panel area to the body centreline, `S_v`, m^2.
    pub area_m2: f64,
    /// Tip over centreline chord of that panel, `lambda_v`.
    pub taper_ratio: f64,
    /// Body depth in the region of the panel, `2 r_1`, m.
    pub body_depth_m: f64,
    /// Horizontal tail, if any: its area (both sides) `S_H` [m^2], the
    /// height of its root chord above the centreline over `b_v`, and its
    /// `x / c_v`.
    pub horizontal_tail: Option<(f64, f64, f64)>,
}

/// Piecewise-linear interpolation in `(x, y)` stations sorted by `x`, held
/// at the end values outside them.
fn chart(stations: &[(f64, f64)], x: f64) -> f64 {
    let (first, last) = (stations[0], stations[stations.len() - 1]);
    if x <= first.0 {
        return first.1;
    }
    if x >= last.0 {
        return last.1;
    }
    let upper = stations
        .iter()
        .position(|(sx, _)| *sx >= x)
        .unwrap_or(stations.len() - 1);
    let ((x0, y0), (x1, y1)) = (stations[upper - 1], stations[upper]);
    y0 + (y1 - y0) * (x - x0) / (x1 - x0)
}

/// Fig. 5.3.1.1-22a, linear in the taper ratio between the two curves.
fn body_gain(span_over_depth: f64, taper_ratio: f64) -> f64 {
    let tapered = chart(&BODY_GAIN_TAPERED, span_over_depth);
    let untapered = chart(&BODY_GAIN_UNTAPERED, span_over_depth);
    let t = ((taper_ratio - 0.6) / 0.4).clamp(0.0, 1.0);
    tapered + (untapered - tapered) * t
}

/// Fig. 5.3.1.1-22b, linear in both the height and the chordwise place.
fn tail_gain(height_ratio: f64, position_ratio: f64) -> f64 {
    let heights: Vec<(f64, usize)> = (0..11).map(|i| (i as f64 / 10.0, i)).collect();
    let row = |values: &[f64; 11]| {
        let stations: Vec<(f64, f64)> = heights.iter().map(|(h, i)| (*h, values[*i])).collect();
        chart(&stations, height_ratio)
    };
    let stations: Vec<(f64, f64)> = TAIL_POSITIONS
        .iter()
        .zip(&TAIL_GAIN)
        .map(|(x, values)| (*x, row(values)))
        .collect();
    chart(&stations, position_ratio)
}

/// `A_eff` of Eq. 5.3.1.1-a, or `None` for a degenerate panel.
#[must_use]
pub fn fin_effective_aspect_ratio(panel: FinPanel) -> Option<f64> {
    let valid = [panel.span_m, panel.area_m2, panel.body_depth_m]
        .iter()
        .all(|v| v.is_finite() && *v > 0.0)
        && panel.taper_ratio.is_finite()
        && panel.taper_ratio >= 0.0;
    if !valid {
        return None;
    }
    let geometric = panel.span_m * panel.span_m / panel.area_m2;
    let with_body = body_gain(panel.span_m / panel.body_depth_m, panel.taper_ratio) * geometric;
    let tail = match panel.horizontal_tail {
        Some((area_m2, height_ratio, position_ratio))
            if area_m2.is_finite()
                && area_m2 > 0.0
                && height_ratio.is_finite()
                && position_ratio.is_finite() =>
        {
            chart(&TAIL_SIZE, area_m2 / panel.area_m2)
                * (tail_gain(height_ratio, position_ratio) - 1.0)
        }
        _ => 0.0,
    };
    let effective = with_body * (1.0 + tail);
    (effective.is_finite() && effective > 0.0).then_some(effective)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_method_reproduces_the_sample_problem_of_the_section() {
        // Printed p. 5.3.1.1-6: A_v 1.50, b_v 15.23 in, r_1 2.8 in, taper
        // 0.160, S_H/S_v 0.820, z_H 0, x/c_v 0.55; the charts read 1.47,
        // K_H 0.83 and 1.10, and A_eff 2.38.
        let area = 15.23 * 15.23 / 1.50;
        let panel = FinPanel {
            span_m: 15.23,
            area_m2: area,
            taper_ratio: 0.160,
            body_depth_m: 5.6,
            horizontal_tail: Some((0.820 * area, 0.0, 0.55)),
        };
        assert!((body_gain(15.23 / 5.6, 0.160) - 1.47).abs() < 0.03);
        assert!((chart(&TAIL_SIZE, 0.820) - 0.83).abs() < 0.01);
        assert!((tail_gain(0.0, 0.55) - 1.10).abs() < 0.01);
        let a_eff = fin_effective_aspect_ratio(panel).unwrap();
        assert!((a_eff - 2.38).abs() < 0.05, "{a_eff}");
        // The T-tail reading of the section's second sample (1.70 at the tip).
        assert!((tail_gain(1.0, 0.7) - 1.70).abs() < 0.01);
        // No horizontal tail: the body gain alone; a thin body: the panel.
        let bare = FinPanel {
            horizontal_tail: None,
            ..panel
        };
        let with_body = fin_effective_aspect_ratio(bare).unwrap();
        assert!((with_body / 1.50 - body_gain(15.23 / 5.6, 0.160)).abs() < 1e-12);
        let thin = FinPanel {
            body_depth_m: 0.5,
            ..bare
        };
        assert!((fin_effective_aspect_ratio(thin).unwrap() / 1.50 - 1.024).abs() < 1e-12);
        assert!(fin_effective_aspect_ratio(FinPanel {
            area_m2: 0.0,
            ..panel
        })
        .is_none());
    }
}
