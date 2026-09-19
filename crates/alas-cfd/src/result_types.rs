// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

/// Frame and normalization conventions recorded in every case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameConvention {
    /// Chord-axis description.
    pub axes: String,
    /// Positive velocity convention for angle of attack.
    pub angle_of_attack: String,
    /// Positive force convention.
    pub forces: String,
    /// Reference point used for pitching moment.
    pub moment_reference: String,
}

impl Default for FrameConvention {
    fn default() -> Self {
        Self {
            axes: "chord +x, section normal +y, extrusion +z".to_owned(),
            angle_of_attack:
                "freestream U=(U cos(alpha), U sin(alpha), 0), positive alpha toward +y".to_owned(),
            forces: "drag positive along freestream; lift positive 90 deg counter-clockwise in x-y; q = rho U^2/2, Aref = chord x extrusion span, lRef = chord"
                .to_owned(),
            moment_reference:
                "quarter-chord at the extrusion mid-plane (x/c=0.25, y=0, z=span/2); Cm positive nose-up (leading edge toward +y), forceCoeffs pitchAxis (0 0 -1)"
                    .to_owned(),
        }
    }
}

/// Machine-readable input provenance stored as `study.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StudyProvenance {
    /// Template identifier.
    pub template_version: String,
    /// Full effective configuration.
    pub config: CfdStudyConfig,
    /// Coordinate snapshot.
    pub airfoil: AirfoilSnapshot,
    /// Derived speed.
    pub effective_speed_m_s: f64,
    /// Derived Reynolds number.
    pub effective_reynolds: f64,
    /// Axes/sign/reference conventions.
    pub frame: FrameConvention,
    /// Numeric reference lengths, directions and moment point used by the
    /// generated dictionaries and the result normalisation.  `None` only for
    /// records written before the typed conventions existed.
    #[serde(default)]
    pub reference: Option<ReferenceConventions>,
    /// Backend actually selected for the run, when an external process was
    /// reached.  `None` is retained for locally generated or preflight-only
    /// cases that never resolved a runnable backend.
    #[serde(default)]
    pub backend: Option<String>,
    /// OpenFOAM version reported by the selected backend.
    #[serde(default)]
    pub openfoam_version: Option<String>,
    /// FNV-1a hashes of generated case artifacts and executed utilities.  The
    /// algorithm is recorded in the report so hashes remain reproducible
    /// without adding a crypto dependency to the desktop binary.
    #[serde(default)]
    pub file_hashes: BTreeMap<String, String>,
}

/// Lifecycle stage emitted by a background study worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CfdStage {
    /// Case files and geometry were prepared.
    GeometryPreparation,
    /// Background and surface mesh utilities are running.
    Meshing,
    /// `checkMesh` quality gate.
    QualityGate,
    /// Optional potential-flow initialization.
    Initialization,
    /// Steady RANS solver.
    Solution,
    /// Coefficients and sampled fields are collected.
    PostProcessing,
}

impl CfdStage {
    /// Stable UI/log label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GeometryPreparation => "geometry preparation",
            Self::Meshing => "meshing",
            Self::QualityGate => "quality gate",
            Self::Initialization => "initialization",
            Self::Solution => "solution",
            Self::PostProcessing => "post-processing",
        }
    }
}

/// Severity of a study lifecycle message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CfdEventSeverity {
    /// Informational progress.
    Info,
    /// The run can continue but the result needs attention.
    Warning,
    /// A stage failed.
    Error,
}

/// Background lifecycle event suitable for a GUI run log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfdRunEvent {
    /// Stage that emitted the event.
    pub stage: CfdStage,
    /// Severity.
    pub severity: CfdEventSeverity,
    /// Human-readable message.
    pub message: String,
    /// Elapsed wall time since the worker started.
    pub elapsed_seconds: f64,
}

