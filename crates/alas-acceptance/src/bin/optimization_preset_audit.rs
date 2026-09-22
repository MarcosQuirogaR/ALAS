// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Optimization-mode acceptance runner: runs the real headless pipeline with
//! `PipelineOptions.optimize = true`, a fixed recorded seed, and a bounded
//! solver setting, for either one measurement preset or the full registered
//! preset matrix.
//!
//! This is deliberately separate from `preset_audit` (baseline, `optimize:
//! false`), which it reuses for its own purpose and does not reimplement. The
//! optimizer has no wall-clock timeout of its own (only a deterministic
//! evaluation-count budget), so a `--timeout-s` guard here is external. On
//! expiry the guard sets the run's cooperative cancellation flag, which
//! `DesignPipeline::run_cancellable` threads into the active search's own
//! generation and poll loops, and then *joins* the worker: no thread is
//! abandoned, no process exit stands in for stopping the work, and the
//! cancelled run's own diagnostics are recorded in the result row.
//!
//! Cancellation is cooperative and therefore bounded rather than immediate:
//! the flag is read before every coupled evaluation, between evaluation
//! blocks and at stage boundaries, so a guard that expires mid-evaluation
//! waits out that evaluation. The row records both the guard limit and the
//! wall time actually observed, which is the only honest way to report the
//! difference.
//!
//! # The service contract this harness measures against
//!
//! Two different latencies, reported separately, because conflating them is
//! what made the earlier 28.76 s figure unreadable:
//!
//! - **acknowledgement** - request to the search *observing* the flag. This
//!   is what a cancel button owes a user, and the contract is
//!   [`ACKNOWLEDGEMENT_CONTRACT_S`] = 1 s. It is met by polling the flag
//!   before every coupled evaluation, so it holds as long as one evaluation
//!   does not itself exceed the contract.
//! - **completion** - request to the search returning. This is bounded by the
//!   work already in flight and **cannot** be promised at any fixed number of
//!   seconds: the bound is one coupled evaluation of the active search, one
//!   screening block, or one supervised external solver call, whichever the
//!   run is inside. The row reports which phase it was, how long that unit
//!   actually took, and therefore what the bound was for this run, rather
//!   than asserting a deadline the physics does not support.

#![allow(clippy::print_stderr, clippy::print_stdout)]

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use alas_config::{presets, solver_presets, AlasConfig, DesignMissionEvidence, SolverSettings};
use alas_exec::{RunEnvironment, ToolLocator, ToolPreferences};
use alas_opt::{CancelSnapshot, CancelWatch, StopReason};
use alas_pipeline::{
    AvlAnalysisStatus, DesignPipeline, FindingCode, FindingSeverity, FlowUnsteadyAnalysisStatus,
    OpenVspExportStatus, PhysicalFinding, PipelineOptions, PipelineResult, PlanningCgStatus,
    VspaeroAnalysisStatus,
};
use serde_json::{json, Value};

/// Fixed recorded seed for every optimization run in this harness. A fixed
/// constant, not a "representative" or hidden default: recorded verbatim in
/// every result row and every saved effective config.
const DEFAULT_SEED: u64 = 20260922;

/// How long the guard waits between "still draining" notices while joining a
/// cancelled worker. It bounds only how often progress is printed; the join
/// itself is unconditional.
///
/// Five seconds, not thirty: with the flag now read before every coupled
/// evaluation the expected drain is one evaluation, so a notice interval an
/// order of magnitude longer than the expected drain would print nothing at
/// all on a healthy run and give an operator nothing to watch on an unhealthy
/// one. Each notice names the phase the run is in and how many analyses it
/// has completed since the request, so a stuck drain says what it is stuck
/// in.
const JOIN_NOTICE_INTERVAL: Duration = Duration::from_secs(5);

/// The acknowledgement contract, seconds: how long the running search may
/// take to *observe* a cancellation request.
///
/// This is the responsiveness promise, and it is deliberately stated on
/// observation rather than on completion. Completion is bounded by the
/// evaluation in flight and is reported, not promised. One second is the
/// figure a GUI cancel needs to feel answered, and the flag is polled before
/// every coupled evaluation, so it is met whenever one evaluation costs less
/// than a second - which the row records, so a host where that is false says
/// so instead of quietly failing the contract.
const ACKNOWLEDGEMENT_CONTRACT_S: f64 = 1.0;

fn main() -> io::Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("measure") => run_measure(&args[1..]),
        Some("matrix") => run_matrix(&args[1..]),
        _ => {
            eprintln!(
                "usage:\n  optimization_preset_audit measure --preset <NAME> --output-dir <DIR> [--seed N] [--solver-preset NAME] [--timeout-s N] [--experiment LABEL --max-iterations N --population-size N]\n  optimization_preset_audit matrix --output-dir <DIR> [--seed N] [--solver-preset NAME] [--timeout-s N]\n\n  --experiment LABEL marks a measurement/engineering-budget run. It is required before\n  --max-iterations or --population-size may change the registered solver preset's search\n  effort, and the label is recorded in every row and artefact the run writes. It changes\n  numerical search effort only: no requirement, constraint, mission, design space or\n  acceptance clause is reachable from it."
            );
            std::process::exit(2);
        }
    }
}

struct Args {
    output_dir: PathBuf,
    seed: u64,
    solver_preset: String,
    timeout_s: Option<u64>,
    preset: Option<String>,
    experiment: Option<String>,
    max_iterations: Option<i64>,
    population_size: Option<i64>,
}

