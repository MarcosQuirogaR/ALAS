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
    run_mses_polar_with_cancel, run_mses_pressure_distribution_with_checkpoint_and_cancel,
    MsesPolarResult, MsesPressureResult,
};
use alas_config::airports::get as get_airport;
use alas_config::design_variables::DesignVector;
use alas_config::presets;
use alas_config::{AlasConfig, Severity};
use alas_exec::storage::{mark_storage_root, StorageCategoryId};
use alas_exec::{RunEnvironment, ToolLocator};
use alas_geom::aircraft::airplane::Airplane;
use alas_mission::MissionResult;
use alas_opt::OptimizationResult;
use alas_route::planner::{
    load_navdata_with_airway_coordinates, plan_route_with_max_stretch, RouteSources,
};
use alas_route::route::{Route, RouteSource};
use alas_route::{fetch_route_with_status, SimbriefFetchStatus};
use serde::{Deserialize, Serialize};

use crate::acceptance::AcceptanceRoute;
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
use crate::feasibility::{
    assess_physical_feasibility_with_load_case, format_feasibility, FeasibilityReport,
};
use crate::flowunsteady::{run_flowunsteady_analysis, FlowUnsteadyAnalysisResult};
use crate::full_analysis::{AnalysisReport, FullAnalysis};
use crate::mission_stage::{self, SelectedLoadCase};
use crate::openvsp::{export_openvsp_script, materialize_openvsp_project, OpenVspExportResult};
use crate::payload_layout_export::export_payload_layout_artifact;
use crate::runs::{RunEvent, RunEventSeverity};
use crate::solver_mode::{AerodynamicSolverMode, OptimizationSolverMode};
use crate::structural::StructuralAnalysisResult;
use crate::vspaero::{run_vspaero_analysis, VspaeroAnalysisResult};

mod curl_transport;
use curl_transport::SystemCurlTransport;
mod events;
pub(crate) use events::{begin_component, finish_component};
use events::{
    begin_stage, emit_diagnostic, emit_diagnostic_with_severity, emit_tool_diagnostics,
    finish_stage, warn_artifact_failure,
};
mod helpers;
mod snapshots;
use helpers::{
    add_manifest_artifact, add_manifest_artifact_if_exists, check_preset_policy,
    persist_mses_polar_diagnostics, persist_mses_raw_exports, validate_bounds,
};
use snapshots::SnapshotPublisher;

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

fn check_cancelled(cancel: Option<&AtomicBool>) -> Result<(), String> {
    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        Err("Cancelled safely at a pipeline stage boundary".to_owned())
    } else {
        Ok(())
    }
}

/// Create and, for a retained output directory, claim this run's analysis
/// workspace: `output_dir` when the caller wants results kept, otherwise a
/// process/nonce-unique directory under the system temporary directory.
///
/// The claim is made here, at creation, rather than left to be inferred from
/// whatever a writer produces later: a run cancelled immediately after setup
/// would otherwise leave a directory Manage Storage cannot recognize as its
/// own. The temporary fallback is solver scratch, already recognized by its
/// `alas-analysis-` prefix ([`alas_exec::storage::SCRATCH_PREFIXES`]); it is
/// not a configured, retained root and must not carry the same claim a
/// user-set output directory gets.
fn prepare_analysis_workspace(output_dir: Option<PathBuf>) -> Result<PathBuf, String> {
    let analysis_dir = output_dir.clone().unwrap_or_else(|| {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("alas-analysis-{}-{nonce}", std::process::id()))
    });
    std::fs::create_dir_all(&analysis_dir)
        .map_err(|error| format!("analysis workspace creation failed: {error}"))?;
    if output_dir.is_some() {
        // Without the claim the run's results are still valid; only Manage
        // Storage loses the ability to recognize and reclaim the directory.
        if let Err(error) = mark_storage_root(StorageCategoryId::GeneratedOutputs, &analysis_dir) {
            tracing::warn!(%error, path = %analysis_dir.display(), "output directory could not be claimed for storage management");
        }
    }
    Ok(analysis_dir)
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
    /// The load case that telemetry was flown at, and how it was chosen.
    pub mission_load_case: Option<SelectedLoadCase>,
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
    Option<SelectedLoadCase>,
);

#[derive(Debug, Clone)]
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

impl PipelineExecutionStatus {
    fn for_options(options: &PipelineOptions, parallel_effective: bool) -> Self {
        Self {
            parallel_requested: options.parallel,
            parallel_effective,
            aerodynamic_solver: options.aerodynamic_solver,
            optimization_solver: options.optimization_solver,
            seed_requested: options.seed,
            seed_applied: options.optimize && options.seed.is_some(),
            quiet_requested: options.quiet,
        }
    }
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

/// The three independently optional observation seams a design-space run can
/// publish through, grouped so
/// [`DesignPipeline::run_with_design_space_events_and_snapshots`] keeps a
/// single logical "how do you want to watch this run" input rather than three
/// unrelated positional callbacks.
pub struct RunObservers<'a> {
    /// Typed per-stage lifecycle events, timers and diagnostics.
    pub events: &'a (dyn Fn(RunEvent) + Sync),
    /// Cumulative, immutable [`PipelineResult`] snapshots published as report
    /// data becomes available.
    pub snapshots: &'a (dyn Fn(PipelineResult) + Sync),
    /// Cooperative cancellation flag, observed at safe stage boundaries.
    pub cancel: &'a AtomicBool,
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
        self.run_inner(
            options,
            environment,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }

