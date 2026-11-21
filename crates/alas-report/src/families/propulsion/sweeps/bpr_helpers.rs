// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Pure helpers of the bypass-ratio sensitivity figure, kept apart so the figure
//! file stays under the repository's source-size limit.

/// Index of the first interior node at which the curve has a corner, i.e. a
/// slope change far larger than the curve's own smooth curvature and than 3 % of
/// its range: the signature of a solver branch or limit switch in the cycle, not of
/// the physics. `None` for a smooth curve.
pub(super) fn branch_switch_index(points: &[(f64, f64)]) -> Option<usize> {
    if points.len() < 6 {
        return None;
    }
    let (lo, hi) = points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p.1), hi.max(p.1))
        });
    let range = (hi - lo).max(f64::EPSILON);
    let slope = |i: usize| {
        (points[i + 1].1 - points[i].1) / (points[i + 1].0 - points[i].0).max(f64::EPSILON)
    };
    // Slope change at node i, scaled by the node spacing so it is a rise over the step.
    let bend = |i: usize| {
        let step = points[i + 1].0 - points[i - 1].0;
        (slope(i) - slope(i - 1)).abs() * step * 0.5
    };
    (2..points.len() - 2).find(|&i| {
        let here = bend(i);
        let neighbours = bend(i - 1).max(bend(i + 1));
        here > 0.03 * range && here > 1.6 * neighbours
    })
}

/// Linear interpolation of `points` (increasing x) at `x`; `None` outside them.
pub(super) fn interpolate(points: &[(f64, f64)], x: f64) -> Option<f64> {
    points.windows(2).find_map(|w| {
        (x >= w[0].0 && x <= w[1].0 && w[1].0 > w[0].0)
            .then(|| w[0].1 + (w[1].1 - w[0].1) * (x - w[0].0) / (w[1].0 - w[0].0))
    })
}

/// Round tick values inside `[lo, hi]`, about `target` of them, with the number of
/// decimals that shows the step.
pub(super) fn nice_ticks(lo: f64, hi: f64, target: usize) -> (Vec<f64>, usize) {
    let span = hi - lo;
    if !(span.is_finite() && span > 0.0) {
        return (Vec::new(), 0);
    }
    let raw = span / target.max(1) as f64;
    let magnitude = 10_f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0]
        .iter()
        .map(|m| m * magnitude)
        .find(|s| *s >= raw)
        .unwrap_or(10.0 * magnitude);
    let first = (lo / step).ceil();
    let ticks: Vec<f64> = (0..)
        .map(|k| (first + f64::from(k)) * step)
        .take_while(|v| *v <= hi + 1e-9 * span)
        .collect();
    let decimals = (0..=4_usize)
        .find(|d| {
            let scaled = step * 10_f64.powi(*d as i32);
            (scaled - scaled.round()).abs() < 1e-6
        })
        .unwrap_or(4);
    (ticks, decimals)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_smooth_convex_curve_has_no_switch_and_a_corner_is_found() {
        let smooth: Vec<(f64, f64)> = (0..40)
            .map(|i| {
                let x = 2.7 + 0.37 * f64::from(i);
                (x, 700.0 / x)
            })
            .collect();
        assert_eq!(branch_switch_index(&smooth), None);
        let mut kinked = smooth.clone();
        for (i, p) in kinked.iter_mut().enumerate().skip(30) {
            p.1 = smooth[30].1 + 8.0 * (i - 30) as f64;
        }
        assert_eq!(branch_switch_index(&kinked), Some(30));
    }

    #[test]
    fn interpolation_and_ticks() {
        assert_eq!(interpolate(&[(0.0, 0.0), (2.0, 4.0)], 1.0), Some(2.0));
        assert_eq!(interpolate(&[(0.0, 0.0), (2.0, 4.0)], 3.0), None);
        let (ticks, decimals) = nice_ticks(12.5, 24.7, 5);
        assert_eq!(ticks, vec![12.5, 15.0, 17.5, 20.0, 22.5]);
        assert_eq!(decimals, 1);
        let (ticks, decimals) = nice_ticks(0.12, 0.31, 4);
        assert!(ticks.len() >= 3 && decimals >= 2, "{ticks:?} {decimals}");
    }
}
