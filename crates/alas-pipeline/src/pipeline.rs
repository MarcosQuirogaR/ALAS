// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/pipeline.py
// Reference: alas @ rust-port-baseline.

//! Complete multi-stage aircraft design, analysis, optimization and export pipeline.
//!
//! [`DesignPipeline::run`] executes the full sequence:
//! - Stage 0: Baseline W&B + stability estimation.
//! - Stage 1: Design space optimization via differential evolution.
//! - Stage 2: High-fidelity fine-VLM full analysis of optimized (and baseline) designs.
//! - Stage 3: Design database, Selig airfoil, and OpenVSP geometry export.
//! - Stage 5: Lateral routing and mission performance simulation.
//! - Stage 6: MSES 2-D transonic viscous/inviscid coupling analysis.
//! - Stage 7: Wingbox structural sizing, mesh deck building, and NASTRAN solves.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use alas_aero::mses::{
    run_mses_polar, run_mses_pressure_distribution, MsesPolarResult, MsesPressureResult,
};
use alas_config::airports::get as get_airport;
use alas_config::design_variables::DesignVector;
use alas_config::presets;
use alas_config::{AlasConfig, Severity};
use alas_exec::{RunEnvironment, ToolLocator};
use alas_geom::aircraft::airplane::Airplane;
use alas_mission::MissionResult;
use alas_opt::OptimizationResult;
use alas_route::planner::{load_navdata, plan_route, RouteSources};
use alas_route::route::{Route, RouteSource};
use alas_route::{fetch_route_with_status, SimbriefFetchStatus};
use serde::{Deserialize, Serialize};

use crate::avl::{run_avl_takeoff_comparison, AvlAnalysisResult};
use crate::baseline::{analyze_baseline, BaselineReport};
use crate::cabin_scene::export_cabin_scene;
use crate::cpacs::{
    export_cpacs, export_cpacs_with_analysis, read_cpacs_file, write_cpacs_run_manifest,
    CpacsDocument, CpacsExportResult,
};
use crate::cpacs_adapters::CpacsAircraftData;
use crate::dual_solver::{run_solver_optimizations, SolverOptimizationSet};
use crate::export::{
    export_airfoil_dat, export_json_with_feasibility_and_cpacs, format_summary, CpacsReference,
    DesignDatabase,
};
use crate::feasibility::{assess_physical_feasibility, format_feasibility, FeasibilityReport};
use crate::flowunsteady::{run_flowunsteady_analysis, FlowUnsteadyAnalysisResult};
use crate::full_analysis::{AnalysisReport, FullAnalysis};
use crate::mission_stage;
use crate::openvsp::{export_openvsp_script, materialize_openvsp_project, OpenVspExportResult};
use crate::payload_layout_export::export_payload_layout_artifact;
use crate::runs::{RunEvent, RunEventKind, RunEventSeverity};
use crate::solver_mode::{AerodynamicSolverMode, OptimizationSolverMode};
use crate::structural::StructuralAnalysisResult;
use crate::vspaero::{run_vspaero_analysis, VspaeroAnalysisResult};

mod curl_transport;
use curl_transport::SystemCurlTransport;
mod helpers;
#[cfg(test)]
use helpers::optimizer_config;
use helpers::{
    add_manifest_artifact, persist_mses_polar_diagnostics, persist_mses_raw_exports,
    validate_bounds,
};

/// The 2-D section condition sent to MSES for a 3-D swept-wing cruise case.
///
/// Reynolds number remains based on the freestream speed, while MSES receives
/// the normal component of Mach. The incidence is a finite-wing exposed-root
/// proxy: geometric body alpha plus the physical fuselage-edge setting, less
/// the mean induced angle. It is not a substitute for a resolved spanwise
/// viscous analysis.
#[derive(Debug, Clone, Copy)]
struct MsesSectionCondition {
    mach: f64,
    reynolds: f64,
    alpha_deg: f64,
}

const PIPELINE_STAGE_COUNT: u8 = 7;

fn check_cancelled(cancel: Option<&AtomicBool>) -> Result<(), String> {
    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        Err("Cancelled safely at a pipeline stage boundary".to_owned())
    } else {
        Ok(())
    }
}

fn emit_event(events: Option<&(dyn Fn(RunEvent) + Sync)>, run_clock: Instant, event: RunEvent) {
    if let Some(callback) = events {
        callback(RunEvent {
            elapsed_ms: run_clock.elapsed().as_millis() as u64,
            ..event
        });
    }
}

fn begin_stage(
    events: Option<&(dyn Fn(RunEvent) + Sync)>,
    run_clock: Instant,
    index: u8,
    stage: &str,
    message: &str,
) -> Instant {
    emit_event(
        events,
        run_clock,
        RunEvent {
            stage: stage.to_owned(),
            message: message.to_owned(),
            fraction: Some(0.0),
            kind: RunEventKind::StageStarted,
            severity: RunEventSeverity::Info,
            stage_index: Some(index),
            stage_count: Some(PIPELINE_STAGE_COUNT),
            elapsed_ms: 0,
            duration_ms: None,
        },
    );
    Instant::now()
}

fn finish_stage(
    events: Option<&(dyn Fn(RunEvent) + Sync)>,
    run_clock: Instant,
    stage_clock: Instant,
    index: u8,
    stage: &str,
) {
    let duration_ms = stage_clock.elapsed().as_millis() as u64;
    emit_event(
        events,
        run_clock,
        RunEvent {
            stage: stage.to_owned(),
            message: format!("Completed in {:.3} s", duration_ms as f64 / 1_000.0),
            fraction: Some(1.0),
            kind: RunEventKind::StageCompleted,
            severity: RunEventSeverity::Info,
            stage_index: Some(index),
            stage_count: Some(PIPELINE_STAGE_COUNT),
            elapsed_ms: 0,
            duration_ms: Some(duration_ms),
        },
    );
}

/// Start a detailed downstream component timer.
///
/// Component events intentionally do not carry the seven-stage index. They
/// are children of the top-level `downstream` stage and are rendered as a
/// separate, indented timing list by the desktop console.
pub(crate) fn begin_component(
    events: Option<&(dyn Fn(RunEvent) + Sync)>,
    run_clock: Instant,
    stage: &str,
    message: &str,
) -> Instant {
    emit_event(
        events,
        run_clock,
        RunEvent {
            stage: stage.to_owned(),
            message: message.to_owned(),
            fraction: Some(0.0),
            kind: RunEventKind::StageStarted,
            severity: RunEventSeverity::Info,
            stage_index: None,
            stage_count: None,
            elapsed_ms: 0,
            duration_ms: None,
        },
    );
    Instant::now()
}

