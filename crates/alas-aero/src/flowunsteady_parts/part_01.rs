// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::fmt::Write as FmtWrite;

/// One whole-aircraft, time-averaged FLOWUnsteady coefficient sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlowUnsteadyPoint {
    /// Geometric angle of attack in degrees.
    pub alpha_deg: f64,
    /// Freestream Mach number.
    pub mach: f64,
    /// Sideslip angle in degrees.
    pub beta_deg: f64,
    /// Time-averaged, wind-axis lift coefficient.
    pub lift_coefficient: f64,
    /// Time-averaged body-axis pitching-moment coefficient.
    pub pitching_moment_coefficient: f64,
}

/// Declared normalization and physical scope of the adapter result.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowUnsteadyPolar {
    /// Reference planform area in square meters.
    pub area_m2: f64,
    /// Reference chord in meters.
    pub chord_m: f64,
    /// Reference span in meters.
    pub span_m: f64,
    /// Moment origin in geometry axes, meters.
    pub moment_reference_m: [f64; 3],
    /// True only for all lifting surfaces, excluding bodies and propulsion.
    pub lifting_surfaces_only: bool,
    /// True only when lift is reported in wind axes.
    pub lift_is_wind_axis: bool,
    /// True only when Cm is about the stated origin in body axes.
    pub pitch_moment_is_body_axis: bool,
    /// Adapter samples in requested order.
    pub points: Vec<FlowUnsteadyPoint>,
}

/// One physical airfoil section attached to an exported lifting surface.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowUnsteadySection {
    /// Leading-edge geometry position in ALAS geometry axes, m.
    pub leading_edge_m: [f64; 3],
    /// Local chord, m.
    pub chord_m: f64,
    /// Local geometric twist about the leading edge, degrees.
    pub twist_deg: f64,
    /// Source airfoil identity; not a solver-specific profile alias.
    pub airfoil_name: String,
    /// Normalized `(x/c, z/c)` contour in the source airfoil ordering.
    pub airfoil_coordinates: Vec<(f64, f64)>,
}

/// One lifting surface, including its physical mirror declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowUnsteadySurface {
    /// ALAS geometry name.
    pub name: String,
    /// Mirror this surface about the XZ plane when true.
    pub symmetric_about_xz: bool,
    /// Root-to-tip loft stations.
    pub sections: Vec<FlowUnsteadySection>,
}

/// A configured control-surface region, deliberately separate from the wing
/// loft because the current ALAS lifting geometry has no deflectable mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowUnsteadyControlSurface {
    /// Stable descriptive role: slat, flap, aileron, spoiler, elevator, rudder.
    pub role: &'static str,
    /// ALAS surface name receiving the region.
    pub surface_name: &'static str,
    /// Leading or trailing edge convention.
    pub edge: &'static str,
    /// Local chord fraction occupied by the control.
    pub chord_fraction: f64,
    /// Rootward normalized span coordinate.
    pub span_start_fraction: f64,
    /// Tipward normalized span coordinate.
    pub span_end_fraction: f64,
    /// Commanded deflection in degrees. The present ALAS polar is clean, so
    /// every exported control has a zero command.
    pub deflection_deg: f64,
    /// False records that the current ALAS geometry did not apply this region
    /// to the exported lifting mesh; an adapter must not silently assume it did.
    pub applied_to_geometry: bool,
}

/// SI freestream and rigid-body state for every requested alpha sample.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowUnsteadyFlightCondition {
    /// ISA geometric altitude, m.
    pub altitude_m: f64,
    /// Static pressure, Pa.
    pub pressure_pa: f64,
    /// Static temperature, K.
    pub temperature_k: f64,
    /// Density, kg/m^3.
    pub density_kg_m3: f64,
    /// Speed of sound, m/s.
    pub speed_of_sound_m_s: f64,
    /// True airspeed, m/s.
    pub true_airspeed_m_s: f64,
    /// Freestream Mach number.
    pub mach: f64,
    /// Sideslip, degrees.
    pub beta_deg: f64,
    /// Body angular rates `[p, q, r]`, rad/s.
    pub angular_rates_rad_s: [f64; 3],
}

/// Explicit numerical request passed to a reviewed adapter.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowUnsteadySolverRequest {
    /// Algorithm class requested from the adapter, not a claim that ALAS ran it.
    pub model: &'static str,
    /// Convective time steps per reference chord.
    pub steps_per_reference_chord: u32,
    /// Wake age retained behind the aircraft, reference chords.
    pub wake_age_reference_chords: f64,
    /// Initial settling interval discarded from time averages, reference chords.
    pub settling_reference_chords: f64,
    /// Averaging interval after settling, reference chords.
    pub averaging_reference_chords: f64,
}

/// Input sufficient for a reviewed adapter to reconstruct the optimized
/// lifting geometry and its clean cruise polar in SI units.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowUnsteadyRequest {
    /// Shared aircraft references, SI.
    pub area_m2: f64,
    /// Shared reference chord, SI.
    pub chord_m: f64,
    /// Shared reference span, SI.
    pub span_m: f64,
    /// Shared moment origin, SI.
    pub moment_reference_m: [f64; 3],
    /// Complete lifting geometry; bodies and propulsion are intentionally absent.
    pub lifting_surfaces: Vec<FlowUnsteadySurface>,
    /// Configured controls and their zero-command, not-applied provenance.
    pub controls: Vec<FlowUnsteadyControlSurface>,
    /// Freestream state shared by every requested alpha.
    pub flight_condition: FlowUnsteadyFlightCondition,
    /// Adapter numerical settings, expressed in convective reference-chord units.
    pub solver: FlowUnsteadySolverRequest,
    /// Requested angle schedule in degrees, at the flight condition above.
    pub alpha_deg: Vec<f64>,
}

/// A malformed adapter request or result is never accepted as solver data.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum FlowUnsteadyError {
    /// A required line or key was absent.
    #[error("FLOWUnsteady adapter output is missing {0}")]
    Missing(&'static str),
    /// A token is not a finite number.
    #[error("invalid FLOWUnsteady {field}: {token}")]
    InvalidNumber {
        /// Name of the rejected physical field.
        field: &'static str,
        /// Raw adapter token rejected as non-finite or non-numeric.
        token: String,
    },
    /// A finite-value field used an unsupported literal such as a malformed
    /// boolean flag.
    #[error("invalid FLOWUnsteady {field}: {token}")]
    InvalidValue {
        /// Name of the rejected field.
        field: &'static str,
        /// Raw literal rejected by the protocol.
        token: String,
    },
    /// The adapter file is a different protocol revision.
    #[error("unsupported FLOWUnsteady adapter protocol")]
    Protocol,
}
