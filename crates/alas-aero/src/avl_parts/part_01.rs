// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::fmt::Write as FmtWrite;

use alas_geom::aircraft::airplane::Airplane;

/// Aerodynamic normalization attached to an AVL case.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AvlReference {
    /// Reference planform area, in square meters.
    pub area_m2: f64,
    /// Pitching-moment reference chord, in meters.
    pub chord_m: f64,
    /// Rolling/yawing-moment reference span, in meters.
    pub span_m: f64,
    /// Moment origin in AVL geometry axes `[x, y, z]`, in meters.
    pub moment_reference_m: [f64; 3],
}

/// Physical scope and coefficient frames of a parsed AVL solve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AvlModel {
    /// True when fuselage and nacelle slender-body elements are omitted.
    pub lifting_surfaces_only: bool,
    /// True when CL is the stability-axis total-lift coefficient.
    pub lift_is_stability_axis: bool,
    /// True when Cm is the standard-axis pitching moment about `Yref`.
    pub pitch_moment_is_standard_axis: bool,
}

/// Coordinate frame used by an AVL output family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// The native format names are the complete field documentation for these records.
#[allow(missing_docs)]
#[rustfmt::skip]
pub enum AvlFrame { StabilityAxes, BodyAxes, GeometryAxes, StabilityForcesBodyMoments }

/// Coefficient row emitted by an AVL derivative listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// The native format names are the complete field documentation for these records.
#[allow(missing_docs)]
#[rustfmt::skip]
pub enum AvlDerivativeCoefficient { Lift, SideForce, Drag, NormalForce, RollingMoment, PitchingMoment, YawingMoment, TrefftzDrag, SpanEfficiency }

/// Trefftz-plane result from one AVL total-force listing.
#[derive(Debug, Clone, Copy, PartialEq)]
// The native format names are the complete field documentation for these records.
#[allow(missing_docs)]
#[rustfmt::skip]
pub struct AvlTrefftzPlane { pub lift_coefficient: f64, pub induced_drag_coefficient: f64, pub side_force_coefficient: f64, pub span_efficiency: Option<f64> }

/// One row of AVL's native `CNC` span-loading table.
#[derive(Debug, Clone, Copy, PartialEq)]
// The native format names are the complete field documentation for these records.
#[allow(missing_docs)]
#[rustfmt::skip]
pub struct AvlSpanLoading { pub index: usize, pub midpoint_m: [f64; 3], pub normal_loading: f64, pub lift_coefficient: f64, pub chord_m: f64, pub width_m: f64, pub area_m2: f64 }

/// One AVL strip loading in a native `STRP` output.
#[derive(Debug, Clone, Copy, PartialEq)]
// The native format names are the complete field documentation for these records.
#[allow(missing_docs)]
#[rustfmt::skip]
pub struct AvlStripLoading { pub index: usize, pub leading_edge_m: [f64; 3], pub chord_m: f64, pub area_m2: f64, pub chord_loading: f64, pub induced_angle_rad: f64, pub perpendicular_lift_coefficient: f64, pub lift_coefficient: f64, pub drag_coefficient: f64, pub viscous_drag_coefficient: f64, pub quarter_chord_moment_coefficient: f64, pub leading_edge_moment_coefficient: f64, pub center_of_pressure_x_over_c: f64 }

/// Surface-integrated and strip-resolved AVL loading.
#[derive(Debug, Clone, PartialEq)]
// The native format names are the complete field documentation for these records.
#[allow(missing_docs)]
#[rustfmt::skip]
pub struct AvlSurfaceStripForces { pub index: usize, pub name: String, pub chordwise_panels: usize, pub spanwise_strips: usize, pub first_strip: usize, pub area_m2: f64, pub average_chord_m: f64, pub reference_coefficients: [f64; 8], pub local_coefficients: [f64; 2], pub strips: Vec<AvlStripLoading> }

/// Parsed native `STRP` output.
#[derive(Debug, Clone, PartialEq)]
// The native format names are the complete field documentation for these records.
#[allow(missing_docs)]
#[rustfmt::skip]
pub struct AvlStripForces { pub reference: AvlReference, pub frame: AvlFrame, pub surfaces: Vec<AvlSurfaceStripForces> }

/// One row in a native AVL derivative matrix.
#[derive(Debug, Clone, PartialEq)]
// The native format names are the complete field documentation for these records.
#[allow(missing_docs)]
#[rustfmt::skip]
pub struct AvlDerivativeRow { pub coefficient: AvlDerivativeCoefficient, pub values: Vec<f64> }