/// How a run's solver settings were arrived at.
///
/// A registered preset and a hand-sized search effort are different claims
/// about a result, and a row that cannot tell them apart is how a reduced
/// budget gets read as `quick_draft`. Every artefact this harness writes
/// carries one of these two, and the experiment variant carries its label.
#[derive(Debug, Clone)]
enum Configuration {
    /// Exactly the registered solver preset, unmodified.
    RegisteredPreset { name: String },
    /// The registered preset with its *numerical search effort* overridden
    /// under an explicit label.
    Experiment {
        label: String,
        derived_from: String,
        overrides: Vec<(&'static str, i64, i64)>,
    },
}

impl Configuration {
    /// Apply the requested search-effort overrides to a registered preset's
    /// settings, rejecting an override that was not labelled.
    fn resolve(args: &Args) -> io::Result<(Self, SolverSettings)> {
        let mut settings = resolve_settings(&args.solver_preset)?;
        let mut overrides = Vec::new();
        if let Some(value) = args.max_iterations {
            overrides.push(("max_iterations", settings.max_iterations, value));
            settings.max_iterations = value;
        }
        if let Some(value) = args.population_size {
            overrides.push(("population_size", settings.population_size, value));
            settings.population_size = value;
        }
        let Some(label) = args.experiment.as_ref() else {
            if overrides.is_empty() {
                return Ok((
                    Self::RegisteredPreset {
                        name: args.solver_preset.clone(),
                    },
                    settings,
                ));
            }
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--max-iterations and --population-size change the registered preset's search \
                 effort and require an explicit --experiment <LABEL>, so the run cannot be read \
                 as the preset it was derived from",
            ));
        };
        // A label that collides with a registered preset name would reproduce
        // exactly the confusion the label exists to prevent.
        if solver_presets::get(label).is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "--experiment label {label:?} is the name of a registered solver preset; \
                     choose a label that cannot be mistaken for one"
                ),
            ));
        }
        Ok((
            Self::Experiment {
                label: label.clone(),
                derived_from: args.solver_preset.clone(),
                overrides,
            },
            settings,
        ))
    }

    /// The token a row reports in `solver_preset`.
    ///
    /// An experiment reports its own label there, never the preset it was
    /// derived from: that field is what a reader compares runs by.
    fn reported_name(&self) -> &str {
        match self {
            Self::RegisteredPreset { name } => name,
            Self::Experiment { label, .. } => label,
        }
    }

    /// The provenance block a row carries.
    fn provenance(&self) -> Value {
        match self {
            Self::RegisteredPreset { name } => json!({
                "kind": "registered_solver_preset",
                "solver_preset": name,
                "search_effort_overrides": [],
            }),
            Self::Experiment {
                label,
                derived_from,
                overrides,
            } => json!({
                "kind": "experiment",
                "experiment_label": label,
                "derived_from_solver_preset": derived_from,
                "search_effort_overrides": overrides
                    .iter()
                    .map(|(field, before, after)| json!({
                        "setting": field,
                        "registered_value": before,
                        "experiment_value": after,
                    }))
                    .collect::<Vec<_>>(),
                "what_this_changes": "numerical search effort only: how many candidates the \
                                      search evaluates and over how many generations",
                "what_this_does_not_change": "the registered aircraft preset, its requirements, \
                                              design mission, design space and bounds, every \
                                              physical constraint and limit, the reporting-fidelity \
                                              re-evaluation, and every feasibility clause in this \
                                              row. None of them is reachable from these flags.",
                "not_a_substitute_for": "the registered quick_draft/balanced/thorough presets, a \
                                         clean-sheet search, the reference baseline, or the \
                                         all-preset acceptance matrix",
            }),
        }
    }
}

fn parse_args(values: &[String]) -> io::Result<Args> {
    let mut output_dir = None;
    let mut seed = DEFAULT_SEED;
    let mut solver_preset = "quick_draft".to_owned();
    let mut timeout_s = None;
    let mut preset = None;
    let mut experiment = None;
    let mut max_iterations = None;
    let mut population_size = None;
    let mut index = 0;
    while index < values.len() {
        match values[index].as_str() {
            "--output-dir" => {
                index += 1;
                output_dir = values.get(index).map(PathBuf::from);
            }
            "--seed" => {
                index += 1;
                seed = values
                    .get(index)
                    .and_then(|v| v.parse().ok())
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidInput, "--seed requires an integer")
                    })?;
            }
            "--solver-preset" => {
                index += 1;
                solver_preset = values.get(index).cloned().ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "--solver-preset requires a name",
                    )
                })?;
            }
            "--timeout-s" => {
                index += 1;
                timeout_s = Some(values.get(index).and_then(|v| v.parse().ok()).ok_or_else(
                    || {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "--timeout-s requires an integer",
                        )
                    },
                )?);
            }
            "--preset" => {
                index += 1;
                preset = values.get(index).cloned();
            }
            "--experiment" => {
                index += 1;
                let label = values.get(index).cloned().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "--experiment requires a label")
                })?;
                experiment = Some(label);
            }
            "--max-iterations" => {
                index += 1;
                max_iterations = Some(values.get(index).and_then(|v| v.parse().ok()).ok_or_else(
                    || {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "--max-iterations requires an integer",
                        )
                    },
                )?);
            }
            "--population-size" => {
                index += 1;
                population_size = Some(values.get(index).and_then(|v| v.parse().ok()).ok_or_else(
                    || {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "--population-size requires an integer",
                        )
                    },
                )?);
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unrecognized argument '{other}'"),
                ));
            }
        }
        index += 1;
    }
    let output_dir = output_dir
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "--output-dir is required"))?;
    Ok(Args {
        output_dir,
        seed,
        solver_preset,
        timeout_s,
        preset,
        experiment,
        max_iterations,
        population_size,
    })
}

fn resolve_settings(name: &str) -> io::Result<SolverSettings> {
    solver_presets::get(name)
        .map(|preset| preset.settings.clone())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))
}

fn run_measure(values: &[String]) -> io::Result<()> {
    let args = parse_args(values)?;
    let preset_name = args.preset.clone().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "measure requires --preset <NAME>",
        )
    })?;
    let (configuration, settings) = Configuration::resolve(&args)?;
    fs::create_dir_all(&args.output_dir)?;

    let locator = ToolLocator::for_current_process();
    let preferences = locator.load_preferences();

    eprintln!(
        "[measure] preset={preset_name} configuration={} seed={} timeout_s={:?} settings={}",
        configuration.reported_name(),
        args.seed,
        args.timeout_s,
        settings_json(&settings)
    );
    let row = evaluate_preset_optimization(
        &preset_name,
        &args.output_dir,
        &locator,
        &preferences,
        args.seed,
        &configuration,
        &settings,
        args.timeout_s.map(Duration::from_secs),
    );

    fs::write(
        args.output_dir.join("measurement_result.json"),
        serde_json::to_vec_pretty(&row).map_err(io::Error::other)?,
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&row).map_err(io::Error::other)?
    );
    Ok(())
}

