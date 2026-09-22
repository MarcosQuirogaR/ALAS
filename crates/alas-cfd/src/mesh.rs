// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native Gmsh geometry and OpenFOAM patch contracts for a two-dimensional
//! airfoil study.
//!
//! The mesher deliberately writes the coordinate snapshot as a polygon made
//! of straight `Line` entities.  This keeps the geometry consumed by Gmsh
//! identical to the database snapshot: no spline interpolation, repaneling,
//! or silently substituted NACA section is involved.  The surrounding fluid
//! is a rectangular face with the polygon as a hole, extruded by one thin
//! cell in `+z` for OpenFOAM's two-dimensional `empty` front/back patches.
//!
//! Boundary-layer controls are expressed in SI metres.  The public sizing
//! report distinguishes a configured wall distance (the distance to a cell
//! centre used for a y+ estimate) from the first-layer thickness sent to the
//! Gmsh `BoundaryLayer` field.  With a cell-centre interpretation, the latter
//! is twice the former.  The requested layer count is honoured by setting the
//! total field thickness to the geometric sum of the requested layers.

use std::collections::BTreeMap;
use std::fmt;
#[cfg(test)]
use std::fs;

use serde::{Deserialize, Serialize};

use super::{AirfoilSnapshot, CfdStudyConfig};

/// Version of the native Gmsh geometry contract.  `v2` adds the optional
/// leading-edge refinement field; at refinement level zero the emitted source
/// is identical to `v1` apart from this header line.  `v3` sizes the
/// boundary-layer stack from the estimated turbulent boundary-layer thickness
/// instead of emitting exactly the configured layer count, so a `v2` case and
/// a `v3` case are not directly comparable.  `v4` additionally sizes the first
/// cell from the configured y+ target rather than taking `first_layer_height_m`
/// literally; the literal value remains available through
/// `MeshSettings::derive_first_layer_from_target_y_plus`.
///
/// `v6` fans the boundary-layer stack around BOTH trailing-edge corners of a
/// blunt section instead of only a sharp edge, which removes the stitched
/// region behind the trailing edge that was the worst face and the worst cell
/// of every `v5` mesh.  A `v5` mesh of the same case is geometrically the same
/// section at the same resolution; only the local topology behind the trailing
/// edge differs.
pub const GMSH_TEMPLATE_VERSION: &str = "alas-airfoil-gmsh-openfoam-v6";

/// Expansion ratio used by the generated boundary-layer field.
pub const BOUNDARY_LAYER_EXPANSION_RATIO: f64 = 1.2;

/// Multiple of the estimated turbulent boundary-layer thickness that the
/// prism stack must span.
///
/// The stack is the only part of the mesh with wall-normal grading; outside
/// it the background field is isotropic and an order of magnitude coarser.
/// A stack that stops inside the boundary layer therefore leaves the outer
/// shear layer to one or two isotropic cells, which thickens the modelled
/// layer and inflates pressure drag.  Spanning `1.5` times the flat-plate
/// estimate keeps the whole layer inside the graded region with margin for
/// the adverse-pressure-gradient thickening the flat-plate correlation does
/// not capture, at a negligible cell cost because the outer layers are the
/// largest ones.
pub const BOUNDARY_LAYER_COVERAGE_FACTOR: f64 = 1.5;

/// Upper bound on the derived layer count.
///
/// `MeshSettings::n_layers` is the user's requested minimum and stays bounded
/// by its own validation range.  The derived count may exceed it to reach
/// [`BOUNDARY_LAYER_COVERAGE_FACTOR`], but never without bound: a very small
/// first layer on a very high Reynolds number would otherwise request an
/// unusable stack instead of reporting the shortfall.
pub const MAX_DERIVED_BOUNDARY_LAYERS: u32 = 60;

/// Maximum number of distinct boundary points accepted at either chordwise
/// extreme.  One point represents a sharp edge and two points represent a
/// blunt edge.
const MAX_EDGE_POINTS: usize = 2;

/// Relative tolerance used when identifying the leading/trailing edge rows.
const EDGE_X_TOLERANCE: f64 = 1.0e-10;

