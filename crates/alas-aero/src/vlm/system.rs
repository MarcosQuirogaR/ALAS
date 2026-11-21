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
use alas_geom::aircraft::wing::{SpacingFunction, Wing, WingXSec};
use alas_math::linalg::DenseMatrix;
use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use super::streamlines::PanelSample;
use super::{VlmError, VlmResult};
use crate::operating_point::{AxisFrame, OperatingPoint};
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

mod schur;
use schur::AicFactorization;

mod fast_kernel;
mod fast_rows;

mod cache;
pub use cache::VlmGeometryCache;

mod memo;
mod superposition;

// The incidence-offset boundary condition is split into its own file to keep
// this one under its production-line budget.
mod incidence;

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

/// The largest 1-norm condition estimate `kappa` at which a mesh whose pivot
/// ratio exceeds [`MAX_PIVOT_RATIO`] is still solved.
///
/// The pivot ratio is a proxy, `max |pivot| / min |pivot|` of the partial-pivot
/// LU, and a large pivot of a well-conditioned matrix inflates it: the A400M
/// at 24 chordwise panels reports a ratio of 2.0e4 (391 at 23) while its
/// condition estimate is 2.3e3 (2.2e3 at 23) and its lift, moments and
/// sideslip derivatives continue smoothly across the step. The estimate
/// (`alas_math::linalg::LuFactorization::condition_estimate`) is a
/// property of the matrix alone. Measured on the A320 and A400M lattices, the
/// meshes that solve correctly sit at or below 2.4e3 (A400M 1x25), and the
/// ones that must be refused at or above 9.4e5 (A320 10x2, a wrong-signed
/// yawing moment; 4x1: 2.1e6; 6x1: 9.5e10; 10x1, a lift coefficient of
/// -2.1e7: 1.4e16). The limit sits between, 40 times the first and a tenth of
/// the second, the same measured-separation reasoning as [`MAX_PIVOT_RATIO`];
/// the round-off bound `kappa u` is far below the physical failure, which is
/// meaninglessness of the discretization, not numerical error.
const MAX_CONDITION_ESTIMATE: f64 = 1.0e5;

