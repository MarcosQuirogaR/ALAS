// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native Athena Vortex Lattice geometry decks and total-force parsing.
//!
//! AVL's coefficients are defined by the references in the geometry header
//! and by the axes selected by the solver. Keeping those values beside every
//! parsed polar prevents an unlabelled coefficient vector from being compared
//! with a differently normalized aircraft model.
//!
//! Reference: M. Drela and H. Youngren, *AVL 3.40 User Primer*, geometry
//! input and OPER total-forces sections, 22 February 2022.

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
                // AVL needs at least two spanwise panels between adjacent
                // SECTION stations.  Keeping the minimum here makes a deck
                // valid for a refined ALAS loft without silently dropping
                // stations or claiming a completed run that exported none.
                .max(wing.xsecs.len().saturating_mul(2))
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

fn required_value(text: &str, label: &'static str) -> Result<f64, AvlError> {
    optional_value(text, label)?.ok_or(AvlError::MissingValue(label))
}

fn optional_value(text: &str, label: &'static str) -> Result<Option<f64>, AvlError> {
    for line in text.lines() {
        let tokens = line.split_whitespace().collect::<Vec<_>>();
        for window in tokens.windows(3) {
            if window[0] == label && window[1] == "=" {
                let normalized = window[2].replace(['D', 'd'], "E");
                let value = normalized
                    .parse::<f64>()
                    .map_err(|_| AvlError::InvalidNumber {
                        label,
                        token: window[2].to_owned(),
                    })?;
                if !value.is_finite() && label != "e" {
                    return Err(AvlError::InvalidNumber {
                        label,
                        token: window[2].to_owned(),
                    });
                }
                return Ok(Some(value));
            }
        }
    }

    // AVL's `MRF` mode retains the same labels in a full-precision,
    // machine-readable layout: values precede a `|` and the corresponding
    // comma-separated labels follow it.  Accepting both forms keeps the
    // parser useful for retained legacy FT files while allowing the product
    // runner to compare SI references without precision loss.
    for line in text.lines() {
        let Some((value_text, label_text)) = line.split_once('|') else {
            continue;
        };
        let labels = label_text.split(',').map(str::trim).collect::<Vec<_>>();
        let Some(index) = labels.iter().position(|candidate| {
            candidate
                .rsplit_once(':')
                .map_or(*candidate, |(_, suffix)| suffix.trim())
                == label
        }) else {
            continue;
        };
        let values = value_text.split_whitespace().collect::<Vec<_>>();
        let Some(token) = values.get(index) else {
            return Err(AvlError::MissingValue(label));
        };
        let normalized = token.replace(['D', 'd'], "E");
        let value = normalized
            .parse::<f64>()
            .map_err(|_| AvlError::InvalidNumber {
                label,
                token: (*token).to_owned(),
            })?;
        if !value.is_finite() && label != "e" {
            return Err(AvlError::InvalidNumber {
                label,
                token: (*token).to_owned(),
            });
        }
        return Ok(Some(value));
    }
    Ok(None)
}

