// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Backend-neutral meshing preflight and case manifest.
//!
//! [`run_mesh_preflight`] is a pure function of the study configuration and
//! the exact airfoil snapshot: no file, clock or host state enters it, so two
//! runs give identical manifests and the JSON can be diffed across machines.
//! The manifest is what the OpenFOAM adapter consumes when it later runs the
//! mesher: patch names, extents, sizes, the geometry hash, every template
//! version and the quality thresholds the converted mesh must meet.  Nothing
//! here executes a mesher or claims mesh quality.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{
    audit_airfoil_geometry, build_gmsh_geo, domain_template, mesh_resolution,
    required_boundary_types, AirfoilGeometryAudit, DomainTemplate, EdgeKind, GmshGeo,
    IssueSeverity, MeshError, MeshGeometryReport, MeshResolutionSpec, DOMAIN_TEMPLATE_VERSION,
    GEOMETRY_AUDIT_VERSION, GMSH_TEMPLATE_VERSION, PRESET_CATALOGUE_VERSION,
};
use crate::{AirfoilSnapshot, CfdStudyConfig, TEMPLATE_VERSION};

/// Version of the manifest contract.
pub const MESH_MANIFEST_VERSION: &str = "alas-airfoil-mesh-manifest-v1";
/// Case-relative path of the written manifest.
pub const MESH_MANIFEST_FILE: &str = "system/mesh-manifest.json";

/// Whether the case may proceed to meshing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreflightStatus {
    /// No blocking issue; the GEO source was built.
    Ready,
    /// At least one blocking issue; no GEO source was built.
    Blocked,
}

/// Which contract raised a preflight issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreflightIssueSource {
    /// Geometry audit.
    Geometry,
    /// Domain contract.
    Domain,
    /// Resolution (presets, refinement, inflation).
    Resolution,
    /// Cross-contract consistency.
    Consistency,
    /// Gmsh source generation.
    GmshSource,
}

/// One preflight finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreflightIssue {
    /// Blocking or advisory.
    pub severity: IssueSeverity,
    /// Originating contract.
    pub source: PreflightIssueSource,
    /// Stable code.
    pub code: String,
    /// Finding.
    pub message: String,
    /// Remedy.
    pub remedy: String,
}

/// Declared mesh-quality numbers, recorded as provenance.
///
/// **These are not the executable acceptance gate, and the field names below
/// read as if they were.** What actually rejects a converted mesh is
/// [`MeshQuality::passed`] — `checkMesh` reporting `Mesh OK` with no failed
/// checks — plus the boundary/patch contract. The numeric fields here are the
/// `checkMesh` defaults this template targets; no code compares a parsed value
/// against them.
///
/// That gap is pre-existing, not introduced with these fields, and it matters:
/// `max_non_orthogonality_deg` is `70.0` while the fine preset measures
/// `71.37 deg` on one face out of 436 389 cells and `checkMesh` still reports
/// `Non-orthogonality check OK` and `Mesh OK`. Under the `checkMesh` verdict
/// the mesh passes; under a literal reading of the number below it does not.
///
/// **Which of those is the ALAS contract is an integration decision and is not
/// made here**, because either choice changes behaviour a caller depends on:
/// enforcing the numbers would newly reject meshes that ship today, and
/// renaming the fields is a public API break. The honest interim state is this
/// doc comment, plus [`MeshQuality::severely_non_orthogonal_faces`] so the
/// count behind a maximum is visible in the record rather than inferred.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeshQualityThresholds {
    /// Face non-orthogonality above which `checkMesh` calls a face *severely*
    /// non-orthogonal, degrees.  Recorded, **not enforced** — see the type
    /// documentation.
    ///
    /// Observed on OpenFOAM v2606 (native Windows, this template's generated
    /// `checkMesh` invocation), verbatim from `G3-fine-p404/logs/checkMesh.log`:
    ///
    /// ```text
    /// Mesh non-orthogonality Max: 71.3691249255 average: 5.34651937631
    ///    *Number of severely non-orthogonal (> 70 degrees) faces: 1.
    ///     Non-orthogonality check OK.
    /// Mesh OK.
    /// ```
    ///
    /// So on **that** invocation `70 deg` is a single-`*` warning that does not
    /// fail the check.  No claim is made here about the angle at which this or
    /// any other OpenFOAM build *does* fail: that number is not in the log, it
    /// varies with the release and with the generation dictionary in use, and
    /// asserting one would be inventing a contract.  What the record carries is
    /// the measured maximum and, in
    /// [`MeshQuality::severely_non_orthogonal_faces`], how many faces are past
    /// this line — one, on the case above.
    pub max_non_orthogonality_deg: f64,
    /// Maximum internal face skewness.
    pub max_internal_skewness: f64,
    /// Maximum boundary face skewness.
    pub max_boundary_skewness: f64,
    /// Maximum cell aspect ratio.
    pub max_aspect_ratio: f64,
    /// Every cell volume must be strictly positive.
    pub require_positive_cell_volumes: bool,
    /// `checkMesh` must report `Mesh OK`.
    pub require_check_mesh_ok: bool,
    /// Required patch names and types.
    pub required_patches: BTreeMap<String, String>,
    /// A non-empty patch outside the contract rejects the mesh.
    pub reject_nonzero_unknown_patches: bool,
    /// Where the numbers come from and how the gate uses them.
    pub basis: String,
}