fn run_matrix(values: &[String]) -> io::Result<()> {
    let args = parse_args(values)?;
    let (configuration, settings) = Configuration::resolve(&args)?;
    fs::create_dir_all(&args.output_dir)?;

    let locator = ToolLocator::for_current_process();
    let preferences = locator.load_preferences();

    let mut rows = Vec::new();
    let mut stopped_early = false;
    for preset_name in presets::available() {
        eprintln!(
            "[matrix] preset={preset_name} configuration={} seed={} timeout_s={:?}",
            configuration.reported_name(),
            args.seed,
            args.timeout_s
        );
        let row = evaluate_preset_optimization(
            preset_name,
            &args.output_dir,
            &locator,
            &preferences,
            args.seed,
            &configuration,
            &settings,
            args.timeout_s.map(Duration::from_secs),
        );
        let is_timeout = row["status"] == Value::String("timeout".to_owned());
        rows.push(row);
        // A timed-out preset's worker is now cancelled and joined, so the
        // following presets would no longer be timed against a live optimizer
        // on the same cores. The matrix still stops: a guard expiry means this
        // configuration's budget does not fit the guard, so every remaining
        // preset would be measured under a setting already known to be wrong,
        // and a partial matrix that says so is more use than a full one of
        // timeouts.
        if is_timeout {
            eprintln!("[matrix] preset={preset_name} exceeded the guard and was cancelled; stopping matrix and persisting partial results");
            stopped_early = true;
            break;
        }
    }

    let all_success = rows
        .iter()
        .all(|row| row["status"] == Value::String("success".to_owned()));
    let document = json!({
        "mode": "optimization",
        "seed": args.seed,
        "solver_preset": configuration.reported_name(),
        "configuration": configuration.provenance(),
        "solver_settings": settings_json(&settings),
        "timeout_s": args.timeout_s,
        "presets_total": presets::available().len(),
        "presets_run": rows.len(),
        "stopped_early_on_timeout": stopped_early,
        "all_success": all_success,
        "presets": rows,
    });
    fs::write(
        args.output_dir.join("optimization_acceptance_matrix.json"),
        serde_json::to_vec_pretty(&document).map_err(io::Error::other)?,
    )?;
    fs::write(
        args.output_dir.join("optimization_acceptance_matrix.txt"),
        format_matrix_report(&document),
    )?;
    println!(
        "Optimization matrix: {} of {} presets run, all_success={} ({})",
        rows.len(),
        presets::available().len(),
        all_success,
        args.output_dir.display()
    );
    if stopped_early {
        std::process::exit(3);
    }
    Ok(())
}

fn settings_json(settings: &SolverSettings) -> Value {
    json!({
        "strategy": settings.strategy,
        "max_iterations": settings.max_iterations,
        "population_size": settings.population_size,
        "tolerance": settings.tolerance,
        "workers": settings.workers,
        "seed": settings.seed,
    })
}

enum RunOutcome {
    Finished(
        Box<thread::Result<Result<PipelineResult, String>>>,
        Duration,
        Box<CancelSnapshot>,
    ),
    /// The guard expired, cancellation was signalled, and the worker was
    /// joined.
    Cancelled {
        /// The configured guard.
        limit: Duration,
        /// Wall time from the worker's first instruction to its return, which
        /// is the guard plus however long the run took to reach its next
        /// cancellation boundary.
        observed: Duration,
        /// What the cancelled run itself reported, verbatim.
        detail: String,
        /// Whether the joined worker panicked instead of returning.
        panicked: bool,
        /// The run's own cancellation telemetry, read after the join.
        telemetry: Box<CancelSnapshot>,
    },
}

fn disconnected(telemetry: CancelSnapshot) -> RunOutcome {
    RunOutcome::Finished(
        Box::new(Ok(Err(
            "worker thread disconnected without a result".to_owned()
        ))),
        Duration::ZERO,
        Box::new(telemetry),
    )
}

/// Run the pipeline on a worker thread under an optional external wall-clock
/// guard.
///
/// On expiry the guard sets the run's cancellation flag and then blocks on the
/// worker until it returns. It does not abandon the thread and does not exit
/// the process: an abandoned optimizer keeps burning the same cores every
/// later measurement is timed on, which is what made the previous guard's
/// wall-clock numbers unusable, and a process exit hides whether the run ever
/// actually stopped. Joining is what turns "we stopped waiting" into "the work
/// stopped", and the two are reported separately below.
fn run_with_timeout(
    config: AlasConfig,
    options: PipelineOptions,
    environment: RunEnvironment,
    timeout: Option<Duration>,
) -> RunOutcome {
    // A `CancelWatch` owns the flag and the telemetry around it. The worker
    // still receives a plain `&AtomicBool`, so nothing in the pipeline, the
    // search stack or the GUI changes signature; the instrumentation points
    // find the watch from the flag they already hold.
    let watch = CancelWatch::new();
    let worker_watch = std::sync::Arc::clone(&watch);
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let start = Instant::now();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            DesignPipeline::new(config).run_cancellable(&options, &environment, worker_watch.flag())
        }));
        let elapsed = start.elapsed();
        let _ = tx.send((result, elapsed));
    });

    // Every return below goes through this, so no path leaves the worker
    // handle unjoined. The channel send is the last thing the worker does, so
    // once a value has been received the join is the thread's own teardown and
    // returns immediately; a disconnected channel means the worker is already
    // gone and the join collects it.
    let finish = |handle: thread::JoinHandle<()>, outcome: RunOutcome| -> RunOutcome {
        let _ = handle.join();
        watch.mark_worker_joined();
        outcome
    };

    let Some(limit) = timeout else {
        return match rx.recv() {
            Ok((result, elapsed)) => {
                let telemetry = Box::new(watch.snapshot());
                finish(
                    handle,
                    RunOutcome::Finished(Box::new(result), elapsed, telemetry),
                )
            }
            Err(_) => {
                let telemetry = watch.snapshot();
                finish(handle, disconnected(telemetry))
            }
        };
    };

    match rx.recv_timeout(limit) {
        Ok((result, elapsed)) => {
            let telemetry = Box::new(watch.snapshot());
            return finish(
                handle,
                RunOutcome::Finished(Box::new(result), elapsed, telemetry),
            );
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let telemetry = watch.snapshot();
            return finish(handle, disconnected(telemetry));
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {}
    }

    eprintln!(
        "[guard] {}s elapsed; requesting cooperative cancellation and joining the worker",
        limit.as_secs()
    );
    // Records the phase and evaluation in flight, then sets the flag.
    watch.request_cancellation();
    let signalled = Instant::now();
    let (result, observed) = loop {
        match rx.recv_timeout(JOIN_NOTICE_INTERVAL) {
            Ok(value) => break value,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // A phase-aware notice: a drain that is taking longer than
                // one evaluation should say what it is waiting for, so the
                // operator does not have to guess between a slow analysis, a
                // block boundary and an external process.
                let progress = watch.snapshot();
                let notice = format!(
                    "still draining {:.1}s after the request; phase {} #{} ({:.1}s in it), \
                     {} analysis/analyses completed since the request, acknowledged {}",
                    signalled.elapsed().as_secs_f64(),
                    progress.phase.as_str(),
                    progress.phase_index,
                    progress.phase_elapsed_s,
                    progress.evaluations_after_request,
                    progress.acknowledgement_latency_s.map_or_else(
                        || "not yet".to_owned(),
                        |value| format!("after {value:.3}s")
                    ),
                );
                eprintln!("[guard] {notice}");
                watch.note_progress(notice);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let telemetry = watch.snapshot();
                return finish(handle, disconnected(telemetry));
            }
        }
    };
    // The worker catches its own unwind and always sends, so a panic arrives
    // in `result` rather than through `join`; both are consulted so neither
    // can hide one.
    let joined_panic = handle.join().is_err();
    watch.mark_worker_joined();
    let (detail, panicked) = match result {
        Ok(Ok(_)) => (
            "the run completed after the cancellation request rather than stopping on it"
                .to_owned(),
            joined_panic,
        ),
        Ok(Err(error)) => (error, joined_panic),
        Err(panic) => (
            format!(
                "worker panicked after the cancellation request: {}",
                panic_message(&panic)
            ),
            true,
        ),
    };
    // The guard, not the search, is what stopped this run; the search's own
    // `cancelled` label is kept in `termination` alongside it.
    let optimizer_termination = watch.snapshot().termination;
    watch.set_stop_reason(
        StopReason::ExternalGuardTimeout,
        optimizer_termination.as_deref(),
    );
    RunOutcome::Cancelled {
        limit,
        observed,
        detail,
        panicked,
        telemetry: Box::new(watch.snapshot()),
    }
}