    /// Execute the same headless run as [`Self::run`] under a cooperative
    /// cancellation flag.
    ///
    /// This is the seam a headless supervisor (an acceptance harness with a
    /// wall-clock guard, a batch runner) needs: [`Self::run`] has no way to
    /// stop, so such a caller could only abandon its worker thread, which
    /// leaves the optimizer running unmonitored against whatever the
    /// supervisor does next. Setting `cancel` stops the run at the next stage
    /// boundary *and*, because the flag is threaded into the optimizer's own
    /// generation and poll loops, at the next boundary inside an active
    /// search; the call then returns an `Err` whose message begins
    /// `"Cancelled safely"`, so the worker can be joined rather than
    /// abandoned.
    ///
    /// Cancellation is cooperative and bounded, not immediate: the longest a
    /// set flag can go unobserved is one evaluation block of the active
    /// search, or one supervised external-tool call, whichever the run is
    /// inside.
    ///
    /// # Errors
    ///
    /// Same as [`Self::run`], plus the cancellation message above.
    pub fn run_cancellable(
        &self,
        options: &PipelineOptions,
        environment: &RunEnvironment,
        cancel: &AtomicBool,
    ) -> Result<PipelineResult, String> {
        self.run_inner(
            options,
            environment,
            None,
            None,
            None,
            None,
            None,
            Some(cancel),
            None,
        )
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

    /// Execute the same run as [`Self::run_with_environment`] while reporting
    /// typed lifecycle events.
    ///
    /// The event stream carries each stage's `elapsed_ms`/`duration_ms`, so a
    /// command-line run can record where its wall time went. The run itself
    /// is identical to [`Self::run_with_environment`]; the callback is the
    /// only added argument.
    pub fn run_with_environment_and_events(
        &self,
        options: &PipelineOptions,
        environment: &RunEnvironment,
        events: &(dyn Fn(RunEvent) + Sync),
    ) -> Result<PipelineResult, String> {
        self.run_inner(
            options,
            environment,
            None,
            None,
            None,
            None,
            Some(events),
            None,
            None,
        )
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
            None,
        )
    }

    /// Execute a desktop design-space run while publishing typed, immutable
    /// result snapshots as report data becomes available. The snapshots use
    /// the same [`PipelineResult`] shape as the final run, with downstream
    /// fields left `None` until their stages finish, so the GUI can render its
    /// ordinary Results gallery without inventing progress-only figures.
    /// Snapshots are cumulative and published in completion order, including
    /// during parallel downstream work. Their feasibility record is not yet
    /// assessed: only the successful return value is a completed run suitable
    /// for a feasibility verdict or final report export. The callback must
    /// enqueue promptly rather than render or block the analysis worker.
    pub fn run_with_design_space_events_and_snapshots(
        &self,
        options: &PipelineOptions,
        environment: &RunEnvironment,
        initial_design: &DesignVector,
        bounds: &[(f64, f64)],
        observers: RunObservers<'_>,
    ) -> Result<PipelineResult, String> {
        validate_bounds(bounds)?;
        self.run_inner(
            options,
            environment,
            None,
            Some(*initial_design),
            Some(bounds),
            None,
            Some(observers.events),
            Some(observers.cancel),
            Some(observers.snapshots),
        )
    }

