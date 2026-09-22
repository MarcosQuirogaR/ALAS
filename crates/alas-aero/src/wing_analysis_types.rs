// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Result, reference and error types of the wing-only aerodynamic entry.
//!
//! Frames, signs, units and omissions are stated in [`super`].

use alas_atmo::Atmosphere;
use alas_geom::builder::BuildError;
use alas_math::linalg::SolveDiagnostics;

use crate::operating_point::OperatingPoint;
use crate::vlm::VlmError;

use super::{FlightCondition, SpeedInput, WingAnalysisInputs, ALPHA_LIMIT_DEG};

/// The reference quantities every coefficient in one outcome is formed with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WingReference {
    /// Reference area, m^2: the main wing planform projected on the geometry
    /// XY plane, including the mirrored half. Unchanged by the empennage
    /// option, so a wing-only and a wing-empennage coefficient are comparable.
    pub area_m2: f64,
    /// Reference span, m: the main wing span projected on the geometry Y axis.
    pub span_m: f64,
    /// Reference chord, m: the main wing mean aerodynamic chord.
    pub chord_m: f64,
    /// Moment reference point in geometry axes, m.
    pub moment_reference_m: [f64; 3],
}

impl WingReference {
    /// Aspect ratio `b^2 / S` of the reference planform.
    pub fn aspect_ratio(&self) -> f64 {
        if self.area_m2 > 0.0 {
            self.span_m * self.span_m / self.area_m2
        } else {
            f64::NAN
        }
    }
}

/// The resolved flight state one point was solved at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedCondition {
    /// Geopotential altitude, m.
    pub altitude_m: f64,
    /// Air density, kg/m^3.
    pub density_kg_m3: f64,
    /// Speed of sound, m/s.
    pub speed_of_sound_m_s: f64,
    /// True airspeed, m/s.
    pub true_airspeed_m_s: f64,
    /// Freestream Mach number.
    pub mach: f64,
    /// Dynamic pressure, Pa.
    pub dynamic_pressure_pa: f64,
    /// Geometric angle of attack, degrees.
    pub alpha_deg: f64,
    /// Reynolds number on the reference chord.
    pub reynolds_chord: f64,
}

/// One surface's share of the modelled configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceShare {
    /// Surface name as the geometry builder lofted it.
    pub name: String,
    /// Lift carried by this surface, N, in wind axes.
    pub lift_n: f64,
    /// Panel count contributed to the lattice.
    pub panel_count: usize,
}

/// One spanwise station of the main wing's load distribution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpanStation {
    /// Spanwise coordinate of the strip centre, m, geometry axes.
    pub y_m: f64,
    /// Station position as a fraction of the reference semispan.
    pub y_over_semispan: f64,
    /// Local chord, m, measured on the strip.
    pub chord_m: f64,
    /// Section lift per unit span, N/m, positive up in wind axes.
    pub lift_per_span_n_m: f64,
    /// Section lift coefficient on the local chord.
    pub section_cl: f64,
    /// Local loading `c_l * c / c_ref`, the dimensionless span loading.
    pub loading: f64,
}

/// One evaluated angle of the sweep.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlphaPoint {
    /// Angle of attack, degrees.
    pub alpha_deg: f64,
    /// Lift coefficient.
    pub cl: f64,
    /// Induced (vortex) drag coefficient; no viscous or wave contribution.
    pub cd_induced: f64,
    /// Pitching-moment coefficient about the moment reference, nose-up
    /// positive.
    pub cm_pitch: f64,
}

/// The static-stability outputs of the modelled configuration.
///
/// Every derivative is a lattice finite difference about the solved point and
/// belongs to the modelled surfaces only. With the fuselage, nacelles and
/// propulsion absent, these are not aircraft stability derivatives.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StabilityOutcome {
    /// `dCL/dalpha`, per radian.
    pub cl_alpha_per_rad: f64,
    /// `dCm/dalpha` about the moment reference, per radian; negative is
    /// longitudinally stable.
    pub cm_alpha_per_rad: f64,
    /// `dCY/dbeta`, per radian.
    pub cy_beta_per_rad: f64,
    /// `dCn/dbeta`, per radian; positive is directionally stable.
    pub cn_beta_per_rad: f64,
    /// `dCl/dbeta`, per radian; negative is the usual dihedral effect.
    pub cl_beta_per_rad: f64,
    /// `dCm/dq_hat`, per radian of nondimensional pitch rate.
    pub cm_q_per_rad: f64,
    /// Longitudinal neutral point, m, geometry axes (`+x` aft).
    pub neutral_point_x_m: f64,
    /// Static margin `(x_np - x_ref) / c_ref`, positive when the neutral
    /// point lies aft of the moment reference.
    pub static_margin: f64,
}

/// Solver evidence for the accepted result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WingSolveDiagnostics {
    /// Horseshoe panels in the lattice.
    pub panel_count: usize,
    /// Relative residual of the dense circulation solve.
    pub residual: f64,
    /// `max|pivot| / min|pivot|` from the factorization.
    pub pivot_ratio: f64,
    /// Spanwise panel multiplier the lattice was assembled with.
    pub spanwise_resolution: usize,
    /// Chordwise panels per strip.
    pub chordwise_resolution: usize,
}