/// Parsed `ST`, `SM`, or `SB` derivative output.
#[derive(Debug, Clone, PartialEq)]
// The native format names are the complete field documentation for these records.
#[allow(missing_docs)]
#[rustfmt::skip]
pub struct AvlDerivatives { pub reference: AvlReference, pub frame: AvlFrame, pub first_order_columns: Vec<String>, pub first_order: Vec<AvlDerivativeRow>, pub rate_columns: Vec<String>, pub rates: Vec<AvlDerivativeRow>, pub control_names: Vec<String>, pub controls: Vec<AvlDerivativeRow>, pub design_names: Vec<String>, pub design: Vec<AvlDerivativeRow>, pub neutral_point_m: Option<f64>, pub spiral_stability_parameter: Option<f64> }

/// One variable/constraint pair in an AVL trim run case.
#[derive(Debug, Clone, PartialEq)]
// The native format names are the complete field documentation for these records.
#[allow(missing_docs)]
#[rustfmt::skip]
pub struct AvlTrimConstraint { pub variable: String, pub constraint: String, pub value: f64 }

/// One scalar parameter saved in an AVL `.run` case.
#[derive(Debug, Clone, PartialEq)]
// The native format names are the complete field documentation for these records.
#[allow(missing_docs)]
#[rustfmt::skip]
pub struct AvlTrimParameter { pub name: String, pub value: f64, pub units: Option<String> }

/// A typed AVL `.run` trim case.
#[derive(Debug, Clone, PartialEq)]
// The native format names are the complete field documentation for these records.
#[allow(missing_docs)]
#[rustfmt::skip]
pub struct AvlTrimCase { pub index: usize, pub title: String, pub constraints: Vec<AvlTrimConstraint>, pub parameters: Vec<AvlTrimParameter> }

/// Output command supported by AVL's `OPER` menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// The native format names are the complete field documentation for these records.
#[allow(missing_docs)]
#[rustfmt::skip]
pub enum AvlOutputKind { TotalForces, StripForces, BodyStripForces, ShearAndBending, SpanLoading, StabilityDerivatives, StabilityForceBodyMomentDerivatives, BodyDerivatives }

impl AvlModel {
    /// ALAS's native AVL export: all lifting surfaces, no slender bodies.
    pub const ALAS_LIFTING_SURFACES: Self = Self {
        lifting_surfaces_only: true,
        lift_is_stability_axis: true,
        pitch_moment_is_standard_axis: true,
    };
}

/// Geometry and discretization requested for one native AVL deck.
#[derive(Debug, Clone, Copy)]
pub struct AvlDeckRequest<'a> {
    /// Aircraft lifting-surface geometry in meters.
    pub airplane: &'a Airplane,
    /// Prandtl-Glauert Mach number written to the header.
    pub mach: f64,
    /// Chordwise horseshoe vortices per surface.
    pub chordwise_vortices: usize,
    /// Spanwise horseshoe vortices per surface.
    pub spanwise_vortices: usize,
}

/// One converged AVL total-force point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AvlPolarPoint {
    /// Physical angle of attack, in degrees.
    pub alpha_deg: f64,
    /// Sideslip angle, in degrees.
    pub beta_deg: f64,
    /// Prandtl-Glauert Mach number.
    pub mach: f64,
    /// Stability-axis total-lift coefficient.
    pub lift_coefficient: f64,
    /// Near-field total drag coefficient; inviscid for the ALAS deck.
    pub total_drag_coefficient: f64,
    /// Trefftz-plane induced-drag coefficient.
    pub induced_drag_coefficient: f64,
    /// Standard-axis pitching-moment coefficient.
    pub pitching_moment_coefficient: f64,
    /// Trefftz-plane span efficiency, if AVL emitted a finite value.
    pub span_efficiency: Option<f64>,
}

/// Strictly parsed native AVL polar.
#[derive(Debug, Clone, PartialEq)]
pub struct AvlPolar {
    /// SI references repeated in every native total-force file.
    pub reference: AvlReference,
    /// Solver geometry and frame contract supplied by the exporter.
    pub model: AvlModel,
    /// Converged points in the requested alpha order.
    pub points: Vec<AvlPolarPoint>,
}

