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
/// leading-edge refinement field; at refinement level zero the emitted
/// source is identical to `v1` apart from this header line.
pub const GMSH_TEMPLATE_VERSION: &str = "alas-airfoil-gmsh-openfoam-v2";

/// Expansion ratio used by the generated boundary-layer field.
pub const BOUNDARY_LAYER_EXPANSION_RATIO: f64 = 1.2;

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
    /// Number of requested layers.
    pub n_layers: u32,
    /// Geometric expansion ratio between successive layers.
    pub expansion_ratio: f64,
    /// Configured distance from the wall to the first cell centre in metres.
    pub requested_wall_distance_m: f64,
    /// Wall distance derived from target y+ and the estimated friction
    /// velocity, in metres.
    pub derived_wall_distance_m: f64,
    /// Distance selected from the explicit `first_layer_height_m` setting.
    /// The explicit setting remains authoritative; the derived value is
    /// retained so a caller can report an inconsistent y+ target.
    pub selected_wall_distance_m: f64,
    /// First-layer thickness sent to Gmsh's `Size` property, in metres.
    /// This is twice [`Self::selected_wall_distance_m`] under the cell-centre
    /// interpretation documented by [`MeshSettings`].
    pub first_layer_thickness_m: f64,
    /// Total thickness sent to the Gmsh `BoundaryLayer` field, in metres.
    pub total_thickness_m: f64,
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
        if artifact.report.topology.trailing_edge == EdgeKind::Sharp {
            assert!(artifact.source.contains("Field[1].FanPointsList"));
        } else {
            assert!(!artifact.source.contains("Field[1].FanPointsList"));
        }
        assert!(artifact.source.contains("Physical Volume(\"fluid\")"));
        assert_eq!(
            artifact.report.boundary_layer.total_thickness_m,
            geometric_layer_sum(
                artifact.report.boundary_layer.first_layer_thickness_m,
                BOUNDARY_LAYER_EXPANSION_RATIO,
                8
            )
        );
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
                ..super::super::MeshSettings::default()
            },
            ..CfdStudyConfig::default()
        };
        let sizing = match boundary_layer_sizing(&config) {
            Ok(sizing) => sizing,
            Err(error) => panic!("failed to size layers: {error}"),
        };
        assert_eq!(sizing.first_layer_thickness_m, 4.0e-4);
        assert_eq!(sizing.n_layers, 4);
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
