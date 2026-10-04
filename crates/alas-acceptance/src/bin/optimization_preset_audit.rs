// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Optimization-mode acceptance runner: runs the real headless pipeline with
//! `PipelineOptions.optimize = true`, a fixed recorded seed, and a bounded
//! solver setting, for either one measurement preset or the full registered
//! preset matrix.
//!
//! This is deliberately separate from `preset_audit` (baseline, `optimize:
//! false`), which it reuses for its own purpose and does not reimplement. The
//! optimizer has separate screening and refinement deadlines and evaluation
//! ceilings. A `--timeout-s` guard here bounds the entire pipeline. On
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

use alas_config::{presets, AlasConfig, DesignMissionEvidence, MtowSizing, SolverSettings};
use alas_exec::{RunEnvironment, ToolLocator, ToolPreferences};
use alas_opt::{CancelSnapshot, CancelWatch, StopReason};
use alas_pipeline::{
    AvlAnalysisStatus, DesignPipeline, FindingCode, FindingSeverity, FlowUnsteadyAnalysisStatus,
    OpenVspExportStatus, PhysicalFinding, PipelineOptions, PipelineResult, PlanningCgStatus,
    VspaeroAnalysisStatus,
};
use serde_json::{json, Value};

#[path = "optimization_preset_audit/comparison.rs"]
mod comparison;
use comparison::{baseline_comparison, design_gross_mass_kg};
#[path = "optimization_preset_audit/configuration.rs"]
mod configuration;
use configuration::Configuration;
#[path = "optimization_preset_audit/controls.rs"]
mod controls;
#[path = "optimization_preset_audit/diagnostics.rs"]
mod diagnostics;
#[path = "optimization_preset_audit/exposure.rs"]
mod exposure;
use diagnostics::dependency_gaps;
#[path = "optimization_preset_audit/preparation.rs"]
mod preparation;
#[path = "optimization_preset_audit/route_profile.rs"]
mod route_profile;
#[path = "optimization_preset_audit/search_evidence.rs"]
mod search_evidence;
#[path = "optimization_preset_audit/snapshot_evidence.rs"]
mod snapshot_evidence;

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
                "usage:\n  optimization_preset_audit measure --preset <NAME> --output-dir <DIR> [--seed N] [--solver-preset NAME] [--timeout-s N] [--native-only | --external-tools] [--mtow-mode MODE] [--experiment LABEL [--time-limit S] [--max-evaluations N] [--screening-time-limit S] [--screening-max-evaluations N] [--replay-evaluations S,R[,P[,Q]]] [--stop-on-evaluations] [--workers N]]\n  optimization_preset_audit matrix --output-dir <DIR> [--seed N] [--solver-preset NAME] [--timeout-s N] [--native-only | --external-tools] [--mtow-mode MODE] [--time-limit S] [--max-evaluations N]\n\n  Native execution is the default; --external-tools allows configured host tools.

  --mtow-mode accepts the user-facing modes fixed_requirement, mtow_band and payload_adjusted.\n  The mode is recorded in the row. --time-limit and\n  --max-evaluations are refused until the solver settings carry such a budget.\n\n  --experiment LABEL marks a measurement/engineering-budget run. It is required before\n  the stage budget flags (refinement --time-limit and --max-evaluations, and their\n  screening counterparts) may change the registered solver preset's search\n  effort, and the label is recorded in every row and artefact the run writes. It changes\n  numerical search effort only. --replay-evaluations S,R,P,Q replays the stage replay_evaluations\n  S,R, refinement planned_evaluations P and restoration_evaluations Q a time-limited run recorded (row field optimizer.stages), bit-identically at any worker count;\n  --stop-on-evaluations ignores both time limits; --workers N sets the native worker threads. No requirement, constraint, mission, design space or\n  acceptance clause is reachable from it."
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
    native_only: bool,
    /// `--mtow-mode`: the takeoff-mass sizing mode, a modelling choice
    /// recorded in the row rather than a search-effort setting.
    mtow_mode: Option<MtowSizing>,
    /// `--time-limit` (seconds) and `--max-evaluations`: the refinement
    /// stage's limits.
    time_limit_s: Option<u64>,
    max_evaluations: Option<u64>,
    /// `--screening-time-limit` and `--screening-max-evaluations`: the
    /// screening stage's limits.
    screening_time_limit_s: Option<u64>,
    screening_max_evaluations: Option<u64>,
    /// `--replay-evaluations S,R,P,Q`: the stage replay counts, refinement
    /// planned budget and restoration count a run recorded, replayed exactly.
    replay_evaluations: Option<(u64, u64, Option<u64>, Option<u64>)>,
    /// `--stop-on-evaluations`: ignore both stage time limits.
    stop_on_evaluations: bool,
    /// `--workers N`: the native worker threads, to check that a replay is
    /// bit-identical at any worker count.
    workers: Option<u64>,
}

