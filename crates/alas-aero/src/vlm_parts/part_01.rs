// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::SubdivideSectionsError;
use alas_math::linalg::SolveDiagnostics;

use crate::operating_point::OperatingPoint;

#[path = "../vlm/stability_derivatives.rs"]
pub mod stability_derivatives;
#[path = "../vlm/streamlines.rs"]
pub mod streamlines;
#[path = "../vlm/system.rs"]
pub mod system;

pub use stability_derivatives::{
    run_with_stability_derivatives, run_with_stability_derivatives_reference_compatibility,
    run_with_stability_derivatives_with_steps, CoefficientDerivatives, VlmStabilityResult,
};
pub use streamlines::{calculate_streamlines, PanelSample};
pub use system::VlmSystem;

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
    /// [`alas_geom::aircraft::wing::Wing::subdivide_sections`] when
    /// `spanwise_resolution > 1`, which always satisfies that method's own
    /// `ratio >= 2` requirement, so [`SubdivideSectionsError::RatioTooSmall`]
    /// cannot occur from this call path.
    #[error("subdividing a wing's spanwise sections failed: {0}")]
    Subdivide(#[from] SubdivideSectionsError),
    /// The AIC matrix was numerically singular at the named elimination step.
    /// Not expected for a well-formed mesh -- a horseshoe's self-influence on
    /// its own collocation point is always well defined -- but library code
    /// reports a numerical surprise rather than panicking on it
    /// (`CONTRIBUTING.md`).
    #[error("the panel influence matrix was numerically singular at row {0}")]
    SingularAic(usize),
    /// The AIC matrix was solvable but so ill-conditioned that the
    /// circulation it returns is not a flow field.
    ///
    /// This is a *meshing* failure wearing numerical clothes. It appears when
    /// the spanwise panel count is pushed far past what the geometry needs --
    /// `AnalysisConfig::spanwise_resolution` multiplies a surface the builder
    /// has already subdivided, so a value of ten means slivers -- and the
    /// horseshoe legs of neighbouring panels approach collinearity. The
    /// residual stays at machine precision throughout (the linear solve is
    /// accurate; it is the system that is meaningless), so only the pivot
    /// ratio distinguishes it.
    ///
    /// The threshold catches the collapse, not the onset: measured over the
    /// registered presets, a usable mesh sits below about 60 and the meshes
    /// that return a negative or absurd lift coefficient sit above 1e4. A
    /// mesh between those can still return a plausible lift with a badly
    /// wrong induced drag, which no linear-algebra diagnostic can detect --
    /// `alas_config::validation` rejects that range at the configuration
    /// boundary instead, and this is the backstop for callers that construct
    /// a [`super::VlmSystem`] directly.
    #[error(
        "the panel influence matrix is too ill-conditioned to trust \
         (pivot ratio {pivot_ratio:.3e}); the spanwise panel resolution is \
         far finer than the geometry supports"
    )]
    IllConditionedAic {
        /// `max |pivot| / min |pivot|` from the factorization.
        pivot_ratio: f64,
    },
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

/// Run a vortex-lattice solve of `airplane` at `op_point` -- `VortexLatticeMethod(...).run()`,
/// with the constructor arguments folded in as documented above.
/// `spanwise_resolution`/`chordwise_resolution` are the only two constructor
/// arguments this program's call sites ever vary.
///
/// One mesh, one factorization, one solve. A caller with several operating
/// points over the same geometry should hold a [`VlmSystem`] instead.
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
    VlmSystem::assemble(airplane, spanwise_resolution, chordwise_resolution)?.solve(op_point)
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
    VlmSystem::assemble(airplane, spanwise_resolution, chordwise_resolution)?
        .solve_reference_compatibility(op_point)
}
