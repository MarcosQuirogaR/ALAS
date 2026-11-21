// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Small helpers shared by more than one aerodynamics figure: computing an
//! axis range from real data (matplotlib's autoscale has no direct
//! equivalent in [`crate::scene::Axes2D`], which takes an explicit range),
//! and drawing heatmap cells that skip rather than color a non-finite value.

use crate::colormap::Colormap;
use crate::scene::{Axes2D, Fill, Scene, SceneElement};

pub(super) use crate::families::common::padded_range;

/// Cell boundaries for `n` sample points taken at the midpoint of each cell,
/// on a linear axis: interior edges are the midpoint of each adjacent pair,
/// and the two outer edges are the first and last sample themselves (matching
/// how `pcolormesh`/`contourf`-style figures extend to the data's own
/// extremes rather than one further half-cell out).
pub(super) fn cell_edges_linear(centers: &[f64]) -> Vec<f64> {
    cell_edges(centers, |a, b| 0.5 * (a + b))
}

/// The same construction as [`cell_edges_linear`], but the interior edges are
/// the *geometric* mean of each adjacent pair, which is the natural midpoint on a
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
/// colormap's floor color. The NeuralFoil sweep can genuinely fail to
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
    scene
        .data_trace
        .note_heatmap(axes, x_edges, y_edges, values);
    let _chrome = crate::plot_data::ChromeGuard::begin();
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

/// Map from the geometric angle a lattice solve was flown at to the reporting
/// angle of the polar sweep.
///
/// On the compressible lattice (A1) the polar is solved at the true angle, so
/// the map is the identity. With the lattice off, the sweep pairs an
/// incompressible lattice CL with the whole-wing Prandtl-Glauert angle
/// `alpha_0 + beta (alpha - alpha_0)`, so that the curve plotted against the
/// reporting angle has the compressible slope `CL_alpha / beta`; an external
/// compressible solver (VSPAERO, flown at the reporting angles) is compared
/// on that same axis. Every figure that draws the sweep uses this one axis,
/// and incompressible analytical references (lifting line, Helmbold) are
/// carried onto it with the same affine map, so no curve sits on a different
/// angle basis from the lattice it is compared with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ReportingAxis {
    scale: f64,
    offset: f64,
}

impl ReportingAxis {
    /// Least-squares affine fit through the sweep's (geometric, reporting)
    /// angle pairs; the identity when the sweep carries no usable pairs.
    pub(super) fn of(polar: &alas_aero::analysis::PolarSweep) -> Self {
        let pairs: Vec<(f64, f64)> = polar
            .geometric_alpha_deg
            .iter()
            .copied()
            .zip(polar.alpha_deg.iter().copied())
            .filter(|(g, r)| g.is_finite() && r.is_finite())
            .collect();
        let n = pairs.len() as f64;
        if pairs.len() < 2 {
            return Self::identity();
        }
        let (sx, sy) = pairs.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
        let (mx, my) = (sx / n, sy / n);
        let sxx: f64 = pairs.iter().map(|p| (p.0 - mx).powi(2)).sum();
        if sxx < 1e-12 {
            return Self::identity();
        }
        let sxy: f64 = pairs.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
        let scale = sxy / sxx;
        Self {
            scale,
            offset: my - scale * mx,
        }
    }

    /// The affine map `reporting = scale * geometric + offset`, degrees.
    pub(super) const fn affine(scale: f64, offset: f64) -> Self {
        Self { scale, offset }
    }

    const fn identity() -> Self {
        Self {
            scale: 1.0,
            offset: 0.0,
        }
    }

    /// The reporting angle of a geometric angle, degrees.
    pub(super) fn map(&self, geometric_deg: f64) -> f64 {
        self.scale * geometric_deg + self.offset
    }
}

#[cfg(test)]
mod reporting_axis_tests {
    use super::*;
    use alas_aero::analysis::PolarSweep;

    fn polar(geometric: Vec<f64>, reporting: Vec<f64>) -> PolarSweep {
        let n = geometric.len();
        PolarSweep {
            alpha_deg: reporting,
            geometric_alpha_deg: geometric,
            compressible_mach: None,
            cl: vec![0.0; n],
            cd: vec![0.0; n],
            cd_induced: vec![0.0; n],
            cd_wave: vec![0.0; n],
            cd_parasite: vec![0.0; n],
            cm: vec![0.0; n],
            l_over_d: vec![0.0; n],
        }
    }

    #[test]
    fn the_axis_recovers_the_affine_relabel_and_carries_a_reference_onto_it() {
        // alpha_0 = -1.5 deg, beta = 0.626: reporting = alpha_0 + beta (g - alpha_0).
        let beta = 0.626;
        let geometric = vec![-4.0, 0.0, 4.0, 8.0];
        let reporting: Vec<f64> = geometric.iter().map(|g| -1.5 + beta * (g + 1.5)).collect();
        let axis = ReportingAxis::of(&polar(geometric, reporting));
        assert!((axis.map(2.0) - (-1.5 + beta * 3.5)).abs() < 1e-12);
        assert!((axis.map(-1.5) + 1.5).abs() < 1e-12);
    }

    #[test]
    fn a_sweep_without_pairs_is_left_on_the_geometric_angle() {
        let axis = ReportingAxis::of(&polar(vec![1.0], vec![0.5]));
        assert_eq!(axis.map(3.0), 3.0);
    }
}

/// Static note stating the basis of the angle axis (translated by the display
/// catalog).
pub(super) const ALPHA_AXIS_NOTE: &str =
    "Angle axis: compressibility-corrected angle at the lift of the incompressible lattice sweep";

/// Draw [`ALPHA_AXIS_NOTE`] centred at `x`, `y`.
pub(super) fn draw_alpha_axis_note(scene: &mut Scene, x: f64, y: f64, pal: &crate::theme::Palette) {
    scene.add(SceneElement::Text {
        text: ALPHA_AXIS_NOTE.to_owned(),
        pos: [x, y],
        font_size: 8.0,
        color: crate::scene::Color::from_hex(pal.tick),
        align: crate::scene::TextAlign::Center,
        baseline: crate::scene::TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });
}

/// Static note for the AVL cross-check rows, which are solved and drawn at the
/// geometric angle (the shared condition of the external incompressible solve).
pub(super) const CROSS_CHECK_AXIS_NOTE: &str =
    "Cross-check rows: geometric angle of the incompressible solves";

/// Draw [`CROSS_CHECK_AXIS_NOTE`] centred at `x`, `y`.
pub(super) fn draw_cross_check_axis_note(
    scene: &mut Scene,
    x: f64,
    y: f64,
    pal: &crate::theme::Palette,
) {
    scene.add(SceneElement::Text {
        text: CROSS_CHECK_AXIS_NOTE.to_owned(),
        pos: [x, y],
        font_size: 8.0,
        color: crate::scene::Color::from_hex(pal.tick),
        align: crate::scene::TextAlign::Center,
        baseline: crate::scene::TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });
}

/// The final path component, for messages that are drawn into a figure and
/// exported: the full case path stays in the run log.
pub(super) fn file_name_of(path: &std::path::Path) -> String {
    path.file_name().map_or_else(
        || "file".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// The reason a file could not be read, in words a figure can carry without
/// the operating system's own message or code.
pub(super) fn file_failure_reason(error: &std::io::Error) -> &'static str {
    match error.kind() {
        std::io::ErrorKind::NotFound => "file not found",
        std::io::ErrorKind::PermissionDenied => "permission denied",
        _ => "could not be read",
    }
}