fn parse_args(values: &[String]) -> io::Result<Args> {
    let mut output_dir = None;
    let mut seed = DEFAULT_SEED;
    let mut solver_preset = "quick_draft".to_owned();
    let mut timeout_s = None;
    let mut preset = None;
    let mut experiment = None;
    let mut native_only = true;
    let mut mtow_mode = None;
    let mut time_limit_s = None;
    let mut max_evaluations = None;
    let mut screening_time_limit_s = None;
    let mut screening_max_evaluations = None;
    let mut replay_evaluations = None;
    let mut stop_on_evaluations = false;
    let mut workers = None;
    let mut index = 0;
    while index < values.len() {
        match values[index].as_str() {
            "--native-only" => native_only = true,
            "--external-tools" => native_only = false,
            "--mtow-mode" => {
                index += 1;
                mtow_mode = Some(controls::parse_mtow_mode(values.get(index))?);
            }
            "--time-limit" => {
                index += 1;
                time_limit_s = Some(controls::parse_count("--time-limit", values.get(index))?);
            }
            "--max-evaluations" => {
                index += 1;
                max_evaluations = Some(controls::parse_count(
                    "--max-evaluations",
                    values.get(index),
                )?);
            }
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
            flag @ ("--screening-time-limit" | "--screening-max-evaluations") => {
                index += 1;
                let value = Some(controls::parse_count(flag, values.get(index))?);
                if flag == "--screening-time-limit" {
                    screening_time_limit_s = value;
                } else {
                    screening_max_evaluations = value;
                }
            }
            "--replay-evaluations" => {
                index += 1;
                replay_evaluations = Some(controls::parse_replay(values.get(index))?);
            }
            "--stop-on-evaluations" => stop_on_evaluations = true,
            "--workers" => {
                index += 1;
                workers = Some(controls::parse_count("--workers", values.get(index))?);
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
        native_only,
        mtow_mode,
        time_limit_s,
        max_evaluations,
        screening_time_limit_s,
        screening_max_evaluations,
        replay_evaluations,
        stop_on_evaluations,
        workers,
    })
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
        args.native_only,
        args.mtow_mode,
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
            args.native_only,
            args.mtow_mode,
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
        "execution_scope": preparation::execution_scope(args.native_only),
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
        "screening": settings.screening,
        "refinement": settings.refinement,
        "tolerance": settings.tolerance,
        "workers": settings.workers,
        "resolved_workers": settings.resolved_workers(),
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
        let snapshots = snapshot_evidence::SnapshotEvidence::start(options.output_dir.as_deref());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let on_event = |event: alas_pipeline::runs::RunEvent| {
                eprintln!(
                    "[stage] {}",
                    serde_json::to_string(&event).unwrap_or_default()
                );
            };
            let preset = presets::get(&config.preset).map_err(|e| e.to_string())?;
            let design = preset.design_vector;
            let bounds = alas_opt::DesignOptimizer::new(config.clone())
                .resolved_bounds(None, Some(&design))
                .map_err(|error| error.to_string())?;
            DesignPipeline::new(config).run_with_design_space_events_and_snapshots(
                &options,
                &environment,
                &design,
                &bounds,
                alas_pipeline::RunObservers {
                    events: &on_event,
                    snapshots: &|snapshot| snapshots.publish(snapshot),
                    cancel: worker_watch.flag(),
                },
            )
        }));
        snapshots.finish();
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

    let guarded_start = Instant::now();
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
    let (result, _worker_elapsed) = loop {
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
        observed: guarded_start.elapsed(),
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
    native_only: bool,
    mtow_mode: Option<MtowSizing>,
) -> Value {
    let result_dir = output_root.join(name).join("optimize");
    let evidence_error = search_evidence::prepare(&result_dir);
    let mut row = evaluate_preset_optimization_inner(
        name,
        output_root,
        locator,
        preferences,
        seed,
        configuration,
        settings,
        timeout,
        native_only,
        mtow_mode,
    );
    search_evidence::attach(&mut row, &result_dir, evidence_error.as_deref());
    row["baseline_execution_scope"] = json!("native_only");
    row["baseline_external_completion_claimed"] = json!(false);
    preparation::record_scope(&mut row, native_only);
    row
}