impl MeshQualityThresholds {
    /// The template's thresholds: OpenFOAM `checkMesh` defaults plus the
    /// patch contract.
    pub fn template_defaults() -> Self {
        Self {
            max_non_orthogonality_deg: 70.0,
            max_internal_skewness: 4.0,
            max_boundary_skewness: 20.0,
            max_aspect_ratio: 1000.0,
            require_positive_cell_volumes: true,
            require_check_mesh_ok: true,
            required_patches: required_boundary_types(),
            reject_nonzero_unknown_patches: true,
            basis: "OpenFOAM checkMesh default thresholds, RECORDED AS PROVENANCE AND NOT ENFORCED: the runner gate is the checkMesh verdict (Mesh OK, no failed checks) plus the boundary contract, and no code compares a parsed value against the numbers in this record. See the MeshQualityThresholds documentation for why the difference is live on the fine preset.".to_owned(),
        }
    }
}

/// Case-relative files the adapter reads or writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedArtifacts {
    /// Native Gmsh source.
    pub gmsh_source: String,
    /// Exact coordinate CSV.
    pub coordinates_csv: String,
    /// Geometry and sizing report.
    pub mesh_report: String,
    /// This manifest.
    pub manifest: String,
    /// Converted mesh directory.
    pub poly_mesh_dir: String,
    /// Converted boundary file whose patch types the contract enforces.
    pub boundary_file: String,
}

/// Tools the adapter is expected to run later; none runs here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolchainExpectation {
    /// Mesher.
    pub mesher: String,
    /// Mesh file format.
    pub mesh_format: String,
    /// Converter.
    pub converter: String,
    /// Quality checker.
    pub checker: String,
    /// Always false for a preflight.
    pub executed: bool,
}

/// Exact section identity carried by the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AirfoilIdentity {
    /// Database name.
    pub name: String,
    /// FNV-1a hash of name and coordinate bytes.
    pub coordinate_hash: String,
    /// Raw coordinate count.
    pub coordinate_count: usize,
    /// Provenance statement.
    pub source: String,
}

/// Units used by every numeric field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitsStatement {
    /// Lengths.
    pub lengths: String,
    /// Coordinates.
    pub coordinates: String,
    /// Velocity.
    pub velocity: String,
    /// Density.
    pub density: String,
    /// Dynamic viscosity.
    pub dynamic_viscosity: String,
    /// Angles.
    pub angles: String,
}

/// The serialisable manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeshPreflight {
    /// Manifest contract version.
    pub manifest_version: String,
    /// Case dictionary template version.
    pub case_template_version: String,
    /// Gmsh source template version.
    pub gmsh_template_version: String,
    /// Domain contract version.
    pub domain_template_version: String,
    /// Geometry audit version.
    pub geometry_audit_version: String,
    /// Preset catalogue version.
    pub preset_catalogue_version: String,
    /// Units.
    pub units: UnitsStatement,
    /// Exact section identity.
    pub airfoil: AirfoilIdentity,
    /// Chord in metres.
    pub chord_m: f64,
    /// Geometry audit.
    pub geometry: AirfoilGeometryAudit,
    /// Domain contract, when the controls were valid.
    pub domain: Option<DomainTemplate>,
    /// Resolution contract, when the controls were valid.
    pub resolution: Option<MeshResolutionSpec>,
    /// Gmsh geometry report, when the source was built.
    pub gmsh: Option<MeshGeometryReport>,
    /// FNV-1a hash of the Gmsh source text, when built.
    pub gmsh_source_hash: Option<String>,
    /// Quality thresholds.
    pub quality_thresholds: MeshQualityThresholds,
    /// Expected files.
    pub artifacts: ExpectedArtifacts,
    /// Expected tools.
    pub toolchain: ToolchainExpectation,
    /// Ready or blocked.
    pub status: PreflightStatus,
    /// All findings in detection order.
    pub issues: Vec<PreflightIssue>,
    /// What this manifest does and does not establish.
    pub validity: String,
}