/// Final process-independent classification of a study.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CfdOutcome {
    /// Utility failure, missing output or invalid mesh prevented a result.
    Failed,
    /// The user cancelled the owned process tree.
    Cancelled,
    /// Solver exited but residual/force/conservation criteria were not met.
    Unconverged,
    /// Residuals, force stabilization and continuity checks passed.
    NumericallyConverged,
}

impl CfdOutcome {
    /// Stable UI/log label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Unconverged => "unconverged",
            Self::NumericallyConverged => "numerically converged",
        }
    }
}

/// One equation residual extracted from a solver log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResidualSample {
    /// Monotonic sample number in the captured log.
    pub iteration: u64,
    /// Equation name, e.g. `Ux` or `p`.
    pub field: String,
    /// Initial residual reported by OpenFOAM.
    pub initial: f64,
    /// Final residual reported by OpenFOAM.
    pub final_residual: f64,
}

/// One force-coefficient sample from `forceCoeffs`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForceSample {
    /// OpenFOAM time/iteration.
    pub time: f64,
    /// Total drag coefficient.
    pub cd: f64,
    /// Total lift coefficient.
    pub cl: f64,
    /// Pitching-moment coefficient around quarter chord.
    pub cm: f64,
    /// Pressure drag contribution when available.
    pub cd_pressure: Option<f64>,
    /// Viscous drag contribution when available.
    pub cd_viscous: Option<f64>,
    /// Pressure lift contribution when available.
    pub cl_pressure: Option<f64>,
    /// Viscous lift contribution when available.
    pub cl_viscous: Option<f64>,
}

/// Dimensional pressure/viscous force components emitted by the OpenFOAM
/// `forces` function object. Components are retained in the solver frame;
/// `apply_force_decomposition` projects them into the configured drag/lift
/// axes and applies the same dynamic-pressure/reference-area normalization as
/// `forceCoeffs`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForceDecompositionSample {
    /// OpenFOAM time/iteration.
    pub time: f64,
    /// Pressure contribution in newtons, `(x, y, z)`.
    pub pressure_force_n: [f64; 3],
    /// Viscous contribution in newtons, `(x, y, z)`.
    pub viscous_force_n: [f64; 3],
    /// Porous contribution when the file includes it.
    pub porous_force_n: Option<[f64; 3]>,
}

/// Continuity/mass-balance diagnostic extracted from solver output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MassBalanceSample {
    /// OpenFOAM time/iteration when known.
    pub time: Option<f64>,
    /// Sum of local continuity errors, when reported.
    pub sum_local: Option<f64>,
    /// Global continuity error, when reported.
    pub global: Option<f64>,
    /// Cumulative continuity error, when reported.
    pub cumulative: Option<f64>,
}

/// Default solver-only verdict for a record archived before the field
/// existed: `Failed`, because an older record carries no evidence that the
/// solver reached any criterion, and absence of evidence must not read as a
/// pass.
fn default_numerical_convergence() -> CfdOutcome {
    CfdOutcome::Failed
}