/// One completed wing analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct WingAnalysisOutcome {
    /// The inputs this outcome was produced from.
    pub inputs: WingAnalysisInputs,
    /// Reference quantities every coefficient uses.
    pub reference: WingReference,
    /// The resolved flight state of the reported point.
    pub condition: ResolvedCondition,
    /// Names of the modelled surfaces, in lattice order.
    pub modelled_surfaces: Vec<SurfaceShare>,
    /// Lift coefficient at the reported point.
    pub cl: f64,
    /// Induced drag coefficient at the reported point.
    pub cd_induced: f64,
    /// Pitching-moment coefficient about the moment reference.
    pub cm_pitch: f64,
    /// Lift, N, wind axes.
    pub lift_n: f64,
    /// Induced drag, N, wind axes.
    pub induced_drag_n: f64,
    /// Pitching moment about the moment reference, N m, nose-up positive.
    pub pitch_moment_n_m: f64,
    /// Span efficiency `CL^2 / (pi * AR * CDi)` of this solved point.
    ///
    /// `None` when the induced drag or the lift is too small for the ratio to
    /// carry information rather than rounding noise.
    pub span_efficiency: Option<f64>,
    /// Main-wing spanwise load distribution at the reported point.
    pub span_load: Vec<SpanStation>,
    /// The evaluated angle sweep.
    pub sweep: Vec<AlphaPoint>,
    /// Static stability of the modelled configuration, when the empennage is
    /// included and the derivative solve succeeded.
    pub stability: Option<StabilityOutcome>,
    /// Solver evidence.
    pub diagnostics: WingSolveDiagnostics,
    /// Whether the reported angle came from a lift-coefficient target.
    pub alpha_from_lift_target: bool,
}

/// Why a wing analysis could not produce an outcome.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum WingAnalysisError {
    /// The submitted inputs are outside the entry's validity domain.
    #[error("the wing analysis inputs are invalid: {0}")]
    InvalidInputs(String),
    /// The geometry scaffold could not be lofted into surfaces.
    #[error("the wing geometry could not be built: {0}")]
    Geometry(String),
    /// The requested surface is absent from the built geometry.
    #[error("the geometry contains no surface named '{0}'")]
    MissingSurface(String),
    /// The lattice solve failed.
    #[error("the vortex-lattice solve failed: {0}")]
    Solve(#[from] VlmError),
    /// The lift-coefficient target has no solution in the accepted angle
    /// range.
    #[error("no angle of attack within plus or minus {ALPHA_LIMIT_DEG} deg reaches CL = {target}")]
    UnreachableLift {
        /// The requested lift coefficient.
        target: f64,
    },
    /// The run was stopped before an outcome existed.
    #[error("the wing analysis was cancelled")]
    Cancelled,
}

impl From<BuildError> for WingAnalysisError {
    fn from(error: BuildError) -> Self {
        Self::Geometry(error.to_string())
    }
}
/// Build the standard-atmosphere state and the freestream `condition` states,
/// returning the lattice operating point and the resolved flight state.
///
/// `alpha_deg` overrides the attitude statement, which is what lets one
/// assembled lattice be solved at every sweep angle. The operating point
/// carries no body rates: a wing analysis is a steady, symmetric-freestream
/// evaluation, and the rate derivatives come from the solver's own finite
/// differences rather than from a rotating base state.
pub(crate) fn operating_point(
    condition: &FlightCondition,
    alpha_deg: f64,
    reference_chord_m: f64,
) -> (OperatingPoint, ResolvedCondition) {
    let atmosphere = Atmosphere::new(condition.altitude_m);
    let speed_of_sound = atmosphere.speed_of_sound();
    let true_airspeed = match condition.speed {
        SpeedInput::TrueAirspeed(value) => value,
        SpeedInput::Mach(value) => value * speed_of_sound,
    };
    let density = atmosphere.density();
    let viscosity = atmosphere.dynamic_viscosity();
    let resolved = ResolvedCondition {
        altitude_m: condition.altitude_m,
        density_kg_m3: density,
        speed_of_sound_m_s: speed_of_sound,
        true_airspeed_m_s: true_airspeed,
        mach: if speed_of_sound > 0.0 {
            true_airspeed / speed_of_sound
        } else {
            f64::NAN
        },
        dynamic_pressure_pa: 0.5 * density * true_airspeed * true_airspeed,
        alpha_deg,
        reynolds_chord: if viscosity > 0.0 {
            density * true_airspeed * reference_chord_m / viscosity
        } else {
            f64::NAN
        },
    };
    let point = OperatingPoint::new(atmosphere, true_airspeed, alpha_deg, 0.0, 0.0, 0.0, 0.0);
    (point, resolved)
}

/// Restate the dense-solve evidence with the mesh it was produced on.
pub(crate) fn diagnostics(
    solve: &SolveDiagnostics,
    panel_count: usize,
    inputs: &WingAnalysisInputs,
) -> WingSolveDiagnostics {
    WingSolveDiagnostics {
        panel_count,
        residual: solve.normalized_residual,
        pivot_ratio: solve.pivot_ratio,
        spanwise_resolution: inputs.spanwise_resolution,
        chordwise_resolution: inputs.chordwise_resolution,
    }
}

/// Components the modelled configuration never contains, for the label every
/// outcome must carry.
pub const OMITTED_COMPONENTS: &[&str] = &[
    "fuselage",
    "nacelles and pylons",
    "control-surface deflections",
    "propulsion and its interference",
    "viscous, profile and wave drag",
];
