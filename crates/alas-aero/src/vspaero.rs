// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! VSPAERO batch inputs and native polar parsing.
//!
//! VSPAERO coefficients are meaningful only with their reference area,
//! reference lengths, moment origin, geometry scope, and axis definitions.
//! The native `.polar` does not repeat all of that metadata, so parsing takes
//! the adjacent `.vspaero` setup and an explicit length unit. This prevents a
//! plausible coefficient vector from losing the normalization that defines it.

use std::collections::HashMap;
use std::fmt::Write as FmtWrite;

/// Unit used by the geometry and reference values in a VSPAERO case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceLengthUnit {
    /// Meters and square meters.
    Meter,
    /// Inches and square inches.
    Inch,
    /// Feet and square feet.
    Foot,
}

impl ReferenceLengthUnit {
    fn meters_per_unit(self) -> f64 {
        match self {
            Self::Meter => 1.0,
            Self::Inch => 0.0254,
            Self::Foot => 0.3048,
        }
    }
}

/// Aerodynamic reference quantities, normalized to SI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VspaeroReference {
    /// Planform reference area, in square meters.
    pub area_m2: f64,
    /// Pitching-moment reference chord, in meters.
    pub chord_m: f64,
    /// Rolling/yawing-moment reference span, in meters.
    pub span_m: f64,
    /// Moment origin in geometry axes `[x, y, z]`, in meters.
    pub moment_reference_m: [f64; 3],
}

/// Geometry represented by the solver mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VspaeroGeometryScope {
    /// Wing and tail lifting surfaces, with no fuselage, nacelle, or gear mesh.
    LiftingSurfacesOnly,
    /// Thick whole-aircraft surface mesh.
    FullAircraft,
}

/// VSPAERO formulation used to produce the polar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VspaeroMethod {
    /// Thin-surface vortex-lattice formulation.
    VortexLattice,
    /// Thick-surface panel formulation.
    Panel,
}

/// Axis definitions attached to parsed coefficient columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VspaeroCoefficientFrames {
    /// `CLtot`, `CDtot`, `CDi`, and `CStot` use wind axes.
    pub forces_are_wind_axes: bool,
    /// `CMxtot`, `CMytot`, and `CMztot` use body axes.
    pub moments_are_body_axes: bool,
}

/// Model metadata not encoded in the native polar itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VspaeroModel {
    /// Solver formulation.
    pub method: VspaeroMethod,
    /// Geometry included in the aerodynamic mesh.
    pub geometry_scope: VspaeroGeometryScope,
    /// Force and moment coefficient frames.
    pub frames: VspaeroCoefficientFrames,
}

impl VspaeroModel {
    /// ALAS's independent thin-surface whole-aircraft lifting model.
    pub const ALAS_VLM: Self = Self {
        method: VspaeroMethod::VortexLattice,
        geometry_scope: VspaeroGeometryScope::LiftingSurfacesOnly,
        frames: VspaeroCoefficientFrames {
            forces_are_wind_axes: true,
            moments_are_body_axes: true,
        },
    };
}

/// One converged row of VSPAERO's native `.polar` output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VspaeroPolarPoint {
    /// Sideslip angle, in degrees.
    pub beta_deg: f64,
    /// Freestream Mach number.
    pub mach: f64,
    /// Geometric angle of attack, in degrees.
    pub alpha_deg: f64,
    /// Reynolds number based on the reference chord.
    pub reynolds: f64,
    /// Total wind-axis lift coefficient.
    pub lift_coefficient: f64,
    /// Induced wind-axis drag coefficient.
    pub induced_drag_coefficient: f64,
    /// Total wind-axis drag coefficient as written by VSPAERO.
    pub total_drag_coefficient: f64,
    /// Wind-axis side-force coefficient.
    pub side_force_coefficient: f64,
    /// Lift-to-total-drag ratio.
    pub lift_to_drag: f64,
    /// Span efficiency, absent where VSPAERO writes NaN.
    pub span_efficiency: Option<f64>,
    /// Body-axis rolling-moment coefficient.
    pub rolling_moment_coefficient: f64,
    /// Body-axis pitching-moment coefficient.
    pub pitching_moment_coefficient: f64,
    /// Body-axis yawing-moment coefficient.
    pub yawing_moment_coefficient: f64,
}

