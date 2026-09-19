// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Small numeric and geometric primitives [`super::WingStructureGeometry`]'s
//! methods share (interpolation, spacing, rounding, line intersection)
//! kept apart from the wingbox logic that calls them since none of them is
//! specific to a wing.

use crate::aircraft::airfoil::Airfoil;

/// `(x_upper, z_upper, x_lower, z_lower)`, each ascending in `x` and
/// normalized to unit chord: native aerodynamic model's own coordinate orientation
/// isn't guaranteed leading-to-trailing edge, so this sorts explicitly
/// rather than assume it.
pub(super) fn airfoil_surfaces(airfoil: &Airfoil) -> (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>) {
    let mut upper: Vec<(f64, f64)> = airfoil.upper_coordinates().to_vec();
    let mut lower: Vec<(f64, f64)> = airfoil.lower_coordinates().to_vec();
    upper.sort_by(|a, b| a.0.total_cmp(&b.0));
    lower.sort_by(|a, b| a.0.total_cmp(&b.0));
    (
        upper.iter().map(|p| p.0).collect(),
        upper.iter().map(|p| p.1).collect(),
        lower.iter().map(|p| p.0).collect(),
        lower.iter().map(|p| p.1).collect(),
    )
}

/// Intersection of segment `a -> b` (parametrized by `t` in `[0, 1]`) with
/// the ray `p + s * d`, `s >= 0` expected by the caller. Returns
/// `Some((s, t))`, or `None` if the segment and the ray's line are parallel.
///
/// Returns both parameters together rather than the Python source's
/// `(Option<f64>, Option<f64>)` (always `None` in lockstep there): a single
/// `Option` over the pair is the same information without a state the type
/// cannot actually reach.
pub(super) fn intersect_line_ray(
    a: (f64, f64),
    b: (f64, f64),
    p: (f64, f64),
    d: (f64, f64),
) -> Option<(f64, f64)> {
    let (xa, ya) = a;
    let (xb, yb) = b;
    let (xp, yp) = p;
    let (dx, dy) = d;

    let dx_seg = xb - xa;
    let dy_seg = yb - ya;

    let det = dx * dy_seg - dy * dx_seg;
    if det.abs() < 1e-8 {
        return None;
    }

    let s = ((xa - xp) * dy_seg - (ya - yp) * dx_seg) / det;
    let t = if dy_seg.abs() > dx_seg.abs() {
        (yp + s * dy - ya) / dy_seg
    } else {
        (xp + s * dx - xa) / dx_seg
    };
    Some((s, t))
}

/// Piecewise-linear interpolation matching `numpy.interp(x, xp, fp)`'s
/// default clamp behaviour: `x` outside `[xp[0], xp[-1]]` clamps to the
/// nearest endpoint's `fp` value rather than extrapolating.
///
/// Duplicated from the equivalent helper in `aircraft::airfoil` and
/// `airfoil_library` rather than shared, since both of those are out of
/// scope for this module to touch (see `docs/PORTING.md`).
pub(super) fn clamped_interp(x: f64, xp: &[f64], fp: &[f64]) -> f64 {
    let Some(&first_x) = xp.first() else {
        return f64::NAN; // Nothing here calls this with an empty surface.
    };
    let last = xp.len() - 1;
    if x <= first_x {
        return fp[0];
    }
    if x >= xp[last] {
        return fp[last];
    }
    for i in 1..xp.len() {
        if x <= xp[i] {
            let (x0, x1) = (xp[i - 1], xp[i]);
            let (y0, y1) = (fp[i - 1], fp[i]);
            if x1 == x0 {
                return y0;
            }
            return y0 + (y1 - y0) * (x - x0) / (x1 - x0);
        }
    }
    fp[last]
}

/// Evenly spaced points from `start` to `stop`, inclusive: NumPy's
/// `linspace(start, stop, num, endpoint=True)`. A copy of
/// `aircraft::spacing::linspace`, which is reachable from here — the copy is
/// historical, not a visibility workaround.
pub(super) fn linspace(start: f64, stop: f64, num: usize) -> Vec<f64> {
    if num == 0 {
        return Vec::new();
    }
    if num == 1 {
        return vec![start];
    }
    let step = (stop - start) / (num - 1) as f64;
    let mut values: Vec<f64> = (0..num).map(|i| start + i as f64 * step).collect();
    let last = values.len() - 1;
    values[last] = stop;
    values
}

/// `x` rounded to 6 decimal places: `np.round(x, 6)`.
///
/// NumPy rounds halfway cases to even; this rounds halfway cases away from
/// zero, `f64::round`'s behaviour. The chord fractions this module rounds
/// are quotients of measured wing geometry, not constructed to land exactly
/// on a `..5` boundary at the sixth decimal, so the two rounding rules are
/// not expected to disagree on real input.
pub(super) fn round6(x: f64) -> f64 {
    (x * 1e6).round() / 1e6
}

/// Whether `a` and `b` agree within NumPy's default `np.isclose` bounds:
/// `|a - b| <= atol + rtol * |b|`, `rtol = 1e-5`, `atol = 1e-8`.
pub(super) fn is_close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-8 + 1e-5 * b.abs()
}

/// The index of `values`'s entry closest to `target`, first occurrence on a
/// tie: `np.argmin(np.abs(values - target))`.
pub(super) fn argmin_abs_diff(values: &[f64], target: f64) -> usize {
    let mut best_index = 0;
    let mut best_diff = f64::INFINITY;
    for (index, &value) in values.iter().enumerate() {
        let diff = (value - target).abs();
        if diff < best_diff {
            best_diff = diff;
            best_index = index;
        }
    }
    best_index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersect_line_ray_returns_none_for_parallel_lines() {
        // The segment and the ray both run purely in +X, so their
        // determinant is exactly zero regardless of offset.
        let hit = intersect_line_ray((0.0, 0.0), (1.0, 0.0), (0.0, 5.0), (1.0, 0.0));
        assert_eq!(hit, None);
    }

    #[test]
    fn intersect_line_ray_finds_the_midpoint_of_a_perpendicular_crossing() {
        let hit = intersect_line_ray((0.0, 0.0), (0.0, 2.0), (-1.0, 1.0), (1.0, 0.0));
        let (s, t) = hit.expect("a horizontal ray crosses a vertical segment");
        assert!((s - 1.0).abs() < 1e-12, "s={s}");
        assert!((t - 0.5).abs() < 1e-12, "t={t}");
    }

    #[test]
    fn round6_matches_a_direct_scale_and_round() {
        assert_eq!(round6(0.123_456_7), 0.123_457);
        assert_eq!(round6(1.0), 1.0);
    }

    #[test]
    fn is_close_matches_numpys_default_bounds() {
        assert!(is_close(1.000_001, 1.0));
        assert!(!is_close(1.001, 1.0));
        assert!(is_close(0.0, 1e-9));
    }
}
