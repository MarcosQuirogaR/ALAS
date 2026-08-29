// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Multi-stage design, analysis, optimization and export pipeline.
//!
//! [`full_analysis`] ports `alas/analysis/full_analysis.py`: high-fidelity
//! aerodynamic polar sweeps, parabolic polar fits, physical mass & CG anchor,
//! static margin, neutral point, CG envelope check, and trimmed operating points.
//!
//! [`baseline`] ports `alas/pipeline.py`'s `_baseline_analysis`: fast initial
//! weight & balance and longitudinal stability estimation.
//!
//! [`structural`] ports `alas/pipeline.py`'s `_run_structural_analysis`: generic
//! wingbox sizing, finite element mesh building, analytical deformation, and NASTRAN solves.
//!
//! [`export`] ports `alas/reporting/design_report.py`: JSON design database and Selig
//! `.dat` airfoil file exports and summary formatting.
//!
//! [`runs`] ports `alas/sidecar/runs.py`: run lifecycle tracking and bounded in-memory registry.
//!
//! [`pipeline`] ports `alas/pipeline.py`: master multi-stage design coordinator.
//!
//! [`openvsp`] materializes the native geometry and a lifting-surface-only
//! VSPAERO mesh. [`vspaero`] runs and parses that independent solver, then
//! admits only reference- and frame-compatible quantities to report overlays.

pub mod avl;
pub mod baseline;
pub mod cpacs;
#[path = "cpacs/adapters.rs"]
pub mod cpacs_adapters;
pub mod dual_solver;
pub mod export;
pub mod feasibility;
pub mod flowunsteady;
pub mod full_analysis;
mod mission_stage;
pub mod openvsp;
mod patran;
pub mod pipeline;
pub mod plot;
pub mod runs;
pub mod solver_mode;
pub mod structural;
pub mod vspaero;
pub mod vspaero_refinement;

pub use alas_exec::RunEnvironment;
pub use alas_opt::{
    ModelCgConstraint, ModelCgConstraintAssessment, ModelCgEnvelopeAssessment,
    ModelCgLoadingAssessment, ModelCgLoadingState, StaticMarginPreferenceAssessment,
};
pub use avl::{
    classify_avl_comparison, run_avl_analysis, run_avl_takeoff_comparison, AvlAnalysisResult,
    AvlAnalysisStatus, AvlComparableQuantity, AvlComparisonReference, AvlComparisonStatus,
};
pub use baseline::{analyze_baseline, BaselineReport};
pub use cpacs::{
    export_cpacs, export_cpacs_with_analysis, read_cpacs, read_cpacs_file, render_cpacs_v35,
    write_cpacs_run_manifest, CpacsAircraft, CpacsAircraftError, CpacsDocument, CpacsEngine,
    CpacsEnginePosition, CpacsExportError, CpacsExportResult, CpacsFuselage, CpacsFuselageElement,
    CpacsFuselageProfile, CpacsFuselageSection, CpacsHeader, CpacsReadError, CpacsRunManifest,
    CpacsSegment, CpacsTransformation, CpacsVersionInfo, CpacsWing, CpacsWingAirfoil,
    CpacsWingElement, CpacsWingSection, CPACS_35_VERSION, CPACS_V35_SCHEMA_URL,
};
pub use cpacs_adapters::{
    CpacsAdapterContract, CpacsAdapterManifest, CpacsAdapterRequest, CpacsAdapterTool,
    CpacsAircraftData, CpacsGeometryDerivation, CpacsGeometryScope, CpacsGeometrySummary,
    CpacsNativeRepresentation, CpacsSource, CPACS_ADAPTER_MANIFEST_VERSION,
};
pub use dual_solver::{
    run_solver_optimizations, SolverOptimizationResult, SolverOptimizationSet,
    SolverOptimizationStatus,
};
pub use export::{
    export_airfoil_dat, export_json, export_json_with_feasibility,
    export_json_with_feasibility_and_cpacs, format_summary, CpacsReference, DesignDatabase,
};
pub use feasibility::{
    assess_physical_feasibility, format_feasibility, CarriedFuelBasis, CgEnvelopeAssessment,
    CruiseEquilibriumAssessment, FeasibilityReport, FindingCode, FindingSeverity,
    FuelCapacityAssessment, FuelCapacityEvidence, FuelLoadingAssessment, MissionFuelAssessment,
    MissionFuelStatus, PhysicalFinding, PlanningCgStatus,
};
pub use flowunsteady::{
    run_flowunsteady_analysis, FlowUnsteadyAnalysisResult, FlowUnsteadyAnalysisStatus,
};
pub use full_analysis::{
    AnalysisReport, DesignPoint, FullAnalysis, PolarFit, PolarFitStatus, TrimmedDesignPoint,
};
pub use openvsp::{
    export_openvsp_script, materialize_openvsp_project, OpenVspExportResult, OpenVspExportStatus,
};
pub use pipeline::{
    DesignPipeline, PipelineExecutionStatus, PipelineOptions, PipelineResult, RoutePlanningStatus,
};
pub use plot::render_scene_svg;
pub use runs::{RunEvent, RunEventKind, RunEventSeverity, RunRegistry, RunState};
pub use solver_mode::{AerodynamicSolverMode, OptimizationSolverMode, SolverKind};
pub use structural::{run_structural_analysis, StructuralAnalysisResult};
pub use vspaero::{
    classify_vspaero_comparison, run_vspaero_analysis, VspaeroAnalysisResult,
    VspaeroAnalysisStatus, VspaeroComparableQuantity, VspaeroComparisonStatus,
};
pub use vspaero_refinement::{
    apply_vspaero_mesh_resolution, assess_vspaero_refinement, VspaeroCoefficientChange,
    VspaeroMeshResolution, VspaeroRefinementAssessment, VspaeroRefinementVerdict,
    VSPAERO_REFINEMENT_LEVELS,
};