fn sanitized_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|character| {
            if character.is_ascii_graphic() || character == ' ' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.trim().is_empty() {
        "ALAS aircraft".to_owned()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::wing::{Wing, WingXSec};

    fn sample_airplane() -> Airplane {
        let foil = Airfoil::from_coordinates("triangle", vec![(1.0, 0.0), (0.0, 0.05), (1.0, 0.0)]);
        Airplane {
            name: "Test".to_owned(),
            xyz_ref: [0.4, 0.0, 0.0],
            wings: vec![Wing::new(
                "Main Wing",
                vec![
                    WingXSec::new([0.0, 0.0, 0.0], 1.0, 2.0, foil.clone()),
                    WingXSec::new([0.2, 2.0, 0.1], 0.5, -1.0, foil),
                ],
                true,
            )],
            fuselages: Vec::new(),
            s_ref: 3.0,
            c_ref: 0.8,
            b_ref: 4.0,
        }
    }

    #[test]
    fn deck_is_self_contained_and_duplicates_symmetric_surfaces() {
        let airplane = sample_airplane();
        let deck = render_geometry(AvlDeckRequest {
            airplane: &airplane,
            mach: 0.2,
            chordwise_vortices: 8,
            spanwise_vortices: 20,
        })
        .unwrap_or_else(|error| panic!("sample geometry is invalid: {error}"));
        assert!(deck.contains("3.000000000000 0.800000000000 4.000000000000"));
        assert!(deck.contains("SURFACE\nMain Wing\n8 1.0 20 -1.0"));
        assert!(deck.contains("YDUPLICATE\n0.0"));
        assert_eq!(deck.matches("SECTION").count(), 2);
        assert_eq!(deck.matches("AIRFOIL").count(), 2);
    }

    #[test]
    fn official_total_force_layout_parses_labels_independent_of_columns() {
        let force = r#"
 Vortex Lattice Output -- Total Forces
 Sref = 0.60508 Cref = 0.20200 Bref = 3.1500
 Xref = 0.090048 Yref = 0.0 Zref = 0.0087358
 Alpha = -1.00454 pb/2V = 0.0 p'b/2V = 0.0
 Beta = 0.00000 qc/2V = 0.0
 Mach = 0.000 rb/2V = 0.0
 CYtot = 0.0 Cmtot = 0.01965
 CLtot = 0.32421
 CDtot = 0.00202
 CDvis = 0.0 CDind = 0.00202
 CYff = 0.0 e = 1.3893
"#;
        let polar = parse_total_forces(&[force], AvlModel::ALAS_LIFTING_SURFACES)
            .unwrap_or_else(|error| panic!("official FT layout is invalid: {error}"));
        assert_eq!(polar.reference.area_m2, 0.60508);
        assert_eq!(polar.points[0].alpha_deg, -1.00454);
        assert_eq!(polar.points[0].lift_coefficient, 0.32421);
        assert_eq!(polar.points[0].pitching_moment_coefficient, 0.01965);
        assert_eq!(polar.points[0].span_efficiency, Some(1.3893));
    }

    #[test]
    fn full_precision_machine_readable_total_force_layout_parses() {
        let force = r#"TOT
VERSION 1.0
Vortex Lattice Output -- Total Forces
  5.290355596969410E+02  9.628258954923000E+00  7.175000000000000E+01      | Sref, Cref, Bref
  3.605872687902900E+01  0.000000000000000E+00  0.000000000000000E+00      | Xref, Yref, Zref
  0.000000000000000E+00 -0.000000000000000E+00 -0.000000000000000E+00      | Alpha, pb/2V, p'b/2V
  0.000000000000000E+00  0.000000000000000E+00      | Beta, qc/2V
  8.400000000000000E-01 -0.000000000000000E+00 -0.000000000000000E+00      | Mach, rb/2V, r'b/2V
 -2.826047969867452E-02 -3.185575063856128E-11 -3.185575063856128E-11      | CXtot, Cltot, Cl'tot
 -8.032832424236166E-10  1.979499918925366E-01      | CYtot, Cmtot
 -7.432419759392327E-01  1.203626666896630E-10  1.203626666896630E-10      | CZtot, Cntot, Cn'tot
  7.432419759392327E-01      | CLtot
  2.826047969867452E-02      | CDtot
  0.000000000000000E+00  2.826047969867452E-02      | CDvis, CDind
  7.346096152745775E-01  2.338343228809003E-02 -4.507555750335080E-10  7.549116708905861E-01      | Trefftz Plane: CLff, CDff, CYff, e
"#;
        let polar = parse_total_forces(&[force], AvlModel::ALAS_LIFTING_SURFACES)
            .unwrap_or_else(|error| panic!("machine-readable FT layout is invalid: {error}"));
        assert_eq!(polar.reference.area_m2, 529.035559696941);
        assert_eq!(polar.reference.chord_m, 9.628258954923);
        assert_eq!(polar.points[0].lift_coefficient, 0.7432419759392327);
        assert_eq!(
            polar.points[0].pitching_moment_coefficient,
            0.1979499918925366
        );
        assert_eq!(polar.points[0].span_efficiency, Some(0.7549116708905861));
    }

    #[test]
    fn polar_rejects_a_reference_change_between_force_files() {
        let common = |area: f64| {
            format!(
                "Sref = {area} Cref = 1 Bref = 4\nXref = 0 Yref = 0 Zref = 0\nAlpha = 0 Beta = 0 Mach = 0\nCLtot = 0 CDtot = 0 CDind = 0 Cmtot = 0 e = 1"
            )
        };
        let first = common(3.0);
        let second = common(4.0);
        assert_eq!(
            parse_total_forces(
                &[first.as_str(), second.as_str()],
                AvlModel::ALAS_LIFTING_SURFACES
            ),
            Err(AvlError::InconsistentReference("Sref"))
        );
    }

    #[test]
    fn trefftz_and_span_loading_records_keep_native_columns_typed() {
        let trefftz =
            parse_trefftz_plane(" 0.72 0.03 -0.01 0.91 | Trefftz Plane: CLff, CDff, CYff, e")
                .unwrap_or_else(|error| panic!("Trefftz record is invalid: {error}"));
        assert_eq!(trefftz.induced_drag_coefficient, 0.03);
        assert_eq!(trefftz.span_efficiency, Some(0.91));

        let cnc = "CNC\nVERSION 1.0\nStrip Loadings: XM, YM, ZM, CNCM, CLM, CHM, DYM, ASM\n 2 | # strips\n1D+0 2 3 4 5 6 7 8\n9 10 11 12 13 14 15 16\n";
        let rows = parse_span_loading(
            cnc,
            AvlReference {
                area_m2: 3.0,
                chord_m: 1.0,
                span_m: 4.0,
                moment_reference_m: [0.0; 3],
            },
        )
        .unwrap_or_else(|error| panic!("CNC record is invalid: {error}"));
        assert_eq!(rows[0].index, 1);
        assert_eq!(rows[1].area_m2, 16.0);
    }

    #[test]
    fn strip_forces_parse_official_surface_and_strip_layout_and_reject_frame() {
        let text = "STRP\nVERSION 1.0\nStandard axis orientation,  X fwd, Z down\n3 1 4 | Sref, Cref, Bref\n0 0 0 | Xref, Yref, Zref\nSurface and Strip Forces by surface\n1 | surfaces\nSURFACE\nWing\n1 1 1 1 | Surface #, # Chordwise, # Spanwise, First strip\n2 1 | Surface area, Ave. chord\n1 2 3 4 5 6 7 8 | CLsurf, Clsurf, CYsurf, Cmsurf, CDsurf, Cnsurf, CDisurf, CDvsurf\n9 10 | CL_srf CD_srf\nStrip Forces referred to Strip Area, Chord\nj, Xle, Yle, Zle, Chord, Area, c_cl, ai, cl_norm, cl, cd, cdv, cm_c/4, cm_LE, C.P.x/c\n1 0 1 2 3 4 5 6 7 8 9 10 11 12 13\n";
        let parsed = parse_strip_forces(text, AvlFrame::StabilityAxes)
            .unwrap_or_else(|error| panic!("STRP record is invalid: {error}"));
        assert_eq!(parsed.surfaces[0].strips[0].index, 1);
        assert_eq!(parsed.surfaces[0].reference_coefficients[6], 7.0);
        assert_eq!(
            parse_strip_forces(text, AvlFrame::BodyAxes),
            Err(AvlError::IncompatibleFrame {
                output: "STRP",
                actual: AvlFrame::StabilityAxes,
                expected: AvlFrame::BodyAxes,
            })
        );
    }

    #[test]
    fn derivatives_parse_native_stability_matrix_and_frame_is_explicit() {
        let text = "DERMATS\nVERSION 1.0\n 3 1 4 | Sref, Cref, Bref\n0 0 0 | Xref, Yref, Zref\nStability-axis derivatives...\nalpha, beta\n1 2 | z' force CL : CLa, CLb\n3 4 | y force CY : CYa, CYb\n5 6 | x' force CD : CDa, CDb\n7 8 | x' mom. Cl' : Cla, Clb\n9 10 | y mom. Cm : Cma, Cmb\n11 12 | z' mom. Cn' : Cna, Cnb\nroll rate p', pitch rate q', yaw rate r'\n1 2 3 | z' force CL : CLp, CLq, CLr\n4 5 6 | y force CY : CYp, CYq, CYr\n7 8 9 | x' force CD : CDp, CDq, CDr\n10 11 12 | x' mom. Cl' : Clp, Clq, Clr\n13 14 15 | y mom. Cm : Cmp, Cmq, Cmr\n16 17 18 | z' mom. Cn' : Cnp, Cnq, Cnr\n0 | # control vars\n0 | # design vars\n-1D30 | Neutral point Xnp\n-1D30 | Clb Cnr / Clr Cnb\n";
        let parsed = parse_derivatives(text, AvlFrame::StabilityAxes)
            .unwrap_or_else(|error| panic!("DERMATS record is invalid: {error}"));
        assert_eq!(parsed.first_order.len(), 6);
        assert_eq!(
            parsed.first_order[2].coefficient,
            AvlDerivativeCoefficient::Drag
        );
        assert_eq!(parsed.rate_columns, ["p'", "q'", "r'"]);
        assert_eq!(parsed.neutral_point_m, None);
        assert_eq!(
            parse_derivatives(
                "DERMATB\naxial   vel. u, sideslip vel. v, normal  vel. w\n",
                AvlFrame::StabilityAxes,
            ),
            Err(AvlError::IncompatibleFrame {
                output: "DERMAT*",
                actual: AvlFrame::GeometryAxes,
                expected: AvlFrame::StabilityAxes,
            })
        );
    }

    #[test]
    fn trim_cases_round_trip_the_native_run_layout_and_commands_are_safe() {
        let text = "Run case  1:  Cruise\n\n alpha -> CL = 0.54\n elevator -> Cm pitchmom = 0\n\n alpha = 1.9 deg\n Mach = 0.7\n";
        let cases = parse_trim_cases(text)
            .unwrap_or_else(|error| panic!("trim record is invalid: {error}"));
        assert_eq!(cases[0].constraints[1].constraint, "Cm pitchmom");
        let rendered = render_trim_case(&cases[0])
            .unwrap_or_else(|error| panic!("trim rendering failed: {error}"));
        assert!(rendered.contains("Run case  1:  Cruise"));
        assert!(rendered.contains("alpha"));
        assert_eq!(
            render_output_command(AvlOutputKind::StabilityDerivatives, "st-mrf.dat")
                .unwrap_or_else(|error| panic!("command rendering failed: {error}")),
            "MRF\nST\nst-mrf.dat\n"
        );
        assert!(render_output_command(AvlOutputKind::TotalForces, "bad\nname").is_err());
    }
}
