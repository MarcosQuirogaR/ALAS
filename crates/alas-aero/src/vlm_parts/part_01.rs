// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::{SpacingFunction, SubdivideSectionsError, Wing};
use alas_math::linalg;
use alas_math::linalg::SolveDiagnostics;

use crate::operating_point::{AxisFrame, OperatingPoint};
use crate::singularities::calculate_induced_velocity_horseshoe;
use crate::vector3::{add3, cross3, dot3, norm3, scale3, sub3};

#[path = "../vlm/stability_derivatives.rs"]
pub mod stability_derivatives;
#[path = "../vlm/streamlines.rs"]
pub mod streamlines;

pub use stability_derivatives::{
    run_with_stability_derivatives, run_with_stability_derivatives_reference_compatibility,
    run_with_stability_derivatives_with_steps, CoefficientDerivatives, VlmStabilityResult,
};
pub use streamlines::{calculate_streamlines, PanelSample};

/// The Kaufmann vortex core smoothing radius `VortexLatticeMethod`'s
/// constructor defaults to and every call site in this program's inputs
/// leaves unset -- see the module doc.
const VORTEX_CORE_RADIUS: f64 = 1e-8;

/// The trailing-leg direction every call site in this program's inputs gets,
/// since `align_trailing_vortices_with_wind` is never set `true` -- see the
/// module doc.
const TRAILING_VORTEX_DIRECTION: [f64; 3] = [1.0, 0.0, 0.0];

/// Why [`run`] could not solve.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum VlmError {
    /// Subdividing a wing's spanwise sections failed. Reachable only through
    /// [`SubdivideSectionsError::Blend`] in practice: `run` only calls
    /// [`Wing::subdivide_sections`] when `spanwise_resolution > 1`, which
    /// always satisfies that method's own `ratio >= 2` requirement, so
    /// [`SubdivideSectionsError::RatioTooSmall`] cannot occur from this call
    /// path.
    #[error("subdividing a wing's spanwise sections failed: {0}")]
    Subdivide(#[from] SubdivideSectionsError),
    /// The AIC matrix was numerically singular at the named elimination step.
    /// Not expected for a well-formed mesh -- a horseshoe's self-influence on
    /// its own collocation point is always well defined -- but library code
    /// reports a numerical surprise rather than panicking on it
    /// (`CONTRIBUTING.md`).
    #[error("the panel influence matrix was numerically singular at row {0}")]
    SingularAic(usize),
    /// A finite-difference derivative step was not finite and positive.
    #[error("stability-derivative steps must be finite and positive")]
    InvalidDerivativeStep,
    /// A generated panel had no usable area for a unit normal.
    #[error("wing {wing_index} generated a degenerate zero-area panel")]
    DegeneratePanel {
        /// Index of the wing that produced the panel.
        wing_index: usize,
    },
    /// A solve produced a non-finite force, moment, coefficient, or residual.
    #[error("the VLM solve produced a non-finite result")]
    NonFiniteResult,
}

