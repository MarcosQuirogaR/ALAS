// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native OpenFOAM wall-field extraction and aerodynamic surface samples.
//!
//! In incompressible OpenFOAM cases `p` and `wallShearStress` are kinematic
//! fields (`m^2/s^2`).  The native mesh parser in [`surface_native`] pairs
//! those fields with the converted airfoil faces before deriving Cp/Cf.

#[path = "surface_native.rs"]
mod surface_native;

use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::Path;

/// Error raised when native surface data cannot be read or paired safely.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SurfaceError {
    /// A caller supplied a non-finite, empty, or otherwise unsafe value.
    InvalidInput(String),
    /// A case file could not be read.
    Io {
        /// File path that could not be read.
        path: String,
        /// Operating-system error text.
        message: String,
    },
    /// A native OpenFOAM file did not follow the supported ASCII grammar.
    Parse {
        /// Native file or export being parsed.
        source: String,
        /// Grammar or dimensionality error text.
        message: String,
    },
    /// Mesh faces and field values could not be paired without ambiguity.
    Mismatch(String),
}
impl fmt::Display for SurfaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(value) | Self::Mismatch(value) => f.write_str(value),
            Self::Io { path, message } => write!(f, "cannot read {path}: {message}"),
            Self::Parse { source, message } => write!(f, "cannot parse {source}: {message}"),
        }
    }
}
impl Error for SurfaceError {}

/// Dimensional reference quantities for an airfoil extrusion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceReference {
    /// Freestream density in kg/m^3.
    pub density_kg_m3: f64,
    /// Freestream speed in m/s.
    pub speed_m_s: f64,
    /// Geometric chord in m.
    pub chord_m: f64,
    /// Flow angle measured from +x, in degrees.
    pub angle_of_attack_deg: f64,
    /// Reference area in m^2, including the physical extrusion span.
    pub reference_area_m2: f64,
    /// Kinematic pressure offset to subtract before multiplying by density,
    /// in Pa.  Use zero for the usual gauge-pressure case.
    pub pressure_reference_pa: f64,
    /// Moment reference point in m in the case coordinate frame.
    pub moment_reference_m: [f64; 3],
}
impl Default for SurfaceReference {
    fn default() -> Self {
        Self {
            density_kg_m3: 1.225,
            speed_m_s: 51.0,
            chord_m: 1.0,
            angle_of_attack_deg: 0.0,
            reference_area_m2: 0.01,
            pressure_reference_pa: 0.0,
            moment_reference_m: [0.25, 0.0, 0.0],
        }
    }
}
impl SurfaceReference {
    /// Construct a reference for a two-dimensional airfoil with the supplied
    /// chord and finite extrusion span.
    pub fn for_two_dimensional_chord(chord_m: f64, span_m: f64) -> Self {
        Self {
            chord_m,
            reference_area_m2: chord_m * span_m,
            moment_reference_m: [0.25 * chord_m, 0.0, 0.0],
            ..Self::default()
        }
    }
    fn validate(&self) -> Result<(), SurfaceError> {
        for (name, value) in [
            ("density", self.density_kg_m3),
            ("speed", self.speed_m_s),
            ("chord", self.chord_m),
            ("reference area", self.reference_area_m2),
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err(SurfaceError::InvalidInput(format!(
                    "surface reference {name} must be finite and positive"
                )));
            }
        }
        if !self.angle_of_attack_deg.is_finite()
            || !self.pressure_reference_pa.is_finite()
            || self.moment_reference_m.iter().any(|v| !v.is_finite())
        {
            return Err(SurfaceError::InvalidInput(
                "surface reference contains a non-finite value".to_owned(),
            ));
        }
        Ok(())
    }
    /// Return q = 0.5*rho*U^2 in Pa.
    pub fn dynamic_pressure_pa(&self) -> f64 {
        0.5 * self.density_kg_m3 * self.speed_m_s * self.speed_m_s
    }
    /// Unit vector in the freestream/drag direction in the x-y plane.
    pub fn drag_direction(&self) -> [f64; 3] {
        let a = self.angle_of_attack_deg.to_radians();
        [a.cos(), a.sin(), 0.0]
    }
    /// Unit vector normal to the freestream, positive toward lift.
    pub fn lift_direction(&self) -> [f64; 3] {
        let a = self.angle_of_attack_deg.to_radians();
        [-a.sin(), a.cos(), 0.0]
    }
}