/// Relative tolerance used when removing a repeated closing coordinate.
const CLOSURE_TOLERANCE: f64 = 1.0e-10;

/// A meshing input or converted-case validation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeshError {
    /// The snapshot or mesh controls contain invalid values.
    InvalidInput(String),
    /// The coordinate loop is finite but outside this module's supported
    /// single-section topology.
    UnsupportedGeometry(String),
    /// A converted boundary file could not be read or written.
    Io(String),
    /// A boundary file was readable but did not contain the required patch
    /// dictionary structure.
    Parse(String),
}

impl fmt::Display for MeshError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(formatter, "invalid meshing input: {message}"),
            Self::UnsupportedGeometry(message) => {
                write!(formatter, "unsupported airfoil geometry: {message}")
            }
            Self::Io(message) => write!(formatter, "mesh file I/O error: {message}"),
            Self::Parse(message) => write!(formatter, "mesh file parse error: {message}"),
        }
    }
}

impl std::error::Error for MeshError {}

/// Whether an edge has one sharp endpoint or a finite blunt face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// The two surfaces meet at one coordinate within the tolerance.
    Sharp,
    /// The upper and lower surfaces terminate at two coordinates joined by a
    /// finite edge segment.
    Blunt,
}

/// Classification of the exact section topology used by the generated GEO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AirfoilTopology {
    /// Leading-edge shape.
    pub leading_edge: EdgeKind,
    /// Trailing-edge shape.
    pub trailing_edge: EdgeKind,
    /// Number of distinct leading-edge boundary coordinates.
    pub leading_edge_points: usize,
    /// Number of distinct trailing-edge boundary coordinates.
    pub trailing_edge_points: usize,
}

/// Boundary-layer dimensions and the y+ estimate recorded with a GEO file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundaryLayerSizing {
    /// Whether a Gmsh `BoundaryLayer` field is emitted.
    pub enabled: bool,
    /// Number of layers actually emitted.  This is the configured count when
    /// that already spans [`Self::target_total_thickness_m`], and otherwise
    /// the smallest count that does, bounded by
    /// [`MAX_DERIVED_BOUNDARY_LAYERS`].
    pub n_layers: u32,
    /// Layer count requested through `MeshSettings::n_layers`, retained so a
    /// derived increase is visible rather than silent.
    pub configured_n_layers: u32,
    /// Geometric expansion ratio between successive layers.
    pub expansion_ratio: f64,
    /// Configured distance from the wall to the first cell centre in metres.
    pub requested_wall_distance_m: f64,
    /// Wall distance derived from target y+ and the estimated friction
    /// velocity, in metres.
    pub derived_wall_distance_m: f64,
    /// Distance actually used by the emitted stack: the derived value when
    /// `MeshSettings::derive_first_layer_from_target_y_plus` is set, otherwise
    /// the explicit `first_layer_height_m`.  Both alternatives are retained
    /// above so the choice is auditable from the report alone.
    pub selected_wall_distance_m: f64,
    /// First-layer thickness sent to Gmsh's `Size` property, in metres.
    /// This is twice [`Self::selected_wall_distance_m`] under the cell-centre
    /// interpretation documented by [`MeshSettings`].
    pub first_layer_thickness_m: f64,
    /// Total thickness sent to the Gmsh `BoundaryLayer` field, in metres.
    pub total_thickness_m: f64,
    /// Smooth flat-plate turbulent boundary-layer thickness at the trailing
    /// edge, `delta = 0.37 c Re_c^(-1/5)`, in metres.  This is a sizing
    /// estimate for an attached, fully turbulent layer at zero pressure
    /// gradient; it is not a solved boundary-layer thickness and is not valid
    /// for separated, transitional or shock-affected flow.
    pub estimated_boundary_layer_thickness_m: f64,
    /// Stack thickness the layer count targets, in metres:
    /// [`BOUNDARY_LAYER_COVERAGE_FACTOR`] times
    /// [`Self::estimated_boundary_layer_thickness_m`].
    pub target_total_thickness_m: f64,
    /// `total_thickness_m / estimated_boundary_layer_thickness_m`.  A value
    /// below one means the graded stack ends inside the estimated boundary
    /// layer and the outer layer is carried by isotropic background cells.
    pub boundary_layer_coverage_ratio: f64,
    /// Estimated friction velocity used for the y+ calculation, in m/s.
    pub friction_velocity_m_s: f64,
    /// Estimated y+ at the first cell centre using the selected wall distance.
    pub estimated_y_plus: f64,
    /// Target y+ copied from the study configuration.
    pub target_y_plus: f64,
    /// Kinematic viscosity used by the estimate, in m^2/s.
    pub kinematic_viscosity_m2_s: f64,
    /// Smooth turbulent flat-plate skin-friction coefficient used for the
    /// estimate.  This is a sizing estimate, not a physical validation.
    pub estimated_skin_friction_coefficient: f64,
}