/// Parsed VSPAERO polar with the metadata required to interpret it.
#[derive(Debug, Clone, PartialEq)]
pub struct VspaeroPolar {
    /// SI reference quantities parsed from the case setup.
    pub reference: VspaeroReference,
    /// Solver and geometry contract supplied by the caller that built the mesh.
    pub model: VspaeroModel,
    /// Converged polar rows in native file order.
    pub points: Vec<VspaeroPolarPoint>,
}

/// Flow schedule and normalization written to a native `.vspaero` setup.
#[derive(Debug, Clone, PartialEq)]
pub struct VspaeroSweepRequest {
    /// SI aerodynamic references.
    pub reference: VspaeroReference,
    /// Freestream Mach number.
    pub mach: f64,
    /// Angle-of-attack schedule, in degrees.
    pub alpha_deg: Vec<f64>,
    /// Sideslip angle, in degrees.
    pub beta_deg: f64,
    /// Reynolds number based on the reference chord.
    pub reynolds: f64,
    /// Freestream speed, in meters per second.
    pub speed_m_s: f64,
    /// Freestream density, in kilograms per cubic meter.
    pub density_kg_m3: f64,
    /// Wake relaxation iterations.
    pub wake_iterations: usize,
}

/// Why a VSPAERO setup or native output could not be accepted.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum VspaeroError {
    /// A required input is non-finite or non-physical.
    #[error("invalid VSPAERO input: {0}")]
    InvalidInput(String),
    /// The native setup omitted a required reference scalar.
    #[error("VSPAERO setup is missing {0}")]
    MissingSetupValue(&'static str),
    /// The polar header omitted a required coefficient column.
    #[error("VSPAERO polar is missing column {0}")]
    MissingColumn(&'static str),
    /// A native numeric token could not be decoded.
    #[error("invalid VSPAERO number at row {row}, column {column}: {token}")]
    InvalidNumber {
        /// One-based file line number.
        row: usize,
        /// Native column name.
        column: &'static str,
        /// Rejected token.
        token: String,
    },
    /// A polar row ended before all named columns were present.
    #[error("VSPAERO polar row {row} has {actual} columns; expected at least {expected}")]
    ShortRow {
        /// One-based file line number.
        row: usize,
        /// Required number of columns.
        expected: usize,
        /// Observed number of columns.
        actual: usize,
    },
    /// The native polar contained no data rows.
    #[error("VSPAERO polar contains no data rows")]
    EmptyPolar,
}

/// Render a native SI `.vspaero` setup without using OpenVSP result APIs.
pub fn render_setup(request: &VspaeroSweepRequest) -> Result<String, VspaeroError> {
    validate_request(request)?;
    let alpha = request
        .alpha_deg
        .iter()
        .map(|value| format!("{value:.12}"))
        .collect::<Vec<_>>()
        .join(", ");
    let reference = request.reference;
    let mut setup = String::new();
    let _ = writeln!(setup, "Sref = {:.12}", reference.area_m2);
    let _ = writeln!(setup, "Cref = {:.12}", reference.chord_m);
    let _ = writeln!(setup, "Bref = {:.12}", reference.span_m);
    let _ = writeln!(setup, "X_cg = {:.12}", reference.moment_reference_m[0]);
    let _ = writeln!(setup, "Y_cg = {:.12}", reference.moment_reference_m[1]);
    let _ = writeln!(setup, "Z_cg = {:.12}", reference.moment_reference_m[2]);
    let _ = writeln!(setup, "Mach = {:.12}", request.mach);
    let _ = writeln!(setup, "AoA = {alpha}");
    let _ = writeln!(setup, "Beta = {:.12}", request.beta_deg);
    let _ = writeln!(setup, "ReCref = {:.12}", request.reynolds);
    let _ = writeln!(setup, "Vinf = {:.12}", request.speed_m_s);
    let _ = writeln!(setup, "Rho = {:.12}", request.density_kg_m3);
    setup.push_str("StallModel = 0\nClo2D = 0\nCLMax2D = 1\nSymmetry = 0\n");
    setup.push_str("FreezeMultiPoleAtIteration = 10000\nFreezeWakeAtIteration = 10000\n");
    setup
        .push_str("FreezeWakeRootVortices = 0\nImplicitWake = 0\nImplicitWakeStartIteration = 0\n");
    setup.push_str("FarDist = -1\nNumWakeNodes = 8\n");
    let _ = writeln!(setup, "WakeIters = {}", request.wake_iterations.max(1));
    setup.push_str("WakeRelax = 1\nForwardGMRESConvergenceFactor = 1\n");
    setup.push_str("AdjointGMRESConvergenceFactor = 1\nNonLinearConvergenceFactor = 1\n");
    setup.push_str("CoreSizeFactor = 1\nFarAway = 5\nUpdateMatrixPreconditioner = 0\n");
    setup.push_str(
        "UseWakeNodeMatrixPreconditioner = 0\nWrite2DFEMFile = 0\nWriteTecplotFile = 0\n",
    );
    setup.push_str(
        "NumberOfControlGroups = 0\nNumberofSurveyPoints = 0\nQuadTreeBufferLevels = 0\n",
    );
    setup.push_str("NumberOfQuadTrees = 1\n1 2 0\nNumberOfInlets = 0\nNumberOfNozzles = 0\n");
    setup.push_str("VSP_StabilityType = 0\n");
    Ok(setup)
}

/// Parse native `.polar` and `.vspaero` text into a fully referenced polar.
pub fn parse_polar(
    polar_text: &str,
    setup_text: &str,
    length_unit: ReferenceLengthUnit,
    model: VspaeroModel,
) -> Result<VspaeroPolar, VspaeroError> {
    let reference = parse_reference(setup_text, length_unit)?;
    let lines = polar_text.lines().collect::<Vec<_>>();
    let (header_line, headers) = lines
        .iter()
        .enumerate()
        .map(|(index, line)| (index, line.split_whitespace().collect::<Vec<_>>()))
        .find(|(_, fields)| {
            ["Beta", "Mach", "AoA", "CLtot", "CDi", "CDtot", "CMytot"]
                .iter()
                .all(|name| fields.contains(name))
        })
        .ok_or(VspaeroError::MissingColumn(
            "Beta/Mach/AoA/CLtot/CDi/CDtot/CMytot",
        ))?;
    let columns = headers
        .iter()
        .enumerate()
        .map(|(index, name)| (*name, index))
        .collect::<HashMap<_, _>>();
    let required = [
        "Beta", "Mach", "AoA", "Re/1e6", "CLtot", "CDi", "CDtot", "CStot", "L/D", "E", "CMxtot",
        "CMytot", "CMztot",
    ];
    for name in required {
        if !columns.contains_key(name) {
            return Err(VspaeroError::MissingColumn(name));
        }
    }
    let maximum_index = required
        .iter()
        .filter_map(|name| columns.get(name).copied())
        .max()
        .unwrap_or(0);
    let mut points = Vec::new();
    for (line_index, line) in lines.iter().enumerate().skip(header_line + 1) {
        if line.trim().is_empty() {
            continue;
        }
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() <= maximum_index {
            return Err(VspaeroError::ShortRow {
                row: line_index + 1,
                expected: maximum_index + 1,
                actual: fields.len(),
            });
        }
        let value = |name: &'static str| -> Result<f64, VspaeroError> {
            let index = columns[name];
            parse_number(fields[index]).map_err(|()| VspaeroError::InvalidNumber {
                row: line_index + 1,
                column: name,
                token: fields[index].to_owned(),
            })
        };
        let point = VspaeroPolarPoint {
            beta_deg: value("Beta")?,
            mach: value("Mach")?,
            alpha_deg: value("AoA")?,
            reynolds: value("Re/1e6")? * 1.0e6,
            lift_coefficient: value("CLtot")?,
            induced_drag_coefficient: value("CDi")?,
            total_drag_coefficient: value("CDtot")?,
            side_force_coefficient: value("CStot")?,
            lift_to_drag: value("L/D")?,
            span_efficiency: finite_option(value("E")?),
            rolling_moment_coefficient: value("CMxtot")?,
            pitching_moment_coefficient: value("CMytot")?,
            yawing_moment_coefficient: value("CMztot")?,
        };
        let required_finite = [
            point.beta_deg,
            point.mach,
            point.alpha_deg,
            point.reynolds,
            point.lift_coefficient,
            point.induced_drag_coefficient,
            point.total_drag_coefficient,
            point.side_force_coefficient,
            point.lift_to_drag,
            point.rolling_moment_coefficient,
            point.pitching_moment_coefficient,
            point.yawing_moment_coefficient,
        ];
        if required_finite.iter().any(|value| !value.is_finite()) {
            return Err(VspaeroError::InvalidInput(format!(
                "non-finite required coefficient on polar row {}",
                line_index + 1
            )));
        }
        points.push(point);
    }
    if points.is_empty() {
        return Err(VspaeroError::EmptyPolar);
    }
    Ok(VspaeroPolar {
        reference,
        model,
        points,
    })
}