// The audit binary threads the whole measurement context through one call;
// grouping it would hide which inputs a row is actually built from.
#[allow(clippy::too_many_arguments)]
fn evaluate_preset_optimization(
    name: &str,
    output_root: &Path,
    locator: &ToolLocator,
    preferences: &ToolPreferences,
    seed: u64,
    configuration: &Configuration,
    settings: &SolverSettings,
    timeout: Option<Duration>,
) -> Value {
    let result_dir = output_root.join(name).join("optimize");
    let Ok(preset) = presets::get(name) else {
        return error_row(
            name,
            &result_dir,
            seed,
            configuration,
            "registered preset lookup failed",
        );
    };
    if let Err(error) = fs::create_dir_all(&result_dir) {
        return error_row(
            name,
            &result_dir,
            seed,
            configuration,
            &format!("cannot create {}: {error}", result_dir.display()),
        );
    }

    // Selected through the same JSON-preset boundary the CLI and GUI use, not
    // reassembled field-by-field: a hand-built config silently drops the
    // cabin seed and the engine binding (see `evaluate_preset` in
    // `matrix_parts/part_01.rs`, which this mirrors for the optimization case
    // it does not cover).
    let mut config = match AlasConfig::from_value(&json!({ "preset": preset.name })) {
        Ok(config) => config,
        Err(error) => return error_row(name, &result_dir, seed, configuration, &error.to_string()),
    };
    config.optimizer.solver = settings.clone();

    if let Err(error) = fs::write(
        result_dir.join("input_config.json"),
        serde_json::to_vec_pretty(&config).unwrap_or_default(),
    ) {
        eprintln!("[warn] could not persist input config for {name}: {error}");
    }

    let environment = locator.resolve_environment(
        Path::new(&config.mses.mses_dir),
        Path::new(&config.structures.nastran_exe_path),
        Path::new(&config.structures.patran_exe_path),
        Path::new(preferences.openvsp_dir.as_deref().unwrap_or("")),
        Path::new(preferences.avl_exe.as_deref().unwrap_or("")),
    );

    let options = PipelineOptions {
        optimize: true,
        compare_baseline: true,
        parallel: true,
        aerodynamic_solver: alas_pipeline::AerodynamicSolverMode::Both,
        optimization_solver: alas_pipeline::OptimizationSolverMode::Vlm,
        output_dir: Some(result_dir.clone()),
        save_plots: false,
        seed: Some(seed),
        quiet: true,
    };

    let outcome = run_with_timeout(config, options, environment, timeout);
    let (run_result, wall_time, telemetry) = match outcome {
        RunOutcome::Cancelled {
            limit,
            observed,
            detail,
            panicked,
            telemetry,
        } => {
            persist_telemetry(&result_dir, &telemetry);
            let acknowledged = telemetry.acknowledged_within(ACKNOWLEDGEMENT_CONTRACT_S);
            return json!({
                "preset": name,
                "status": "timeout",
                "status_detail": format!(
                    "no result within the external {}s guard; cancellation was signalled and the worker was joined after {:.1}s total. The run reported: {detail}",
                    limit.as_secs(),
                    observed.as_secs_f64()
                ),
                "seed": seed,
                "solver_preset": configuration.reported_name(),
                "configuration": configuration.provenance(),
                "solver_settings": settings_json(settings),
                "output_dir": result_dir.display().to_string(),
                "timeout_s": limit.as_secs(),
                "wall_time_s": observed.as_secs_f64(),
                // How long the run took to reach a cancellation boundary after
                // the flag was set. Cooperative cancellation is bounded, not
                // immediate, and this is the size of that bound as measured.
                // Kept for continuity with the pre-change row; the telemetry
                // below splits it into acknowledgement and drain.
                "cancellation_latency_s": observed.as_secs_f64() - limit.as_secs_f64(),
                "cancellation": cancellation_json(&telemetry),
                "acknowledgement_contract_s": ACKNOWLEDGEMENT_CONTRACT_S,
                "acknowledgement_contract_met": acknowledged,
                "worker_joined": true,
                "worker_panicked": panicked,
                // A time-limited run is never feasible and never converged:
                // the search was stopped from outside before it decided
                // anything.
                "execution_passed": false,
                "physical_passed": false,
                "optimizer_converged": false,
                "optimizer_feasible": false,
                "feasibility_blockers": ["external wall-clock guard expired; the search was cancelled before it terminated on its own criterion"],
            });
        }
        RunOutcome::Finished(result, elapsed, telemetry) => (*result, elapsed, telemetry),
    };
    persist_telemetry(&result_dir, &telemetry);

    let pipeline_result = match run_result {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => {
            let infeasible = error.contains("no feasible design");
            return json!({
                "preset": name,
                "status": if infeasible { "infeasible" } else { "error" },
                "status_detail": error,
                "seed": seed,
                "solver_preset": configuration.reported_name(),
                "configuration": configuration.provenance(),
                "solver_settings": settings_json(settings),
                "output_dir": result_dir.display().to_string(),
                "wall_time_s": wall_time.as_secs_f64(),
            });
        }
        Err(panic) => {
            let message = panic_message(&panic);
            return json!({
                "preset": name,
                "status": "error",
                "status_detail": format!("pipeline panicked: {message}"),
                "seed": seed,
                "solver_preset": configuration.reported_name(),
                "configuration": configuration.provenance(),
                "solver_settings": settings_json(settings),
                "output_dir": result_dir.display().to_string(),
                "wall_time_s": wall_time.as_secs_f64(),
            });
        }
    };

    build_success_row(
        name,
        &result_dir,
        seed,
        configuration,
        settings,
        wall_time,
        &pipeline_result,
        &telemetry,
    )
}

/// Write the run's cancellation telemetry next to its other evidence.
///
/// Always, not only for a cancelled run: the per-evaluation timings are how a
/// later run's cancellation bound is predicted, and they are only measurable
/// on a run that was allowed to work.
fn persist_telemetry(result_dir: &Path, telemetry: &CancelSnapshot) {
    let path = result_dir.join("cancellation_telemetry.json");
    match serde_json::to_vec_pretty(telemetry) {
        Ok(bytes) => {
            if let Err(error) = fs::write(&path, bytes) {
                eprintln!("[warn] could not persist {}: {error}", path.display());
            }
        }
        Err(error) => eprintln!("[warn] could not serialize cancellation telemetry: {error}"),
    }
}