/// Why an AVL deck or total-force file could not be accepted.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AvlError {
    /// A required physical or discretization input was invalid.
    #[error("invalid AVL input: {0}")]
    InvalidInput(String),
    /// A total-force listing omitted a required scalar.
    #[error("AVL total-force output is missing {0}")]
    MissingValue(&'static str),
    /// A labelled scalar was not a finite Fortran-readable number.
    #[error("invalid AVL number for {label}: {token}")]
    InvalidNumber {
        /// Native output label.
        label: &'static str,
        /// Rejected token.
        token: String,
    },
    /// Multiple force files disagreed about their geometry references.
    #[error("AVL force files use inconsistent {0}")]
    InconsistentReference(&'static str),
    /// No force files were supplied.
    #[error("AVL polar contains no total-force files")]
    EmptyPolar,
    /// A native output was generated in a frame different from the requested one.
    #[error("AVL {output} uses {actual:?}; expected {expected:?}")]
    IncompatibleFrame {
        /// Output family being interpreted.
        output: &'static str,
        /// Frame found in the native header.
        actual: AvlFrame,
        /// Frame accepted by the caller.
        expected: AvlFrame,
    },
    /// A native output did not follow its documented record structure.
    #[error("invalid AVL output: {0}")]
    InvalidOutput(String),
}

/// Render a self-contained AVL geometry deck in SI units.
///
/// Airfoil coordinates are embedded after each `SECTION`; the solver extracts
/// their mean camber lines. Symmetric ALAS surfaces use `YDUPLICATE 0.0`, not
/// AVL's aerodynamic image plane, so the same deck remains valid at sideslip.
pub fn render_geometry(request: AvlDeckRequest<'_>) -> Result<String, AvlError> {
    validate_deck_request(request)?;
    let airplane = request.airplane;
    let mut deck = String::new();
    writeln!(deck, "{}", sanitized_name(&airplane.name)).ok();
    writeln!(deck, "{:.12}", request.mach).ok();
    writeln!(deck, "0 0 0.0").ok();
    writeln!(
        deck,
        "{:.12} {:.12} {:.12}",
        airplane.s_ref, airplane.c_ref, airplane.b_ref
    )
    .ok();
    writeln!(
        deck,
        "{:.12} {:.12} {:.12}",
        airplane.xyz_ref[0], airplane.xyz_ref[1], airplane.xyz_ref[2]
    )
    .ok();
    writeln!(deck, "0.0").ok();

    for wing in &airplane.wings {
        writeln!(deck, "\nSURFACE").ok();
        writeln!(deck, "{}", sanitized_name(&wing.name)).ok();
        writeln!(
            deck,
            "{} 1.0 {} -1.0",
            request.chordwise_vortices,
            request
                .spanwise_vortices
                // AVL needs more than one spanwise panel between adjacent
                // SECTION stations after its cosine redistribution is applied.
                // Four panels per station is a conservative lower bound for
                // the refined ALAS loft: it keeps the native solver from
                // rejecting dense section layouts while preserving the
                // requested resolution when it is already higher.
                .max(wing.xsecs.len().saturating_mul(4))
        )
        .ok();
        if wing.symmetric {
            writeln!(deck, "YDUPLICATE\n0.0").ok();
        }
        for section in &wing.xsecs {
            writeln!(deck, "SECTION").ok();
            writeln!(
                deck,
                "{:.12} {:.12} {:.12} {:.12} {:.12}",
                section.xyz_le[0],
                section.xyz_le[1],
                section.xyz_le[2],
                section.chord,
                section.twist
            )
            .ok();
            if !section.airfoil.coordinates.is_empty() {
                // AVL's fixed input workspace rejects a long contour with an
                // "airfoil array overflow" before it reaches OPER.  The
                // solver uses the contour to obtain the camber line, so a
                // deterministic, endpoint/leading-edge-preserving reduction
                // is preferable to an unparseable deck.
                writeln!(deck, "AIRFOIL 0.0 1.0").ok();
                write_airfoil_points(&mut deck, &section.airfoil.coordinates);
            }
        }
    }
    Ok(deck)
}

const MAX_AVL_AIRFOIL_POINTS: usize = 199;

fn write_airfoil_points(deck: &mut String, coordinates: &[(f64, f64)]) {
    if coordinates.len() <= MAX_AVL_AIRFOIL_POINTS {
        for &(x, y) in coordinates {
            writeln!(deck, "{x:.12} {y:.12}").ok();
        }
        return;
    }

    let last = coordinates.len() - 1;
    let leading_edge = coordinates
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| left.0.total_cmp(&right.0))
        .map_or(0, |(index, _)| index);
    let mut indices = (0..MAX_AVL_AIRFOIL_POINTS)
        .map(|slot| slot * last / (MAX_AVL_AIRFOIL_POINTS - 1))
        .collect::<Vec<_>>();
    if !indices.contains(&leading_edge) {
        // Replace the closest interior sample so the minimum-x point remains
        // represented without exceeding AVL's input-array bound.
        let replacement = indices
            .iter()
            .enumerate()
            .filter(|(_, index)| **index != 0 && **index != last)
            .min_by_key(|(_, index)| index.abs_diff(leading_edge))
            .map_or(1, |(position, _)| position);
        indices[replacement] = leading_edge;
        indices.sort_unstable();
        indices.dedup();
    }
    for index in indices {
        let (x, y) = coordinates[index];
        writeln!(deck, "{x:.12} {y:.12}").ok();
    }
}