/// Finish a detailed downstream component timer.
pub(crate) fn finish_component(
    events: Option<&(dyn Fn(RunEvent) + Sync)>,
    run_clock: Instant,
    stage_clock: Instant,
    stage: &str,
    status: &str,
) {
    let duration_ms = stage_clock.elapsed().as_millis() as u64;
    let message = if status.eq_ignore_ascii_case("skipped") {
        status.to_owned()
    } else {
        format!("{status} in {:.3} s", duration_ms as f64 / 1_000.0)
    };
    emit_event(
        events,
        run_clock,
        RunEvent {
            stage: stage.to_owned(),
            message,
            fraction: Some(1.0),
            kind: RunEventKind::StageCompleted,
            severity: RunEventSeverity::Info,
            stage_index: None,
            stage_count: None,
            elapsed_ms: 0,
            duration_ms: Some(duration_ms),
        },
    );
}

fn emit_diagnostic(
    events: Option<&(dyn Fn(RunEvent) + Sync)>,
    run_clock: Instant,
    stage: &str,
    message: &str,
) {
    emit_diagnostic_with_severity(events, run_clock, stage, message, RunEventSeverity::Info);
}

fn emit_diagnostic_with_severity(
    events: Option<&(dyn Fn(RunEvent) + Sync)>,
    run_clock: Instant,
    stage: &str,
    message: &str,
    severity: RunEventSeverity,
) {
    emit_event(
        events,
        run_clock,
        RunEvent {
            stage: stage.to_owned(),
            message: message.to_owned(),
            fraction: None,
            kind: RunEventKind::Diagnostic,
            severity,
            stage_index: None,
            stage_count: Some(PIPELINE_STAGE_COUNT),
            elapsed_ms: 0,
            duration_ms: None,
        },
    );
}

#[allow(clippy::too_many_arguments)]
fn emit_tool_diagnostics(
    events: Option<&(dyn Fn(RunEvent) + Sync)>,
    run_clock: Instant,
    environment: &RunEnvironment,
    openvsp: Option<&OpenVspExportResult>,
    vspaero: Option<&VspaeroAnalysisResult>,
    avl: Option<&AvlAnalysisResult>,
    flowunsteady: Option<&FlowUnsteadyAnalysisResult>,
    structures: Option<&StructuralAnalysisResult>,
    mses: Option<&MsesPolarResult>,
) {
    let configured = [
        ("OpenVSP", environment.openvsp_exe.as_deref()),
        ("VSPAERO", environment.vspaero_exe.as_deref()),
        ("AVL", environment.avl_exe.as_deref()),
        ("FLOWUnsteady", environment.flowunsteady_exe.as_deref()),
        ("MSES", environment.mses_dir.as_deref()),
        ("Nastran", environment.nastran_exe.as_deref()),
        ("Patran", environment.patran_exe.as_deref()),
    ];
    for (tool, path) in configured {
        let message = path.map_or_else(
            || format!("{tool}: not configured"),
            |path| format!("{tool}: resolved {}", path.display()),
        );
        emit_diagnostic_with_severity(
            events,
            run_clock,
            "external_tools",
            &message,
            if path.is_some() {
                RunEventSeverity::Info
            } else {
                RunEventSeverity::Warning
            },
        );
    }
    for (tool, status) in [
        (
            "OpenVSP",
            openvsp.map(|value| {
                status_with_detail(
                    format!("{:?}", value.status),
                    value.runtime_error.as_deref(),
                )
            }),
        ),
        (
            "VSPAERO",
            vspaero.map(|value| {
                status_with_detail(format!("{:?}", value.status), value.error.as_deref())
            }),
        ),
        (
            "AVL",
            avl.map(|value| {
                status_with_detail(format!("{:?}", value.status), value.error.as_deref())
            }),
        ),
        (
            "FLOWUnsteady",
            flowunsteady.map(|value| {
                status_with_detail(format!("{:?}", value.status), value.error.as_deref())
            }),
        ),
        (
            "Structures",
            structures
                .map(|value| status_with_detail(value.status.clone(), value.error.as_deref())),
        ),
        (
            "MSES",
            mses.map(|value| {
                status_with_detail(format!("{:?}", value.status), value.error.as_deref())
            }),
        ),
    ] {
        emit_diagnostic(
            events,
            run_clock,
            "external_tools",
            &format!(
                "{tool}: {}",
                status.unwrap_or_else(|| "not requested".to_owned())
            ),
        );
    }

    // The overall structural result is intentionally still `ok` when the
    // analytical sizing path succeeded.  Surface the individual native
    // solver outcomes as well, otherwise a missing MSC DLL is hidden behind
    // the useful-but-different analytical answer.
    if let Some(structures) = structures {
        for (tool, outcome) in [
            (
                "MSC Nastran SOL 101",
                structures.nastran.as_ref().map(|result| {
                    (
                        result.static_solve.status,
                        result.static_solve.error.as_deref(),
                    )
                }),
            ),
            (
                "MSC Nastran SOL 103",
                structures
                    .nastran
                    .as_ref()
                    .map(|result| (result.modes.status, result.modes.error.as_deref())),
            ),
            (
                "NASTRAN-95 SOL 101",
                structures.nastran95.as_ref().map(|result| {
                    (
                        result.static_solve.status,
                        result.static_solve.error.as_deref(),
                    )
                }),
            ),
            (
                "NASTRAN-95 SOL 103",
                structures
                    .nastran95
                    .as_ref()
                    .map(|result| (result.modes.status, result.modes.error.as_deref())),
            ),
        ] {
            if let Some((status, error)) = outcome {
                emit_diagnostic_with_severity(
                    events,
                    run_clock,
                    "external_tools",
                    &format!(
                        "{tool}: {}",
                        status_with_detail(status.as_str().to_owned(), error)
                    ),
                    if status == alas_struct::nastran::ResultStatus::Ok {
                        RunEventSeverity::Info
                    } else {
                        RunEventSeverity::Warning
                    },
                );
            }
        }
        if let Some(patran) = structures.patran.as_ref() {
            emit_diagnostic_with_severity(
                events,
                run_clock,
                "external_tools",
                &format!(
                    "Patran: {}",
                    status_with_detail(patran.status.clone(), patran.error.as_deref())
                ),
                if patran.status.eq_ignore_ascii_case("ok") {
                    RunEventSeverity::Info
                } else {
                    RunEventSeverity::Warning
                },
            );
        }
    }
}

fn status_with_detail(status: String, error: Option<&str>) -> String {
    let Some(error) = error.map(str::trim).filter(|error| !error.is_empty()) else {
        return status;
    };
    const MAX_CHARS: usize = 1_000;
    let detail = error.chars().take(MAX_CHARS).collect::<String>();
    if detail.chars().count() < error.chars().count() {
        format!("{status}: {detail} …")
    } else {
        format!("{status}: {detail}")
    }
}

