// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Safe import, export, and registration of user-supplied Selig airfoils.
//!
//! Imported outlines are intentionally validated rather than repaired.  A
//! repair (sorting points, closing a trailing edge, or removing a crossing)
//! would change the section the user supplied and could make a downstream
//! aerodynamic result impossible to reproduce.  The accepted convention is a
//! normalized Selig loop: upper trailing edge to leading edge, then lower
//! surface back to the same trailing-edge point.

use serde::{Deserialize, Serialize};

use crate::aircraft::airfoil::Airfoil;

/// An actionable failure while reading or validating a custom airfoil.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AirfoilImportError {
    /// The input contained no usable name or coordinate rows.
    #[error("airfoil data is empty")]
    Empty,
    /// The airfoil name is missing or contains a control character.
    #[error("airfoil name is empty or contains a control character")]
    InvalidName,
    /// A coordinate row did not contain exactly two numbers.
    #[error("invalid coordinate at line {line}: expected exactly two numbers")]
    InvalidCoordinate {
        /// One-based input line containing the malformed row.
        line: usize,
    },
    /// A coordinate row contained NaN or infinity.
    #[error("non-finite coordinate at line {line}")]
    NonFinite {
        /// One-based input line containing NaN or infinity.
        line: usize,
    },
    /// A valid outline needs enough points to contain two surfaces.
    #[error("airfoil has too few coordinates: expected at least 5, got {0}")]
    TooFewCoordinates(usize),
    /// The first and last points do not close the outline.
    #[error("airfoil trailing edge is open: first and last points must coincide")]
    OpenTrailingEdge,
    /// The loop does not start and finish at the normalized trailing edge.
    #[error("airfoil trailing edge must have x/c near 1.0")]
    InvalidTrailingEdge,
    /// The outline has no unambiguous normalized leading edge.
    #[error("airfoil has no leading-edge point with x/c near 0.0")]
    MissingLeadingEdge,
    /// The points do not travel monotonically from each trailing edge to the
    /// leading edge.
    #[error("invalid Selig point order near line {line}: x/c must decrease to the leading edge and increase afterward")]
    InvalidOrder {
        /// One-based input line where monotonic order failed.
        line: usize,
    },
    /// Two non-closing points are coincident or nearly coincident.
    #[error("duplicate airfoil point at indices {first} and {second}")]
    DuplicatePoint {
        /// Index of the first coincident point.
        first: usize,
        /// Index of the second coincident point.
        second: usize,
    },
    /// Two non-adjacent outline segments cross or touch.
    #[error("self-intersection between airfoil segments {first} and {second}")]
    SelfIntersection {
        /// Index of the first intersecting segment.
        first: usize,
        /// Index of the second intersecting segment.
        second: usize,
    },
    /// The outline encloses no measurable area.
    #[error("airfoil outline is degenerate and encloses no area")]
    DegenerateOutline,
    /// A custom name would shadow a built-in airfoil.
    #[error("custom airfoil name '{0}' conflicts with a built-in airfoil")]
    BuiltInName(String),
    /// A custom name is already registered.
    #[error("custom airfoil name '{0}' is already registered")]
    DuplicateName(String),
    /// A JSON workspace record was not valid.
    #[error("invalid serialized airfoil record: {0}")]
    InvalidRecord(String),
    /// A file could not be read.
    #[error("could not read airfoil file '{path}': {detail}")]
    Read {
        /// The path displayed to the user.
        path: String,
        /// The underlying I/O failure.
        detail: String,
    },
    /// A file could not be written.
    #[error("could not write airfoil file '{path}': {detail}")]
    Write {
        /// The path displayed to the user.
        path: String,
        /// The underlying I/O failure.
        detail: String,
    },
    /// A path did not use the supported `.dat` extension.
    #[error("unsupported airfoil file extension for '{0}'; expected .dat")]
    UnsupportedExtension(String),
}

