// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The panel mesh, the influence matrix and its factorization, held together
//! so that one geometry can be solved at many operating points.
//!
//! The aerodynamic influence coefficient (AIC) matrix couples every panel to
//! every other through the horseshoe kernel and depends on the geometry
//! alone: the operating point enters only through the right-hand side, the
//! freestream and rotation velocity at each collocation point. A polar sweep
//! over fifteen angles of attack therefore needs one O(n^3) factorization
//! and fifteen O(n^2) substitutions, not fifteen factorizations; at the fine
//! product mesh (800 panels) refactoring per point would dominate the full
//! analysis.
//!
//! Assembly is row-parallel: every row of the matrix is one collocation
//! point's view of every horseshoe, independent of every other row, so the
//! rows are filled on the rayon pool into disjoint slices. The per-entry
//! arithmetic is unchanged and the result is bit-identical to the serial
//! loop, as is the induced-velocity evaluation at the vortex centers, which
//! is parallel over points with a sequential sum per point.

use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::{SpacingFunction, Wing};
use alas_math::linalg::{DenseMatrix, LuFactorization};
use rayon::prelude::*;

use super::streamlines::PanelSample;
use super::{VlmError, VlmResult, TRAILING_VORTEX_DIRECTION, VORTEX_CORE_RADIUS};
use crate::operating_point::{AxisFrame, OperatingPoint};
use crate::singularities::calculate_induced_velocity_horseshoe;
use crate::vector3::{add3, cross3, dot3, norm3, scale3, sub3};

/// Panel count below which the O(n^2) loops stay on the calling thread.
///
/// Waking the pool costs on the order of 0.1 ms per parallel region; at the
/// default 1x1 mesh (50 panels) the whole solve is smaller than that, so the
/// two regions below only pay off once a mesh has a few hundred panels.
const PARALLEL_PANEL_THRESHOLD: usize = 128;

/// The largest `max |pivot| / min |pivot|` a solve may report and still be
/// treated as a flow field, see [`VlmError::IllConditionedAic`].
///
/// Measured across the registered presets at every mesh from 1x1 to 10x16:
/// meshes whose lift is correct report 2 to 60, and every mesh that returns a negative or
/// absurd lift coefficient reports above 1e4: the A320 at a spanwise
/// resolution of ten and one chordwise panel reports 9.1e7 and a lift
/// coefficient of -2.1e7. Two orders of margin above the usable range keeps
/// this a backstop against collapse rather than a second opinion on meshing.
const MAX_PIVOT_RATIO: f64 = 1.0e4;

/// One panel's four quad-mesh corners and the vortex-lattice quantities
/// derived from them: the per-panel arrays `run` builds and consumes,
/// grouped so the assembly loop reads as one step per panel rather than
/// eight parallel index operations.
pub(super) struct Panel {
    normal_direction: [f64; 3],
    left_vortex_vertex: [f64; 3],
    right_vortex_vertex: [f64; 3],
    vortex_center: [f64; 3],
    vortex_bound_leg: [f64; 3],
    collocation_point: [f64; 3],
    /// Kept alongside the derived quantities above so [`VlmResult::panels`]
    /// can report the raw mesh, not just what the AIC assembly needs, see
    /// [`PanelSample`].
    front_left: [f64; 3],
    back_left: [f64; 3],
    back_right: [f64; 3],
    front_right: [f64; 3],
    is_trailing_edge: bool,
    wing_index: usize,
}

impl Panel {
    /// Derive one panel's vortex-lattice quantities from its four quad-mesh
    /// corners, in `run`'s own front-left/back-left/back-right/front-right
    /// order.
    pub(super) fn from_quad(
        front_left: [f64; 3],
        back_left: [f64; 3],
        back_right: [f64; 3],
        front_right: [f64; 3],
        is_trailing_edge: bool,
        wing_index: usize,
    ) -> Result<Self, VlmError> {
        let diag1 = sub3(front_right, back_left);
        let diag2 = sub3(front_left, back_right);
        let cross = cross3(diag1, diag2);
        let area_normal = norm3(cross);
        if !area_normal.is_finite() || area_normal <= f64::EPSILON {
            return Err(VlmError::DegeneratePanel { wing_index });
        }
        let normal_direction = scale3(cross, 1.0 / area_normal);

        let left_vortex_vertex = add3(scale3(front_left, 0.75), scale3(back_left, 0.25));
        let right_vortex_vertex = add3(scale3(front_right, 0.75), scale3(back_right, 0.25));
        let vortex_center = scale3(add3(left_vortex_vertex, right_vortex_vertex), 0.5);
        let vortex_bound_leg = sub3(right_vortex_vertex, left_vortex_vertex);

        let collocation_left = add3(scale3(front_left, 0.25), scale3(back_left, 0.75));
        let collocation_right = add3(scale3(front_right, 0.25), scale3(back_right, 0.75));
        let collocation_point = scale3(add3(collocation_left, collocation_right), 0.5);

        Ok(Self {
            normal_direction,
            left_vortex_vertex,
            right_vortex_vertex,
            vortex_center,
            vortex_bound_leg,
            collocation_point,
            front_left,
            back_left,
            back_right,
            front_right,
            is_trailing_edge,
            wing_index,
        })
    }
}