/// Mesh quality evidence from `checkMesh`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeshQuality {
    /// Whether checkMesh explicitly reported `Mesh OK` and no failed checks.
    pub passed: bool,
    /// Cell count, if parsed.
    pub cells: Option<u64>,
    /// Maximum non-orthogonality, if parsed.
    pub max_non_orthogonality_deg: Option<f64>,
    /// Maximum skewness, if parsed.
    pub max_skewness: Option<f64>,
    /// Minimum cell volume, if parsed.
    pub min_volume_m3: Option<f64>,
    /// Faces `checkMesh` called *severely* non-orthogonal (`> 70 deg`), if the
    /// line was present.
    ///
    /// On the runs measured here `checkMesh` prints this as a single-`*`
    /// warning and still reports `Non-orthogonality check OK` and `Mesh OK`.
    /// No angle is claimed at which it would instead *fail*: that number is not
    /// in the log and depends on the release and the generation dictionary.
    ///
    /// The maximum angle alone cannot be read without this count — on the fine
    /// preset one face out of 436 389 cells reaches `71.37 deg` against an
    /// average of `5.35 deg`, which is a different mesh from one where
    /// thousands do.  What the count does **not** establish is that the outlier
    /// is aerodynamically irrelevant; its location and its effect on the
    /// integrated loads were not measured.
    ///
    /// `None` means the warning line was absent, which is the normal case.
    #[serde(default)]
    pub severely_non_orthogonal_faces: Option<u64>,
    /// Largest BOUNDARY-face skewness and the patch it occurs on.
    ///
    /// Dimensionless.  `checkMesh`'s headline `Max skewness` is the internal
    /// maximum; this is read from the `boundaryField` of the written
    /// `skewness` field and is the only exact boundary value available.
    /// `None` means the field was not written, so the boundary limit is not
    /// demonstrated by a measured maximum.
    #[serde(default)]
    pub max_boundary_skewness: Option<f64>,
    /// Patch carrying [`Self::max_boundary_skewness`].
    #[serde(default)]
    pub max_boundary_skewness_patch: Option<String>,
    /// Faces violating `maxInternalSkewness` or `maxBoundarySkewness`, from
    /// `checkMesh -meshQuality`.
    ///
    /// `Some(0)` demonstrates both declared skewness limits; `Some(n > 0)` is a
    /// violation of at least one, which the text does not attribute.  `None`
    /// means the run did not use `-meshQuality`, so neither limit is proven.
    #[serde(default)]
    pub skewness_faces_in_error: Option<u64>,
    /// Faces violating `maxNonOrtho`, from `checkMesh -meshQuality`.
    #[serde(default)]
    pub non_orthogonality_faces_in_error: Option<u64>,
    /// Raw quality output retained for audit/export.
    pub raw_output: String,
    /// Percentile summaries computed from native OpenFOAM cell-quality fields.
    ///
    /// An empty vector means that `checkMesh` returned only scalar summaries or
    /// that the requested fields were not emitted.  The values are never
    /// reconstructed from the max/min values in the text log.
    #[serde(default)]
    pub distributions: Vec<ScalarDistribution>,
    /// Measured wall resolution from the solver-attached yPlus function
    /// object.  This remains optional because a failed or preflight-only case
    /// may never produce a turbulence field.
    #[serde(default)]
    pub near_wall: Option<NearWallDiagnostics>,
    /// Percentile summary of the solved wall-face y+ field, kept separate from
    /// cell-quality distributions because it is a solution diagnostic.
    #[serde(default)]
    pub near_wall_distribution: Option<ScalarDistribution>,
}

impl Default for MeshQuality {
    fn default() -> Self {
        Self {
            passed: false,
            cells: None,
            max_non_orthogonality_deg: None,
            max_skewness: None,
            min_volume_m3: None,
            severely_non_orthogonal_faces: None,
            max_boundary_skewness: None,
            max_boundary_skewness_patch: None,
            skewness_faces_in_error: None,
            non_orthogonality_faces_in_error: None,
            raw_output: String::new(),
            distributions: Vec::new(),
            near_wall: None,
            near_wall_distribution: None,
        }
    }
}

/// Finite-value percentile evidence read from a native OpenFOAM scalar field.
///
/// Percentiles are stored instead of the full cell vector so result artifacts
/// remain compact while retaining a traceable distribution shape.  The source
/// path and sample count identify the exact field used for the summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScalarDistribution {
    /// Native field object name, such as `aspectRatio` or `yPlus`.
    pub field: String,
    /// Human-readable quantity label.
    pub label: String,
    /// SI or dimensionless unit recorded for the field.
    pub unit: String,
    /// Relative path to the native OpenFOAM field.
    pub source: String,
    /// Number of finite values represented by the summary.
    pub sample_count: usize,
    /// Minimum finite value.
    pub min: f64,
    /// Arithmetic mean of finite values.
    pub mean: f64,
    /// Maximum finite value.
    pub max: f64,
    /// Percentile coordinates in percent, paired with `values`.
    pub percentiles: Vec<f64>,
    /// Interpolated finite values at `percentiles`.
    pub values: Vec<f64>,
}