    // Coordinated analysis inputs are kept explicit at this integration boundary.
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
        snapshots: Option<&(dyn Fn(PipelineResult) + Sync)>,
    ) -> Result<PipelineResult, String> {
        let run_clock = Instant::now();
        let report = |message: &str| {
            if let Some(callback) = progress {
                callback(message);
            }
        };
        report("Validating run configuration");
        check_cancelled(cancel)?;
        validate_run_configuration(&self.config, initial_design.as_ref(), bounds)?;
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
        // `output_dir` controls retention, not which physics stages run:
        // without it every writer and external adapter works in an isolated
        // temporary workspace rather than treating `None` as "disabled".
        let analysis_dir = prepare_analysis_workspace(options.output_dir.clone())?;
        report("Analysis workspace ready");
        emit_diagnostic(events, run_clock, "setup", "Analysis workspace ready");
        // The desktop design editor owns an explicit vector.  Preserve it
        // verbatim for the reference/full-analysis path; the optimizer has
        // its own canonical nominal step when it actually searches a
        // cabin-derived clean-sheet space.  Mutating the vector here would
        // make the values shown in the editor differ from the aircraft being
        // reviewed, and would also rewrite a named preset on a no-opt run.
        let nominal_design = initial_design.unwrap_or_else(|| self.configured_nominal_design());
        let fixed_design_review = bounds.is_some_and(bounds_are_fixed);

        // The route depends on the configuration, not on the design, so plan
        // it once here rather than inside the mission stage. Two callers now
        // need it: the mission stage as before, and the optimizer's
        // reporting-fidelity acceptance check, which has to fly the same
        // route the published mission will. Planning it twice would also mean
        // two SimBrief fetches per run.
        let planned_route = if self.config.mission.enabled {
            let planned = self.plan_active_route(dispatched_route);
            if planned.is_some() {
                emit_diagnostic(events, run_clock, "setup", "Route planned for this run");
            }
            planned
        } else {
            None
        };

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
        // Resolve the two airports once, with the same fallback
        // `evaluate_active_mission` applies: a planned route carries its
        // endpoint records only when the planner had them, and a great-circle
        // plan for a configured city pair does not. Doing it here keeps the
        // acceptance check and the published mission on one route.
        let acceptance_route = planned_route.as_ref().and_then(|planned| {
            let origin = planned
                .route
                .origin_airport
                .clone()
                .or_else(|| get_airport(&self.config.departure_airport).ok().cloned())?;
            let destination = planned
                .route
                .dest_airport
                .clone()
                .or_else(|| get_airport(&self.config.arrival_airport).ok().cloned())?;
            Some(AcceptanceRoute {
                origin,
                destination,
                distance_m: planned.route.total_distance_m(),
            })
        });
        // The optimization stage is where a cancellation request almost
        // always lands: it is the only stage whose duration is a search
        // budget rather than a fixed sequence of analyses. Marking it means a
        // telemetry snapshot can say "the request arrived in the search" and
        // not merely "somewhere in the pipeline".
        alas_opt::CancelScope::attach(cancel).enter(alas_opt::CancelPhase::PipelineStage, 2);
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
                acceptance_route.as_ref(),
                cancel,
            ))
        } else {
            None
        };
        let (optimized_design, optimization_result, branch_report) = if let Some(solutions) =
            solver_optimizations.as_ref()
        {
            match solutions.selected(options.optimization_solver) {
                Ok(selected) => {
                    let design = selected
                        .design
                        .ok_or_else(|| "selected optimizer returned no design".to_owned())?;
                    (
                        design,
                        selected.optimization.clone(),
                        selected.report.clone(),
                    )
                }
                Err(error) if fixed_design_review && fixed_review_error_is_reportable(&error) => {
                    // A fully pinned desktop vector is a fixed-design
                    // physical review, even when the active optimizer
                    // constraints reject it. Keep the solver failure and
                    // its provenance in the result, then run every
                    // analysis stage so the user gets the actual masses,
                    // geometry, and typed feasibility findings needed to
                    // correct the design. An invalid vector is never
                    // promoted to an optimizer finalist.
                    tracing::warn!(%error, "fixed design review has no feasible optimizer finalist");
                    report(&format!(
                        "Optimizer finalist unavailable; reviewing the pinned design ({error})"
                    ));
                    (nominal_design, None, None)
                }
                Err(error) => return Err(error),
            }
        } else {
            (nominal_design, None, None)
        };
        // Say what the reporting-fidelity re-evaluation did, in the run log,
        // before any downstream stage speaks. A user who sees a converged
        // search and an infeasible aircraft is entitled to read which of the
        // two the run is actually claiming.
        if let Some(acceptance) = optimization_result
            .as_ref()
            .and_then(|optimization| optimization.delivered_acceptance.as_ref())
        {
            let (severity, message) = if acceptance.verified
                && acceptance.delivered_is_search_finalist
            {
                (
                    RunEventSeverity::Info,
                    format!(
                        "Finalist accepted at reporting fidelity ({} candidate(s) re-evaluated, {:.2} s)",
                        acceptance.candidates_evaluated, acceptance.wall_time_s
                    ),
                )
            } else if acceptance.verified {
                (
                    RunEventSeverity::Warning,
                    format!(
                        "Search finalist rejected at reporting fidelity by {} ({}); delivered a verified fallback candidate instead ({} candidate(s) re-evaluated, {:.2} s). This run is NOT reported as converged.",
                        acceptance.finalist_rejected_by.join(", "),
                        acceptance.rejection_messages.join("; "),
                        acceptance.candidates_evaluated,
                        acceptance.wall_time_s
                    ),
                )
            } else {
                (
                    RunEventSeverity::Error,
                    format!(
                        "No candidate survived the reporting-fidelity re-evaluation; the delivered design is rejected by {} ({}) ({} candidate(s) re-evaluated, {:.2} s). This run is NOT reported as converged.",
                        acceptance.delivered_rejected_by.join(", "),
                        acceptance.rejection_messages.join("; "),
                        acceptance.candidates_evaluated,
                        acceptance.wall_time_s
                    ),
                )
            };
            report(&message);
            emit_diagnostic_with_severity(events, run_clock, "optimization", &message, severity);
        }
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
        let full = FullAnalysis::new(self.config.clone());
        let mut optimized_report = match branch_report {
            Some(report) => report,
            None => match self.aircraft_override.as_ref() {
                Some(airplane) => full.run_on_airplane(&optimized_design, airplane.clone())?,
                None => full.run(&optimized_design, true)?,
            },
        };
        if options.optimize
            && optimization_result.is_some()
            && self.aircraft_override.is_none()
            && !optimized_report
                .geometry_summary
                .contains_key("analysis_mass_basis_is_sized")
        {
            // The optimizer's product objective closes the mass/dispatch
            // fixed point below the configured MTOW limit.  A branch report
            // built directly at `requirements.mtow_kg` would consequently
            // calculate cruise lift, trim and component fuel for a heavier
            // aircraft than the one that actually won the search.  Replay
            // the typed finalist assessment and bind the report to its
            // closed takeoff mass before any export or downstream tool sees
            // it.  A disagreement is a real integration error, not a reason
            // to silently fall back to the ceiling-mass report.
            let assessment = alas_opt::assess_product_candidate(&self.config, &optimized_design)
                .map_err(|error| {
                    format!(
                        "optimized finalist could not be re-evaluated at its exported design: {error}"
                    )
                })?;
            if !assessment.hard_feasible {
                let violations = assessment.violated_hard_ids().join(", ");
                return Err(format!(
                    "optimized finalist is not hard-feasible on replay: {}",
                    if violations.is_empty() {
                        "unidentified hard residual".to_owned()
                    } else {
                        violations
                    }
                ));
            }
            // Bind the report to the vector the assessment was *evaluated*
            // on, not the one it was handed. A clean-sheet design space
            // derives the fuselage coordinate from the cabin load case, so
            // the two are the same vector for an optimizer finalist and can
            // differ for any other supplied design (see
            // `alas_opt::ResolvedProductState::design`). Reporting the
            // caller's vector there would publish a different aeroplane from
            // the one this gate just passed.
            let assessed_design = assessment.resolved.design;
            if assessed_design != optimized_design {
                emit_diagnostic(
                    events,
                    run_clock,
                    "full_analysis",
                    &format!(
                        "Finalist geometry re-derived by the design space: fuselage length {:.6} m evaluated against {:.6} m supplied; the report is bound to the evaluated aircraft",
                        assessed_design.fuselage_length_m, optimized_design.fuselage_length_m,
                    ),
                );
            }
            optimized_report = full.run_at_sized_takeoff_mass(
                &assessed_design,
                true,
                assessment.sized.takeoff_mass_kg,
            )?;
            emit_diagnostic(
                events,
                run_clock,
                "full_analysis",
                &format!(
                    "Finalist report bound to mission-sized takeoff mass {:.3} kg (MTOW limit {:.3} kg)",
                    assessment.sized.takeoff_mass_kg,
                    self.config.requirements.mtow_kg,
                ),
            );
        }
        emit_diagnostic(
            events,
            run_clock,
            "full_analysis",
            &format!(
                "Partial result available: alpha={:.3} deg, CL={:.5}, CD={:.5}, L/D={:.2}",
                optimized_report.design_point.alpha_deg,
                optimized_report.design_point.cl,
                optimized_report.design_point.cd,
                optimized_report.design_point.l_over_d,
            ),
        );
        // The optimized report is the first complete figure-producing data
        // boundary. Publish it before CPACS/export/downstream work starts so
        // the desktop can render the same report gallery it will keep after
        // the run, while the remaining stages continue in the worker.
        let live_results = SnapshotPublisher::new(snapshots, || PipelineResult {
            config: self.config.clone(),
            optimized_design: Some(optimized_design),
            optimized_report: Some(optimized_report.clone()),
            optimization_result: optimization_result.clone(),
            solver_optimizations: solver_optimizations.clone(),
            baseline_report: baseline_report.clone(),
            baseline_analysis: None,
            baseline_analysis_error: None,
            route: planned_route.as_ref().map(|planned| planned.route.clone()),
            route_status: planned_route.as_ref().map(|planned| planned.status.clone()),
            mission_result: None,
            mission_load_case: None,
            feasibility: FeasibilityReport::default(),
            mses_result: None,
            mses_pressure: None,
            structural_result: None,
            design_database: None,
            openvsp_export: None,
            cpacs_export: None,
            cpacs_manifest: None,
            vspaero_result: None,
            avl_result: None,
            flowunsteady_result: None,
            execution: PipelineExecutionStatus::for_options(options, false),
        });
        finish_stage(events, run_clock, stage_clock, 3, "full_analysis");
        check_cancelled(cancel)?;

        // Stage 3: CPACS export. Downstream writers consume the aircraft
        // reconstructed from this document, so CPACS is the non-GUI geometry
        // boundary rather than a sidecar copy of the built geometry.
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
        live_results.update(|snapshot| {
            snapshot.optimized_report = Some(optimized_report.clone());
            snapshot.cpacs_export = cpacs_export.clone();
        });
        finish_stage(events, run_clock, stage_clock, 4, "geometry_export");
        check_cancelled(cancel)?;

        let openvsp_export = if self.config.downstream.openvsp {
            let af_path = analysis_dir.join("airfoils/optimized_root.dat");
            let openvsp_path = analysis_dir.join("openvsp/optimized_aircraft.vspscript");
            if let Err(error) =
                export_airfoil_dat(&optimized_report, &self.config, &af_path, "ALAS_Optimized")
            {
                warn_artifact_failure(
                    events,
                    run_clock,
                    "geometry_export",
                    "Root airfoil Selig file",
                    &error,
                );
            }
            match export_openvsp_script(&optimized_report, &self.config, &openvsp_path) {
                Ok(export) => Some(match environment.openvsp_exe.as_deref() {
                    Some(executable) => materialize_openvsp_project(export, executable, 120.0),
                    None => export,
                }),
                Err(error) => {
                    // VSPAERO consumes this geometry and is skipped without it.
                    warn_artifact_failure(
                        events,
                        run_clock,
                        "geometry_export",
                        "OpenVSP geometry script",
                        &error,
                    );
                    None
                }
            }
        } else {
            None
        };
        live_results.update(|snapshot| snapshot.openvsp_export = openvsp_export.clone());
        let avl_requested = self.config.downstream.avl
            && matches!(
                options.aerodynamic_solver,
                AerodynamicSolverMode::Avl | AerodynamicSolverMode::Both
            );
        let run_vspaero = || {
            let stage_clock =
                begin_component(events, run_clock, "downstream/vspaero", "VSPAERO analysis");
            if !self.config.downstream.vspaero {
                finish_component(
                    events,
                    run_clock,
                    stage_clock,
                    "downstream/vspaero",
                    "Skipped",
                );
                return None;
            }
            let result = openvsp_export.as_ref().map(|openvsp| {
                run_vspaero_analysis(
                    &optimized_report,
                    &self.config,
                    openvsp,
                    environment.vspaero_exe.as_deref(),
                    // A five-minute wall clock cut the installed A380-like
                    // 15-point sweep off halfway; fifteen minutes lets one
                    // full sweep finish under the process-tree timeout.
                    900.0,
                )
            });
            live_results.update(|snapshot| snapshot.vspaero_result = result.clone());
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
            live_results.update(|snapshot| snapshot.avl_result = result.clone());
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
            if !self.config.downstream.flowunsteady {
                finish_component(
                    events,
                    run_clock,
                    stage_clock,
                    "downstream/flowunsteady",
                    "Skipped",
                );
                return None;
            }
            let result = Some(run_flowunsteady_analysis(
                &optimized_report,
                &self.config,
                &analysis_dir,
                environment.flowunsteady_exe.as_deref(),
                900.0,
            ));
            live_results.update(|snapshot| snapshot.flowunsteady_result = result.clone());
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
                live_results.update(|snapshot| {
                    snapshot.baseline_analysis = Some(optimized_report.clone());
                });
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
                live_results.update(|snapshot| {
                    snapshot.baseline_analysis = result.0.clone();
                    snapshot.baseline_analysis_error = result.1.clone();
                });
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
            let result = self.evaluate_active_mission(&optimized_report, planned_route.as_ref());
            if let Ok((route, status, mission, load_case)) = &result {
                live_results.update(|snapshot| {
                    snapshot.route = route.clone();
                    snapshot.route_status = status.clone();
                    snapshot.mission_result = mission.clone();
                    snapshot.mission_load_case = load_case.clone();
                });
            }
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
            let result =
                self.run_mses_stage(&optimized_report, environment.mses_dir.as_deref(), cancel);
            live_results.update(|snapshot| {
                snapshot.mses_result = result.0.clone();
                snapshot.mses_pressure = result.1.clone();
            });
            let (polar, pressure) = &result;
            let polar_summary = polar.as_ref().map_or_else(
                || "unavailable".to_owned(),
                |value| {
                    let alpha_range = finite_range_text(&value.alpha_deg, 3);
                    let cl_range = finite_range_text(&value.cl, 4);
                    let cd_range = finite_range_text(&value.cd, 5);
                    format!(
                        "{}/{} alpha points, status={}, OSMAP={}, alpha={alpha_range} deg, CL={cl_range}, CD={cd_range}",
                        value.converged_alpha_count,
                        value.requested_alpha_count,
                        value.status.as_str(),
                        value.osmap_status.as_str(),
                    )
                },
            );
            let pressure_summary = pressure.as_ref().map_or_else(
                || "unavailable".to_owned(),
                |value| {
                    let mach_range = finite_range_text(&value.field_mach, 3);
                    let cp_range = finite_range_text(&value.field_cp, 4);
                    format!(
                        "status={}, upper={}, lower={}, Mach field={}, Cp field={}, Mach={mach_range}, Cp={cp_range}, OSMAP={} ({})",
                        value.status.as_str(),
                        value.cp_upper.len(),
                        value.cp_lower.len(),
                        value.field_mach.len(),
                        value.field_cp.iter().filter(|cp| cp.is_finite()).count(),
                        value.osmap_status.as_str(),
                        if value.transition_model_is_valid() { "valid" } else { "unverified" },
                    )
                },
            );
            emit_diagnostic(
                events,
                run_clock,
                "downstream/mses",
                &format!("MSES polar {polar_summary}; pressure {pressure_summary}"),
            );
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
                // Structural load cards are sized against the same takeoff
                // mass that built the selected report.  For a mission-sized
                // finalist the configured MTOW remains the upper limit, but
                // using that larger limit here would make the wingbox and
                // NASTRAN deck describe a different aircraft than the mass,
                // CG and mission records above.
                let structural_config = config_for_report_mass(&self.config, &optimized_report);
                emit_diagnostic(
                    events,
                    run_clock,
                    "downstream/structural",
                    &format!(
                        "Structural loads use report mass basis {:.3} kg (configured MTOW limit {:.3} kg)",
                        report_mass_basis_kg(&optimized_report, self.config.requirements.mtow_kg),
                        self.config.requirements.mtow_kg,
                    ),
                );
                let result = Some(
                    crate::structural::run_structural_analysis_with_environment_events(
                        &structural_config,
                        &optimized_report,
                        work_dir.as_deref(),
                        environment,
                        events,
                        run_clock,
                    ),
                );
                live_results.update(|snapshot| snapshot.structural_result = result.clone());
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
        live_results.update(|snapshot| {
            snapshot.execution.parallel_effective = options.parallel;
        });
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
        let (route, route_status, mission_result, mission_load_case) = mission_outputs;
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
        let feasibility = assess_physical_feasibility_with_load_case(
            &self.config,
            &optimized_design,
            &optimized_report,
            mission_result.as_ref(),
            mission_load_case.as_ref(),
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
                    warn_artifact_failure(
                        events,
                        run_clock,
                        "finalization",
                        "Design database",
                        &error,
                    );
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
                    warn_artifact_failure(
                        events,
                        run_clock,
                        "finalization",
                        "Payload-layout render artifact",
                        &error,
                    );
                    None
                }
            }
        });
        let cabin_scene_artifact = options.output_dir.as_ref().and_then(|out_dir| {
            let path = out_dir.join("cabin_scene_v2.json");
            match export_cabin_scene(&self.config, &optimized_report, &path) {
                Ok(_) => Some(path),
                Err(error) => {
                    warn_artifact_failure(
                        events,
                        run_clock,
                        "finalization",
                        "Cabin scene v2",
                        &error,
                    );
                    None
                }
            }
        });

        if let (Some(out_dir), Some(polar)) = (options.output_dir.as_deref(), &mses_result) {
            if let Err(error) = persist_mses_polar_diagnostics(polar, out_dir) {
                warn_artifact_failure(
                    events,
                    run_clock,
                    "downstream/mses",
                    "MSES polar diagnostics",
                    &error,
                );
            }
        }
        if let (Some(out_dir), Some(pressure)) = (options.output_dir.as_deref(), &mses_pressure) {
            if let Err(error) = persist_mses_raw_exports(pressure, out_dir) {
                warn_artifact_failure(
                    events,
                    run_clock,
                    "downstream/mses",
                    "MSES raw output",
                    &error,
                );
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
                if let Some(result) = &mses_result {
                    stages.insert("mses".to_owned(), result.status.as_str().to_owned());
                }
                // The polar and pressure solves are supervised separately.
                // A polar can legitimately stop outside its converged domain
                // while the fixed-point pressure solve still returns a
                // native flowfield for the contour figures. Keep both
                // statuses in the manifest instead of collapsing that useful
                // distinction into one red/green value.
                if let Some(result) = &mses_pressure {
                    stages.insert(
                        "mses_pressure".to_owned(),
                        result.status.as_str().to_owned(),
                    );
                }
                if let Some(result) = &structural_result {
                    stages.insert("structures".to_owned(), result.status.clone());
                    for (prefix, solver) in [
                        ("msc_nastran", result.nastran.as_ref()),
                        ("nastran95", result.nastran95.as_ref()),
                    ] {
                        let Some(solver) = solver else { continue };
                        for (solution, status) in [
                            ("sol101", solver.static_solve.status),
                            ("sol103", solver.modes.status),
                            ("sol111", solver.vibration.status),
                        ] {
                            stages
                                .insert(format!("{prefix}_{solution}"), status.as_str().to_owned());
                        }
                    }
                    if let Some(patran) = result.patran.as_ref() {
                        stages.insert("patran".to_owned(), patran.status.clone());
                    }
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
                if let Some(openvsp) = openvsp_export.as_ref() {
                    add_manifest_artifact_if_exists(
                        &mut artifacts,
                        out_dir,
                        "openvsp_script",
                        &openvsp.script_path,
                    );
                    add_manifest_artifact_if_exists(
                        &mut artifacts,
                        out_dir,
                        "openvsp_project",
                        &openvsp.vsp3_path,
                    );
                    add_manifest_artifact_if_exists(
                        &mut artifacts,
                        out_dir,
                        "openvsp_cad_preview",
                        &openvsp.preview_path,
                    );
                    add_manifest_artifact_if_exists(
                        &mut artifacts,
                        out_dir,
                        "openvsp_vspaero_geometry",
                        &openvsp.vspaero_geometry_path,
                    );
                    if let Some(path) = openvsp.runtime_stdout_path.as_ref() {
                        add_manifest_artifact_if_exists(
                            &mut artifacts,
                            out_dir,
                            "openvsp_stdout",
                            path,
                        );
                    }
                    if let Some(path) = openvsp.runtime_stderr_path.as_ref() {
                        add_manifest_artifact_if_exists(
                            &mut artifacts,
                            out_dir,
                            "openvsp_stderr",
                            path,
                        );
                    }
                }
                if let Some(vspaero) = vspaero_result.as_ref() {
                    for (name, path) in [
                        ("vspaero_setup", &vspaero.setup_path),
                        ("vspaero_polar", &vspaero.polar_path),
                        ("vspaero_stdout", &vspaero.stdout_path),
                        ("vspaero_stderr", &vspaero.stderr_path),
                    ] {
                        add_manifest_artifact_if_exists(&mut artifacts, out_dir, name, path);
                    }
                    let history = vspaero.case_path.with_extension("history");
                    add_manifest_artifact_if_exists(
                        &mut artifacts,
                        out_dir,
                        "vspaero_history",
                        &history,
                    );
                    for (name, path) in [
                        (
                            "vspaero_load_distribution",
                            vspaero.case_path.with_extension("lod"),
                        ),
                        ("vspaero_adb", vspaero.case_path.with_extension("adb")),
                        (
                            "vspaero_adb_cases",
                            vspaero.case_path.with_extension("adb.cases"),
                        ),
                        (
                            "vspaero_quad_cases",
                            vspaero.case_path.with_extension("quad.cases"),
                        ),
                    ] {
                        add_manifest_artifact_if_exists(&mut artifacts, out_dir, name, &path);
                    }
                }
                if let Some(avl) = avl_result.as_ref() {
                    for (name, path) in [
                        ("avl_geometry", &avl.geometry_path),
                        ("avl_session", &avl.session_path),
                        ("avl_stdout", &avl.stdout_path),
                        ("avl_stderr", &avl.stderr_path),
                    ] {
                        add_manifest_artifact_if_exists(&mut artifacts, out_dir, name, path);
                    }
                    for (index, path) in avl.force_paths.iter().enumerate() {
                        add_manifest_artifact_if_exists(
                            &mut artifacts,
                            out_dir,
                            &format!("avl_force_{:03}", index + 1),
                            path,
                        );
                    }
                }
                if let Some(flow) = flowunsteady_result.as_ref() {
                    for (name, path) in [
                        ("flowunsteady_request", &flow.request_path),
                        ("flowunsteady_result", &flow.result_path),
                        ("flowunsteady_stdout", &flow.stdout_path),
                        ("flowunsteady_stderr", &flow.stderr_path),
                    ] {
                        add_manifest_artifact_if_exists(&mut artifacts, out_dir, name, path);
                    }
                }
                for (name, path) in [
                    (
                        "mses_polar_diagnostics",
                        out_dir.join("mses/polar_diagnostics.json"),
                    ),
                    (
                        "mses_pressure_diagnostics",
                        out_dir.join("mses/pressure_diagnostics.json"),
                    ),
                    ("mses_bl_dump", out_dir.join("mses/bl_dump.txt")),
                    ("mses_flowfield", out_dir.join("mses/flowfield.txt")),
                ] {
                    add_manifest_artifact_if_exists(&mut artifacts, out_dir, name, &path);
                }
                if let Some(structures) = structural_result.as_ref() {
                    let structural_paths = [
                        ("structures_mesh", out_dir.join("structures/wing_mesh.bdf")),
                        (
                            "structures_sol101_bdf",
                            out_dir.join("structures/sol101/wing_sol101.bdf"),
                        ),
                        (
                            "structures_sol101_f06",
                            out_dir.join("structures/sol101/wing_sol101.f06"),
                        ),
                        (
                            "structures_sol101_op2",
                            out_dir.join("structures/sol101/wing_sol101.op2"),
                        ),
                        (
                            "structures_sol103_bdf",
                            out_dir.join("structures/sol103/wing_sol103.bdf"),
                        ),
                        (
                            "structures_sol103_f06",
                            out_dir.join("structures/sol103/wing_sol103.f06"),
                        ),
                        (
                            "structures_sol103_op2",
                            out_dir.join("structures/sol103/wing_sol103.op2"),
                        ),
                        (
                            "structures_sol111_bdf",
                            out_dir.join("structures/sol111_sine/wing_sol111_sine.bdf"),
                        ),
                        (
                            "structures_sol111_f06",
                            out_dir.join("structures/sol111_sine/wing_sol111_sine.f06"),
                        ),
                        (
                            "structures_sol111_op2",
                            out_dir.join("structures/sol111_sine/wing_sol111_sine.op2"),
                        ),
                    ];
                    for (name, path) in structural_paths {
                        add_manifest_artifact_if_exists(&mut artifacts, out_dir, name, &path);
                    }
                    if let Some(patran) = structures.patran.as_ref() {
                        for (index, (_, path)) in patran.png_paths.iter().enumerate() {
                            add_manifest_artifact_if_exists(
                                &mut artifacts,
                                out_dir,
                                &format!("patran_deformation_{:03}", index + 1),
                                path,
                            );
                        }
                    }
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
            mission_load_case,
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
            execution: PipelineExecutionStatus::for_options(options, parallel_effective),
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
        planned_route: Option<&PlannedRoute>,
    ) -> Result<MissionStageOutputs, String> {
        if !self.config.mission.enabled {
            return Ok((None, None, None, None));
        }
        // The route is planned once per run, before the search starts,
        // because it depends on the configuration and not on the design. That
        // is what lets the optimizer's reporting-fidelity acceptance check fly
        // the same route this stage does, instead of a second route fetched
        // from a network service that may answer differently.
        let planned = planned_route
            .cloned()
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
        let (mission, load) = mission_stage::evaluate(
            &self.config,
            report,
            origin,
            destination,
            planned.route.total_distance_m(),
        )
        .map_err(|error| format!("native mission stage failed: {error}"))?;
        let (route, status) = (Some(planned.route), Some(planned.status));
        Ok((route, status, Some(mission), Some(load)))
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
        let navdata = load_navdata_with_airway_coordinates(
            &navdata_dir,
            self.config.mission.use_airway_endpoint_coordinates,
        );
        let sources = RouteSources {
            dispatched: dispatched_route,
            routes_dir: Some(routes_dir.as_path()),
            navdata: navdata.as_ref(),
            great_circle_points: self.config.mission.great_circle_points.max(1) as usize,
        };
        let route = plan_route_with_max_stretch(
            origin,
            dest,
            sources,
            self.config.mission.max_airway_stretch,
        );
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
        cancel: Option<&AtomicBool>,
    ) -> (Option<MsesPolarResult>, Option<MsesPressureResult>) {
        if !self.config.mses.enabled {
            return (None, None);
        }
        let condition = mses_section_condition(&self.config, report);
        let root_airfoil = report
            .airplane
            .wings
            .first()
            .and_then(|wing| wing.xsecs.first())
            .map(|section| &section.airfoil);
        // An enabled stage that cannot run still returns typed results, so
        // the run log and manifest say why instead of "not requested".
        let unavailable = |status, error: &str| {
            let airfoil_name = root_airfoil
                .map(|airfoil| airfoil.name.clone())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "optimized_root_section".to_owned());
            (
                Some(MsesPolarResult {
                    status,
                    error: Some(error.to_owned()),
                    airfoil_name,
                    mach: condition.mach,
                    reynolds: condition.reynolds,
                    ..MsesPolarResult::default()
                }),
                Some(MsesPressureResult {
                    status,
                    error: Some(error.to_owned()),
                    alpha_deg: condition.alpha_deg,
                    ..MsesPressureResult::default()
                }),
            )
        };
        let Some(dir) = mses_dir else {
            return unavailable(
                alas_aero::mses::MsesStatus::Absent,
                "MSES executables not configured (Setup > External Tools)",
            );
        };
        let Some(root_airfoil) = root_airfoil else {
            return unavailable(
                alas_aero::mses::MsesStatus::NotRun,
                "the analyzed aircraft has no main-wing root section to send to MSES",
            );
        };

        let polar = run_mses_polar_with_cancel(
            root_airfoil,
            condition.mach,
            condition.reynolds,
            condition.alpha_deg,
            &self.config.mses,
            dir,
            cancel,
        );
        let best_checkpoint = polar.closest_checkpoint(condition.alpha_deg);
        let pressure = run_mses_pressure_distribution_with_checkpoint_and_cancel(
            root_airfoil,
            condition.mach,
            condition.reynolds,
            condition.alpha_deg,
            &self.config.mses,
            dir,
            None,
            best_checkpoint,
            cancel,
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

/// Read the report's explicit mass provenance without treating a missing or
/// malformed value as a new mass limit. `fallback_kg` is the configured
/// MTOW, which is the appropriate basis for reference and fixed requirement
/// reports.
fn report_mass_basis_kg(report: &AnalysisReport, fallback_kg: f64) -> f64 {
    let is_sized = report
        .geometry_summary
        .get("analysis_mass_basis_is_sized")
        .is_some_and(|value| value.is_finite() && *value > 0.5);
    let sized = report
        .geometry_summary
        .get("analysis_mass_basis_kg")
        .copied()
        .filter(|value| value.is_finite() && *value > 0.0);
    if is_sized {
        if let Some(value) = sized {
            return value;
        }
    }
    fallback_kg
}

/// Clone the public configuration for a downstream discipline that consumes
/// the report's actual mass. The configured MTOW is retained by the caller as
/// a limit and remains in the final result; only the structural load cards
/// need the closed mission-sized value as their working mass.
fn config_for_report_mass(config: &AlasConfig, report: &AnalysisReport) -> AlasConfig {
    config.at_closure_mass(report_mass_basis_kg(report, config.requirements.mtow_kg))
}

/// `lo..hi` over the finite entries of `values` at `decimals` places, or
/// `none` when there are none.
fn finite_range_text(values: &[f64], decimals: usize) -> String {
    let mut finite = values.iter().copied().filter(|value| value.is_finite());
    let Some(first) = finite.next() else {
        return "none".to_owned();
    };
    let (lo, hi) = finite.fold((first, first), |(lo, hi), value| {
        (lo.min(value), hi.max(value))
    });
    format!("{lo:.decimals$}..{hi:.decimals$}")
}

/// Whether a design-space call pins every coordinate to one literal value.
///
/// This is the explicit review contract used by the desktop when it asks the
/// pipeline to analyse a design without a feasible search winner.  A
/// partially bounded optimization still has to return an error rather than
/// silently publishing an infeasible candidate.
fn bounds_are_fixed(bounds: &[(f64, f64)]) -> bool {
    !bounds.is_empty() && bounds.iter().all(|&(lower, upper)| lower == upper)
}

/// Whether a failed single-point search still leaves a useful fixed-aircraft
/// review to run. Sizing and layout misses are reportable on the concrete
/// aircraft, while an invalid trim requirement prevents the native analysis
/// from constructing a physical operating point at all.
fn fixed_review_error_is_reportable(error: &str) -> bool {
    error.contains("no feasible design") && !error.contains("trim_cruise_cl_exceeds_max")
}

/// Enforce the blocking cross-field configuration contract at the public
/// execution boundary. The GUI performs the same check for button state, but
/// library and CLI callers must receive it even when they bypass that UI.
fn validate_run_configuration(
    config: &AlasConfig,
    initial_design: Option<&DesignVector>,
    bounds: Option<&[(f64, f64)]>,
) -> Result<(), String> {
    check_preset_policy(config, initial_design, bounds)?;
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
// Failed expectations and unwraps here are failed test assertions.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "pipeline_tests.rs"]
mod tests;