/// Mesh every wing on `airplane` into quad panels, exactly as `run`'s own
/// meshing step does: [`Wing::subdivide_sections`] with
/// [`SpacingFunction::Cosspace`] when `spanwise_resolution > 1`, then
/// [`Wing::mesh_thin_surface`] at `chordwise_resolution` with camber.
fn mesh_panels(
    airplane: &Airplane,
    spanwise_resolution: usize,
    chordwise_resolution: usize,
) -> Result<Vec<Panel>, VlmError> {
    let mut panels = Vec::new();
    for (wing_index, wing) in airplane.wings.iter().enumerate() {
        let subdivided;
        let wing_ref: &Wing = if spanwise_resolution > 1 {
            subdivided = wing.subdivide_sections(spanwise_resolution, SpacingFunction::Cosspace)?;
            &subdivided
        } else {
            wing
        };

        let (points, faces) = wing_ref.mesh_thin_surface(chordwise_resolution, true);
        // Upstream's `(arange(len(faces)) + 1) % chordwise_resolution == 0`,
        // evaluated per wing (including its mirrored half, already appended
        // to `faces` by `mesh_thin_surface` when the wing is symmetric)
        // before the per-wing arrays are concatenated.
        for (i, face) in faces.iter().enumerate() {
            let is_trailing_edge = (i + 1) % chordwise_resolution == 0;
            panels.push(Panel::from_quad(
                points[face[0]],
                points[face[1]],
                points[face[2]],
                points[face[3]],
                is_trailing_edge,
                wing_index,
            )?);
        }
    }
    Ok(panels)
}

/// The velocity every horseshoe vortex (strength `vortex_strengths[j]`)
/// induces at `points[i]`, summed over every panel, plus the freestream and
/// rotation-induced velocity at that point: `get_velocity_at_points`
/// (through `get_induced_velocity_at_points`), scoped to the internal use
/// the solve makes of it. Parallel over points; the sum over panels for one
/// point is sequential and in panel order, so the result does not depend on
/// the pool.
fn velocity_at_points(
    points: &[[f64; 3]],
    panels: &[Panel],
    vortex_strengths: &[f64],
    op_point: &OperatingPoint,
    steady_freestream_velocity: [f64; 3],
    reference: [f64; 3],
) -> Vec<[f64; 3]> {
    let rotation_velocities = op_point.rotation_velocity_geometry_axes_about(points, reference);
    let at_point = |(&point, &rotation_velocity): (&[f64; 3], &[f64; 3])| {
        let induced =
            panels
                .iter()
                .zip(vortex_strengths)
                .fold([0.0, 0.0, 0.0], |acc, (panel, &gamma)| {
                    let contribution = calculate_induced_velocity_horseshoe(
                        point,
                        panel.left_vortex_vertex,
                        panel.right_vortex_vertex,
                        TRAILING_VORTEX_DIRECTION,
                        gamma,
                        VORTEX_CORE_RADIUS,
                    );
                    add3(acc, contribution)
                });
        add3(induced, add3(steady_freestream_velocity, rotation_velocity))
    };
    if panels.len() < PARALLEL_PANEL_THRESHOLD {
        points
            .iter()
            .zip(&rotation_velocities)
            .map(at_point)
            .collect()
    } else {
        points
            .par_iter()
            .zip(rotation_velocities.par_iter())
            .map(at_point)
            .collect()
    }
}

/// One airplane's panel mesh and factored influence matrix at a fixed
/// resolution, ready to solve at any operating point.
///
/// [`super::run`] is `assemble` followed by one [`VlmSystem::solve`]. A
/// caller with a schedule of operating points over the same geometry (a
/// polar sweep, the probes of a neutral-point or trim estimate, the
/// finite-difference stencil of the stability derivatives) assembles once
/// and solves repeatedly.
pub struct VlmSystem<'a> {
    airplane: &'a Airplane,
    panels: Vec<Panel>,
    factorization: LuFactorization,
}