/// Geometry and sizing evidence accompanying generated Gmsh source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeshGeometryReport {
    /// Native geometry template identifier.
    pub template_version: String,
    /// Exact database name from the snapshot.
    pub airfoil_name: String,
    /// Exact coordinate hash copied from the snapshot.
    pub coordinate_hash: String,
    /// Number of coordinates in the snapshot, including a repeated closing
    /// coordinate if the input carried one.
    pub input_coordinate_count: usize,
    /// Number of points emitted to Gmsh after removing only a repeated closing
    /// coordinate, if present.
    pub emitted_coordinate_count: usize,
    /// Whether the last input coordinate repeated the first and was omitted
    /// from the point table while the closing line preserved the same edge.
    pub removed_closing_duplicate: bool,
    /// Polygon area after scaling to the configured chord, in m^2.  The sign
    /// is the source winding sign; the hole loop is reversed when necessary.
    pub signed_area_m2: f64,
    /// Configured physical chord in metres.
    pub chord_m: f64,
    /// Rectangular domain minimum x in metres.
    pub domain_min_x_m: f64,
    /// Rectangular domain maximum x in metres.
    pub domain_max_x_m: f64,
    /// Rectangular domain minimum y in metres.
    pub domain_min_y_m: f64,
    /// Rectangular domain maximum y in metres.
    pub domain_max_y_m: f64,
    /// One-cell extrusion span in metres.
    pub extrusion_span_m: f64,
    /// Exact section topology classification.
    pub topology: AirfoilTopology,
    /// Whether the GEO uses one line per exact input coordinate.
    pub exact_polygon_geometry: bool,
    /// Native Gmsh source has no spline or resampling error because this is
    /// always true for a successful build.
    pub geometry_resampling_error_m: f64,
    /// Boundary-layer sizing evidence.
    pub boundary_layer: BoundaryLayerSizing,
}

/// A generated native Gmsh `.geo` artifact and its evidence report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GmshGeo {
    /// Complete source suitable for `gmsh -3 case.geo -format msh2`.
    pub source: String,
    /// Geometry, topology and boundary-layer evidence.
    pub report: MeshGeometryReport,
}

/// One patch entry parsed from an OpenFOAM `polyBoundaryMesh` file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryPatch {
    /// Patch dictionary name.
    pub name: String,
    /// OpenFOAM `type` value.
    pub patch_type: String,
    /// Optional `physicalType` value.
    pub physical_type: Option<String>,
    /// Optional face count from `nFaces`.
    pub n_faces: Option<u64>,
}

/// Evidence returned by boundary inspection or correction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryPatchReport {
    /// Parsed patches, in the contract order.
    pub patches: Vec<BoundaryPatch>,
    /// Required patch names and the type each must carry.
    pub required_types: BTreeMap<String, String>,
    /// Whether the file was rewritten by
    /// [`ensure_boundary_patch_types`].
    pub updated: bool,
    /// Non-contract patch dictionaries found in the boundary list.
    pub unknown_patches: Vec<String>,
}
#[path = "mesh_boundary.rs"]
mod boundary;
#[path = "mesh_domain.rs"]
mod domain;
#[path = "mesh_generation.rs"]
mod generation;
#[path = "mesh_geometry.rs"]
mod geometry_audit;
#[path = "mesh_preflight.rs"]
mod preflight;
#[path = "mesh_presets.rs"]
mod presets;