/// Measured and configured near-wall resolution for the airfoil patch.
///
/// The measured statistics come from OpenFOAM's solver-attached `yPlus`
/// function object.  The target and selected wall distance are copied from
/// the effective mesh settings so a result can show both what was requested
/// and what the solved field produced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NearWallDiagnostics {
    /// Numeric OpenFOAM time represented by the summary.
    pub time: f64,
    /// Patch represented by the summary.
    pub patch_name: String,
    /// Number of native wall-face values when the field list was available.
    pub sample_count: Option<usize>,
    /// Minimum measured y+ on the patch.
    pub min_y_plus: f64,
    /// Maximum measured y+ on the patch.
    pub max_y_plus: f64,
    /// Area or face average reported by OpenFOAM.
    pub average_y_plus: f64,
    /// Requested target from the study settings.
    pub target_y_plus: f64,
    /// Explicit wall distance used to size the first cell centre, in metres.
    pub selected_wall_distance_m: f64,
    /// Flat-plate estimate recorded when the GEO was generated.
    pub estimated_y_plus: Option<f64>,
    /// Relative source path of the parsed summary.
    pub source: String,
}

/// A field or sampled artifact found under the case directory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldArtifact {
    /// Relative path inside the case.
    pub relative_path: String,
    /// Field or artifact name.
    pub name: String,
    /// Numeric time directory, when applicable.
    pub time: Option<f64>,
    /// Whether the artifact is a native OpenFOAM field or a sampled export.
    pub kind: String,
}

/// Parsed results and provenance from one case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CfdResults {
    /// Final numerical/process classification.
    pub outcome: CfdOutcome,
    /// Case directory that produced these results.
    pub case_dir: PathBuf,
    /// Input contract copied from `study.json`.
    pub provenance: StudyProvenance,
    /// Solver residual samples.
    pub residuals: Vec<ResidualSample>,
    /// Force history.
    pub forces: Vec<ForceSample>,
    /// Continuity diagnostics.
    pub mass_balance: Vec<MassBalanceSample>,
    /// Mesh quality evidence.
    pub mesh_quality: MeshQuality,
    /// Whether the converted mesh meets the DECLARED numeric limits, check by
    /// check, with unmeasured checks marked as such.
    ///
    /// Separate from `checkMesh`'s own verdict, which is one of the checks
    /// inside it.  A case can converge numerically on a mesh that violates a
    /// declared limit; both verdicts are kept so neither hides the other.
    #[serde(default)]
    pub mesh_qualification: MeshQualification,
    /// Solver-only verdict, before the mesh contract is applied.
    ///
    /// `outcome` is the worse of this and the mesh qualification.  This field
    /// keeps the numerical result visible when a mesh failure overrides it.
    #[serde(default = "default_numerical_convergence")]
    pub numerical_convergence: CfdOutcome,
    /// Whether each solved field was still being updated between the last two
    /// written times.
    ///
    /// This is what separates a converged equation from an abandoned one; the
    /// residual history cannot.  Absent on results archived before the check
    /// existed, which reads as "not observed", never as "not updated".
    #[serde(default)]
    pub field_updates: FieldUpdateEvidence,
    /// Actual native/sampled field outputs found after post-processing.
    pub fields: Vec<FieldArtifact>,
    /// Face-resolved pressure and wall-shear distribution from the latest
    /// native OpenFOAM time, when the required mesh and fields were emitted.
    pub surface: Option<surface::SurfaceDistribution>,
    /// Explicit reason surface extraction was unavailable.  Missing surface
    /// data never becomes synthetic Cp/Cf values.
    pub surface_error: Option<String>,
    /// Captured stdout/stderr summaries by utility.
    pub command_logs: BTreeMap<String, String>,
    /// Human-readable reason when the result is failed/unconverged.
    pub status_detail: String,
}