impl MeshPreflight {
    /// Whether the case may proceed to meshing.
    pub fn is_ready(&self) -> bool {
        self.status == PreflightStatus::Ready
    }

    /// Blocking issues only.
    pub fn errors(&self) -> impl Iterator<Item = &PreflightIssue> {
        self.issues
            .iter()
            .filter(|issue| issue.severity == IssueSeverity::Error)
    }

    /// One-line statement of the blocking issues; empty when ready.
    pub fn blocking_summary(&self) -> String {
        self.errors()
            .map(|issue| {
                format!(
                    "{} [{}]: {} ({})",
                    self.airfoil.name, issue.code, issue.message, issue.remedy
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Pretty JSON with a trailing newline.
    pub fn to_json_pretty(&self) -> Result<String, MeshError> {
        serde_json::to_string_pretty(self)
            .map(|json| json + "\n")
            .map_err(|error| MeshError::Io(format!("cannot encode mesh manifest: {error}")))
    }
}

/// Manifest plus the Gmsh source it validated, when ready.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshPreflightBundle {
    /// The manifest.
    pub manifest: MeshPreflight,
    /// The Gmsh source and report; `None` when blocked.
    pub geo: Option<GmshGeo>,
}

fn issue(
    severity: IssueSeverity,
    source: PreflightIssueSource,
    code: &str,
    message: String,
    remedy: &str,
) -> PreflightIssue {
    PreflightIssue {
        severity,
        source,
        code: code.to_owned(),
        message,
        remedy: remedy.to_owned(),
    }
}

/// Audit the geometry, build the domain and resolution contracts, and build
/// the Gmsh source when nothing blocks.
///
/// An invalid study configuration is the caller's contract and returns
/// `Err`; every geometry, domain, resolution or consistency finding is
/// reported inside the manifest instead, so a blocked manifest can still be
/// shown and saved.
pub fn run_mesh_preflight(
    config: &CfdStudyConfig,
    airfoil: &AirfoilSnapshot,
) -> Result<MeshPreflightBundle, MeshError> {
    use IssueSeverity::{Error, Warning};
    use PreflightIssueSource as Source;
    config
        .validate()
        .map_err(|errors| MeshError::InvalidInput(errors.join(" ")))?;
    let geometry = audit_airfoil_geometry(airfoil, config.chord_m);
    let mut issues = geometry
        .issues
        .iter()
        .map(|finding| {
            issue(
                finding.severity,
                Source::Geometry,
                &format!("{:?}", finding.code),
                finding.message.clone(),
                &finding.remedy,
            )
        })
        .collect::<Vec<_>>();
    let metrics = geometry.metrics.as_ref();
    let domain = match domain_template(config, metrics.map(|m| m.max_thickness_ratio)) {
        Ok(domain) => {
            for warning in &domain.warnings {
                issues.push(issue(Warning, Source::Domain, "domain_extent", warning.clone(), "accepted; establish far-field independence with a domain study before quoting coefficients"));
            }
            Some(domain)
        }
        Err(error) => {
            issues.push(issue(
                Error,
                Source::Domain,
                "domain_controls",
                error.to_string(),
                "correct the domain extents in the mesh settings",
            ));
            None
        }
    };
    let resolution = match mesh_resolution(config, metrics.map(|m| m.perimeter_chord)) {
        Ok(resolution) => {
            for warning in &resolution.warnings {
                issues.push(issue(
                    Warning,
                    Source::Resolution,
                    "resolution",
                    warning.clone(),
                    "accepted; adjust the layer controls if the solved y+ confirms the estimate",
                ));
            }
            Some(resolution)
        }
        Err(error) => {
            issues.push(issue(
                Error,
                Source::Resolution,
                "resolution_controls",
                error.to_string(),
                "correct the preset, refinement or inflation controls",
            ));
            None
        }
    };
    if let (Some(edge), Some(resolution)) = (geometry.trailing_edge.as_ref(), resolution.as_ref()) {
        if let (EdgeKind::Blunt, Some(gap_m)) = (edge.kind, edge.gap_m) {
            let first_layer = resolution.inflation.first_layer_thickness_m;
            let surface = resolution.refinement.surface_size_m;
            if resolution.inflation.enabled && gap_m < first_layer {
                issues.push(issue(Warning, Source::Consistency, "trailing_edge_base_below_first_layer", format!("blunt base {gap_m:.3e} m is thinner than the first inflation layer {first_layer:.3e} m"), "expect distorted base cells; reduce the first-layer height or accept the mesher's intersection handling"));
            } else if gap_m < surface {
                issues.push(issue(Warning, Source::Consistency, "trailing_edge_base_below_surface_size", format!("blunt base {gap_m:.3e} m is shorter than the surface size {surface:.3e} m"), "the base face is resolved by its two exact points; a finer preset resolves it with more cells"));
            }
        }
    }
    let blocked = issues.iter().any(|item| item.severity == Error);
    let (gmsh, gmsh_source_hash, geo) = if blocked {
        (None, None, None)
    } else {
        match build_gmsh_geo(config, airfoil) {
            Ok(geo) => {
                if geometry.topology != Some(geo.report.topology) {
                    issues.push(issue(
                        Error,
                        Source::Consistency,
                        "topology_mismatch",
                        "the audit and the Gmsh builder classified the edges differently"
                            .to_owned(),
                        "report this section; the contracts must agree",
                    ));
                }
                (
                    Some(geo.report.clone()),
                    Some(fnv1a64(geo.source.as_bytes())),
                    Some(geo),
                )
            }
            Err(error) => {
                issues.push(issue(Error, Source::GmshSource, "gmsh_source", error.to_string(), "the geometry passed the audit but the Gmsh builder rejected it; report this section"));
                (None, None, None)
            }
        }
    };
    let status = if issues.iter().any(|item| item.severity == Error) {
        PreflightStatus::Blocked
    } else {
        PreflightStatus::Ready
    };
    let manifest = MeshPreflight {
        manifest_version: MESH_MANIFEST_VERSION.to_owned(),
        case_template_version: TEMPLATE_VERSION.to_owned(),
        gmsh_template_version: GMSH_TEMPLATE_VERSION.to_owned(),
        domain_template_version: DOMAIN_TEMPLATE_VERSION.to_owned(),
        geometry_audit_version: GEOMETRY_AUDIT_VERSION.to_owned(),
        preset_catalogue_version: PRESET_CATALOGUE_VERSION.to_owned(),
        units: units_statement(),
        airfoil: AirfoilIdentity {
            name: airfoil.name.clone(),
            coordinate_hash: airfoil.coordinate_hash.clone(),
            coordinate_count: airfoil.coordinates.len(),
            source: "database snapshot; exact coordinates, no resampling or substitution".to_owned(),
        },
        chord_m: config.chord_m,
        geometry,
        domain,
        resolution,
        gmsh,
        gmsh_source_hash,
        quality_thresholds: MeshQualityThresholds::template_defaults(),
        artifacts: expected_artifacts(),
        toolchain: ToolchainExpectation {
            mesher: "gmsh".to_owned(),
            mesh_format: "msh2".to_owned(),
            converter: "gmshToFoam".to_owned(),
            checker: "checkMesh".to_owned(),
            executed: false,
        },
        status,
        issues,
        validity: "preflight only: the geometry, domain and resolution contracts are validated numerically; no mesh was generated, no quality metric was measured, and no domain independence or physical validity is implied".to_owned(),
    };
    Ok(MeshPreflightBundle { manifest, geo })
}

/// Write the manifest to [`MESH_MANIFEST_FILE`] under `case_dir`.
pub fn write_mesh_manifest(
    case_dir: &Path,
    manifest: &MeshPreflight,
) -> Result<PathBuf, MeshError> {
    let path = case_dir.join(MESH_MANIFEST_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            MeshError::Io(format!("cannot create {}: {error}", parent.display()))
        })?;
    }
    fs::write(&path, manifest.to_json_pretty()?)
        .map_err(|error| MeshError::Io(format!("cannot write {}: {error}", path.display())))?;
    Ok(path)
}

fn expected_artifacts() -> ExpectedArtifacts {
    ExpectedArtifacts {
        gmsh_source: "system/airfoil.geo".to_owned(),
        coordinates_csv: "constant/airfoil.csv".to_owned(),
        mesh_report: "system/mesh-report.json".to_owned(),
        manifest: MESH_MANIFEST_FILE.to_owned(),
        poly_mesh_dir: "constant/polyMesh".to_owned(),
        boundary_file: "constant/polyMesh/boundary".to_owned(),
    }
}

fn units_statement() -> UnitsStatement {
    UnitsStatement {
        lengths: "metres".to_owned(),
        coordinates: "unit chord (x/c, y/c)".to_owned(),
        velocity: "m/s".to_owned(),
        density: "kg/m^3".to_owned(),
        dynamic_viscosity: "Pa s".to_owned(),
        angles: "degrees".to_owned(),
    }
}

/// FNV-1a over raw bytes; a provenance checksum, not a security boundary.
fn fnv1a64(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64-{hash:016x}")
}