// Keep the inner measurement inputs explicit, matching the scope-recording wrapper.
// Each argument records an independent audit input; keep their provenance explicit.
#[allow(clippy::too_many_arguments)]
fn evaluate_preset_optimization_inner(
    name: &str,
    output_root: &Path,
    locator: &ToolLocator,
    preferences: &ToolPreferences,
    seed: u64,
    configuration: &Configuration,
    settings: &SolverSettings,
    timeout: Option<Duration>,
    native_only: bool,
    mtow_mode: Option<MtowSizing>,
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
    // `matrix/evaluate.rs`, which this mirrors for the optimization case
    // it does not cover).
    let mut config = match AlasConfig::from_value(&json!({ "preset": preset.name })) {
        Ok(config) => config,
        Err(error) => return error_row(name, &result_dir, seed, configuration, &error.to_string()),
    };
    config.optimizer.solver = settings.clone();
    if let Some(mode) = mtow_mode {
        config.optimizer.objective.mtow_sizing = mode;
    }
    preparation::apply_machine_preferences(&mut config, preferences);
    preparation::initialize_route_profile(&mut config);
    preparation::apply_execution_scope(&mut config, native_only);
    let scope = preparation::execution_scope(native_only);
    eprintln!("[scope] preset={name} execution_scope={scope}; native-only does not certify external-solver completion");
    let _ = fs::write(
        result_dir.join("input_config.audit.json"),
        serde_json::to_vec_pretty(&json!({"execution_scope": scope,
            "config_file": "input_config.json",
            "optional_external_execution_requested": !native_only}))
        .unwrap_or_default(),
    );

    if let Err(error) = fs::write(
        result_dir.join("input_config.json"),
        serde_json::to_vec_pretty(&config).unwrap_or_default(),
    ) {
        eprintln!("[warn] could not persist input config for {name}: {error}");
    }

    let environment = if native_only {
        RunEnvironment::default()
    } else {
        locator.resolve_environment(
            Path::new(&config.mses.mses_dir),
            Path::new(&config.structures.nastran_exe_path),
            Path::new(&config.structures.patran_exe_path),
            Path::new(preferences.openvsp_dir.as_deref().unwrap_or("")),
            Path::new(preferences.avl_exe.as_deref().unwrap_or("")),
        )
    };

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

    let baseline_config = config.clone();
    let baseline_environment = environment.clone();
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
                // Use the watch's request-to-join interval; worker start and
                // coordinator guard have different origins under contention.
                "cancellation_latency_s": telemetry.join_latency_s,
                "cancellation": cancellation_json(&telemetry),
                "acknowledgement_contract_s": ACKNOWLEDGEMENT_CONTRACT_S,
                "acknowledgement_contract_met": acknowledged,
                "worker_joined": true,
                "worker_panicked": panicked,
                // A time-limited run is never feasible and never converged:
                // the search was stopped from outside before it decided
                // anything.
                "execution_passed": false,
                "full_analysis_completed": false,
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

    let baseline_run = controls::run_baseline_pipeline(
        baseline_config,
        baseline_environment,
        seed,
        &result_dir,
        timeout,
    );
    let exposure = exposure::Exposure {
        preset_design: &preset.design_vector,
        baseline_run: baseline_run.as_ref().ok(),
        baseline_run_error: baseline_run.as_ref().err().map(String::as_str),
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
        &exposure,
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
        "execution_passed": false,
        "full_analysis_completed": false,
        "physical_passed": false,
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
    exposure: &exposure::Exposure<'_>,
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
    let oew_kg = report.map_or(0.0, |r| {
        (design_gross_mass_kg(r) - payload_kg - fuel_kg).max(0.0)
    });
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
            // for the older population names, which have no kernel behind
            // them and run mesh adaptive direct search.
            "method": optimization.map(|o| o.method.clone()),
            "requested_method": settings.method.clone(),
            "strategy": optimization.map(|o| o.strategy.clone()),
            "termination": optimization.map(|o| o.termination.clone()),
            // Per stage: replay with `--replay-evaluations S,R,P,Q`.
            "stages": optimization
                .and_then(|o| o.search_diagnostics.as_ref())
                .map(exposure::stages_json),
            "cancelled": optimizer_cancelled,
            "converged": optimizer_converged,
            "delivered_feasible": optimizer_feasible,
            "best_valid": optimizer_best_valid,
            "total_full_fidelity_valid": optimization.map(|o| exposure::full_fidelity_valid(&o.history)),
            "valid_candidate_definition": "Distinct design vectors in the full in-loop history with valid=true, zero hard violation, and finite objective and ranking cost. Separate screening scores are excluded; reporting mesh verification is recorded for the delivered finalist.",
            "best_cost": optimization.map(|o| o.best_cost),
            "reported_wall_time_s": optimization.map(|o| o.wall_time_s),
        },
        // Present on a completed run too: it carries the per-evaluation cost
        // that sets the cancellation bound, which is only measurable on a run
        // that was allowed to work.
        "cancellation": cancellation_json(telemetry),
        "baseline_vs_optimized": baseline_comparison(result),
        "like_for_like": exposure::exposure_json(result, exposure),
        "mtow_mode": result.config.optimizer.objective.mtow_sizing.as_str(),
        "execution_passed": execution_passed,
        "full_analysis_completed": diagnostics::full_analysis_completed(result),
        "analysis_evidence": diagnostics::evidence(result),
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
        // The delivered report's own model CG verdict line, so a feasible
        // delivery that prints a failing hard constraint is visible here.
        "model_cg_status": alas_pipeline::format_feasibility(&result.feasibility)
            .lines()
            .find(|line| line.starts_with("Model CG status"))
            .map(str::to_owned),
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
        "oew_kg": "Operating empty mass, kilograms: the sum of the reported component masses \
                   excluding Payload and Fuel. Fuel is the signed MTOW - MZFW closure at the \
                   design gross mass the candidate was sized at, which equals mtow_kg only for a \
                   fixed-aircraft basis; a clean-sheet candidate closes at its own take-off mass.",
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

fn format_matrix_report(document: &Value) -> String {
    let mut text = String::new();
    text.push_str("ALAS OPTIMIZATION-MODE PRESET ACCEPTANCE MATRIX\n");
    text.push_str(&format!(
        "seed={} solver_preset={} timeout_s={:?} execution_scope={}\n\n",
        document["seed"],
        document["solver_preset"],
        document["timeout_s"],
        document["execution_scope"]
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
        "Definitions. Kernel is the search that actually executed: differential evolution\n\
         (L-SHADE, epsilon-constrained) is the only kernel this build runs; a saved config\n\
         naming a retired token is migrated to it at load time. Conv is the search's verdict on\n\
         its own stopping criterion; a cancelled or budget-exhausted run is\n\
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

#[cfg(test)]
#[path = "optimization_preset_audit/tests.rs"]
mod tests;