/// One native wall face and its derived pressure/skin-friction quantities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceSample {
    /// Zero-based face index within the named wall patch.
    pub patch_face_index: usize,
    /// Zero-based face index in `polyMesh/faces`.
    pub global_face_index: usize,
    /// Face centroid in m.
    pub center_m: [f64; 3],
    /// Oriented face area vector in m^2.
    pub area_vector_m2: [f64; 3],
    /// Face area in m^2.
    pub face_area_m2: f64,
    /// Two-dimensional wall-face length in m.
    pub surface_length_m: f64,
    /// Ordered arc length from the first airfoil sample, in m.
    pub arc_length_m: f64,
    /// Unit tangent chosen toward increasing chordwise x.
    pub tangent_plus_chord: [f64; 3],
    /// OpenFOAM kinematic pressure p in m^2/s^2.
    pub p_kinematic_m2_s2: f64,
    /// Dynamic pressure rho*p in Pa after the reference offset is applied.
    pub pressure_pa: f64,
    /// OpenFOAM kinematic wall shear in m^2/s^2.
    pub wall_shear_kinematic_m2_s2: [f64; 3],
    /// Wall shear stress in Pa, with the native field convention.
    pub wall_shear_stress_pa: [f64; 3],
    /// Magnitude of wall shear stress in Pa.
    pub wall_shear_magnitude_pa: f64,
    /// Signed coefficient on the fluid projected toward increasing x.
    /// `wallShearStress` is reported with the opposite (fluid-on-wall) sign.
    pub cf: f64,
    /// Non-negative magnitude of the skin-friction coefficient.
    pub cf_magnitude: f64,
    /// Pressure coefficient `(p - p_ref)/q`.
    pub cp: f64,
}

/// Ordering used when producing surface distributions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurfaceOrder {
    /// Order reconstructed from the native mesh edge topology.
    MeshTopology,
    /// Fallback nearest-neighbour order based on face centres.
    GeometricNearestNeighbour,
}

/// Integrated loads and coefficients derived from wall samples.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceForceSummary {
    /// Pressure contribution to the integrated force in N.
    pub pressure_force_n: [f64; 3],
    /// Viscous contribution to the integrated force in N.
    pub viscous_force_n: [f64; 3],
    /// Sum of pressure and viscous force in N.
    pub total_force_n: [f64; 3],
    /// Pressure contribution to the moment in N m.
    pub pressure_moment_nm: [f64; 3],
    /// Viscous contribution to the moment in N m.
    pub viscous_moment_nm: [f64; 3],
    /// Sum of pressure and viscous moments in N m.
    pub total_moment_nm: [f64; 3],
    /// Pressure drag coefficient.
    pub cd_pressure: f64,
    /// Viscous drag coefficient.
    pub cd_viscous: f64,
    /// Total drag coefficient.
    pub cd: f64,
    /// Pressure lift coefficient.
    pub cl_pressure: f64,
    /// Viscous lift coefficient.
    pub cl_viscous: f64,
    /// Total lift coefficient.
    pub cl: f64,
    /// Pitch coefficient about the negative-z convention used by forceCoeffs.
    pub cm_pressure: f64,
    /// Viscous pitch coefficient about the negative-z convention.
    pub cm_viscous: f64,
    /// Total pitch coefficient about the negative-z convention.
    pub cm: f64,
}

/// Complete native distribution, retaining source paths and ordering quality.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceDistribution {
    /// Numeric OpenFOAM time represented by this distribution.
    pub time: f64,
    /// Wall patch from which samples were extracted.
    pub patch_name: String,
    /// Source pressure field path relative to the time directory.
    pub pressure_field: String,
    /// Source wall-shear field path relative to the time directory.
    pub wall_shear_field: String,
    /// Ordering used for the returned samples.
    pub order: SurfaceOrder,
    /// Per-face geometric and aerodynamic values.
    pub samples: Vec<SurfaceSample>,
    /// Integrated forces, moments, and nondimensional coefficients.
    pub forces: SurfaceForceSummary,
}

