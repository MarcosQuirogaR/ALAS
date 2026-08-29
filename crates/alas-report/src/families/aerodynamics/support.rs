// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Small helpers shared by more than one aerodynamics figure: computing an
//! axis range from real data (matplotlib's autoscale has no direct
//! equivalent in [`crate::scene::Axes2D`], which takes an explicit range),
//! and drawing heatmap cells that skip rather than color a non-finite value.

use crate::colormap::Colormap;
use crate::scene::{Axes2D, Fill, Scene, SceneElement};

/// The padded `(min, max)` of every finite value in `values`, or `(0, 1)` if
/// none is finite. `pad_frac` widens each side by that fraction of the span,
/// matching matplotlib's default 5% autoscale margin at `0.05`.
pub(super) fn padded_range(values: impl Iterator<Item = f64>, pad_frac: f64) -> (f64, f64) {
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

/// Cell boundaries for `n` sample points taken at the midpoint of each cell,
/// on a linear axis: interior edges are the midpoint of each adjacent pair,
/// and the two outer edges are the first and last sample themselves (matching
/// how `pcolormesh`/`contourf`-style figures extend to the data's own
/// extremes rather than one further half-cell out).
pub(super) fn cell_edges_linear(centers: &[f64]) -> Vec<f64> {
    cell_edges(centers, |a, b| 0.5 * (a + b))
}

/// The same construction as [`cell_edges_linear`], but the interior edges are
/// the *geometric* mean of each adjacent pair -- the natural midpoint on a
/// log-scaled axis, matching the Reynolds-number axis this crate's contour
/// figures use.
pub(super) fn cell_edges_log(centers: &[f64]) -> Vec<f64> {
    cell_edges(centers, |a, b| (a * b).sqrt())
}

fn cell_edges(centers: &[f64], midpoint: impl Fn(f64, f64) -> f64) -> Vec<f64> {
    if centers.is_empty() {
        return Vec::new();
    }
    let mut edges = Vec::with_capacity(centers.len() + 1);
    edges.push(centers[0]);
    for pair in centers.windows(2) {
        edges.push(midpoint(pair[0], pair[1]));
    }
    edges.push(centers[centers.len() - 1]);
    edges
}

/// Draw one colored rectangle per grid cell, as [`Axes2D::add_heatmap_grid`]
/// does, except a non-finite `values` entry draws nothing rather than the
/// colormap's floor color -- the NeuralFoil sweep can genuinely fail to
/// converge at a given (Re, alpha), and a blank cell says so; a colored one
/// would claim a value that was never computed.
#[allow(clippy::too_many_arguments)]
pub(super) fn add_heatmap_grid_nan_aware(
    axes: &Axes2D,
    scene: &mut Scene,
    x_edges: &[f64],
    y_edges: &[f64],
    values: &[f64],
    cmap: Colormap,
    vmin: f64,
    vmax: f64,
) {
    let nx = x_edges.len().saturating_sub(1);
    let ny = y_edges.len().saturating_sub(1);
    let span = (vmax - vmin).max(1e-12);
    for iy in 0..ny {
        for ix in 0..nx {
            let v = values[iy * nx + ix];
            if !v.is_finite() {
                continue;
            }
            let t = ((v - vmin) / span).clamp(0.0, 1.0);
            let color = cmap.sample(t);
            let p0 = axes.map_point(x_edges[ix], y_edges[iy]);
            let p1 = axes.map_point(x_edges[ix + 1], y_edges[iy + 1]);
            let (x0, x1) = (p0[0].min(p1[0]), p0[0].max(p1[0]));
            let (y0, y1) = (p0[1].min(p1[1]), p0[1].max(p1[1]));
            scene.add(SceneElement::Rect {
                x: x0,
                y: y0,
                width: (x1 - x0).max(0.5),
                height: (y1 - y0).max(0.5),
                rx: 0.0,
                fill: Some(Fill::new(color)),
                stroke: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padded_range_widens_both_sides_by_the_requested_fraction() {
        let (lo, hi) = padded_range([1.0, 2.0, 3.0].into_iter(), 0.1);
        assert!((lo - 0.8).abs() < 1e-9);
        assert!((hi - 3.2).abs() < 1e-9);
    }

    #[test]
    fn padded_range_ignores_non_finite_values() {
        let (lo, hi) = padded_range([f64::NAN, 1.0, f64::INFINITY, 4.0].into_iter(), 0.0);
        assert_eq!((lo, hi), (1.0, 4.0));
    }

    #[test]
    fn padded_range_falls_back_to_unit_interval_with_no_finite_data() {
        assert_eq!(padded_range(std::iter::empty(), 0.1), (0.0, 1.0));
    }

    #[test]
    fn linear_cell_edges_bracket_every_center_at_its_midpoint() {
        let edges = cell_edges_linear(&[0.0, 1.0, 3.0]);
        assert_eq!(edges, vec![0.0, 0.5, 2.0, 3.0]);
    }

    #[test]
    fn log_cell_edges_use_the_geometric_mean() {
        let edges = cell_edges_log(&[1.0, 100.0]);
        assert_eq!(edges, vec![1.0, 10.0, 100.0]);
    }

    #[test]
    fn nan_aware_heatmap_skips_non_finite_cells() {
        let axes = Axes2D::new((0.0, 0.0, 200.0, 100.0), (0.0, 2.0), (0.0, 1.0));
        let mut scene = Scene::new(200.0, 100.0, None);
        let x_edges = [0.0, 1.0, 2.0];
        let y_edges = [0.0, 1.0];
        let values = [f64::NAN, 1.0];
        add_heatmap_grid_nan_aware(
            &axes,
            &mut scene,
            &x_edges,
            &y_edges,
            &values,
            Colormap::Viridis,
            0.0,
            1.0,
        );
        assert_eq!(scene.elements.len(), 1);
    }
}