/// Parse one or more native `FT` total-force listings into a polar.
pub fn parse_total_forces(files: &[&str], model: AvlModel) -> Result<AvlPolar, AvlError> {
    let Some(first) = files.first() else {
        return Err(AvlError::EmptyPolar);
    };
    let reference = parse_reference(first)?;
    let mut points = Vec::with_capacity(files.len());
    for text in files {
        let candidate = parse_reference(text)?;
        ensure_same_reference(reference, candidate)?;
        points.push(AvlPolarPoint {
            alpha_deg: required_value(text, "Alpha")?,
            beta_deg: required_value(text, "Beta")?,
            mach: required_value(text, "Mach")?,
            lift_coefficient: required_value(text, "CLtot")?,
            total_drag_coefficient: required_value(text, "CDtot")?,
            induced_drag_coefficient: required_value(text, "CDind")?,
            pitching_moment_coefficient: required_value(text, "Cmtot")?,
            span_efficiency: optional_value(text, "e")?.filter(|value| value.is_finite()),
        });
    }
    Ok(AvlPolar {
        reference,
        model,
        points,
    })
}

#[path = "../avl/formats.rs"]
mod formats;
pub use formats::*;

fn validate_deck_request(request: AvlDeckRequest<'_>) -> Result<(), AvlError> {
    let airplane = request.airplane;
    if !request.mach.is_finite() || !(0.0..1.0).contains(&request.mach) {
        return Err(AvlError::InvalidInput(format!(
            "Mach must be finite and subsonic, got {}",
            request.mach
        )));
    }
    for (name, value) in [
        ("Sref", airplane.s_ref),
        ("Cref", airplane.c_ref),
        ("Bref", airplane.b_ref),
    ] {
        if !value.is_finite() || value <= 0.0 {
            return Err(AvlError::InvalidInput(format!(
                "{name} must be finite and positive, got {value}"
            )));
        }
    }
    if request.chordwise_vortices == 0 || request.spanwise_vortices == 0 {
        return Err(AvlError::InvalidInput(
            "both panel counts must be positive".to_owned(),
        ));
    }
    if airplane.wings.is_empty() {
        return Err(AvlError::InvalidInput(
            "at least one lifting surface is required".to_owned(),
        ));
    }
    for wing in &airplane.wings {
        if wing.xsecs.len() < 2 {
            return Err(AvlError::InvalidInput(format!(
                "surface {} has fewer than two sections",
                wing.name
            )));
        }
        for section in &wing.xsecs {
            if !section.chord.is_finite() || section.chord <= 0.0 {
                return Err(AvlError::InvalidInput(format!(
                    "surface {} has a non-positive chord",
                    wing.name
                )));
            }
            if section
                .xyz_le
                .iter()
                .chain(std::iter::once(&section.twist))
                .any(|value| !value.is_finite())
            {
                return Err(AvlError::InvalidInput(format!(
                    "surface {} has non-finite geometry",
                    wing.name
                )));
            }
        }
    }
    Ok(())
}

fn parse_reference(text: &str) -> Result<AvlReference, AvlError> {
    Ok(AvlReference {
        area_m2: required_value(text, "Sref")?,
        chord_m: required_value(text, "Cref")?,
        span_m: required_value(text, "Bref")?,
        moment_reference_m: [
            required_value(text, "Xref")?,
            required_value(text, "Yref")?,
            required_value(text, "Zref")?,
        ],
    })
}

fn ensure_same_reference(left: AvlReference, right: AvlReference) -> Result<(), AvlError> {
    for (name, left, right) in [
        ("Sref", left.area_m2, right.area_m2),
        ("Cref", left.chord_m, right.chord_m),
        ("Bref", left.span_m, right.span_m),
        (
            "Xref",
            left.moment_reference_m[0],
            right.moment_reference_m[0],
        ),
        (
            "Yref",
            left.moment_reference_m[1],
            right.moment_reference_m[1],
        ),
        (
            "Zref",
            left.moment_reference_m[2],
            right.moment_reference_m[2],
        ),
    ] {
        if (left - right).abs() > 1.0e-10 * left.abs().max(right.abs()).max(1.0) {
            return Err(AvlError::InconsistentReference(name));
        }
    }
    Ok(())
}
