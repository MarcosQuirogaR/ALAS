// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Streamline post-processing for ALAS's in-process vortex-lattice model and
// its per-panel geometry
// arrays (`front_left_vertices`, `back_left_vertices`, `back_right_vertices`,
// `front_right_vertices`, `vortex_centers`, `is_trailing_edge`) its `run()`
// stores on the solved instance.
// Numerical provenance is recorded in docs/PORTING.md and the repository's
// third-party notice.

//! Per-panel mesh geometry read back off a solved [`super::VlmResult`], for
//! figures that need more than the net totals `run` reports: the spanwise
//! lift distribution and the wake streamlines. Both are P11 (Figures)
//! concerns: the vortex-lattice solve itself, and its force summation, are
//! unchanged by anything in this file; [`calculate_streamlines`] only reuses
//! the already-solved circulation strengths to sample the flow field at new
//! points.

use crate::operating_point::OperatingPoint;
use crate::singularities::calculate_induced_velocity_horseshoe;
use crate::vector3::{add3, norm3, scale3};

use super::{TRAILING_VORTEX_DIRECTION, VORTEX_CORE_RADIUS};

/// One panel's raw quad-mesh corners and vortex-lattice points, in `run`'s own
/// front-left/back-left/back-right/front-right order: upstream's
/// `front_left_vertices`/`back_left_vertices`/`back_right_vertices`/
/// `front_right_vertices`/`left_vortex_vertices`/`right_vortex_vertices`/
/// `vortex_centers`/`is_trailing_edge`, one row per panel, aligned with
/// [`super::VlmResult::vortex_strengths`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelSample {
    /// The mesh quad's front-left corner (upstream's `faces[:, 0]`).
    pub front_left: [f64; 3],
    /// The mesh quad's back-left corner (`faces[:, 1]`).
    pub back_left: [f64; 3],
    /// The mesh quad's back-right corner (`faces[:, 2]`).
    pub back_right: [f64; 3],
    /// The mesh quad's front-right corner (`faces[:, 3]`).
    pub front_right: [f64; 3],
    /// The bound vortex's left endpoint, 3/4 back from the front-left corner.
    pub left_vortex_vertex: [f64; 3],
    /// The bound vortex's right endpoint, 3/4 back from the front-right corner.
    pub right_vortex_vertex: [f64; 3],
    /// The midpoint of the bound vortex leg.
    pub vortex_center: [f64; 3],
    /// Whether this panel is the last chordwise panel of its spanwise strip:
    /// upstream's `(arange(len(faces)) + 1) % chordwise_resolution == 0`,
    /// evaluated per wing (including its mirrored half, if symmetric) before
    /// concatenation.
    pub is_trailing_edge: bool,
    /// Which `airplane.wings` entry this panel came from.
    pub wing_index: usize,
}

/// The velocity every horseshoe (`panels[i]`, strength `vortex_strengths[i]`)
/// induces at `points`, plus the freestream and rotation-induced velocity at
/// each point: `get_velocity_at_points`, restated over [`PanelSample`]
/// rather than [`super::Panel`] so it can be called again after the solve, on
/// arbitrary points, for a streamline trace.
fn induced_velocity_field(
    points: &[[f64; 3]],
    panels: &[PanelSample],
    vortex_strengths: &[f64],
    op_point: &OperatingPoint,
) -> Vec<[f64; 3]> {
    let steady_freestream_velocity = op_point.freestream_velocity_geometry_axes();
    let rotation_velocities = op_point.rotation_velocity_geometry_axes(points);
    points
        .iter()
        .zip(rotation_velocities)
        .map(|(&point, rotation_velocity)| {
            let induced = panels.iter().zip(vortex_strengths).fold(
                [0.0, 0.0, 0.0],
                |acc, (panel, &gamma)| {
                    let contribution = calculate_induced_velocity_horseshoe(
                        point,
                        panel.left_vortex_vertex,
                        panel.right_vortex_vertex,
                        TRAILING_VORTEX_DIRECTION,
                        gamma,
                        VORTEX_CORE_RADIUS,
                    );
                    add3(acc, contribution)
                },
            );
            add3(induced, add3(steady_freestream_velocity, rotation_velocity))
        })
        .collect()
}