/// The nominal design and the delivered design, side by side.
///
/// `compare_baseline` is on for every run this harness starts, so an
/// optimization run analyses the registered preset's own design vector at
/// reporting fidelity as well as the search's. Without both columns a row
/// says what the optimizer produced but not what it produced it *against*,
/// and an improvement claim cannot be checked.
///
/// Units and frame are the run's own, restated in `metric_conventions`:
/// lengths in metres in body axes with the origin at the fuselage nose and
/// `+x` aft, masses in kilograms, `l_over_d` dimensionless at the reported
/// design point. Deltas are optimized minus baseline, so a negative fuel
/// delta is less fuel.
fn baseline_comparison(result: &PipelineResult) -> Value {
    let metrics = |report: Option<&alas_pipeline::AnalysisReport>| {
        report.map_or_else(
            || json!(null),
            |report| {
                json!({
                    "cruise_l_over_d": report.design_point.l_over_d,
                    "trimmed_l_over_d": report.trimmed_design_point.as_ref().map(|point| point.l_over_d),
                    "x_neutral_point_m": report.x_neutral_point,
                    "static_margin": report.static_margin,
                    "cd0": report.polar_fit.cd0,
                    "fuel_kg": report.component_masses.get("Fuel").copied(),
                    "payload_kg": report.component_masses.get("Payload").copied(),
                    "wing_area_m2": report.airplane.s_ref,
                })
            },
        )
    };
    let delta = |extract: fn(&alas_pipeline::AnalysisReport) -> f64| match (
        result.baseline_analysis.as_ref(),
        result.optimized_report.as_ref(),
    ) {
        (Some(baseline), Some(optimized)) => json!(extract(optimized) - extract(baseline)),
        _ => json!(null),
    };
    json!({
        "baseline": metrics(result.baseline_analysis.as_ref()),
        "optimized": metrics(result.optimized_report.as_ref()),
        "delta": {
            "cruise_l_over_d": delta(|report| report.design_point.l_over_d),
            "x_neutral_point_m": delta(|report| report.x_neutral_point),
            "static_margin": delta(|report| report.static_margin),
            "fuel_kg": delta(|report| report.component_masses.get("Fuel").copied().unwrap_or(f64::NAN)),
            "wing_area_m2": delta(|report| report.airplane.s_ref),
        },
        "delta_convention": "optimized minus baseline, in the units of the same field above",
        "comparable": result.baseline_analysis.is_some() && result.optimized_report.is_some(),
    })
}