/// Every field `run`'s upstream docstring lists, plus the solved circulation
/// vector -- `VortexLatticeMethod.run`'s returned `dict`, restated as a typed
/// struct. `alas-aero::analysis` and, eventually, `alas-stab` (P7) each read
/// a different subset of this, so nothing here is trimmed to only what
/// today's callers use.
#[derive(Debug, Clone, PartialEq)]
pub struct VlmResult {
    /// Net aerodynamic force in geometry axes, N -- `F_g`.
    pub force_geometry: [f64; 3],
    /// Net aerodynamic force in body axes, N -- `F_b`.
    pub force_body: [f64; 3],
    /// Net aerodynamic force in wind axes, N -- `F_w`.
    pub force_wind: [f64; 3],
    /// Net aerodynamic moment about geometry axes, Nm -- `M_g`.
    pub moment_geometry: [f64; 3],
    /// Net aerodynamic moment about body axes, Nm -- `M_b`.
    pub moment_body: [f64; 3],
    /// Net aerodynamic moment about wind axes, Nm -- `M_w`.
    pub moment_wind: [f64; 3],
    /// Lift, N, wind axes by definition -- `L`.
    pub lift: f64,
    /// Drag, N, wind axes by definition -- `D`.
    pub drag: f64,
    /// Side force, N, wind axes -- `Y`.
    pub side_force: f64,
    /// Rolling moment about the body X axis, Nm; positive is roll-right --
    /// `l_b`.
    pub roll_moment: f64,
    /// Pitching moment about the body Y axis, Nm; positive is pitch-up --
    /// `m_b`.
    pub pitch_moment: f64,
    /// Yawing moment about the body Z axis, Nm; positive is nose-right --
    /// `n_b`.
    pub yaw_moment: f64,
    /// Lift coefficient -- `CL`.
    pub cl_lift: f64,
    /// Drag coefficient -- `CD`.
    pub cd_drag: f64,
    /// Side-force coefficient -- `CY`.
    pub cy_side: f64,
    /// Rolling-moment coefficient, body axes -- `Cl`.
    pub cl_roll: f64,
    /// Pitching-moment coefficient, body axes -- `Cm`.
    pub cm_pitch: f64,
    /// Yawing-moment coefficient, body axes -- `Cn`.
    pub cn_yaw: f64,
    /// The solved circulation strength of every horseshoe vortex, in the
    /// panel order `run` built the mesh in. Not part of upstream's returned
    /// `dict` -- it lives on the solved `VortexLatticeMethod` instance as
    /// `self.vortex_strengths` instead -- but returned here since this port
    /// has no persistent instance to hang it on afterward, and it is the
    /// strongest available check that panel assembly and the solve are both
    /// right (a wrong AIC assembly can still integrate to a coincidentally
    /// close total force).
    pub vortex_strengths: Vec<f64>,
    /// Every panel's raw mesh geometry, aligned with [`Self::vortex_strengths`]
    /// -- upstream's per-panel instance arrays (`front_left_vertices`, ...,
    /// `is_trailing_edge`), exposed for figures that read more than the net
    /// totals above: the spanwise lift distribution and the wake streamlines.
    pub panels: Vec<PanelSample>,
    /// Each panel's own aerodynamic force in geometry axes, N, aligned with
    /// [`Self::panels`] -- the per-panel terms [`Self::force_geometry`] sums.
    /// Not part of upstream's returned `dict` for the same reason
    /// `vortex_strengths` is not: it lives on the solved instance instead
    /// (`self.forces_geometry`).
    pub panel_forces_geometry: Vec<[f64; 3]>,
    /// Residual and pivot-ratio evidence from the dense circulation solve.
    ///
    /// Keeping this alongside the aerodynamic result makes mesh/conditioning
    /// review possible without reconstructing the AIC matrix after the solve.
    pub solve_diagnostics: SolveDiagnostics,
}

/// One panel's four quad-mesh corners and the vortex-lattice quantities
/// derived from them -- the per-panel arrays `run` builds and consumes,
/// grouped so the assembly loop below reads as one step per panel rather
/// than eight parallel index operations.
struct Panel {
    normal_direction: [f64; 3],
    left_vortex_vertex: [f64; 3],
    right_vortex_vertex: [f64; 3],
    vortex_center: [f64; 3],
    vortex_bound_leg: [f64; 3],
    collocation_point: [f64; 3],
    /// Kept alongside the derived quantities above so [`VlmResult::panels`]
    /// can report the raw mesh, not just what the AIC assembly needs -- see
    /// [`streamlines::PanelSample`].
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
    fn from_quad(
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
/// rotation-induced velocity at that point -- `get_velocity_at_points`
/// (through `get_induced_velocity_at_points`), scoped to the internal use
/// [`run`] makes of it. See the module doc for why the two upstream methods
/// are collapsed into this one non-broadcast helper.
fn velocity_at_points(
    points: &[[f64; 3]],
    panels: &[Panel],
    vortex_strengths: &[f64],
    op_point: &OperatingPoint,
    steady_freestream_velocity: [f64; 3],
    reference: [f64; 3],
) -> Vec<[f64; 3]> {
    let rotation_velocities = op_point.rotation_velocity_geometry_axes_about(points, reference);
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

/// Run a vortex-lattice solve of `airplane` at `op_point` -- `VortexLatticeMethod(...).run()`,
/// with the constructor arguments folded in as documented above.
/// `spanwise_resolution`/`chordwise_resolution` are the only two constructor
/// arguments this program's call sites ever vary.
///
/// # Errors
///
/// See [`VlmError`].
pub fn run(
    airplane: &Airplane,
    op_point: &OperatingPoint,
    spanwise_resolution: usize,
    chordwise_resolution: usize,
) -> Result<VlmResult, VlmError> {
    run_with_rotation_reference(
        airplane,
        op_point,
        spanwise_resolution,
        chordwise_resolution,
        airplane.xyz_ref,
    )
}

/// Run the frozen reference VLM convention used by the AeroSandbox fixtures.
///
/// The translated reference solver evaluates rotation-induced velocity about
/// the geometry origin. Product analyses use [`run`], which evaluates that
/// velocity about `airplane.xyz_ref` so rigid-body rates remain invariant under
/// a rigid translation. This seam is for frozen parity fixtures only; product
/// callers must use [`run`].
pub fn run_reference_compatibility(
    airplane: &Airplane,
    op_point: &OperatingPoint,
    spanwise_resolution: usize,
    chordwise_resolution: usize,
) -> Result<VlmResult, VlmError> {
    run_with_rotation_reference(
        airplane,
        op_point,
        spanwise_resolution,
        chordwise_resolution,
        [0.0; 3],
    )
}
