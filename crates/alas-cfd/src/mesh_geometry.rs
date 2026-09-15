// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Typed geometry audit of a database airfoil before any mesher runs.
//!
//! The audit consumes the exact coordinate snapshot resolved by
//! [`crate::resolve_airfoil`].  It never resamples, repairs or substitutes the
//! section: every finding is reported as a coded issue with a remedy, and the
//! caller decides.  Coordinates are unit-chord `(x/c, y/c)`; every length in
//! this module is a chord fraction unless the field name ends in `_m`.

use serde::{Deserialize, Serialize};

use super::{AirfoilTopology, EdgeKind};
use crate::geometry::{canonical_topology_points, coordinate_hash};
use crate::AirfoilSnapshot;

#[path = "mesh_geometry_loop.rs"]
mod loop_audit;
use loop_audit::audit_loop;

/// Version of the audit contract recorded in every manifest.
pub const GEOMETRY_AUDIT_VERSION: &str = "alas-airfoil-geometry-audit-v1";
/// Largest blunt trailing-edge gap (chord fraction) the single-section
/// template accepts; larger bases need a dedicated bluff-body template.
pub const MAX_TRAILING_EDGE_GAP: f64 = 0.10;
/// Smallest maximum thickness ratio the template accepts.
pub const MIN_THICKNESS_RATIO: f64 = 0.01;
/// Largest maximum thickness ratio the template accepts.
pub const MAX_THICKNESS_RATIO: f64 = 0.60;
/// Minimum number of distinct loop points after closure removal.
pub const MIN_LOOP_POINTS: usize = 8;
/// Consecutive points closer than this chord fraction are reported as a
/// negligible segment (the mesher may collapse them).
pub const NEGLIGIBLE_SEGMENT: f64 = 1.0e-7;
/// Number of chordwise stations sampled for thickness and camber.
const METRIC_STATIONS: usize = 200;

/// Severity of an audit or preflight finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    /// Blocks case generation.
    Error,
    /// Recorded, never blocking.
    Warning,
}

/// Stable machine-readable code of a geometry finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeometryIssueCode {
    /// The snapshot has no name.
    EmptyName,
    /// The snapshot hash does not match its coordinates.
    HashMismatch,
    /// The chord used for metre conversions is not positive and finite.
    InvalidChord,
    /// Fewer than [`MIN_LOOP_POINTS`] distinct points.
    TooFewPoints,
    /// A coordinate is NaN or infinite.
    NonFinite,
    /// Coordinates are not normalised to a unit chord.
    NotNormalized,
    /// Two loop points coincide.
    DuplicatePoint,
    /// Two non-adjacent loop segments cross.
    SelfIntersection,
    /// The loop is not a single trailing-edge to leading-edge to trailing-edge
    /// traversal (Selig order).
    NotSingleLoop,
    /// The lower surface is listed first; the mesher accepts it but the
    /// database convention is upper surface first.
    LowerSurfaceFirst,
    /// The leading edge is not at the origin of the chord frame.
    LeadingEdgeOffOrigin,
    /// More than two distinct points at the leading-edge extreme.
    LeadingEdgeMultiplePoints,
    /// More than two distinct points at the trailing-edge extreme.
    TrailingEdgeMultiplePoints,
    /// Blunt base thicker than [`MAX_TRAILING_EDGE_GAP`].
    TrailingEdgeGapTooLarge,
    /// Maximum thickness ratio outside the accepted range.
    ThicknessOutOfRange,
    /// The loop encloses no measurable area.
    DegenerateArea,
    /// Two consecutive points closer than [`NEGLIGIBLE_SEGMENT`].
    NegligibleSegment,
}

/// One coded finding with the indices it refers to and a remedy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeometryIssue {
    /// Blocking or advisory.
    pub severity: IssueSeverity,
    /// Stable code.
    pub code: GeometryIssueCode,
    /// Human-readable statement of the finding.
    pub message: String,
    /// What the user can do about it.
    pub remedy: String,
    /// Snapshot coordinate indices involved, if any.
    pub indices: Vec<usize>,
}