fn mses_section_condition(config: &AlasConfig, report: &AnalysisReport) -> MsesSectionCondition {
    let freestream_mach = config.requirements.cruise_mach;
    let atmo = alas_atmo::Atmosphere::new(config.requirements.cruise_altitude_m);
    let velocity = freestream_mach * atmo.speed_of_sound();
    let inboard_section = config
        .geometry
        .wing
        .inboard_aerodynamic_station(&report.design)
        .ok();
    let section_chord_m =
        inboard_section.map_or(report.design.root_chord_m, |section| section.chord_m);
    let section_twist_deg = inboard_section
        .map_or(config.geometry.wing.root_twist_deg, |section| {
            section.twist_deg
        });
    let reynolds =
        (atmo.density() * velocity * section_chord_m) / atmo.dynamic_viscosity().max(1e-9);
    let mach = freestream_mach * report.design.sweep_deg.to_radians().cos();
    let body_alpha_deg = report
        .trimmed_design_point
        .map_or(report.design_point.alpha_deg, |trim| {
            trim.geometric_body_alpha_deg
        });
    let induced_angle_deg = mean_induced_angle_deg(
        report
            .trimmed_design_point
            .map_or(report.design_point.cl, |trim| trim.cl),
        report.polar_fit.aspect_ratio,
        report.polar_fit.oswald_e,
    );
    let alpha_deg = body_alpha_deg + section_twist_deg - induced_angle_deg;

    MsesSectionCondition {
        mach,
        reynolds,
        alpha_deg,
    }
}

/// Mean finite-wing downwash angle used only to map a trimmed 3-D state onto
/// the root-section MSES proxy. The lifting-line relation is deliberately
/// bounded so malformed report data cannot manufacture an arbitrary section
/// incidence.
fn mean_induced_angle_deg(cl: f64, aspect_ratio: f64, oswald_e: f64) -> f64 {
    let denominator = std::f64::consts::PI * aspect_ratio * oswald_e;
    if !cl.is_finite() || !denominator.is_finite() || denominator <= 1e-9 {
        return 0.0;
    }
    (cl / denominator).atan().to_degrees()
}

/// Controls which stages of the pipeline to run and execution parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipelineOptions {
    /// Whether to execute Stage 1 (Design space optimizer).
    pub optimize: bool,
    /// Whether to run full high-fidelity analysis on the baseline as well.
    pub compare_baseline: bool,
    /// Whether to run independent downstream stages concurrently.
    pub parallel: bool,
    /// Which aerodynamic result family the caller wants to expose.
    #[serde(default)]
    pub aerodynamic_solver: AerodynamicSolverMode,
    /// Which solver is allowed to produce an alternative optimized design.
    #[serde(default)]
    pub optimization_solver: OptimizationSolverMode,
    /// Target directory for the canonical CPACS aircraft and compatibility artifacts.
    pub output_dir: Option<PathBuf>,
    /// Whether to generate and save comparison plots.
    pub save_plots: bool,
    /// Optional random seed for reproducible optimization.
    pub seed: Option<u64>,
    /// Request suppression of verbose application progress outputs.
    ///
    /// The pipeline library does not print; the CLI consumes this flag for its
    /// own summaries; the execution status records the selected value.
    pub quiet: bool,
}

impl Default for PipelineOptions {
    fn default() -> Self {
        Self {
            optimize: true,
            compare_baseline: true,
            parallel: true,
            aerodynamic_solver: AerodynamicSolverMode::Both,
            optimization_solver: OptimizationSolverMode::Vlm,
            output_dir: Some(PathBuf::from("outputs")),
            save_plots: false,
            seed: None,
            quiet: false,
        }
    }
}

/// Comprehensive outcome of all stages executed in a pipeline run.
#[derive(Debug, Clone, PartialEq)]
pub struct PipelineResult {
    /// Configuration evaluated.
    pub config: AlasConfig,
    /// Winning design vector produced by optimization (or baseline design if unoptimized).
    pub optimized_design: Option<DesignVector>,
    /// Full aerodynamic and mass report for the optimized design.
    pub optimized_report: Option<AnalysisReport>,
    /// Optimization convergence history and metrics.
    pub optimization_result: Option<OptimizationResult>,
    /// Independently retained VLM and AVL optimization branches.
    pub solver_optimizations: Option<SolverOptimizationSet>,
    /// Fast baseline weight & balance and stability estimation.
    pub baseline_report: Option<BaselineReport>,
    /// Full aerodynamic and mass report for the baseline design.
    pub baseline_analysis: Option<AnalysisReport>,
    /// Failure detail when the optional full baseline analysis could not be
    /// constructed.  A missing baseline report is therefore distinguishable
    /// from a caller that did not request baseline comparison.
    pub baseline_analysis_error: Option<String>,
    /// Lateral airway or great-circle route flown.
    pub route: Option<Route>,
    /// Observable dispatch-tier outcome and the route source ultimately used.
    pub route_status: Option<RoutePlanningStatus>,
    /// Flown mission telemetry, when a mission stage supplied one.
    pub mission_result: Option<MissionResult>,
    /// Explicit conservation-law and configured-limit failures for this run.
    pub feasibility: FeasibilityReport,
    /// MSES 2-D polar sweep results for the root section.
    pub mses_result: Option<MsesPolarResult>,
    /// MSES 2-D surface pressure and flowfield results.
    pub mses_pressure: Option<MsesPressureResult>,
    /// Wingbox structural sizing, mesh health, analytical and NASTRAN results.
    pub structural_result: Option<StructuralAnalysisResult>,
    /// Serialized design database written to disk.
    pub design_database: Option<DesignDatabase>,
    /// OpenVSP input script and its explicit runtime-evidence status.
    pub openvsp_export: Option<OpenVspExportResult>,
    /// CPACS 3.5 aircraft-data artifact.
    pub cpacs_export: Option<CpacsExportResult>,
    /// CPACS-centered manifest linking all retained downstream artifacts.
    pub cpacs_manifest: Option<PathBuf>,
    /// Independent VSPAERO lifting-surface solve and comparison status.
    pub vspaero_result: Option<VspaeroAnalysisResult>,
    /// Independent Athena Vortex Lattice solve and comparison status.
    pub avl_result: Option<AvlAnalysisResult>,
    /// Optional external FLOWUnsteady adapter result and comparison status.
    pub flowunsteady_result: Option<FlowUnsteadyAnalysisResult>,
    /// Which requested execution controls were applied to this run.
    pub execution: PipelineExecutionStatus,
}