/// The largest uninterruptible unit this run executed, seconds.
///
/// This is the cancellation bound the run actually had. `None` means nothing
/// was timed, which happens only when no search ran.
fn acknowledgement_bound(telemetry: &CancelSnapshot) -> Option<f64> {
    match (telemetry.longest_evaluation_s, telemetry.longest_block_s) {
        (Some(evaluation), Some(block)) => Some(evaluation.max(block)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

/// The telemetry summary a result row carries inline.
///
/// The full event timeline goes to `cancellation_telemetry.json`; this is the
/// part a reader needs to judge the two contracts without opening it.
fn cancellation_json(telemetry: &CancelSnapshot) -> Value {
    json!({
        "requested": telemetry.requested,
        // Which boundary the drain was waiting for. This is the field the
        // 28.76 s figure was missing.
        "requested_during_phase": telemetry.requested_during,
        "requested_during_phase_index": telemetry.requested_during_index,
        "requested_at_evaluation": telemetry.requested_at_evaluation,
        "first_observed_in_phase": telemetry.first_observed_in,
        "acknowledgement_latency_s": telemetry.acknowledgement_latency_s,
        // What the acknowledgement latency was *bounded* by on this run, as
        // opposed to what it happened to be. The flag cannot be read inside a
        // coupled analysis or inside an evaluation block, so a request that
        // arrives at the start of the most expensive unit this run executed
        // waits that long. Reported so the 1 s target is checkable rather
        // than asserted: a host or preset where one analysis costs more than
        // a second says so here.
        "acknowledgement_bound_s": acknowledgement_bound(telemetry),
        "acknowledgement_bound_source": "the longer of one coupled evaluation and one \
                                         uninterruptible evaluation block, both measured on \
                                         this run",
        "drain_latency_s": telemetry.drain_latency_s,
        "join_latency_s": telemetry.join_latency_s,
        "evaluations_completed": telemetry.evaluations_completed,
        "evaluations_after_request": telemetry.evaluations_after_request,
        "longest_evaluation_s": telemetry.longest_evaluation_s,
        "longest_block_s": telemetry.longest_block_s,
        "longest_block_evaluations": telemetry.longest_block_evaluations,
        "external_processes_started": telemetry.external_processes_started,
        "external_processes_skipped": telemetry.external_processes_skipped,
        "external_processes_terminated": telemetry.external_processes_terminated,
        "stop_reason": telemetry.stop_reason,
        "optimizer_termination": telemetry.termination,
        "events_recorded": telemetry.events.len(),
        "events_dropped": telemetry.events_dropped,
        "units": "every *_s field is floating-point seconds on the monotonic clock; \
                  acknowledgement is request to first observation by the running search, \
                  drain is request to search return, join is request to worker collection",
    })
}

fn panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = panic.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

fn error_row(
    name: &str,
    result_dir: &Path,
    seed: u64,
    configuration: &Configuration,
    detail: &str,
) -> Value {
    json!({
        "preset": name,
        "status": "error",
        "status_detail": detail,
        "seed": seed,
        "solver_preset": configuration.reported_name(),
                "configuration": configuration.provenance(),
        "output_dir": result_dir.display().to_string(),
    })
}

// A success row is assembled from every measured quantity at once, so the
// argument list is the row definition rather than incidental coupling.
#[allow(clippy::too_many_arguments)]
fn build_success_row(
    name: &str,
    result_dir: &Path,
    seed: u64,
    configuration: &Configuration,
    settings: &SolverSettings,
    wall_time: Duration,
    result: &PipelineResult,
    telemetry: &CancelSnapshot,
) -> Value {
    if let Ok(yaml) = serde_yaml::to_string(&result.config) {
        let _ = fs::write(result_dir.join("effective_config.yaml"), yaml);
    }

    let optimization = result.optimization_result.as_ref();
    let optimizer_best_valid = optimization.map(|o| o.best_valid);
    // The optimizer's own feasibility flag is a search-quality signal, not a
    // manufacturability claim: no wing-fuselage intersection or geometric-
    // plausibility constraint exists in `alas-opt` today
    // (`docs/optimizer-design-vector.md`). This row never asserts the
    // opposite.
    //
    // `is_delivered_feasible` is the stricter conjunction the optimizer itself
    // owns: a valid winner, a finite objective, a run that was not cancelled,
    // and a reporting-fidelity verdict that accepted the delivered design.
    // `converged` is reported separately and is never folded into it, because
    // a budget-exhausted search can deliver a verified aircraft and a
    // converged one can still be rejected at reporting fidelity.
    let optimizer_cancelled = optimization.is_some_and(alas_opt::OptimizationResult::was_cancelled);
    let optimizer_feasible =
        optimization.is_some_and(alas_opt::OptimizationResult::is_delivered_feasible);
    let optimizer_converged = optimization.is_some_and(alas_opt::OptimizationResult::converged);
    let status = if optimizer_cancelled {
        "cancelled"
    } else if optimizer_best_valid == Some(false)
        || optimization.is_some_and(|o| !o.is_delivered_feasible())
    {
        "infeasible"
    } else {
        "success"
    };

    let report = result
        .optimized_report
        .as_ref()
        .or(result.baseline_analysis.as_ref());
    let mtow_kg = result.config.requirements.mtow_kg;
    let payload_kg = report
        .and_then(|r| r.component_masses.get("Payload").copied())
        .unwrap_or(0.0);
    let fuel_kg = report
        .and_then(|r| r.component_masses.get("Fuel").copied())
        .unwrap_or(0.0);
    let oew_kg = (mtow_kg - payload_kg - fuel_kg).max(0.0);
    let cruise_l_over_d = report.map(|r| r.design_point.l_over_d);
    let neutral_point_x = report.map(|r| r.x_neutral_point);
    let static_margin = report.map(|r| r.static_margin);

    let public_planning_cg_violation = matches!(
        result.feasibility.cg_envelope.planning_status,
        PlanningCgStatus::ForwardLimitViolation | PlanningCgStatus::AftLimitViolation
    ) || result
        .feasibility
        .contains(FindingCode::PublicPlanningCgEnvelopeViolation);
    let governing_error_findings: Vec<&PhysicalFinding> = result
        .feasibility
        .findings
        .iter()
        .filter(|finding| finding.severity == FindingSeverity::Error && finding_governs(finding))
        .collect();

    let execution_passed = result.optimized_design.is_some()
        && cruise_l_over_d.is_some_and(f64::is_finite)
        && neutral_point_x.is_some_and(f64::is_finite);

    let evidence = preset_design_mission_evidence(name);
    let design_mission_source_backed = matches!(evidence, DesignMissionEvidence::SourceBacked(_));
    let design_mission_status = design_mission_status_label(&evidence);

    // Every clause a final candidate must satisfy before this harness may call
    // it physically feasible, each named so a failure says which one refused.
    // The list is the verdict: `physical_passed` is exactly "no blockers", so
    // a future clause cannot be added to the list and silently left out of the
    // boolean, and the boolean cannot be true while the list is non-empty.
    let mut feasibility_blockers: Vec<String> = Vec::new();
    if !execution_passed {
        feasibility_blockers.push(
            "the run delivered no optimized design with finite reported aerodynamics".to_owned(),
        );
    }
    if optimization.is_none() {
        feasibility_blockers.push(
            "the run produced no optimizer result to judge the final candidate by".to_owned(),
        );
    }
    if optimizer_cancelled {
        feasibility_blockers.push(
            "the search was cancelled and reached no stopping criterion of its own".to_owned(),
        );
    }
    if optimization.is_some() && !optimizer_feasible {
        feasibility_blockers.push(format!(
            "the optimizer did not deliver a feasible candidate (termination {})",
            optimization.map_or("none", |o| o.termination.as_str())
        ));
    }
    if optimization.is_some() && !optimizer_converged {
        feasibility_blockers.push(format!(
            "the search did not converge (termination {})",
            optimization.map_or("none", |o| o.termination.as_str())
        ));
    }
    if !governing_error_findings.is_empty() {
        feasibility_blockers.push(format!(
            "{} governing error-severity physical finding(s)",
            governing_error_findings.len()
        ));
    }
    if public_planning_cg_violation {
        feasibility_blockers.push("the public planning CG envelope is violated".to_owned());
    }
    // Written as an explicit finite-and-positive test rather than a negated
    // comparison so a NaN mass is a blocker instead of quietly passing.
    if !mtow_kg.is_finite() || mtow_kg <= 0.0 {
        feasibility_blockers.push("MTOW is not a finite positive mass".to_owned());
    }
    if !oew_kg.is_finite() || oew_kg <= 0.0 {
        feasibility_blockers.push(
            "OEW, as MTOW minus payload minus fuel, is not a finite positive mass".to_owned(),
        );
    }
    if !cruise_l_over_d.is_some_and(|v| v > 8.0) {
        feasibility_blockers.push(
            "the cruise lift-to-drag ratio is missing or at or below the 8.0 plausibility floor"
                .to_owned(),
        );
    }
    if !neutral_point_x.is_some_and(|v| v > 0.0) {
        feasibility_blockers
            .push("the neutral point is missing or not aft of the fuselage nose origin".to_owned());
    }
    if !design_mission_source_backed {
        feasibility_blockers.push(
            "no source-backed design mission is registered for this preset, so the mission the \
             candidate is sized against is unverified"
                .to_owned(),
        );
    }
    let physical_passed = feasibility_blockers.is_empty();

    let dependency_gaps = dependency_gaps(result);

    json!({
        "preset": name,
        "status": status,
        "seed": seed,
        "solver_preset": configuration.reported_name(),
                "configuration": configuration.provenance(),
        "solver_settings": settings_json(settings),
        "output_dir": result_dir.display().to_string(),
        "wall_time_s": wall_time.as_secs_f64(),
        "optimizer": {
            // The kernel that actually executed, as the optimizer reports it,
            // not the method token the configuration requested: the two differ
            // for the legacy population names, which have no kernel behind
            // them and run mesh adaptive direct search.
            "method": optimization.map(|o| o.method.clone()),
            "requested_method": settings.method.clone(),
            "strategy": optimization.map(|o| o.strategy.clone()),
            "termination": optimization.map(|o| o.termination.clone()),
            "cancelled": optimizer_cancelled,
            "converged": optimizer_converged,
            "delivered_feasible": optimizer_feasible,
            "best_valid": optimizer_best_valid,
            "best_cost": optimization.map(|o| o.best_cost),
            "reported_wall_time_s": optimization.map(|o| o.wall_time_s),
        },
        // Present on a completed run too: it carries the per-evaluation cost
        // that sets the cancellation bound, which is only measurable on a run
        // that was allowed to work.
        "cancellation": cancellation_json(telemetry),
        "baseline_vs_optimized": baseline_comparison(result),
        "execution_passed": execution_passed,
        "physical_passed": physical_passed,
        "feasibility_blockers": feasibility_blockers,
        "design_mission_status": design_mission_status,
        "design_mission_source_backed": design_mission_source_backed,
        "mtow_kg": mtow_kg,
        "oew_kg": oew_kg,
        "cruise_l_over_d": cruise_l_over_d,
        "neutral_point_x": neutral_point_x,
        "static_margin": static_margin,
        "metric_conventions": metric_conventions(),
        "public_planning_cg_status": format!("{:?}", result.feasibility.cg_envelope.planning_status),
        "governing_error_findings": governing_error_findings
            .iter()
            .map(|f| format_finding(f))
            .collect::<Vec<_>>(),
        "dependency_gaps": dependency_gaps,
        "seed_applied": result.execution.seed_applied,
    })
}

/// Units, frame, sign convention and validity condition for every physical
/// metric this row reports, recorded in the row itself.
///
/// A number without its frame is not a measurement: `neutral_point_x` in
/// particular is a station, and the feasibility clause that requires it to be
/// positive is only meaningful once the origin and the positive direction are
/// stated. These are the pipeline's own conventions, restated here so a saved
/// result is readable without the source that produced it.
fn metric_conventions() -> Value {
    json!({
        "frame": "Aircraft body axes: origin at the fuselage nose, x positive aft along the \
                  fuselage reference line, y positive to starboard, z positive up. Lengths in \
                  metres.",
        "mtow_kg": "Maximum take-off mass, kilograms, from the requirements group of the \
                    effective configuration. Not a computed result: it is the sizing requirement \
                    the candidate was closed against.",
        "oew_kg": "Operating empty mass, kilograms, computed here as mtow_kg - payload - fuel \
                   from the reported component mass breakdown; a missing payload or fuel entry \
                   is read as zero, which inflates this figure rather than failing silently, so \
                   it is only meaningful when the mass breakdown is complete.",
        "cruise_l_over_d": "Lift-to-drag ratio at the reported design point, dimensionless, \
                            trimmed. Valid only for the design-point Mach, altitude and mass \
                            recorded in the same report.",
        "neutral_point_x": "Stick-fixed neutral point station, metres aft of the fuselage nose \
                            along +x. Positive by construction for any physical aircraft, which \
                            is why a non-positive value is a feasibility blocker rather than a \
                            small number.",
        "static_margin": "(x_neutral_point - x_cg) / mean_aerodynamic_chord, dimensionless \
                          fraction of MAC. Positive means the neutral point is aft of the centre \
                          of gravity, the longitudinally stable sense. Not itself a pass/fail \
                          clause here: the CG envelope status is.",
        "seed": "The recorded seed is applied to the optimizer and to every seeded sampling \
                 stage; `seed_applied` in this row reports whether the run confirmed it. The \
                 search replays exactly for a fixed seed, bounds, starting point and evaluator.",
    })
}

fn preset_design_mission_evidence(name: &str) -> DesignMissionEvidence {
    presets::get(name)
        .map(|preset| preset.reference.design_mission_evidence.clone())
        .unwrap_or_default()
}

fn design_mission_status_label(evidence: &DesignMissionEvidence) -> &'static str {
    match evidence {
        DesignMissionEvidence::Unverified => "UNVERIFIED - no source-backed mission registered",
        DesignMissionEvidence::SourceBacked(_) => "NOT EVALUATED",
    }
}

fn is_mission_finding(code: FindingCode) -> bool {
    matches!(
        code,
        FindingCode::MissionUnavailable
            | FindingCode::MissionNotConverged
            | FindingCode::InvalidMissionFuelBurn
            | FindingCode::MissionFuelShortfall
    )
}

fn finding_governs(finding: &PhysicalFinding) -> bool {
    !is_mission_finding(finding.code)
}

fn format_finding(finding: &PhysicalFinding) -> String {
    match (finding.actual, finding.limit) {
        (Some(actual), Some(limit)) if !finding.unit.is_empty() => format!(
            "{} (actual {:.3} {}, limit {:.3} {})",
            finding.message, actual, finding.unit, limit, finding.unit
        ),
        _ => finding.message.clone(),
    }
}

/// Downstream external-tool stages this preset's config reached but for which
/// no executable was discovered on this host, distinct from a stage that ran
/// and failed. Only the "not discovered" variants are gaps; a launch failure
/// or solver failure is a real error surfaced elsewhere in the row.
fn dependency_gaps(result: &PipelineResult) -> Vec<String> {
    let mut gaps = Vec::new();
    if let Some(openvsp) = &result.openvsp_export {
        if openvsp.status == OpenVspExportStatus::ScriptWrittenRuntimeUnverified {
            gaps.push("openvsp: script written, no installed runtime discovered".to_owned());
        }
    }
    if let Some(vspaero) = &result.vspaero_result {
        if vspaero.status == VspaeroAnalysisStatus::NotConfigured {
            gaps.push("vspaero: no native executable configured or discovered".to_owned());
        }
    }
    if let Some(avl) = &result.avl_result {
        if avl.status == AvlAnalysisStatus::NotConfigured {
            gaps.push("avl: no solver executable available".to_owned());
        }
    }
    if let Some(mses) = &result.mses_result {
        use alas_aero::mses::MsesStatus;
        match mses.status {
            MsesStatus::Absent => gaps.push("mses: no MSES directory found".to_owned()),
            MsesStatus::Incomplete => {
                gaps.push("mses: directory present but missing one or more programs".to_owned())
            }
            _ => {}
        }
    }
    if let Some(flowunsteady) = &result.flowunsteady_result {
        if flowunsteady.status == FlowUnsteadyAnalysisStatus::NotConfigured {
            gaps.push("flowunsteady: no adapter executable configured".to_owned());
        }
    }
    gaps
}

fn format_matrix_report(document: &Value) -> String {
    let mut text = String::new();
    text.push_str("ALAS OPTIMIZATION-MODE PRESET ACCEPTANCE MATRIX\n");
    text.push_str(&format!(
        "seed={} solver_preset={} timeout_s={:?}\n\n",
        document["seed"], document["solver_preset"], document["timeout_s"]
    ));
    text.push_str(
        "Preset       | Status               | Wall(s) | Kernel               | Term                 | Exec | Conv | Feas | Phys | Mission\n",
    );
    text.push_str(
        "-------------+----------------------+---------+----------------------+----------------------+------+------+------+------+--------\n",
    );
    let flag =
        |row: &Value, key: &str| row[key].as_bool().map_or("-".to_owned(), |v| v.to_string());
    if let Some(rows) = document["presets"].as_array() {
        for row in rows {
            let name = row["preset"].as_str().unwrap_or("?");
            let status = row["status"].as_str().unwrap_or("?");
            let wall = row["wall_time_s"]
                .as_f64()
                .map_or("-".to_owned(), |v| format!("{v:.1}"));
            let kernel = row["optimizer"]["method"].as_str().unwrap_or("-");
            let termination = row["optimizer"]["termination"].as_str().unwrap_or("-");
            let exec = flag(row, "execution_passed");
            let converged = row["optimizer"]["converged"]
                .as_bool()
                .map_or("-".to_owned(), |v| v.to_string());
            let feasible = row["optimizer"]["delivered_feasible"]
                .as_bool()
                .map_or("-".to_owned(), |v| v.to_string());
            let phys = flag(row, "physical_passed");
            let mission = row["design_mission_status"].as_str().unwrap_or("-");
            text.push_str(&format!(
                "{name:<12} | {status:<20} | {wall:<7} | {kernel:<20} | {termination:<20} | {exec:<4} | {converged:<4} | {feasible:<4} | {phys:<4} | {mission}\n"
            ));
        }
    }
    // Every clause that refused a row, named. A `Phys` column of `false` with
    // no stated reason is what let a reader assume the reason was the known
    // mission gap; this prints the actual list instead.
    if let Some(rows) = document["presets"].as_array() {
        text.push_str("\nFeasibility blockers, per preset:\n");
        for row in rows {
            let name = row["preset"].as_str().unwrap_or("?");
            match row["feasibility_blockers"].as_array() {
                Some(blockers) if !blockers.is_empty() => {
                    text.push_str(&format!("  {name}:\n"));
                    for blocker in blockers {
                        text.push_str(&format!(
                            "    - {}\n",
                            blocker.as_str().unwrap_or("(unnamed)")
                        ));
                    }
                }
                Some(_) => text.push_str(&format!("  {name}: none\n")),
                None => text.push_str(&format!(
                    "  {name}: not assessed (the run did not reach a final candidate)\n"
                )),
            }
        }
    }
    text.push_str(&format!(
        "\nAll presets ran to a success status: {}\n",
        document["all_success"]
    ));
    text.push_str(
        "Definitions. Kernel is the search that actually executed, which is not always the\n\
         configured method token: the legacy population names have no kernel behind them and\n\
         run mesh adaptive direct search, reported as `mads`. Conv is the search's verdict on\n\
         its own stopping criterion; a cancelled, watchdog-stopped or budget-exhausted run is\n\
         never converged. Feas is the optimizer's delivered-candidate verdict: a valid winner,\n\
         a finite objective, not cancelled, and accepted by the reporting-fidelity\n\
         re-evaluation. Phys additionally requires convergence, no governing error-severity\n\
         finding, no planning CG violation, physically plausible reported metrics, and a\n\
         source-backed design mission. None of these is a manufacturability claim: no\n\
         wing-fuselage intersection or geometric-plausibility constraint exists in `alas-opt`\n\
         (`docs/optimizer-design-vector.md`). A design-mission status of UNVERIFIED is a known,\n\
         documented gap and no preset currently clears it, so Phys is expected false\n\
         workspace-wide until a source-backed mission is registered; it is listed as a blocker\n\
         rather than excused.\n",
    );
    text
}

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn args() -> Args {
        Args {
            output_dir: PathBuf::from("."),
            seed: DEFAULT_SEED,
            solver_preset: "quick_draft".to_owned(),
            timeout_s: None,
            preset: Some("A320-200".to_owned()),
            experiment: None,
            max_iterations: None,
            population_size: None,
        }
    }

    #[test]
    fn an_unmodified_run_reports_the_registered_preset_and_its_settings_verbatim() {
        let (configuration, settings) =
            Configuration::resolve(&args()).expect("quick_draft is registered");
        assert_eq!(configuration.reported_name(), "quick_draft");
        assert_eq!(
            configuration.provenance()["kind"],
            "registered_solver_preset"
        );
        let registered = resolve_settings("quick_draft").expect("quick_draft is registered");
        assert_eq!(settings.max_iterations, registered.max_iterations);
        assert_eq!(settings.population_size, registered.population_size);
        assert_eq!(settings.tolerance, registered.tolerance);
    }

    /// The rule the whole mechanism exists for: search effort cannot be
    /// reduced without a label, so a reduced-budget run can never be read as
    /// the preset it was derived from.
    #[test]
    fn an_unlabelled_search_effort_override_is_refused() {
        let mut generations_only = args();
        generations_only.max_iterations = Some(2);
        let error = Configuration::resolve(&generations_only)
            .expect_err("an unlabelled override must not resolve");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(
            error.to_string().contains("--experiment"),
            "the refusal must name the flag that would make the run honest: {error}"
        );

        let mut population_only = args();
        population_only.population_size = Some(1);
        assert!(Configuration::resolve(&population_only).is_err());
    }

    #[test]
    fn an_experiment_label_may_not_impersonate_a_registered_preset() {
        let mut request = args();
        request.max_iterations = Some(2);
        request.experiment = Some("balanced".to_owned());
        let error =
            Configuration::resolve(&request).expect_err("a registered name is not a valid label");
        assert!(error.to_string().contains("registered solver preset"));
    }

    #[test]
    fn a_labelled_experiment_reports_its_label_its_origin_and_every_override() {
        let mut request = args();
        request.max_iterations = Some(2);
        request.population_size = Some(1);
        request.experiment = Some("a320-finite-measurement-2026-09-22".to_owned());
        let (configuration, settings) =
            Configuration::resolve(&request).expect("a labelled override resolves");

        assert_eq!(settings.max_iterations, 2);
        assert_eq!(settings.population_size, 1);
        // Everything the flags do not reach is still the registered preset's.
        let registered = resolve_settings("quick_draft").expect("quick_draft is registered");
        assert_eq!(settings.tolerance, registered.tolerance);
        assert_eq!(settings.strategy, registered.strategy);
        assert_eq!(settings.method, registered.method);

        assert_eq!(
            configuration.reported_name(),
            "a320-finite-measurement-2026-09-22",
            "the row must never report this run as the preset it was derived from"
        );
        let provenance = configuration.provenance();
        assert_eq!(provenance["kind"], "experiment");
        assert_eq!(provenance["derived_from_solver_preset"], "quick_draft");
        let overrides = provenance["search_effort_overrides"]
            .as_array()
            .expect("the overrides are a list");
        assert_eq!(overrides.len(), 2);
        assert_eq!(overrides[0]["setting"], "max_iterations");
        assert_eq!(overrides[0]["registered_value"], registered.max_iterations);
        assert_eq!(overrides[0]["experiment_value"], 2);
    }

    #[test]
    fn the_acknowledgement_bound_is_the_largest_uninterruptible_unit() {
        let watch = CancelWatch::new();
        let scope = alas_opt::CancelScope::attach(Some(watch.flag()));
        assert!(acknowledgement_bound(&watch.snapshot()).is_none());
        scope.evaluation(|| std::thread::sleep(Duration::from_millis(2)));
        scope.block(8, || std::thread::sleep(Duration::from_millis(20)));
        let snapshot = watch.snapshot();
        let bound = acknowledgement_bound(&snapshot).expect("both units were timed");
        assert_eq!(
            bound,
            snapshot
                .longest_block_s
                .expect("the block was the longer unit")
        );
        assert!(bound > snapshot.longest_evaluation_s.unwrap_or(0.0));
    }
}