pub use boundary::{
    ensure_boundary_patch_types, inspect_boundary_patch_types, required_boundary_types,
};
pub use domain::{
    domain_template, patch_contract, DomainExtents, DomainTemplate, FrameStatement, PatchRole,
    PatchSpec, WakeBox, DOMAIN_TEMPLATE_VERSION, MAX_RECOMMENDED_BLOCKAGE,
    RECOMMENDED_DOWNSTREAM_CHORDS, RECOMMENDED_HALF_HEIGHT_CHORDS, RECOMMENDED_UPSTREAM_CHORDS,
};
pub use generation::{
    boundary_layer_sizing, build_gmsh_geo, derive_first_layer_wall_distance_m, generate_gmsh_geo,
};
#[cfg(test)]
use generation::{coordinate_hash, geometric_layer_sum};
pub use geometry_audit::{
    audit_airfoil_geometry, AirfoilGeometryAudit, ClosureKind, GeometryIssue, GeometryIssueCode,
    IssueSeverity, SectionMetrics, TrailingEdgeMeshing, TrailingEdgeTreatment, Winding,
    GEOMETRY_AUDIT_VERSION, MAX_THICKNESS_RATIO, MAX_TRAILING_EDGE_GAP, MIN_LOOP_POINTS,
    MIN_THICKNESS_RATIO, NEGLIGIBLE_SEGMENT,
};
pub use preflight::{
    run_mesh_preflight, write_mesh_manifest, ExpectedArtifacts, MeshPreflight, MeshPreflightBundle,
    MeshQualityThresholds, PreflightIssue, PreflightIssueSource, PreflightStatus,
    ToolchainExpectation, MESH_MANIFEST_FILE, MESH_MANIFEST_VERSION,
};
pub use presets::{
    mesh_resolution, preset_catalogue, InflationSpec, MeshResolutionSpec, PresetSummary,
    RefinementSpec, YPlusConsistency, MAX_RECOMMENDED_INFLATION_CHORDS, PRESET_CATALOGUE_VERSION,
    Y_PLUS_CONSISTENCY_BAND,
};
#[cfg(test)]
// Tests assert on values they parsed or built here, so a failed expect is
// the assertion failing rather than a library invariant breaking.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn exact_polygon_geo_contains_all_contract_groups() {
        let config = CfdStudyConfig {
            mesh: super::super::MeshSettings {
                boundary_layers: true,
                n_layers: 8,
                ..super::super::MeshSettings::default()
            },
            ..CfdStudyConfig::default()
        };
        let airfoil = match super::super::resolve_airfoil("SC2-0714") {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("failed to resolve fixture: {error}"),
        };
        let artifact = match build_gmsh_geo(&config, &airfoil) {
            Ok(artifact) => artifact,
            Err(error) => panic!("failed to build GEO: {error}"),
        };
        assert!(artifact.report.exact_polygon_geometry);
        assert_eq!(artifact.report.geometry_resampling_error_m, 0.0);
        assert!(artifact.source.contains("Mesh.Algorithm = 5;"));
        assert!(artifact
            .source
            .contains("Physical Surface(\"frontAndBack\")"));
        assert!(artifact.source.contains("Physical Surface(\"airfoil\")"));
        assert!(artifact.source.contains("Field[1] = BoundaryLayer;"));
        assert!(artifact.source.contains("Field[1].Thickness"));
        // Both edge kinds fan now.  Excluding a blunt edge was measured to be
        // the cause of the worst face and the worst cell in every generated
        // mesh; see the emitter for the before/after numbers.  A sharp edge
        // fans its single closing point, a blunt edge its two corners.
        let fan = artifact
            .source
            .lines()
            .find(|line| line.starts_with("Field[1].FanPointsList"))
            .unwrap_or_else(|| panic!("no fan points emitted"));
        let expected_corners = match artifact.report.topology.trailing_edge {
            EdgeKind::Sharp => 1,
            EdgeKind::Blunt => 2,
        };
        assert_eq!(
            fan.matches(char::is_numeric).count().min(2),
            expected_corners.min(2),
            "{fan}"
        );
        assert_eq!(fan.split(',').count(), expected_corners, "{fan}");
        assert!(artifact.source.contains("Physical Volume(\"fluid\")"));
        let layer = &artifact.report.boundary_layer;
        assert_eq!(layer.configured_n_layers, 8);
        // The emitted stack is still an exact geometric sum, but the count is
        // whatever spans the estimated boundary layer rather than the
        // configured floor.
        assert_eq!(
            layer.total_thickness_m,
            geometric_layer_sum(
                layer.first_layer_thickness_m,
                BOUNDARY_LAYER_EXPANSION_RATIO,
                layer.n_layers
            )
        );
        assert!(layer.n_layers > layer.configured_n_layers);
        assert!(layer.boundary_layer_coverage_ratio >= BOUNDARY_LAYER_COVERAGE_FACTOR);
        assert!(artifact
            .source
            .contains(&format!("Field[1].NbLayers = {};", layer.n_layers)));
    }

    #[test]
    fn a_configured_stack_shorter_than_the_boundary_layer_is_extended_to_cover_it() {
        let config = CfdStudyConfig::default();
        let sizing = match boundary_layer_sizing(&config) {
            Ok(sizing) => sizing,
            Err(error) => panic!("failed to size layers: {error}"),
        };
        let configured_thickness = geometric_layer_sum(
            sizing.first_layer_thickness_m,
            BOUNDARY_LAYER_EXPANSION_RATIO,
            sizing.configured_n_layers,
        );
        // Negative control: the previous contract emitted exactly the
        // configured count, which for the default 1 m / Re 3.45e6 case stops
        // inside the boundary layer.
        assert!(configured_thickness < sizing.estimated_boundary_layer_thickness_m);
        assert!(sizing.total_thickness_m >= sizing.target_total_thickness_m);
        assert!(sizing.n_layers <= MAX_DERIVED_BOUNDARY_LAYERS);
    }

    #[test]
    fn the_first_cell_is_sized_from_the_y_plus_target_unless_the_override_is_set() {
        let config = CfdStudyConfig::default();
        let derived = match boundary_layer_sizing(&config) {
            Ok(sizing) => sizing,
            Err(error) => panic!("failed to size layers: {error}"),
        };
        assert!(config.mesh.derive_first_layer_from_target_y_plus);
        assert_eq!(
            derived.selected_wall_distance_m,
            derived.derived_wall_distance_m
        );
        assert_eq!(
            derived.requested_wall_distance_m,
            config.mesh.first_layer_height_m
        );
        // The whole point: the estimate now lands on the configured target
        // instead of wherever the raw length happened to fall.
        assert!(
            (derived.estimated_y_plus - config.mesh.target_y_plus).abs() < 1.0e-9,
            "estimated y+ {} vs target {}",
            derived.estimated_y_plus,
            config.mesh.target_y_plus
        );

        let mut literal = CfdStudyConfig::default();
        literal.mesh.derive_first_layer_from_target_y_plus = false;
        let literal = match boundary_layer_sizing(&literal) {
            Ok(sizing) => sizing,
            Err(error) => panic!("failed to size layers: {error}"),
        };
        assert_eq!(
            literal.selected_wall_distance_m,
            literal.requested_wall_distance_m
        );
        // Negative control for the defect: taken literally, the default length
        // misses its own y+ target by a third at the default Reynolds number,
        // and by more at higher ones because the length does not move with the
        // flow state at all.
        assert!(
            literal.estimated_y_plus / literal.target_y_plus > 1.3,
            "literal estimate {} vs target {}",
            literal.estimated_y_plus,
            literal.target_y_plus
        );
        assert!(literal.selected_wall_distance_m > derived.selected_wall_distance_m);
    }

    #[test]
    fn a_stack_that_already_covers_the_boundary_layer_is_left_alone() {
        let config = CfdStudyConfig {
            mesh: super::super::MeshSettings {
                boundary_layers: true,
                n_layers: 30,
                first_layer_height_m: 5.0e-4,
                derive_first_layer_from_target_y_plus: false,
                ..super::super::MeshSettings::default()
            },
            ..CfdStudyConfig::default()
        };
        let sizing = match boundary_layer_sizing(&config) {
            Ok(sizing) => sizing,
            Err(error) => panic!("failed to size layers: {error}"),
        };
        assert_eq!(sizing.n_layers, sizing.configured_n_layers);
        assert!(sizing.boundary_layer_coverage_ratio > BOUNDARY_LAYER_COVERAGE_FACTOR);
    }

    #[test]
    fn disabled_boundary_layers_emit_no_stack_and_report_no_coverage() {
        let config = CfdStudyConfig {
            mesh: super::super::MeshSettings {
                boundary_layers: false,
                ..super::super::MeshSettings::default()
            },
            ..CfdStudyConfig::default()
        };
        let sizing = match boundary_layer_sizing(&config) {
            Ok(sizing) => sizing,
            Err(error) => panic!("failed to size layers: {error}"),
        };
        assert_eq!(sizing.n_layers, 0);
        assert_eq!(sizing.configured_n_layers, 0);
        assert_eq!(sizing.total_thickness_m, 0.0);
        assert_eq!(sizing.boundary_layer_coverage_ratio, 0.0);
        assert!(sizing.estimated_boundary_layer_thickness_m > 0.0);
    }

    #[test]
    fn invalid_snapshot_hash_is_rejected_without_substitution() {
        let airfoil = AirfoilSnapshot {
            name: "fixture".to_owned(),
            coordinates: vec![
                (1.0, 0.02),
                (0.5, 0.08),
                (0.0, 0.0),
                (0.5, -0.08),
                (1.0, -0.02),
                (0.75, -0.05),
                (0.25, -0.05),
                (0.25, 0.05),
            ],
            coordinate_hash: "wrong".to_owned(),
        };
        let result = build_gmsh_geo(&CfdStudyConfig::default(), &airfoil);
        assert!(
            matches!(result, Err(MeshError::InvalidInput(message)) if message.contains("hash"))
        );
    }

    #[test]
    fn layer_sizing_reports_centroid_factor_and_y_plus() {
        let config = CfdStudyConfig {
            mesh: super::super::MeshSettings {
                boundary_layers: true,
                n_layers: 4,
                first_layer_height_m: 2.0e-4,
                target_y_plus: 30.0,
                derive_first_layer_from_target_y_plus: false,
                ..super::super::MeshSettings::default()
            },
            ..CfdStudyConfig::default()
        };
        let sizing = match boundary_layer_sizing(&config) {
            Ok(sizing) => sizing,
            Err(error) => panic!("failed to size layers: {error}"),
        };
        assert_eq!(sizing.first_layer_thickness_m, 4.0e-4);
        assert_eq!(sizing.configured_n_layers, 4);
        assert!(sizing.derived_wall_distance_m.is_finite());
        assert!(sizing.estimated_y_plus.is_finite());
    }

    #[test]
    fn repeated_closing_coordinate_is_removed_after_hash_validation() {
        let coordinates = vec![
            (1.0, 0.02),
            (0.875, 0.04),
            (0.50, 0.08),
            (0.25, 0.05),
            (0.0, 0.0),
            (0.25, -0.05),
            (0.50, -0.08),
            (0.875, -0.04),
            (1.0, -0.02),
            (1.0, 0.02),
        ];
        let airfoil = AirfoilSnapshot {
            name: "closed-fixture".to_owned(),
            coordinate_hash: coordinate_hash("closed-fixture", &coordinates),
            coordinates,
        };
        let artifact = match build_gmsh_geo(&CfdStudyConfig::default(), &airfoil) {
            Ok(artifact) => artifact,
            Err(error) => panic!("failed to build closed fixture: {error}"),
        };
        assert!(artifact.report.removed_closing_duplicate);
        assert_eq!(artifact.report.input_coordinate_count, 10);
        assert_eq!(artifact.report.emitted_coordinate_count, 9);
        assert_eq!(artifact.source.matches("Point(").count(), 13);
    }

    #[test]
    fn preset_spacing_is_chord_based_and_domain_independent() {
        let airfoil = match super::super::resolve_airfoil("SC2-0714") {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("failed to resolve fixture: {error}"),
        };
        let config = CfdStudyConfig {
            mesh: super::super::MeshSettings {
                preset: super::super::MeshPreset::Coarse,
                boundary_layers: false,
                ..super::super::MeshSettings::default()
            },
            ..CfdStudyConfig::default()
        };
        let coarse = build_gmsh_geo(&config, &airfoil).expect("coarse GEO");
        let mut enlarged_config = config.clone();
        enlarged_config.mesh.upstream_chords = 20.0;
        enlarged_config.mesh.downstream_chords = 40.0;
        enlarged_config.mesh.half_height_chords = 20.0;
        let enlarged = build_gmsh_geo(&enlarged_config, &airfoil).expect("enlarged GEO");
        let assignment = |source: &str, prefix: &str| {
            source
                .lines()
                .find(|line| line.starts_with(prefix))
                .and_then(|line| line.split('=').nth(1))
                .and_then(|value| value.trim().trim_end_matches(';').parse::<f64>().ok())
                .expect("GEO assignment")
        };
        assert_eq!(
            assignment(&coarse.source, "Mesh.CharacteristicLengthMax"),
            assignment(&enlarged.source, "Mesh.CharacteristicLengthMax")
        );
        let point_size = |source: &str| {
            source
                .lines()
                .find(|line| line.starts_with("Point(5) ="))
                .and_then(|line| line.split('{').nth(1))
                .and_then(|value| {
                    value
                        .trim_end_matches("};")
                        .split(',')
                        .nth(3)
                        .map(str::trim)
                })
                .and_then(|value| value.parse::<f64>().ok())
                .expect("airfoil point size")
        };
        assert_eq!(point_size(&coarse.source), point_size(&enlarged.source));

        let mut medium = config.clone();
        medium.mesh.preset = super::super::MeshPreset::Medium;
        let medium = build_gmsh_geo(&medium, &airfoil).expect("medium GEO");
        let mut fine = config;
        fine.mesh.preset = super::super::MeshPreset::Fine;
        let fine = build_gmsh_geo(&fine, &airfoil).expect("fine GEO");
        assert!(
            assignment(&medium.source, "Mesh.CharacteristicLengthMax")
                < assignment(&coarse.source, "Mesh.CharacteristicLengthMax")
        );
        assert!(
            assignment(&fine.source, "Mesh.CharacteristicLengthMax")
                < assignment(&medium.source, "Mesh.CharacteristicLengthMax")
        );
        assert!(point_size(&medium.source) < point_size(&coarse.source));
        assert!(point_size(&fine.source) < point_size(&medium.source));
    }

    #[test]
    fn nonzero_default_faces_are_rejected_after_conversion() {
        let contents = r#"
            FoamFile { format ascii; class polyBoundaryMesh; }
            6
            (
                frontAndBack { type empty; nFaces 2; startFace 0; }
                farField { type patch; nFaces 2; startFace 2; }
                outlet { type patch; nFaces 2; startFace 4; }
                inlet { type patch; nFaces 2; startFace 6; }
                airfoil { type wall; nFaces 2; startFace 8; }
                defaultFaces { type patch; nFaces 1; startFace 10; }
            )
        "#;
        let path = std::env::temp_dir().join(format!(
            "alas-cfd-boundary-{}-{}.dict",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::write(&path, contents).expect("write boundary fixture");
        let result = ensure_boundary_patch_types(&path);
        let _ = fs::remove_file(&path);
        assert!(matches!(
            result,
            Err(MeshError::Parse(message)) if message.contains("defaultFaces") && message.contains("1 faces")
        ));
    }
}