/// One row from a `surfaceFormat raw` export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawSurfaceScalar {
    /// Sample location in m.
    pub point_m: [f64; 3],
    /// Scalar value in the field's native units.
    pub value: f64,
}
/// One vector row from a `surfaceFormat raw` export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawSurfaceVector {
    /// Sample location in m.
    pub point_m: [f64; 3],
    /// Vector value in the field's native units.
    pub value: [f64; 3],
}
/// Paired pressure and wall-shear values at one ordered raw-surface point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SampledSurfaceSample {
    /// Sample location in m.
    pub point_m: [f64; 3],
    /// Ordered arc length in m.
    pub arc_length_m: f64,
    /// Unit tangent toward increasing chordwise x.
    pub tangent_plus_chord: [f64; 3],
    /// Kinematic pressure in m^2/s^2.
    pub p_kinematic_m2_s2: f64,
    /// Pressure after density conversion in Pa.
    pub pressure_pa: f64,
    /// Kinematic wall shear in m^2/s^2.
    pub wall_shear_kinematic_m2_s2: [f64; 3],
    /// Wall shear stress in Pa.
    pub wall_shear_stress_pa: [f64; 3],
    /// Magnitude of wall shear stress in Pa.
    pub wall_shear_magnitude_pa: f64,
    /// Signed skin-friction coefficient toward increasing x.
    pub cf: f64,
    /// Non-negative skin-friction magnitude.
    pub cf_magnitude: f64,
    /// Pressure coefficient.
    pub cp: f64,
}
/// Ordered samples reconstructed from paired raw surface exports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SampledSurfaceDistribution {
    /// Ordering strategy used for the samples.
    pub order: SurfaceOrder,
    /// Ordered paired surface values.
    pub samples: Vec<SampledSurfaceSample>,
}

/// Return the greatest numeric case-time directory, preserving its spelling.
pub fn latest_numeric_time(case_dir: &Path) -> Result<String, SurfaceError> {
    let mut selected = None;
    for entry in fs::read_dir(case_dir).map_err(|e| surface_native::io(case_dir, e))? {
        let entry = entry.map_err(|e| surface_native::io(case_dir, e))?;
        if !entry
            .file_type()
            .map_err(|e| surface_native::io(&entry.path(), e))?
            .is_dir()
        {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(value) = name.parse::<f64>() else {
            continue;
        };
        if value.is_finite()
            && selected
                .as_ref()
                .is_none_or(|old: &(f64, String)| value > old.0)
        {
            selected = Some((value, name));
        }
    }
    selected.map(|(_, name)| name).ok_or_else(|| {
        SurfaceError::InvalidInput(format!(
            "case {} has no numeric time directory",
            case_dir.display()
        ))
    })
}

/// Parse native fields and exact wall-face geometry from one case/time.
pub fn parse_surface_case(
    case_dir: &Path,
    time: &str,
    reference: &SurfaceReference,
) -> Result<SurfaceDistribution, SurfaceError> {
    reference.validate()?;
    let time_name = if time == "latest" {
        latest_numeric_time(case_dir)?
    } else {
        time.to_owned()
    };
    if time_name.is_empty()
        || time_name.contains('/')
        || time_name.contains('\\')
        || time_name == "."
        || time_name == ".."
    {
        return Err(SurfaceError::InvalidInput(
            "invalid OpenFOAM time directory".to_owned(),
        ));
    }
    surface_native::parse_case(case_dir, &time_name, reference)
}

/// Parse a scalar raw surface export (`x y z value`).
pub fn parse_raw_scalar_surface(text: &str) -> Result<Vec<RawSurfaceScalar>, SurfaceError> {
    let (rows, declared) = surface_native::raw_rows(text, 1)?;
    let values = rows
        .into_iter()
        .map(|(point, value)| RawSurfaceScalar {
            point_m: point,
            value: value[0],
        })
        .collect::<Vec<_>>();
    surface_native::count_check(values.len(), declared, "scalar raw surface")?;
    Ok(values)
}

/// Parse a vector raw surface export (`x y z vx vy vz`).
pub fn parse_raw_vector_surface(text: &str) -> Result<Vec<RawSurfaceVector>, SurfaceError> {
    let (rows, declared) = surface_native::raw_rows(text, 3)?;
    let values = rows
        .into_iter()
        .map(|(point, value)| RawSurfaceVector {
            point_m: point,
            value: [value[0], value[1], value[2]],
        })
        .collect::<Vec<_>>();
    surface_native::count_check(values.len(), declared, "vector raw surface")?;
    Ok(values)
}

/// Build Cp/Cf samples from paired native `surfaceFormat raw` files.
pub fn parse_sampled_surface_fields(
    pressure_text: &str,
    wall_shear_text: &str,
    reference: &SurfaceReference,
) -> Result<SampledSurfaceDistribution, SurfaceError> {
    reference.validate()?;
    surface_native::parse_sampled(pressure_text, wall_shear_text, reference)
}
