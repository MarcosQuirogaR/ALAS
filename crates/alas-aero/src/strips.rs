// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Spanwise strips of one lifting surface of a solved lattice.
//!
//! A strip is the run of chordwise panels the mesh built between two adjacent
//! spanwise sections, listed leading edge to trailing edge in
//! [`crate::vlm::VlmResult::panels`] and ending at a panel with
//! `is_trailing_edge`. [`wing_strips`] is the single place that groups the
//! panels this way: [`crate::wing_analysis`] builds its span-load table from
//! it, and a caller that needs the strip layout (for example to apply a viscous
//! section correction with
//! [`crate::vlm::VlmSystem::solve_with_incidence_offsets`]) reads the same
//! grouping.
//!
//! Frames and units: geometry axes (x aft, y right, z up), metres, newtons.
//! The strips are returned in lattice order, not sorted by span.

use crate::operating_point::{AxisFrame, OperatingPoint};
use crate::vlm::VlmResult;

/// One chordwise strip of a lifting surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WingStrip {
    /// Index of the surface in the airplane's wing list.
    pub wing_index: usize,
    /// Index of the strip's leading-edge panel in [`VlmResult::panels`].
    pub first_panel: usize,
    /// Number of chordwise panels in the strip.
    pub panel_count: usize,
    /// Spanwise coordinate of the strip centre, m (y of the leading edge).
    pub y_m: f64,
    /// Strip width along y, m.
    pub width_m: f64,
    /// Chord, m: distance from the leading-edge midpoint to the trailing-edge
    /// midpoint on the meshed camber surface.
    pub chord_m: f64,
    /// Leading-edge midpoint, geometry axes, m.
    pub leading_edge_m: [f64; 3],
    /// Trailing-edge midpoint, geometry axes, m.
    pub trailing_edge_m: [f64; 3],
    /// Lift carried by the strip, N, wind axes (sum of the panel lifts).
    pub lift_n: f64,
}

/// One panel's lift, N: the wind-axis `-z` component of its geometry-axis
/// force.
pub(crate) fn panel_lift(force: [f64; 3], point: &OperatingPoint) -> f64 {
    let (_, _, wind_z) = point.convert_axes(
        force[0],
        force[1],
        force[2],
        AxisFrame::Geometry,
        AxisFrame::Wind,
    );
    -wind_z
}

/// Group the panels of surface `wing_index` of `result` into strips.
///
/// The values are returned as computed, without screening: a degenerate mesh
/// can give a non-finite or zero width or chord, which the caller decides how
/// to treat (the span-load table of [`crate::wing_analysis`] drops such
/// strips).
pub fn wing_strips(
    result: &VlmResult,
    point: &OperatingPoint,
    wing_index: usize,
) -> Vec<WingStrip> {
    let mut strips = Vec::new();
    let mut leading_edge: Option<[f64; 3]> = None;
    let mut first_panel = 0;
    let mut strip_lift = 0.0;
    let mut strip_width = 0.0;
    let mut strip_y = 0.0;
    for (index, (panel, force)) in result
        .panels
        .iter()
        .zip(&result.panel_forces_geometry)
        .enumerate()
    {
        if panel.wing_index != wing_index {
            continue;
        }
        if leading_edge.is_none() {
            leading_edge = Some(midpoint(panel.front_left, panel.front_right));
            first_panel = index;
            strip_width = (panel.front_right[1] - panel.front_left[1]).abs();
            strip_y = 0.5 * (panel.front_right[1] + panel.front_left[1]);
            strip_lift = 0.0;
        }
        strip_lift += panel_lift(*force, point);
        if !panel.is_trailing_edge {
            continue;
        }
        let trailing_edge = midpoint(panel.back_left, panel.back_right);
        let leading = leading_edge.take().unwrap_or(trailing_edge);
        strips.push(WingStrip {
            wing_index,
            first_panel,
            panel_count: index + 1 - first_panel,
            y_m: strip_y,
            width_m: strip_width,
            chord_m: distance(leading, trailing_edge),
            leading_edge_m: leading,
            trailing_edge_m: trailing_edge,
            lift_n: strip_lift,
        });
    }
    strips
}

/// Midpoint of two mesh corners.
fn midpoint(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        0.5 * (left[0] + right[0]),
        0.5 * (left[1] + right[1]),
        0.5 * (left[2] + right[2]),
    ]
}

/// Euclidean distance between two mesh points, m.
fn distance(from: [f64; 3], to: [f64; 3]) -> f64 {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let dz = to[2] - from[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}