impl<'a> VlmSystem<'a> {
    /// Mesh `airplane`, assemble the influence matrix and factor it.
    ///
    /// # Errors
    ///
    /// [`VlmError::Subdivide`] and [`VlmError::DegeneratePanel`] from the
    /// meshing, [`VlmError::SingularAic`] from the factorization.
    pub fn assemble(
        airplane: &'a Airplane,
        spanwise_resolution: usize,
        chordwise_resolution: usize,
    ) -> Result<Self, VlmError> {
        let panels = mesh_panels(airplane, spanwise_resolution, chordwise_resolution)?;
        let n = panels.len();

        // AIC[i][j]: the velocity horseshoe j (unit strength) induces at
        // collocation point i, dotted with that collocation panel's own
        // normal. Row i is written by one task into its own slice.
        let mut aic = vec![0.0_f64; n * n];
        let fill_row = |(row, collocation_panel): (&mut [f64], &Panel)| {
            for (entry, source_panel) in row.iter_mut().zip(&panels) {
                let induced = calculate_induced_velocity_horseshoe(
                    collocation_panel.collocation_point,
                    source_panel.left_vortex_vertex,
                    source_panel.right_vortex_vertex,
                    TRAILING_VORTEX_DIRECTION,
                    1.0,
                    VORTEX_CORE_RADIUS,
                );
                *entry = dot3(induced, collocation_panel.normal_direction);
            }
        };
        if n < PARALLEL_PANEL_THRESHOLD {
            aic.chunks_mut(n.max(1)).zip(&panels).for_each(fill_row);
        } else {
            aic.par_chunks_mut(n)
                .zip(panels.par_iter())
                .for_each(fill_row);
        }

        let factorization = DenseMatrix::from_row_major(n, &aic)
            .factor()
            .map_err(VlmError::SingularAic)?;
        Ok(Self {
            airplane,
            panels,
            factorization,
        })
    }

