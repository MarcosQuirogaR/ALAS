// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Axis-range and sampling helpers shared by more than one figure family.
//!
//! Family-local variants that differ in padding or end-point handling stay
//! in their families: they are pinned by figure fixtures, and merging them
//! would move plotted ranges.

/// The padded `(min, max)` of every finite value in `values`, or `(0, 1)` if
/// none is finite. `pad_frac` widens each side by that fraction of the span,
/// matching matplotlib's default 5% autoscale margin at `0.05`.
pub(crate) fn padded_range(values: impl Iterator<Item = f64>, pad_frac: f64) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for v in values {
        if v.is_finite() {
            lo = lo.min(v);
            hi = hi.max(v);
        }
    }
    if !lo.is_finite() || !hi.is_finite() {
        return (0.0, 1.0);
    }
    let span = (hi - lo).max(1e-9);
    let pad = span * pad_frac;
    (lo - pad, hi + pad)
}

/// `n` evenly spaced samples from `start` to `stop` inclusive, as
/// `numpy.linspace`: the last sample is exactly `stop`, not the accumulated
/// `start + (n - 1) * step`. The geometry crate's implementation, shared so
/// the figures sample exactly as the geometry does.
pub(crate) use alas_geom::aircraft::spacing::linspace;

/// Two axis ranges, one per pixel extent (`u_px`, `v_px`), that share a
/// single data-units-per-pixel scale and are centred on each data interval:
/// the self-contained substitute for `ax.set_aspect("equal")` that
/// [`crate::scene::Axes2D`] cannot do on its own. `pad_frac` grows both data
/// intervals before fitting, so drawn geometry does not touch the panel
/// frame.
pub(crate) fn equal_aspect_ranges(
    u_lo: f64,
    u_hi: f64,
    u_px: f64,
    v_lo: f64,
    v_hi: f64,
    v_px: f64,
    pad_frac: f64,
) -> ((f64, f64), (f64, f64)) {
    let u_span = (u_hi - u_lo).abs().max(1e-6) * (1.0 + pad_frac);
    let v_span = (v_hi - v_lo).abs().max(1e-6) * (1.0 + pad_frac);
    let scale = (u_px / u_span).min(v_px / v_span);
    let u_half = u_px / scale / 2.0;
    let v_half = v_px / scale / 2.0;
    let u_c = (u_lo + u_hi) / 2.0;
    let v_c = (v_lo + v_hi) / 2.0;
    ((u_c - u_half, u_c + u_half), (v_c - v_half, v_c + v_half))
}

#[cfg(test)]
mod tests {
    use super::{equal_aspect_ranges, linspace, padded_range};

    #[test]
    fn linspace_ends_exactly_on_stop() {
        assert!(linspace(0.0, 1.0, 0).is_empty());
        assert_eq!(linspace(2.0, 5.0, 1), vec![2.0]);
        let values = linspace(0.1, 0.7, 7);
        assert_eq!(values.len(), 7);
        assert_eq!(values[6], 0.7);
    }

    #[test]
    fn padded_range_ignores_non_finite_values() {
        assert_eq!(padded_range([f64::NAN].into_iter(), 0.05), (0.0, 1.0));
        let (lo, hi) = padded_range([1.0, f64::INFINITY, 3.0].into_iter(), 0.5);
        assert_eq!((lo, hi), (0.0, 4.0));
    }

    #[test]
    fn equal_aspect_ranges_share_one_scale() {
        let ((u0, u1), (v0, v1)) = equal_aspect_ranges(0.0, 10.0, 200.0, 0.0, 1.0, 100.0, 0.0);
        assert!(((u1 - u0) / 200.0 - (v1 - v0) / 100.0).abs() < 1e-12);
        assert_eq!((u0, u1), (0.0, 10.0));
    }
}