fn validate_request(request: &VspaeroSweepRequest) -> Result<(), VspaeroError> {
    let reference = request.reference;
    let positive = [
        ("reference area", reference.area_m2),
        ("reference chord", reference.chord_m),
        ("reference span", reference.span_m),
        ("Reynolds number", request.reynolds),
        ("freestream speed", request.speed_m_s),
        ("density", request.density_kg_m3),
    ];
    if let Some((name, value)) = positive
        .into_iter()
        .find(|(_, value)| !value.is_finite() || *value <= 0.0)
    {
        return Err(VspaeroError::InvalidInput(format!(
            "{name} must be positive and finite, got {value}"
        )));
    }
    let finite = [request.mach, request.beta_deg]
        .into_iter()
        .chain(reference.moment_reference_m)
        .chain(request.alpha_deg.iter().copied());
    if finite.into_iter().any(|value| !value.is_finite()) {
        return Err(VspaeroError::InvalidInput(
            "flow schedule and moment reference must be finite".to_owned(),
        ));
    }
    if request.alpha_deg.is_empty() {
        return Err(VspaeroError::InvalidInput(
            "angle-of-attack schedule is empty".to_owned(),
        ));
    }
    Ok(())
}

fn parse_reference(
    setup_text: &str,
    length_unit: ReferenceLengthUnit,
) -> Result<VspaeroReference, VspaeroError> {
    let values = setup_text
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(name, value)| (name.trim(), value.trim()))
        .collect::<HashMap<_, _>>();
    let scalar = |name: &'static str| -> Result<f64, VspaeroError> {
        let token = values
            .get(name)
            .ok_or(VspaeroError::MissingSetupValue(name))?;
        parse_number(token).map_err(|()| VspaeroError::InvalidNumber {
            row: 0,
            column: name,
            token: (*token).to_owned(),
        })
    };
    let length_scale = length_unit.meters_per_unit();
    let reference = VspaeroReference {
        area_m2: scalar("Sref")? * length_scale * length_scale,
        chord_m: scalar("Cref")? * length_scale,
        span_m: scalar("Bref")? * length_scale,
        moment_reference_m: [
            scalar("X_cg")? * length_scale,
            scalar("Y_cg")? * length_scale,
            scalar("Z_cg")? * length_scale,
        ],
    };
    if [reference.area_m2, reference.chord_m, reference.span_m]
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return Err(VspaeroError::InvalidInput(
            "parsed aerodynamic references must be positive and finite".to_owned(),
        ));
    }
    Ok(reference)
}