/// Trace streamlines from `seed_points` through the solved flow field:
/// `VortexLatticeMethod.calculate_streamlines`. Forward-Euler integration with
/// the velocity vector renormalized to a fixed step length at every step;
/// upstream's own doc says why fancier ODE integration is not worth it near a
/// vortex filament's near-singular field. Returns one polyline of up to
/// `n_steps` points per seed, in `[x, y, z]` geometry-axis coordinates.
pub fn calculate_streamlines(
    panels: &[PanelSample],
    vortex_strengths: &[f64],
    op_point: &OperatingPoint,
    seed_points: &[[f64; 3]],
    n_steps: usize,
    length: f64,
) -> Vec<Vec<[f64; 3]>> {
    let n_steps = n_steps.max(1);
    let step_length = if n_steps > 1 {
        length / n_steps as f64
    } else {
        0.0
    };

    let mut lines: Vec<Vec<[f64; 3]>> = seed_points
        .iter()
        .map(|&seed| {
            let mut line = Vec::with_capacity(n_steps);
            line.push(seed);
            line
        })
        .collect();
    let mut current: Vec<[f64; 3]> = seed_points.to_vec();

    for _ in 1..n_steps {
        let velocities = induced_velocity_field(&current, panels, vortex_strengths, op_point);
        for ((point, line), v) in current.iter_mut().zip(lines.iter_mut()).zip(&velocities) {
            let speed = norm3(*v).max(1e-12);
            let step = scale3(*v, step_length / speed);
            *point = add3(*point, step);
            line.push(*point);
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_atmo::Atmosphere;

    fn level_flight_point(alpha: f64) -> OperatingPoint {
        OperatingPoint::new(Atmosphere::new(0.0), 50.0, alpha, 0.0, 0.0, 0.0, 0.0)
    }

    fn single_horseshoe() -> PanelSample {
        PanelSample {
            front_left: [0.0, -2.5, 0.0],
            back_left: [1.0, -2.5, 0.0],
            back_right: [1.0, 2.5, 0.0],
            front_right: [0.0, 2.5, 0.0],
            left_vortex_vertex: [0.75, -2.5, 0.0],
            right_vortex_vertex: [0.75, 2.5, 0.0],
            vortex_center: [0.75, 0.0, 0.0],
            is_trailing_edge: true,
            wing_index: 0,
        }
    }

    #[test]
    fn a_streamline_has_one_point_per_step_starting_at_its_seed() {
        let panels = [single_horseshoe()];
        let op_point = level_flight_point(0.0);
        let seeds = [[1.0, 1.0, 0.0]];
        let lines = calculate_streamlines(&panels, &[1.0], &op_point, &seeds, 5, 10.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].len(), 5);
        assert_eq!(lines[0][0], seeds[0]);
    }

    #[test]
    fn each_step_advances_by_the_fixed_arc_length() {
        // Forward-Euler with a renormalized velocity: consecutive points are
        // exactly `length / n_steps` apart, regardless of the local speed.
        let panels = [single_horseshoe()];
        let op_point = level_flight_point(2.0);
        let seeds = [[-5.0, 0.3, 0.0]];
        let n_steps = 20;
        let length = 8.0;
        let lines = calculate_streamlines(&panels, &[0.6], &op_point, &seeds, n_steps, length);
        let step_length = length / n_steps as f64;
        for pair in lines[0].windows(2) {
            let d = [
                pair[1][0] - pair[0][0],
                pair[1][1] - pair[0][1],
                pair[1][2] - pair[0][2],
            ];
            let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            assert!(
                (dist - step_length).abs() < 1e-9,
                "step length {dist} != {step_length}"
            );
        }
    }

    #[test]
    fn n_steps_of_one_returns_only_the_seed() {
        let panels = [single_horseshoe()];
        let op_point = level_flight_point(0.0);
        let seeds = [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];
        let lines = calculate_streamlines(&panels, &[0.5], &op_point, &seeds, 1, 5.0);
        assert_eq!(lines[0], vec![seeds[0]]);
        assert_eq!(lines[1], vec![seeds[1]]);
    }
}