/// Loop orientation in the chord frame (`+x` chord, `+y` normal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Winding {
    /// Positive shoelace area: the Selig convention after normalisation.
    CounterClockwise,
    /// Negative shoelace area.
    Clockwise,
}

/// How the snapshot closes its loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClosureKind {
    /// The last point differs from the first; the closing edge is implicit.
    Open,
    /// The last point repeats the first and is dropped from the loop.
    RepeatedFirstPoint,
}

/// How the mesher treats the trailing edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrailingEdgeMeshing {
    /// One coordinate: the boundary-layer field fans around it.
    FanAtSharpEdge,
    /// Two coordinates joined by a finite base face that is meshed as wall.
    ResolvedBluntFace,
}

/// Trailing-edge classification and the meshing consequence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrailingEdgeTreatment {
    /// Sharp or blunt.
    pub kind: EdgeKind,
    /// Vertical gap between the trailing-edge points, chord fraction.
    pub gap_chord: f64,
    /// The same gap in metres for the configured chord, when the chord is valid.
    pub gap_m: Option<f64>,
    /// Number of distinct coordinates at the trailing-edge extreme.
    pub distinct_points: usize,
    /// Whether the snapshot repeated its first point.
    pub closure: ClosureKind,
    /// Meshing consequence.
    pub meshing: TrailingEdgeMeshing,
}

/// Section metrics measured on the exact loop by linear interpolation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionMetrics {
    /// Maximum thickness ratio `t/c`.
    pub max_thickness_ratio: f64,
    /// Chord station of the maximum thickness.
    pub max_thickness_x: f64,
    /// Maximum camber ratio (signed, positive toward `+y`).
    pub max_camber_ratio: f64,
    /// Chord station of the maximum camber magnitude.
    pub max_camber_x: f64,
    /// Leading-edge coordinate (minimum `x`).
    pub leading_edge: (f64, f64),
    /// Trailing-edge `x` (maximum `x`).
    pub trailing_edge_x: f64,
    /// Shortest loop segment, chord fraction.
    pub min_segment_chord: f64,
    /// Longest loop segment, chord fraction.
    pub max_segment_chord: f64,
    /// Loop perimeter, chord fraction.
    pub perimeter_chord: f64,
    /// Signed shoelace area in chord units squared.
    pub signed_area_chord2: f64,
    /// Points on the first listed surface (trailing edge to leading edge).
    pub first_surface_points: usize,
    /// Points on the second listed surface.
    pub second_surface_points: usize,
}

/// Complete audit of one snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirfoilGeometryAudit {
    /// Audit contract version.
    pub audit_version: String,
    /// Exact snapshot name.
    pub name: String,
    /// Exact snapshot hash.
    pub coordinate_hash: String,
    /// Raw coordinate count, including a repeated closing point.
    pub input_coordinate_count: usize,
    /// Distinct loop points after closure removal.
    pub loop_point_count: usize,
    /// Closure kind of the raw list.
    pub closure: ClosureKind,
    /// Loop orientation, when measurable.
    pub winding: Option<Winding>,
    /// Whether the first listed surface is the upper surface.
    pub upper_surface_first: Option<bool>,
    /// Exact edge topology, when classifiable.
    pub topology: Option<AirfoilTopology>,
    /// Trailing-edge treatment, when classifiable.
    pub trailing_edge: Option<TrailingEdgeTreatment>,
    /// Section metrics, when measurable.
    pub metrics: Option<SectionMetrics>,
    /// Findings in detection order.
    pub issues: Vec<GeometryIssue>,
}

impl AirfoilGeometryAudit {
    /// Whether no blocking issue was found.
    pub fn passed(&self) -> bool {
        !self
            .issues
            .iter()
            .any(|issue| issue.severity == IssueSeverity::Error)
    }