mod mesh;
pub(super) use mesh::*;
pub use mesh::{lattice_components, lattice_grouping_diagnostics, LatticeGroupingDiagnostic};
mod surface_flow;

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
    factorization: Arc<AicFactorization>,
    /// The near-field horseshoe kernel at every vortex center against every
    /// panel: see [`build_kernel_cache`]. Candidate assembly supplies this
    /// eagerly for reuse. Ordinary `assemble` fills it on the second solve:
    /// a system solved only once can evaluate its kernel directly.
    /// Holds `None` above `kernel_cache::KERNEL_CACHE_MAX_PANELS`, where every
    /// solve evaluates the kernel directly. Both paths are bit-identical.
    /// Shared by the systems of one lattice held in [`memo`].
    kernel_cache: memo::KernelSlot,
    /// The superposition basis of this lattice, see [`superposition`];
    /// shared like the kernel.
    basis: superposition::BasisSlot,
    /// The Trefftz-plane drag form of this lattice, built on the first
    /// [`VlmSystem::trefftz_induced_drag_coefficient`]; geometry-only.
    trefftz: Arc<OnceLock<Option<super::trefftz::TrefftzOperator>>>,
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
        Self::assemble_with_core(airplane, spanwise_resolution, chordwise_resolution, false)
    }

    /// [`Self::assemble`] with the frozen reference lattice: the legs seen by
    /// another lattice component keep the inter-surface core of one surface's
    /// halves. For parity fixtures only; product callers use
    /// [`Self::assemble`].
    ///
    /// # Errors
    ///
    /// See [`Self::assemble`].
    pub fn assemble_reference_compatibility(
        airplane: &'a Airplane,
        spanwise_resolution: usize,
        chordwise_resolution: usize,
    ) -> Result<Self, VlmError> {
        Self::assemble_with_core(airplane, spanwise_resolution, chordwise_resolution, true)
    }

    fn assemble_with_core(
        airplane: &'a Airplane,
        spanwise_resolution: usize,
        chordwise_resolution: usize,
        reference_core: bool,
    ) -> Result<Self, VlmError> {
        let _profile =
            alas_math::stage_profile::scope(alas_math::stage_profile::Stage::VlmAssembly);
        alas_math::stage_profile::count(alas_math::stage_profile::Counter::VlmAssemblies, 1);
        let mut panels = mesh_panels(
            airplane,
            spanwise_resolution,
            chordwise_resolution,
            !reference_core,
        )?;
        if reference_core {
            use_reference_component_core(&mut panels);
        }
        if let Some((factorization, kernel_cache, basis)) = memo::get(&panels, reference_core) {
            return Ok(Self {
                airplane,
                panels,
                factorization,
                kernel_cache,
                basis,
                trefftz: Arc::default(),
                solves: AtomicUsize::new(0),
            });
        }
        let n = panels.len();
        alas_math::stage_profile::count(alas_math::stage_profile::Counter::VlmColdAssemblies, 1);
        alas_math::stage_profile::count(alas_math::stage_profile::Counter::VlmRefactorizations, 1);
        let fill_profile =
            alas_math::stage_profile::scope(alas_math::stage_profile::Stage::VlmColdFill);

        // AIC[i][j]: the velocity horseshoe j (unit strength) induces at
        // collocation point i, dotted with that collocation panel's own
        // normal. Row i is written by one task into its own slice.
        let mut aic = vec![0.0_f64; n * n];
        let fast = super::speed::fast_kernel();
        let unit_constant = 1.0 / (4.0 * std::f64::consts::PI);
        let columns = fast_kernel::all_columns(&panels, fast);
        let fill_row = |scratch: &mut fast_rows::RowScratch,
                        (row, collocation_panel): (&mut [f64], &Panel)| {
            if let Some(columns) = &columns {
                fast_rows::evaluate_row(
                    collocation_panel.collocation_point,
                    collocation_panel.core_component,
                    columns,
                    (1.0, unit_constant),
                    scratch,
                );
                for (j, entry) in row.iter_mut().enumerate() {
                    let induced = [
                        scratch.velocity[0][j],
                        scratch.velocity[1][j],
                        scratch.velocity[2][j],
                    ];
                    *entry = dot3(induced, collocation_panel.normal_direction);
                }
                return;
            }
            for (entry, source_panel) in row.iter_mut().zip(&panels) {
                let induced = fast_kernel::horseshoe(
                    fast,
                    collocation_panel.collocation_point,
                    source_panel.left_vortex_vertex,
                    source_panel.right_vortex_vertex,
                    (1.0, unit_constant),
                    inter_surface_core(collocation_panel.core_component, source_panel),
                );
                *entry = dot3(induced, collocation_panel.normal_direction);
            }
        };
        if n < PARALLEL_PANEL_THRESHOLD {
            let mut scratch = fast_rows::RowScratch::default();
            aic.chunks_mut(n.max(1))
                .zip(&panels)
                .for_each(|item| fill_row(&mut scratch, item));
        } else {
            aic.par_chunks_mut(n).zip(panels.par_iter()).for_each_init(
                fast_rows::RowScratch::default,
                |scratch, item| {
                    fill_row(scratch, item);
                },
            );
        }

        drop(fill_profile);
        let _factor_profile =
            alas_math::stage_profile::scope(alas_math::stage_profile::Stage::VlmColdFactor);
        let factorization = DenseMatrix::from_row_major(n, &aic)
            .factor()
            .map_err(VlmError::SingularAic)?;
        let factorization = Arc::new(AicFactorization::Full(factorization));
        let kernel_cache = memo::KernelSlot::default();
        let basis = superposition::BasisSlot::default();
        memo::put(
            &panels,
            reference_core,
            &factorization,
            &kernel_cache,
            &basis,
        );
        Ok(Self {
            airplane,
            panels,
            factorization,
            kernel_cache,
            basis,
            trefftz: Arc::default(),
            solves: AtomicUsize::new(0),
        })
    }

    /// Assemble using exact geometry-only work retained by `cache`.
    ///
    /// Identical panels reuse their mutual influence and near-field kernel.
    /// Identical complete meshes also reuse the factorization. Changed
    /// incidence retains full panel rotation and the full-LU conditioning
    /// gate. Wake traces are checked separately before sharing drag data.
    ///
    /// # Errors
    ///
    /// See [`Self::assemble`].
    pub fn assemble_cached(
        airplane: &'a Airplane,
        spanwise_resolution: usize,
        chordwise_resolution: usize,
        cache: &mut VlmGeometryCache,
    ) -> Result<Self, VlmError> {
        cache.assemble(airplane, spanwise_resolution, chordwise_resolution)
    }

    /// The airplane this system was assembled from.
    pub fn airplane(&self) -> &'a Airplane {
        self.airplane
    }

    /// The Trefftz-plane induced-drag coefficient of `result`, a solve of
    /// this system at `op_point`, referenced to `airplane.s_ref`. The drag
    /// form is built once per system (see [`super::trefftz`]).
    pub fn trefftz_induced_drag_coefficient(
        &self,
        result: &VlmResult,
        op_point: &OperatingPoint,
    ) -> Option<f64> {
        let _profile = alas_math::stage_profile::scope(alas_math::stage_profile::Stage::VlmTrefftz);
        self.trefftz
            .get_or_init(|| super::trefftz::TrefftzOperator::from_panels(&result.panels))
            .as_ref()?
            .induced_drag_coefficient(result, op_point, self.airplane.s_ref)
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
        self.solve_about(
            op_point,
            self.airplane.xyz_ref,
            incidence::NormalChange::None,
        )
    }

    /// Solve at `op_point` with an incidence offset on every panel's
    /// flow-tangency condition: the modified-incidence handle a viscous
    /// section correction needs.
    ///
    /// `offsets_rad[i]` rotates panel `i`'s boundary-condition normal about
    /// the panel's bound-vortex leg, so a positive offset raises the panel's
    /// effective angle of attack by that many radians (positive nose-up about
    /// the leg, as a positive twist does) without changing the geometry, the
    /// influence matrix or its factorization. The forces are still evaluated
    /// on the true freestream and the true panel geometry (Kutta-Joukowski
    /// with the total induced velocity), so only the circulation changes.
    /// All-zero offsets reproduce [`Self::solve`] exactly.
    ///
    /// The offsets are in the order of [`VlmResult::panels`]; use
    /// [`crate::strips::wing_strips`] to find the panels of each strip.
    ///
    /// # Errors
    ///
    /// [`VlmError::NonFiniteResult`] for a non-finite offset or a length
    /// different from [`Self::panel_count`], and as [`Self::solve`].
    pub fn solve_with_incidence_offsets(
        &self,
        op_point: &OperatingPoint,
        offsets_rad: &[f64],
    ) -> Result<VlmResult, VlmError> {
        if offsets_rad.len() != self.panels.len() || !offsets_rad.iter().all(|o| o.is_finite()) {
            return Err(VlmError::NonFiniteResult);
        }
        self.solve_about(
            op_point,
            self.airplane.xyz_ref,
            incidence::NormalChange::Offsets(offsets_rad),
        )
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
        self.solve_about(op_point, [0.0; 3], incidence::NormalChange::None)
    }

    fn solve_about(
        &self,
        op_point: &OperatingPoint,
        rotation_reference: [f64; 3],
        normal_change: incidence::NormalChange<'_>,
    ) -> Result<VlmResult, VlmError> {
        let _profile = alas_math::stage_profile::scope(alas_math::stage_profile::Stage::VlmSolve);
        alas_math::stage_profile::count(alas_math::stage_profile::Counter::VlmSolves, 1);
        let airplane = self.airplane;
        let panels = &self.panels;
        let n = panels.len();

        let steady_freestream_velocity = op_point.freestream_velocity_geometry_axes();
        let flow = surface_flow::SurfaceFlow::new(airplane, panels);

        let collocation_points: Vec<[f64; 3]> =
            panels.iter().map(|p| p.collocation_point).collect();
        let rotation_at_collocation =
            op_point.rotation_velocity_geometry_axes_about(&collocation_points, rotation_reference);

        // The right-hand side: minus the freestream-plus-rotation velocity
        // through each collocation panel's normal.
        let rhs: Vec<f64> = panels
            .iter()
            .zip(&rotation_at_collocation)
            .enumerate()
            .map(|(index, (panel, &rotation_velocity))| {
                let freestream_velocity = add3(steady_freestream_velocity, rotation_velocity);
                let normal = normal_change.normal(panel, index);
                -dot3(freestream_velocity, normal)
            })
            .collect();

        // A repeated solve reads the kernel cache and, without rotation
        // rates, combines the lattice's basis solves (see `superposition`):
        // the same solve to rounding. A first solve stays direct, so a
        // system solved once pays for no basis.
        let vortex_centers: Vec<[f64; 3]> = panels.iter().map(|p| p.vortex_center).collect();
        let kernel = if self.solves.fetch_add(1, Ordering::Relaxed) == 0 {
            None
        } else {
            self.kernel_cache
                .get_or_init(|| build_kernel_cache(&vortex_centers, panels).map(Arc::new))
                .as_ref()
                .map(|entries| entries.as_slice())
        };
        let (vortex_strengths, solve_diagnostics, induced) = superposition::strengths(
            &self.basis,
            &self.factorization,
            kernel,
            panels,
            op_point,
            &normal_change,
            &rhs,
        );
        if !solve_diagnostics.residual_norm.is_finite()
            || !solve_diagnostics.normalized_residual.is_finite()
            || !solve_diagnostics.pivot_ratio.is_finite()
            || !solve_diagnostics.minimum_pivot.is_finite()
        {
            return Err(VlmError::NonFiniteResult);
        }
        if !solve_diagnostics.residual_is_bounded() {
            return Err(VlmError::UnboundedResidual {
                normalized_residual: solve_diagnostics.normalized_residual,
                limit: alas_math::linalg::MAX_NORMALIZED_RESIDUAL,
            });
        }
        if solve_diagnostics.pivot_ratio > MAX_PIVOT_RATIO && {
            let condition = self.factorization.condition_estimate();
            condition.is_nan() || condition > MAX_CONDITION_ESTIMATE
        } {
            return Err(VlmError::IllConditionedAic {
                pivot_ratio: solve_diagnostics.pivot_ratio,
                limit: MAX_PIVOT_RATIO,
            });
        }

        let v_centers = match induced {
            Some(induced) => induced
                .into_iter()
                .map(|sum| add3(sum, steady_freestream_velocity))
                .collect(),
            None => velocity_at_points(
                &vortex_centers,
                panels,
                &vortex_strengths,
                op_point,
                steady_freestream_velocity,
                rotation_reference,
                kernel,
            ),
        };

        let density = op_point.atmosphere.density();
        let mut force_geometry = [0.0; 3];
        let mut moment_geometry = [0.0; 3];
        // Recorded per panel as the loop goes, alongside the running totals:
        // same operations in the same order, so the totals are unaffected;
        // this is only an additional read-out.
        let mut panel_forces_geometry = Vec::with_capacity(n);
        for (index, ((panel, &gamma), &v_center)) in panels
            .iter()
            .zip(&vortex_strengths)
            .zip(&v_centers)
            .enumerate()
        {
            let vi_cross_li = cross3(v_center, panel.vortex_bound_leg);
            let force_panel = scale3(vi_cross_li, density * gamma * flow.force_scale(index));
            let moment_panel = cross3(sub3(panel.vortex_center, airplane.xyz_ref), force_panel);
            force_geometry = add3(force_geometry, force_panel);
            moment_geometry = add3(moment_geometry, moment_panel);
            panel_forces_geometry.push(force_panel);
        }

        let vortex_strengths = flow.reported_circulations(vortex_strengths);
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
                    normal_direction: p.normal_direction,
                })
                .collect(),
            panel_forces_geometry,
            solve_diagnostics,
        })
    }
}