/// Provenance of the route used by a mission-enabled pipeline run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutePlanningStatus {
    /// Source selected after all configured routing tiers were considered.
    pub selected_source: RouteSource,
    /// Outcome of the optional live SimBrief tier.
    pub simbrief: SimbriefFetchStatus,
}

type MissionStageOutputs = (
    Option<Route>,
    Option<RoutePlanningStatus>,
    Option<MissionResult>,
);

#[derive(Debug)]
struct PlannedRoute {
    route: Route,
    status: RoutePlanningStatus,
}

/// Runtime status for controls that affect orchestration rather than physics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineExecutionStatus {
    /// Whether the caller requested concurrent downstream stages.
    pub parallel_requested: bool,
    /// Whether downstream stages actually ran concurrently.
    pub parallel_effective: bool,
    /// Aerodynamic result family selected by the caller.
    pub aerodynamic_solver: AerodynamicSolverMode,
    /// Optimization backend selected by the caller.
    pub optimization_solver: OptimizationSolverMode,
    /// Seed requested by the caller, if any.
    pub seed_requested: Option<u64>,
    /// Whether that seed was applied to the optimizer configuration.
    pub seed_applied: bool,
    /// Whether the caller requested quiet output.
    pub quiet_requested: bool,
}

/// Master pipeline coordinator.
#[derive(Debug, Clone)]
pub struct DesignPipeline {
    /// Active configuration.
    pub config: AlasConfig,
    /// Optional aircraft geometry imported from a CPACS 3.5 document.
    ///
    /// When present, the pipeline evaluates this geometry without allowing
    /// the design-variable optimizer to replace it with a configuration-built
    /// aircraft. The public constructor keeps the existing configuration
    /// workflow unchanged.
    aircraft_override: Option<Airplane>,
}

impl DesignPipeline {
    /// Create a new design pipeline with `config`.
    pub fn new(config: AlasConfig) -> Self {
        Self {
            config,
            aircraft_override: None,
        }
    }

    /// Create a pipeline whose aircraft geometry was imported from a generic
    /// aircraft-data source.
    ///
    /// The configuration continues to supply requirements and non-geometric
    /// physics inputs. The imported CPACS aircraft remains authoritative for
    /// the geometry passed to the existing analysis formulas.
    pub fn new_with_airplane(config: AlasConfig, airplane: Airplane) -> Self {
        Self {
            config,
            aircraft_override: Some(airplane),
        }
    }

    /// Create a pipeline from a validated CPACS 3.5 document.
    ///
    /// In addition to reconstructing the native geometry, this imports the
    /// standard engine geometry and take-off cycle values that the existing
    /// mass and mission formulas already consume.
    pub fn new_with_cpacs_document(
        mut config: AlasConfig,
        document: CpacsDocument,
    ) -> Result<Self, String> {
        let airplane = document
            .to_airplane()
            .map_err(|error| format!("CPACS geometry conversion failed: {error}"))?;
        document
            .apply_engine_data_to_config(&mut config, &airplane)
            .map_err(|error| format!("CPACS engine conversion failed: {error}"))?;
        Ok(Self::new_with_airplane(config, airplane))
    }

    /// Execute the full end-to-end design and analysis pipeline.
    pub fn run(
        &self,
        options: &PipelineOptions,
        environment: &RunEnvironment,
    ) -> Result<PipelineResult, String> {
        self.run_inner(options, environment, None, None, None, None, None, None)
    }

    /// Execute a run using the one resolved external-tool environment shared
    /// by the command line and desktop application.
    pub fn run_with_environment(
        &self,
        options: &PipelineOptions,
        environment: &RunEnvironment,
    ) -> Result<PipelineResult, String> {
        self.run(options, environment)
    }

    /// Execute a run with an optional already-fetched dispatch route.
    ///
    /// The application owns HTTPS transport; this method is the input seam
    /// that lets it hand the parsed SimBrief route into the same public path
    /// as imported, airway and great-circle routing.
    pub fn run_with_environment_and_route(
        &self,
        options: &PipelineOptions,
        environment: &RunEnvironment,
        dispatched_route: Option<Route>,
    ) -> Result<PipelineResult, String> {
        self.run_inner(
            options,
            environment,
            dispatched_route,
            None,
            None,
            None,
            None,
            None,
        )
    }

    /// Execute a desktop run at the design point and bounds currently shown
    /// to the user.
    ///
    /// The configuration tree does not own these values: the design-space
    /// editor does. Passing them explicitly keeps the preview, baseline and
    /// optimizer on the same aircraft.
    pub fn run_with_design_space(
        &self,
        options: &PipelineOptions,
        environment: &RunEnvironment,
        initial_design: &DesignVector,
        bounds: &[(f64, f64)],
    ) -> Result<PipelineResult, String> {
        validate_bounds(bounds)?;
        self.run_inner(
            options,
            environment,
            None,
            Some(*initial_design),
            Some(bounds),
            None,
            None,
            None,
        )
    }

    /// Execute a desktop-style design-space run while reporting stage and
    /// downstream-component progress. The callback is deliberately
    /// synchronous and typed: callers can forward it across their own worker
    /// boundary without the pipeline depending on a GUI or logging
    /// implementation.
    pub fn run_with_design_space_and_progress(
        &self,
        options: &PipelineOptions,
        environment: &RunEnvironment,
        initial_design: &DesignVector,
        bounds: &[(f64, f64)],
        progress: &(dyn Fn(&str) + Sync),
    ) -> Result<PipelineResult, String> {
        validate_bounds(bounds)?;
        self.run_inner(
            options,
            environment,
            None,
            Some(*initial_design),
            Some(bounds),
            Some(progress),
            None,
            None,
        )
    }

