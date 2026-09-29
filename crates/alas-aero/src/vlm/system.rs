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
//! and fifteen O(n^2) substitutions, not fifteen factorizations. Before this
//! split the full analysis at the fine product mesh (800 panels) spent most
//! of its 4.5 s refactoring the same matrix.
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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use super::streamlines::PanelSample;
use super::{VlmError, VlmResult, TRAILING_VORTEX_DIRECTION, VORTEX_CORE_RADIUS};
use crate::operating_point::{AxisFrame, OperatingPoint};
use crate::singularities::calculate_induced_velocity_horseshoe_cored;
use crate::vector3::{add3, cross3, dot3, norm3, scale3, sub3};

/// Panel count below which the O(n^2) loops stay on the calling thread.
///
/// Waking the pool costs on the order of 0.1 ms per parallel region; at the
/// default 1x1 mesh (50 panels) the whole solve is smaller than that, so the
/// two regions below only pay off once a mesh has a few hundred panels.
const PARALLEL_PANEL_THRESHOLD: usize = 128;

// The horseshoe-kernel cache (`KERNEL_CACHE_MAX_PANELS`, `build_kernel_cache`)
// is split into its own file to keep this one under its production-line
// budget; see that module's own doc.
mod kernel_cache;
use kernel_cache::{build_kernel_cache, velocity_at_points};

/// The largest `max |pivot| / min |pivot|` a solve may report and still be
/// treated as a flow field, see [`VlmError::IllConditionedAic`].
///
/// Measured across the registered presets at every mesh from 1x1 to 10x16
/// in an internal VLM resolution-sensitivity study: meshes whose
/// lift is correct report 2 to 60, and every mesh that returns a negative or
/// absurd lift coefficient reports above 1e4: the A320 at a spanwise
/// resolution of ten and one chordwise panel reports 9.1e7 and a lift
/// coefficient of -2.1e7. Two orders of margin above the usable range keeps
/// this a backstop against collapse rather than a second opinion on meshing.
const MAX_PIVOT_RATIO: f64 = 1.0e4;

mod mesh;
pub(super) use mesh::*;

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
    /// The near-field horseshoe kernel at every vortex center against every
    /// panel: see [`build_kernel_cache`]. Filled by the second solve, not by
    /// `assemble`: filling it costs one uncached solve's kernel pass, so a
    /// system solved once (every [`super::run`]) would only pay for it.
    /// Holds `None` above `kernel_cache::KERNEL_CACHE_MAX_PANELS`, where every
    /// solve evaluates the kernel directly. Both paths are bit-identical.
    kernel_cache: OnceLock<Option<Vec<[f64; 3]>>>,
    solves: AtomicUsize,
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
                let induced = calculate_induced_velocity_horseshoe_cored(
                    collocation_panel.collocation_point,
                    source_panel.left_vortex_vertex,
                    source_panel.right_vortex_vertex,
                    TRAILING_VORTEX_DIRECTION,
                    1.0,
                    VORTEX_CORE_RADIUS,
                    inter_surface_core(collocation_panel.wing_index, source_panel),
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
            kernel_cache: OnceLock::new(),
            solves: AtomicUsize::new(0),
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
        let kernel = if self.solves.fetch_add(1, Ordering::Relaxed) == 0 {
            None
        } else {
            self.kernel_cache
                .get_or_init(|| build_kernel_cache(&vortex_centers, panels))
                .as_deref()
        };
        let v_centers = velocity_at_points(
            &vortex_centers,
            panels,
            &vortex_strengths,
            op_point,
            steady_freestream_velocity,
            rotation_reference,
            kernel,
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