    /// Blocking issues only.
    pub fn errors(&self) -> impl Iterator<Item = &GeometryIssue> {
        self.issues
            .iter()
            .filter(|issue| issue.severity == IssueSeverity::Error)
    }

    /// One-line summary of the blocking issues, empty when the audit passed.
    pub fn blocking_summary(&self) -> String {
        self.errors()
            .map(|issue| format!("{}: {} ({})", self.name, issue.message, issue.remedy))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn push(
        &mut self,
        severity: IssueSeverity,
        code: GeometryIssueCode,
        message: String,
        remedy: &str,
        indices: Vec<usize>,
    ) {
        self.issues.push(GeometryIssue {
            severity,
            code,
            message,
            remedy: remedy.to_owned(),
            indices,
        });
    }
}

/// Audit a snapshot for the configured physical chord in metres.
///
/// The function never fails: unusable input produces an audit whose
/// [`AirfoilGeometryAudit::passed`] is false and whose issues explain why.
pub fn audit_airfoil_geometry(airfoil: &AirfoilSnapshot, chord_m: f64) -> AirfoilGeometryAudit {
    let mut audit = AirfoilGeometryAudit {
        audit_version: GEOMETRY_AUDIT_VERSION.to_owned(),
        name: airfoil.name.clone(),
        coordinate_hash: airfoil.coordinate_hash.clone(),
        input_coordinate_count: airfoil.coordinates.len(),
        loop_point_count: 0,
        closure: ClosureKind::Open,
        winding: None,
        upper_surface_first: None,
        topology: None,
        trailing_edge: None,
        metrics: None,
        issues: Vec::new(),
    };
    if airfoil.name.trim().is_empty() {
        audit.push(
            IssueSeverity::Error,
            GeometryIssueCode::EmptyName,
            "the airfoil snapshot has no name".to_owned(),
            "select a database section; provenance requires its exact name",
            Vec::new(),
        );
    }
    if airfoil.coordinate_hash != coordinate_hash(&airfoil.name, &airfoil.coordinates) {
        audit.push(
            IssueSeverity::Error,
            GeometryIssueCode::HashMismatch,
            "the coordinate hash does not match the snapshot coordinates".to_owned(),
            "re-resolve the section from the database instead of editing the snapshot",
            Vec::new(),
        );
    }
    let chord_ok = chord_m.is_finite() && chord_m > 0.0;
    if !chord_ok {
        audit.push(
            IssueSeverity::Error,
            GeometryIssueCode::InvalidChord,
            format!("chord {chord_m} m is not finite and positive"),
            "set a positive chord in metres",
            Vec::new(),
        );
    }
    let non_finite = airfoil
        .coordinates
        .iter()
        .enumerate()
        .filter(|(_, (x, y))| !x.is_finite() || !y.is_finite())
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if !non_finite.is_empty() {
        audit.push(
            IssueSeverity::Error,
            GeometryIssueCode::NonFinite,
            format!("{} coordinate(s) are NaN or infinite", non_finite.len()),
            "repair the coordinate file; the section cannot be meshed",
            non_finite,
        );
        return audit;
    }
    let points = canonical_topology_points(&airfoil.coordinates);
    audit.loop_point_count = points.len();
    audit.closure = if points.len() == airfoil.coordinates.len() {
        ClosureKind::Open
    } else {
        ClosureKind::RepeatedFirstPoint
    };
    if points.len() < MIN_LOOP_POINTS {
        audit.push(
            IssueSeverity::Error,
            GeometryIssueCode::TooFewPoints,
            format!(
                "{} distinct loop points; at least {MIN_LOOP_POINTS} are required",
                points.len()
            ),
            "use a section with a resolved coordinate list",
            Vec::new(),
        );
        return audit;
    }
    audit_loop(&mut audit, &points, chord_ok.then_some(chord_m));
    audit
}