    /// Execute a design-space run with typed lifecycle events and cooperative
    /// cancellation. Cancellation is observed only at safe stage boundaries;
    /// an active external process is allowed to finish its supervised call.
    pub fn run_with_design_space_events(
        &self,
        options: &PipelineOptions,
        environment: &RunEnvironment,
        initial_design: &DesignVector,
        bounds: &[(f64, f64)],
        events: &(dyn Fn(RunEvent) + Sync),
        cancel: &AtomicBool,
    ) -> Result<PipelineResult, String> {
        validate_bounds(bounds)?;
        self.run_inner(
            options,
            environment,
            None,
            Some(*initial_design),
            Some(bounds),
            None,
            Some(events),
            Some(cancel),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn run_inner(
        &self,
        options: &PipelineOptions,
        environment: &RunEnvironment,
        dispatched_route: Option<Route>,
        initial_design: Option<DesignVector>,
        bounds: Option<&[(f64, f64)]>,
        progress: Option<&(dyn Fn(&str) + Sync)>,
        events: Option<&(dyn Fn(RunEvent) + Sync)>,
        cancel: Option<&AtomicBool>,
    ) -> Result<PipelineResult, String> {
        let run_clock = Instant::now();
        let report = |message: &str| {
            if let Some(callback) = progress {
                callback(message);
            }
        };
        report("Validating run configuration");
        check_cancelled(cancel)?;
        validate_run_configuration(&self.config)?;
        if self.aircraft_override.is_some() && options.optimize {
            return Err(
                "CPACS-backed runs currently require --no-optimize; design variables cannot replace imported geometry"
                    .to_owned(),
            );
        }
        if self.aircraft_override.is_some() && options.compare_baseline {
            return Err(
                "CPACS-backed runs currently require --no-baseline; the baseline builder is configuration-based"
                    .to_owned(),
            );
        }
        // `output_dir` controls retention, not whether the requested physics
        // stages run. When retention is disabled, run every writer and
        // external adapter in an isolated temporary workspace instead of
        // using `None` as a stage-disable signal.
        let analysis_dir = options.output_dir.clone().unwrap_or_else(|| {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default();
            std::env::temp_dir().join(format!("alas-analysis-{}-{nonce}", std::process::id()))
        });
        std::fs::create_dir_all(&analysis_dir)
            .map_err(|error| format!("analysis workspace creation failed: {error}"))?;
        report("Analysis workspace ready");
        emit_diagnostic(events, run_clock, "setup", "Analysis workspace ready");
        let nominal_design = initial_design.unwrap_or_else(|| self.configured_nominal_design());

        // Stage 0: Baseline W&B + stability estimation.
        report("Stage 1/7: baseline weight, balance, and stability");
        let mut stage_clock = begin_stage(
            events,
            run_clock,
            1,
            "baseline",
            "Baseline weight, balance, and stability",
        );
        let baseline_report = self
            .aircraft_override
            .is_none()
            .then(|| analyze_baseline(&self.config, &nominal_design));
        finish_stage(events, run_clock, stage_clock, 1, "baseline");
        check_cancelled(cancel)?;

        // Stage 1: Design space optimization.
        report(if options.optimize {
            "Stage 2/7: design-space optimization"
        } else {
            "Stage 2/7: optimization skipped"
        });
        stage_clock = begin_stage(
            events,
            run_clock,
            2,
            "optimization",
            if options.optimize {
                "Design-space optimization"
            } else {
                "Optimization skipped"
            },
        );
        let solver_optimizations = if options.optimize {
            Some(run_solver_optimizations(
                &self.config,
                options.optimization_solver,
                options.parallel,
                options.seed,
                environment,
                &nominal_design,
                bounds,
                Some(analysis_dir.as_path()),
            ))
        } else {
            None
        };
        let (optimized_design, optimization_result, branch_report) =
            if let Some(solutions) = solver_optimizations.as_ref() {
                let selected = solutions.selected(options.optimization_solver)?;
                let design = selected
                    .design
                    .ok_or_else(|| "selected optimizer returned no design".to_owned())?;
                (
                    design,
                    selected.optimization.clone(),
                    selected.report.clone(),
                )
            } else {
                (nominal_design, None, None)
            };
        finish_stage(events, run_clock, stage_clock, 2, "optimization");
        check_cancelled(cancel)?;

        // Stage 2: Full analysis on optimized design.
        report("Stage 3/7: full aircraft analysis");
        stage_clock = begin_stage(
            events,
            run_clock,
            3,
            "full_analysis",
            "Full aircraft analysis",
        );
        let full = if self.aircraft_override.is_some() {
            FullAnalysis::new_preserving_engine_config(self.config.clone())
        } else {
            FullAnalysis::new(self.config.clone())
        };
        let mut optimized_report = match branch_report {
            Some(report) => report,
            None => match self.aircraft_override.as_ref() {
                Some(airplane) => full.run_on_airplane(&optimized_design, airplane.clone())?,
                None => full.run(&optimized_design, true)?,
            },
        };
        finish_stage(events, run_clock, stage_clock, 3, "full_analysis");
        check_cancelled(cancel)?;

        // Stage 3: CPACS geometry export and canonicalization.
        //
        // Downstream writers consume the typed aircraft reconstructed from
        // this document, so CPACS is the non-GUI geometry boundary rather
        // than a sidecar copy of the configuration-built geometry.
        report("Stage 4/7: CPACS export and geometry canonicalization");
        stage_clock = begin_stage(
            events,
            run_clock,
            4,
            "geometry_export",
            "CPACS export and geometry canonicalization",
        );
        let cpacs_path = analysis_dir.join("cpacs/optimized_aircraft.cpacs.xml");
        let cpacs_export = Some(
            export_cpacs(&optimized_report, &self.config, &cpacs_path)
                .map_err(|error| format!("CPACS export failed: {error}"))?,
        );
        if let Some(export) = cpacs_export.as_ref() {
            let document = read_cpacs_file(&export.path)
                .map_err(|error| format!("CPACS canonicalization read failed: {error}"))?;
            optimized_report.airplane = document
                .to_airplane()
                .map_err(|error| format!("CPACS canonicalization failed: {error}"))?;
        }
        finish_stage(events, run_clock, stage_clock, 4, "geometry_export");
        check_cancelled(cancel)?;

        // Native tool writers receive the CPACS-canonicalized report.
        let openvsp_export = {
            let af_path = analysis_dir.join("airfoils/optimized_root.dat");
            let openvsp_path = analysis_dir.join("openvsp/optimized_aircraft.vspscript");
            let _ = export_airfoil_dat(&optimized_report, &self.config, &af_path, "ALAS_Optimized");
            match export_openvsp_script(&optimized_report, &self.config, &openvsp_path) {
                Ok(export) => Some(match environment.openvsp_exe.as_deref() {
                    Some(executable) => materialize_openvsp_project(export, executable, 120.0),
                    None => export,
                }),
                Err(error) => {
                    tracing::warn!(%error, "OpenVSP geometry script export failed");
                    None
                }
            }
        };
        let avl_requested = matches!(
            options.aerodynamic_solver,
            AerodynamicSolverMode::Avl | AerodynamicSolverMode::Both
        );
        let run_vspaero = || {
            let stage_clock =
                begin_component(events, run_clock, "downstream/vspaero", "VSPAERO analysis");
            let result = openvsp_export.as_ref().map(|openvsp| {
                run_vspaero_analysis(
                    &optimized_report,
                    &self.config,
                    openvsp,
                    environment.vspaero_exe.as_deref(),
                    // The retained OpenVSP mesh is substantially larger than
                    // the small smoke cases used by the executor tests.  A
                    // five-minute wall clock cut the installed A380-like
                    // case off halfway through its 15-point sweep; keep the
                    // process-tree timeout, but allow one full legacy sweep
                    // to finish when the solver is available.
                    900.0,
                )
            });
            finish_component(
                events,
                run_clock,
                stage_clock,
                "downstream/vspaero",
                if result.is_some() {
                    "Completed"
                } else {
                    "Skipped"
                },
            );
            result
        };
        let run_avl = || {
            let stage_clock = begin_component(
                events,
                run_clock,
                "downstream/avl",
                "AVL take-off comparison",
            );
            if !avl_requested {
                finish_component(events, run_clock, stage_clock, "downstream/avl", "Skipped");
                return None;
            }
            let result = Some(run_avl_takeoff_comparison(
                &optimized_report,
                &self.config,
                &analysis_dir,
                environment.avl_exe.as_deref(),
                300.0,
            ));
            finish_component(
                events,
                run_clock,
                stage_clock,
                "downstream/avl",
                "Completed",
            );
            result
        };
        let run_flowunsteady = || {
            let stage_clock = begin_component(
                events,
                run_clock,
                "downstream/flowunsteady",
                "FLOWUnsteady analysis",
            );
            let result = Some(run_flowunsteady_analysis(
                &optimized_report,
                &self.config,
                &analysis_dir,
                environment.flowunsteady_exe.as_deref(),
                900.0,
            ));
            finish_component(
                events,
                run_clock,
                stage_clock,
                "downstream/flowunsteady",
                "Completed",
            );
            result
        };
        let run_baseline_analysis = || -> (Option<AnalysisReport>, Option<String>) {
            let stage_clock = begin_component(
                events,
                run_clock,
                "downstream/baseline_analysis",
                "Baseline comparison",
            );
            if !options.compare_baseline {
                finish_component(
                    events,
                    run_clock,
                    stage_clock,
                    "downstream/baseline_analysis",
                    "Skipped",
                );
                (None, None)
            } else if !options.optimize && self.aircraft_override.is_none() {
                finish_component(
                    events,
                    run_clock,
                    stage_clock,
                    "downstream/baseline_analysis",
                    "Completed",
                );
                (Some(optimized_report.clone()), None)
            } else {
                let result = match full.run(&nominal_design, true) {
                    Ok(report) => (Some(report), None),
                    Err(error) => (None, Some(error)),
                };
                finish_component(
                    events,
                    run_clock,
                    stage_clock,
                    "downstream/baseline_analysis",
                    "Completed",
                );
                result
            }
        };
        let run_mission = || {
            let stage_clock = begin_component(
                events,
                run_clock,
                "downstream/mission",
                "Mission and route analysis",
            );
            let result = self.evaluate_active_mission(&optimized_report, dispatched_route);
            finish_component(
                events,
                run_clock,
                stage_clock,
                "downstream/mission",
                if self.config.mission.enabled {
                    "Completed"
                } else {
                    "Skipped"
                },
            );
            result
        };
        let run_mses = || {
            let stage_clock =
                begin_component(events, run_clock, "downstream/mses", "MSES analysis");
            let result = self.run_mses_stage(&optimized_report, environment.mses_dir.as_deref());
            finish_component(
                events,
                run_clock,
                stage_clock,
                "downstream/mses",
                if self.config.mses.enabled {
                    "Completed"
                } else {
                    "Skipped"
                },
            );
            result
        };
        let run_structural = || {
            let stage_clock = begin_component(
                events,
                run_clock,
                "downstream/structural",
                "Structural sizing and analysis",
            );
            if self.config.structures.enabled {
                let work_dir = Some(analysis_dir.join("structures"));
                let result = Some(
                    crate::structural::run_structural_analysis_with_environment_events(
                        &self.config,
                        &optimized_report,
                        work_dir.as_deref(),
                        environment,
                        events,
                        run_clock,
                    ),
                );
                finish_component(
                    events,
                    run_clock,
                    stage_clock,
                    "downstream/structural",
                    "Completed",
                );
                result
            } else {
                finish_component(
                    events,
                    run_clock,
                    stage_clock,
                    "downstream/structural",
                    "Skipped",
                );
                None
            }
        };
        report(if options.parallel {
            "Stage 5/7: downstream analyses (parallel)"
        } else {
            "Stage 5/7: downstream analyses (sequential)"
        });
        stage_clock = begin_stage(
            events,
            run_clock,
            5,
            "downstream",
            if options.parallel {
                "Downstream analyses (parallel)"
            } else {
                "Downstream analyses (sequential)"
            },
        );
        let (
            vspaero_result,
            avl_result,
            flowunsteady_result,
            baseline_analysis,
            baseline_analysis_error,
            mission_outputs,
            mses_outputs,
            structural_result,
            parallel_effective,
        ) = if options.parallel {
            std::thread::scope(|scope| {
                let vspaero = scope.spawn(run_vspaero);
                let avl = scope.spawn(run_avl);
                let flowunsteady = scope.spawn(run_flowunsteady);
                let baseline = scope.spawn(run_baseline_analysis);
                let mission = scope.spawn(run_mission);
                let mses = scope.spawn(run_mses);
                let structural = scope.spawn(run_structural);
                let vspaero_result = vspaero
                    .join()
                    .map_err(|_| "VSPAERO worker panicked".to_owned())?;
                let avl_result = avl.join().map_err(|_| "AVL worker panicked".to_owned())?;
                let flowunsteady_result = flowunsteady
                    .join()
                    .map_err(|_| "FLOWUnsteady worker panicked".to_owned())?;
                let (baseline_analysis, baseline_analysis_error) = baseline
                    .join()
                    .map_err(|_| "baseline-analysis worker panicked".to_owned())?;
                let mission_outputs = mission
                    .join()
                    .map_err(|_| "mission worker panicked".to_owned())??;
                let mses_outputs = mses.join().map_err(|_| "MSES worker panicked".to_owned())?;
                let structural_result = structural
                    .join()
                    .map_err(|_| "structures worker panicked".to_owned())?;
                Ok::<_, String>((
                    vspaero_result,
                    avl_result,
                    flowunsteady_result,
                    baseline_analysis,
                    baseline_analysis_error,
                    mission_outputs,
                    mses_outputs,
                    structural_result,
                    true,
                ))
            })?
        } else {
            let (baseline_analysis, baseline_analysis_error) = run_baseline_analysis();
            (
                run_vspaero(),
                run_avl(),
                run_flowunsteady(),
                baseline_analysis,
                baseline_analysis_error,
                run_mission()?,
                run_mses(),
                run_structural(),
                false,
            )
        };
        let (route, route_status, mission_result) = mission_outputs;
        let (mses_result, mses_pressure) = mses_outputs;
        finish_stage(events, run_clock, stage_clock, 5, "downstream");
        emit_tool_diagnostics(
            events,
            run_clock,
            environment,
            openvsp_export.as_ref(),
            vspaero_result.as_ref(),
            avl_result.as_ref(),
            flowunsteady_result.as_ref(),
            structural_result.as_ref(),
            mses_result.as_ref(),
        );
        check_cancelled(cancel)?;
        report("Stage 6/7: physical feasibility assessment");
        stage_clock = begin_stage(
            events,
            run_clock,
            6,
            "feasibility",
            "Physical feasibility assessment",
        );
        let feasibility = assess_physical_feasibility(
            &self.config,
            &optimized_design,
            &optimized_report,
            mission_result.as_ref(),
        );
        finish_stage(events, run_clock, stage_clock, 6, "feasibility");
        check_cancelled(cancel)?;
        if let Some(export) = cpacs_export.as_ref() {
            export_cpacs_with_analysis(
                &optimized_report,
                &self.config,
                Some(&feasibility),
                mission_result.as_ref(),
                &export.path,
            )
            .map_err(|error| format!("CPACS analysis export failed: {error}"))?;
        }
        let cpacs_adapter_manifest =
            match cpacs_export.as_ref() {
                Some(export) => {
                    let data = CpacsAircraftData::from_report(&optimized_report, export);
                    let path = analysis_dir.join("cpacs/adapter_manifest.json");
                    Some(data.manifest().write_json(&path).map_err(|error| {
                        format!("CPACS adapter manifest export failed: {error}")
                    })?)
                }
                None => None,
            };
        let design_database = options.output_dir.as_ref().and_then(|out_dir| {
            let path = out_dir.join("design_database.json");
            let cpacs = cpacs_export.as_ref().map(|export| CpacsReference {
                path: export
                    .path
                    .strip_prefix(out_dir)
                    .unwrap_or(&export.path)
                    .display()
                    .to_string(),
                version: export.cpacs_version.clone(),
                aircraft_model_uid: export.aircraft_model_uid.clone(),
                engine_uid: export.engine_uid.clone(),
            });
            match cpacs.map_or_else(
                || {
                    crate::export::export_json_with_feasibility(
                        &optimized_report,
                        &self.config,
                        &feasibility,
                        &path,
                    )
                },
                |cpacs| {
                    export_json_with_feasibility_and_cpacs(
                        &optimized_report,
                        &self.config,
                        &feasibility,
                        cpacs,
                        &path,
                    )
                },
            ) {
                Ok(database) => Some(database),
                Err(error) => {
                    tracing::warn!(%error, "design database export failed");
                    None
                }
            }
        });
        let payload_layout_artifact = options.output_dir.as_ref().and_then(|out_dir| {
            let layout = optimized_report.payload_layout.as_ref()?;
            let path = out_dir.join("payload_layout.json");
            match export_payload_layout_artifact(&self.config, optimized_design, layout, &path) {
                Ok(_) => Some(path),
                Err(error) => {
                    tracing::warn!(%error, "payload-layout render artifact export failed");
                    None
                }
            }
        });
        let cabin_scene_artifact = options.output_dir.as_ref().and_then(|out_dir| {
            let path = out_dir.join("cabin_scene_v2.json");
            match export_cabin_scene(&self.config, &optimized_report, &path) {
                Ok(_) => Some(path),
                Err(error) => {
                    tracing::warn!(%error, "cabin-scene v2 export failed");
                    None
                }
            }
        });

        if let (Some(out_dir), Some(polar)) = (options.output_dir.as_deref(), &mses_result) {
            if let Err(error) = persist_mses_polar_diagnostics(polar, out_dir) {
                tracing::warn!(%error, "MSES polar-diagnostic retention failed");
            }
        }
        if let (Some(out_dir), Some(pressure)) = (options.output_dir.as_deref(), &mses_pressure) {
            if let Err(error) = persist_mses_raw_exports(pressure, out_dir) {
                tracing::warn!(%error, "MSES raw-output retention failed");
            }
        }

        report("Stage 7/7: finalizing artifacts and run manifest");
        stage_clock = begin_stage(
            events,
            run_clock,
            7,
            "finalization",
            "Finalizing artifacts and run manifest",
        );
        let cpacs_manifest = match (options.output_dir.as_ref(), cpacs_export.as_ref()) {
            (Some(out_dir), Some(export)) => {
                let mut stages = BTreeMap::new();
                stages.insert("cpacs_export".to_owned(), "completed".to_owned());
                if cpacs_adapter_manifest.is_some() {
                    stages.insert("cpacs_adapter_manifest".to_owned(), "completed".to_owned());
                }
                if let Some(result) = &avl_result {
                    stages.insert("avl".to_owned(), result.status.as_str().to_owned());
                }
                if let Some(result) = &vspaero_result {
                    stages.insert("vspaero".to_owned(), result.status.as_str().to_owned());
                }
                if let Some(result) = &flowunsteady_result {
                    stages.insert("flowunsteady".to_owned(), result.status.as_str().to_owned());
                }
                let mut artifacts = BTreeMap::new();
                add_manifest_artifact(&mut artifacts, out_dir, "cpacs_input", &export.path);
                if let Some(path) = cpacs_adapter_manifest.as_ref() {
                    add_manifest_artifact(&mut artifacts, out_dir, "cpacs_adapter_manifest", path);
                }
                if let Some(path) = payload_layout_artifact.as_ref() {
                    add_manifest_artifact(&mut artifacts, out_dir, "payload_layout", path);
                }
                if let Some(path) = cabin_scene_artifact.as_ref() {
                    add_manifest_artifact(&mut artifacts, out_dir, "cabin_scene_v2", path);
                }
                let manifest_path = out_dir.join("cpacs/run_manifest.json");
                Some(
                    write_cpacs_run_manifest(&manifest_path, export, stages, artifacts)
                        .map_err(|error| format!("CPACS run manifest export failed: {error}"))?,
                )
            }
            _ => None,
        };

        finish_stage(events, run_clock, stage_clock, 7, "finalization");
        check_cancelled(cancel)?;
        Ok(PipelineResult {
            config: self.config.clone(),
            optimized_design: Some(optimized_design),
            optimized_report: Some(optimized_report),
            optimization_result,
            solver_optimizations,
            baseline_report,
            baseline_analysis,
            baseline_analysis_error,
            route,
            route_status,
            mission_result,
            feasibility,
            mses_result,
            mses_pressure,
            structural_result,
            design_database,
            openvsp_export,
            cpacs_export,
            cpacs_manifest,
            vspaero_result,
            avl_result,
            flowunsteady_result,
            execution: PipelineExecutionStatus {
                parallel_requested: options.parallel,
                parallel_effective,
                aerodynamic_solver: options.aerodynamic_solver,
                optimization_solver: options.optimization_solver,
                seed_requested: options.seed,
                seed_applied: options.optimize && options.seed.is_some(),
                quiet_requested: options.quiet,
            },
        })
    }

    fn configured_nominal_design(&self) -> DesignVector {
        if self.config.preset.is_empty() {
            return DesignVector::default();
        }
        match presets::get(&self.config.preset) {
            Ok(preset) => preset.design_vector,
            Err(error) => {
                tracing::warn!(%error, "configured preset has no registered design vector");
                DesignVector::default()
            }
        }
    }

    fn evaluate_active_mission(
        &self,
        report: &AnalysisReport,
        dispatched_route: Option<Route>,
    ) -> Result<MissionStageOutputs, String> {
        if !self.config.mission.enabled {
            return Ok((None, None, None));
        }
        let planned = self
            .plan_active_route(dispatched_route)
            .ok_or_else(|| "mission is enabled but route planning produced no route".to_owned())?;
        let selected_origin = get_airport(&self.config.departure_airport)
            .map_err(|error| format!("mission departure airport could not be resolved: {error}"))?;
        let selected_destination = get_airport(&self.config.arrival_airport)
            .map_err(|error| format!("mission arrival airport could not be resolved: {error}"))?;
        let origin = planned
            .route
            .origin_airport
            .as_ref()
            .unwrap_or(selected_origin);
        let destination = planned
            .route
            .dest_airport
            .as_ref()
            .unwrap_or(selected_destination);
        let mission = mission_stage::evaluate(
            &self.config,
            report,
            origin,
            destination,
            planned.route.total_distance_m(),
        )
        .map_err(|error| format!("native mission stage failed: {error}"))?;
        Ok((Some(planned.route), Some(planned.status), Some(mission)))
    }

    fn plan_active_route(&self, dispatched_route: Option<Route>) -> Option<PlannedRoute> {
        let origin = get_airport(&self.config.departure_airport).ok()?;
        let dest = get_airport(&self.config.arrival_airport).ok()?;
        let (dispatched_route, simbrief) = if let Some(route) = dispatched_route {
            (Some(route), SimbriefFetchStatus::SuppliedByCaller)
        } else {
            let outcome = fetch_route_with_status(
                &SystemCurlTransport,
                &self.config.mission.simbrief_username,
                origin,
                dest,
                self.config.mission.simbrief_timeout_s,
                self.config.mission.simbrief_overrides_airports,
            );
            (outcome.route, outcome.status)
        };
        let locator = ToolLocator::for_current_process();
        let routes_dir = locator.resolve_data_path(Path::new(&self.config.mission.routes_dir));
        let navdata_dir = locator.resolve_data_path(Path::new(&self.config.mission.navdata_dir));
        let navdata = load_navdata(&navdata_dir);
        let sources = RouteSources {
            dispatched: dispatched_route,
            routes_dir: Some(routes_dir.as_path()),
            navdata: navdata.as_ref(),
            great_circle_points: self.config.mission.great_circle_points.max(1) as usize,
        };
        let route = plan_route(origin, dest, sources);
        Some(PlannedRoute {
            status: RoutePlanningStatus {
                selected_source: route.source,
                simbrief,
            },
            route,
        })
    }

    fn run_mses_stage(
        &self,
        report: &AnalysisReport,
        mses_dir: Option<&Path>,
    ) -> (Option<MsesPolarResult>, Option<MsesPressureResult>) {
        if !self.config.mses.enabled {
            return (None, None);
        }
        let condition = mses_section_condition(&self.config, report);
        let dir = match mses_dir {
            Some(d) => d,
            None => {
                let error = "MSES executables not configured (Setup > External Tools)".to_owned();
                let airfoil_name = report
                    .airplane
                    .wings
                    .first()
                    .and_then(|wing| wing.xsecs.first())
                    .map(|section| section.airfoil.name.clone())
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| "optimized_root_section".to_owned());
                return (
                    Some(MsesPolarResult {
                        status: alas_aero::mses::MsesStatus::Absent,
                        error: Some(error.clone()),
                        airfoil_name,
                        mach: condition.mach,
                        reynolds: condition.reynolds,
                        ..MsesPolarResult::default()
                    }),
                    Some(MsesPressureResult {
                        status: alas_aero::mses::MsesStatus::Absent,
                        error: Some(error),
                        alpha_deg: condition.alpha_deg,
                        ..MsesPressureResult::default()
                    }),
                );
            }
        };
        let wing = match report.airplane.wings.first() {
            Some(w) => w,
            None => return (None, None),
        };
        let root_airfoil = match wing.xsecs.first() {
            Some(x) => &x.airfoil,
            None => return (None, None),
        };

        let polar = run_mses_polar(
            root_airfoil,
            condition.mach,
            condition.reynolds,
            condition.alpha_deg,
            &self.config.mses,
            dir,
        );
        let pressure = run_mses_pressure_distribution(
            root_airfoil,
            condition.mach,
            condition.reynolds,
            condition.alpha_deg,
            &self.config.mses,
            dir,
            None,
        );

        (Some(polar), Some(pressure))
    }

    /// Format summary of the primary analysis for reporting.
    pub fn summary(&self, result: &PipelineResult) -> String {
        match result.optimized_report {
            Some(ref rep) => format!(
                "{}\n{}",
                format_summary(rep, Some(&self.config)),
                format_feasibility(&result.feasibility)
            ),
            None => "No analysis report available.".to_owned(),
        }
    }
}

/// Enforce the blocking cross-field configuration contract at the public
/// execution boundary. The GUI performs the same check for button state, but
/// library and CLI callers must receive it even when they bypass that UI.
fn validate_run_configuration(config: &AlasConfig) -> Result<(), String> {
    if !config.preset.is_empty() {
        presets::get(&config.preset).map_err(|error| {
            format!(
                "configuration preset identity is not registered: {error}; clear the preset field or select a registered aircraft preset"
            )
        })?;
    }
    let errors = alas_config::validate(config)
        .into_iter()
        .filter(|issue| issue.severity == Severity::Error)
        .map(|issue| format!("{}: {}", issue.field_path, issue.message))
        .collect::<Vec<_>>();
    if errors.is_empty() {
        return Ok(());
    }
    Err(format!(
        "configuration validation failed:\n- {}",
        errors.join("\n- ")
    ))
}

#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;