    /// The airplane this system was assembled from.
    pub fn airplane(&self) -> &'a Airplane {
        self.airplane
    }

    /// The number of horseshoe panels, the order of the influence matrix.
    pub fn panel_count(&self) -> usize {
        self.panels.len()
    }

    /// Solve at `op_point` with the product rotation reference,
    /// `airplane.xyz_ref`: what [`super::run`] does.
    ///
    /// # Errors
    ///
    /// [`VlmError::NonFiniteResult`].
    pub fn solve(&self, op_point: &OperatingPoint) -> Result<VlmResult, VlmError> {
        self.solve_about(op_point, self.airplane.xyz_ref)
    }

    /// Solve at `op_point` with the frozen reference convention, rotation
    /// about the geometry origin: what [`super::run_reference_compatibility`]
    /// does. For parity fixtures only.
    ///
    /// # Errors
    ///
    /// [`VlmError::NonFiniteResult`].
    pub fn solve_reference_compatibility(
        &self,
        op_point: &OperatingPoint,
    ) -> Result<VlmResult, VlmError> {
        self.solve_about(op_point, [0.0; 3])
    }

    fn solve_about(
        &self,
        op_point: &OperatingPoint,
        rotation_reference: [f64; 3],
    ) -> Result<VlmResult, VlmError> {
        let airplane = self.airplane;
        let panels = &self.panels;
        let n = panels.len();

        let steady_freestream_velocity = op_point.freestream_velocity_geometry_axes();

        let collocation_points: Vec<[f64; 3]> =
            panels.iter().map(|p| p.collocation_point).collect();
        let rotation_at_collocation =
            op_point.rotation_velocity_geometry_axes_about(&collocation_points, rotation_reference);

        // The right-hand side: minus the freestream-plus-rotation velocity
        // through each collocation panel's normal.
        let rhs: Vec<f64> = panels
            .iter()
            .zip(&rotation_at_collocation)
            .map(|(panel, &rotation_velocity)| {
                let freestream_velocity = add3(steady_freestream_velocity, rotation_velocity);
                -dot3(freestream_velocity, panel.normal_direction)
            })
            .collect();

        let (vortex_strengths, solve_diagnostics) = self.factorization.solve_vector(&rhs);
        if !solve_diagnostics.residual_norm.is_finite()
            || !solve_diagnostics.normalized_residual.is_finite()
            || !solve_diagnostics.pivot_ratio.is_finite()
            || !solve_diagnostics.minimum_pivot.is_finite()
        {
            return Err(VlmError::NonFiniteResult);
        }
        if solve_diagnostics.pivot_ratio > MAX_PIVOT_RATIO {
            return Err(VlmError::IllConditionedAic {
                pivot_ratio: solve_diagnostics.pivot_ratio,
            });
        }

        let vortex_centers: Vec<[f64; 3]> = panels.iter().map(|p| p.vortex_center).collect();
        let v_centers = velocity_at_points(
            &vortex_centers,
            panels,
            &vortex_strengths,
            op_point,
            steady_freestream_velocity,
            rotation_reference,
        );

        let density = op_point.atmosphere.density();
        let mut force_geometry = [0.0; 3];
        let mut moment_geometry = [0.0; 3];
        // Recorded per panel as the loop goes, alongside the running totals:
        // same operations in the same order, so the totals are unaffected;
        // this is only an additional read-out.
        let mut panel_forces_geometry = Vec::with_capacity(n);
        for ((panel, &gamma), &v_center) in panels.iter().zip(&vortex_strengths).zip(&v_centers) {
            let vi_cross_li = cross3(v_center, panel.vortex_bound_leg);
            let force_panel = scale3(vi_cross_li, density * gamma);
            let moment_panel = cross3(sub3(panel.vortex_center, airplane.xyz_ref), force_panel);
            force_geometry = add3(force_geometry, force_panel);
            moment_geometry = add3(moment_geometry, moment_panel);
            panel_forces_geometry.push(force_panel);
        }

        let (fbx, fby, fbz) = op_point.convert_axes(
            force_geometry[0],
            force_geometry[1],
            force_geometry[2],
            AxisFrame::Geometry,
            AxisFrame::Body,
        );
        let force_body = [fbx, fby, fbz];
        let (fwx, fwy, fwz) = op_point.convert_axes(
            force_body[0],
            force_body[1],
            force_body[2],
            AxisFrame::Body,
            AxisFrame::Wind,
        );
        let force_wind = [fwx, fwy, fwz];

        let (mbx, mby, mbz) = op_point.convert_axes(
            moment_geometry[0],
            moment_geometry[1],
            moment_geometry[2],
            AxisFrame::Geometry,
            AxisFrame::Body,
        );
        let moment_body = [mbx, mby, mbz];
        let (mwx, mwy, mwz) = op_point.convert_axes(
            moment_body[0],
            moment_body[1],
            moment_body[2],
            AxisFrame::Body,
            AxisFrame::Wind,
        );
        let moment_wind = [mwx, mwy, mwz];

        let lift = -force_wind[2];
        let drag = -force_wind[0];
        let side_force = force_wind[1];
        let roll_moment = moment_body[0];
        let pitch_moment = moment_body[1];
        let yaw_moment = moment_body[2];

        let q = op_point.dynamic_pressure();
        let s_ref = airplane.s_ref;
        let b_ref = airplane.b_ref;
        let c_ref = airplane.c_ref;
        let cl_lift = lift / q / s_ref;
        let cd_drag = drag / q / s_ref;
        let cy_side = side_force / q / s_ref;
        let cl_roll = roll_moment / q / s_ref / b_ref;
        let cm_pitch = pitch_moment / q / s_ref / c_ref;
        let cn_yaw = yaw_moment / q / s_ref / b_ref;
        let coefficient_values = [
            lift,
            drag,
            side_force,
            roll_moment,
            pitch_moment,
            yaw_moment,
            cl_lift,
            cd_drag,
            cy_side,
            cl_roll,
            cm_pitch,
            cn_yaw,
        ];
        if !force_geometry
            .iter()
            .chain(force_body.iter())
            .chain(force_wind.iter())
            .chain(moment_geometry.iter())
            .chain(moment_body.iter())
            .chain(moment_wind.iter())
            .chain(coefficient_values.iter())
            .all(|value| value.is_finite())
        {
            return Err(VlmError::NonFiniteResult);
        }

        Ok(VlmResult {
            force_geometry,
            force_body,
            force_wind,
            moment_geometry,
            moment_body,
            moment_wind,
            lift,
            drag,
            side_force,
            roll_moment,
            pitch_moment,
            yaw_moment,
            cl_lift,
            cd_drag,
            cy_side,
            cl_roll,
            cm_pitch,
            cn_yaw,
            vortex_strengths,
            panels: panels
                .iter()
                .map(|p| PanelSample {
                    front_left: p.front_left,
                    back_left: p.back_left,
                    back_right: p.back_right,
                    front_right: p.front_right,
                    left_vortex_vertex: p.left_vortex_vertex,
                    right_vortex_vertex: p.right_vortex_vertex,
                    vortex_center: p.vortex_center,
                    is_trailing_edge: p.is_trailing_edge,
                    wing_index: p.wing_index,
                })
                .collect(),
            panel_forces_geometry,
            solve_diagnostics,
        })
    }
}