/// Provenance attached to an imported airfoil.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AirfoilProvenance {
    /// User-visible source label, normally the imported path.
    pub source: String,
    /// Input format, currently `dat`.
    pub format: String,
}

impl Default for AirfoilProvenance {
    fn default() -> Self {
        Self {
            source: "user".to_owned(),
            format: "dat".to_owned(),
        }
    }
}

/// A validated airfoil and the source from which it was imported.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedAirfoil {
    /// Validated airfoil geometry.
    pub airfoil: Airfoil,
    /// Source metadata retained for workspace persistence and audit output.
    pub provenance: AirfoilProvenance,
}

/// Serializable form of a custom airfoil used in a workspace envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirfoilRecord {
    /// Display and lookup name.
    pub name: String,
    /// Selig coordinates in the validated order.
    pub coordinates: Vec<(f64, f64)>,
    /// Source metadata retained with the workspace.
    #[serde(default)]
    pub provenance: AirfoilProvenance,
}

impl From<&ImportedAirfoil> for AirfoilRecord {
    fn from(imported: &ImportedAirfoil) -> Self {
        Self {
            name: imported.airfoil.name.clone(),
            coordinates: imported.airfoil.coordinates.clone(),
            provenance: imported.provenance.clone(),
        }
    }
}

mod parser;
mod registry;
mod validation;

pub use parser::{import_dat, parse_dat, parse_dat_with_provenance, read_dat, write_dat};
pub use registry::{get, names, records, register, registered, replace_records};
pub use validation::validate_coordinates;

#[cfg(test)]
mod tests {
    use super::*;

    fn closed_outline() -> Vec<(f64, f64)> {
        vec![
            (1.0, 0.0),
            (0.5, 0.08),
            (0.0, 0.0),
            (0.5, -0.08),
            (1.0, 0.0),
        ]
    }

    #[test]
    fn parses_closed_selig_data_and_preserves_name() {
        let text = "demo\n1 0\n0.5 0.08\n0 0\n0.5 -0.08\n1 0\n";
        let imported = parse_dat_with_provenance(text, "demo.dat").expect("valid outline");
        assert_eq!(imported.airfoil.name, "demo");
        assert_eq!(imported.airfoil.coordinates, closed_outline());
        assert_eq!(imported.provenance.source, "demo.dat");
    }

    #[test]
    fn rejects_open_trailing_edge() {
        let mut coordinates = closed_outline();
        coordinates[4].1 = -0.01;
        let error = validate_coordinates("open", &coordinates).unwrap_err();
        assert_eq!(error, AirfoilImportError::OpenTrailingEdge);
    }

    #[test]
    fn rejects_wrong_order() {
        let coordinates = vec![(1.0, 0.0), (0.4, 0.05), (0.6, 0.02), (0.0, 0.0), (1.0, 0.0)];
        assert!(matches!(
            validate_coordinates("wrong-order", &coordinates),
            Err(AirfoilImportError::InvalidOrder { .. })
        ));
    }

    #[test]
    fn rejects_nonfinite_rows_with_line_number() {
        let error = parse_dat("demo\n1 0\n0.5 NaN\n0 0\n0.5 -0.08\n1 0", "demo.dat").unwrap_err();
        assert_eq!(error, AirfoilImportError::NonFinite { line: 3 });
    }

    #[test]
    fn rejects_a_self_intersecting_outline() {
        let coordinates = vec![(1.0, 0.0), (0.2, 0.2), (0.0, 0.0), (0.8, 0.2), (1.0, 0.0)];
        assert!(matches!(
            validate_coordinates("crossing", &coordinates),
            Err(AirfoilImportError::SelfIntersection { .. })
        ));
    }

    #[test]
    fn headerless_data_uses_the_source_stem() {
        let text = "1 0\n0.5 0.08\n0 0\n0.5 -0.08\n1 0";
        let airfoil = parse_dat(text, "folder/demo.dat").expect("valid headerless outline");
        assert_eq!(airfoil.name, "demo");
    }
}