fn parse_number(token: &str) -> Result<f64, ()> {
    let lower = token.to_ascii_lowercase();
    if lower.contains("nan") || lower.contains("#ind") {
        return Ok(f64::NAN);
    }
    token.parse::<f64>().map_err(|_| ())
}

fn finite_option(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_round_trip_preserves_si_references() {
        let request = VspaeroSweepRequest {
            reference: VspaeroReference {
                area_m2: 122.6,
                chord_m: 4.2,
                span_m: 35.8,
                moment_reference_m: [12.0, 0.0, 1.5],
            },
            mach: 0.78,
            alpha_deg: vec![-2.0, 0.0, 3.0],
            beta_deg: 0.0,
            reynolds: 2.1e7,
            speed_m_s: 230.0,
            density_kg_m3: 0.36,
            wake_iterations: 5,
        };
        let setup =
            render_setup(&request).unwrap_or_else(|error| panic!("render VSPAERO setup: {error}"));
        let parsed = parse_reference(&setup, ReferenceLengthUnit::Meter)
            .unwrap_or_else(|error| panic!("parse VSPAERO setup: {error}"));
        assert_eq!(parsed, request.reference);
        assert!(setup.contains("AoA = -2.000000000000, 0.000000000000, 3.000000000000"));
    }

    #[test]
    fn invalid_references_are_rejected_before_a_case_is_written() {
        let request = VspaeroSweepRequest {
            reference: VspaeroReference {
                area_m2: 0.0,
                chord_m: 1.0,
                span_m: 1.0,
                moment_reference_m: [0.0; 3],
            },
            mach: 0.0,
            alpha_deg: vec![0.0],
            beta_deg: 0.0,
            reynolds: 1.0,
            speed_m_s: 1.0,
            density_kg_m3: 1.0,
            wake_iterations: 1,
        };
        assert!(matches!(
            render_setup(&request),
            Err(VspaeroError::InvalidInput(_))
        ));
    }
}
