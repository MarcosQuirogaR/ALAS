// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Runs started by the server: one at a time, on their own thread, polled and
//! cancelled by id; and the tools that read what a run wrote.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use alas_pipeline::runs::RunEvent;
use alas_pipeline::PipelineResult;
use serde_json::{json, Value};

use super::tools::{bool_arg, str_arg, tool, Outcome};
use super::workspace::{self, Workspace, RUNS_DIR};
use super::Server;
use crate::cli::{execute_run, CliArgs, RunOutcome};

/// File the server writes beside a run's outputs.
pub const RESULTS_FILE: &str = "results.json";

/// Schema tag of [`RESULTS_FILE`].
pub const RESULTS_SCHEMA: &str = "alas.headless-results.v1";

/// Events kept per run for `run_status` (the newest).
const EVENTS_KEPT: usize = 40;

/// Largest file `read_output_file` returns, bytes.
const MAX_READ_BYTES: u64 = 1024 * 1024;

/// Most files `get_results` lists.
const MAX_LISTED_FILES: usize = 500;

/// What a run has done so far.
struct RunState {
    status: &'static str,
    events: Vec<Value>,
    event_count: usize,
    error: Option<String>,
    elapsed_s: f64,
}

struct Run {
    id: String,
    name: String,
    output: String,
    started: Instant,
    cancel: AtomicBool,
    state: Mutex<RunState>,
}

impl Run {
    fn update(&self, change: impl FnOnce(&mut RunState)) {
        if let Ok(mut state) = self.state.lock() {
            change(&mut state);
        }
    }

    fn status(&self) -> Value {
        let Ok(state) = self.state.lock() else {
            return json!({ "run_id": self.id, "status": "unknown" });
        };
        let elapsed_s = if state.status == "running" {
            self.started.elapsed().as_secs_f64()
        } else {
            state.elapsed_s
        };
        json!({
            "run_id": self.id,
            "run_name": self.name,
            "output_dir": self.output,
            "status": state.status,
            "elapsed_s": elapsed_s,
            "event_count": state.event_count,
            "recent_events": state.events,
            "error": state.error,
        })
    }
}

/// The session's runs.
#[derive(Default)]
pub struct RunManager {
    runs: Vec<Arc<Run>>,
    worker: Option<JoinHandle<()>>,
}

impl RunManager {
    /// Whether a run is still going.
    pub fn is_running(&self) -> bool {
        self.worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
    }

    fn find(&self, id: &str) -> Result<&Arc<Run>, String> {
        self.runs
            .iter()
            .find(|run| run.id == id)
            .ok_or_else(|| format!("no run {id:?} in this session"))
    }

    /// Cancel a run in progress and wait for it.
    pub fn shutdown(&mut self) {
        for run in &self.runs {
            run.cancel.store(true, Ordering::Relaxed);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Dispatch the run tools.
pub fn call(server: &mut Server, name: &str, args: &Value) -> Outcome {
    let result = match name {
        "start_run" => start_run(server, args),
        "run_status" => run_status(&server.runs, args),
        "cancel_run" => cancel_run(&server.runs, args),
        "get_results" => get_results(&server.workspace, args),
        "read_output_file" => read_output_file(&server.workspace, args),
        _ => return Outcome::Unknown,
    };
    result.into()
}

fn start_run(server: &mut Server, args: &Value) -> Result<Value, String> {
    if server.runs.is_running() {
        return Err("a run is in progress; wait for it or cancel it first".to_owned());
    }
    let ws = &server.workspace;
    let config_path = ws.resolve(str_arg(args, "config_path")?)?;
    let name = str_arg(args, "run_name")?;
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        || name.starts_with('.')
    {
        return Err("run_name: letters, digits, '-', '_' and '.' only".to_owned());
    }
    let output = ws.resolve(&format!("{RUNS_DIR}/{name}"))?;
    if output.exists() {
        return Err(format!("{RUNS_DIR}/{name} exists; choose another run_name"));
    }
    let required = |key: &str| {
        args.get(key)
            .and_then(Value::as_bool)
            .ok_or(format!("argument {key} (true or false) is required"))
    };
    let document = workspace::read_document(&config_path)?;
    let (config, _) = workspace::load(&document)?;
    workspace::check_locations(&config)?;
    if !config.mission.simbrief_username.trim().is_empty() {
        return Err(
            "the server does not reach the network: clear mission.simbrief_username".to_owned(),
        );
    }
    crate::custom_airports::install(&ws.airport_store(), Some(&document))?;

    let mut cli = CliArgs {
        config: Some(config_path),
        output: output.clone(),
        no_optimize: !required("optimize")?,
        no_baseline: !required("baseline")?,
        no_mission: !bool_arg(args, "mission", true)?,
        no_parallel: !bool_arg(args, "parallel", true)?,
        plots: bool_arg(args, "plots", true)?,
        plot_data: bool_arg(args, "plot_data", true)?,
        quiet: true,
        ..CliArgs::default()
    };
    cli.plots |= cli.plot_data;
    if let Some(seed) = args.get("seed").filter(|v| !v.is_null()) {
        cli.seed = Some(seed.as_u64().ok_or("seed must be a non-negative integer")?);
    }
    if let Some(mode) = args.get("aero_solver").and_then(Value::as_str) {
        cli.aerodynamic_solver = mode.parse()?;
    }
    if let Some(mode) = args.get("optimization_solver").and_then(Value::as_str) {
        cli.optimization_solver = mode.parse()?;
    }
    // The seed and the mission switch are configuration overrides, applied
    // as the command line applies them.
    let mut config = crate::cli::load_config(&cli)?;
    // `load_config` reads the file again from disk, so the config that runs
    // is checked itself: a rewrite between the two reads must
    // not reach the solver with an unchecked location.
    workspace::check_locations(&config)?;
    // Tool and data locations come from the user's preferences only; the
    // document was checked to leave them at their defaults.
    let preferences = ws.locator().load_preferences();
    crate::cli::apply_cli_tool_preferences(&mut config, &preferences, true);

    let run = Arc::new(Run {
        id: format!("run-{}", server.runs.runs.len() + 1),
        name: name.to_owned(),
        output: ws.relative(&output),
        started: Instant::now(),
        cancel: AtomicBool::new(false),
        state: Mutex::new(RunState {
            status: "running",
            events: Vec::new(),
            event_count: 0,
            error: None,
            elapsed_s: 0.0,
        }),
    });
    let worker_run = Arc::clone(&run);
    let locator = ws.locator().clone();
    let worker = alas_gui::worker_stack::spawn_analysis_thread("alas-server-run", move || {
        let run = worker_run;
        let record = |event: RunEvent| {
            let value = serde_json::to_value(&event).unwrap_or(Value::Null);
            run.update(|state| {
                state.event_count += 1;
                state.events.push(value);
                if state.events.len() > EVENTS_KEPT {
                    state.events.remove(0);
                }
            });
        };
        let outcome = execute_run(&cli, config, &locator, &record, Some(&run.cancel));
        let (status, error) = match outcome {
            Ok(outcome) => match write_results(&cli.output, &outcome) {
                Ok(()) => ("completed", None),
                Err(error) => ("completed", Some(format!("results.json: {error}"))),
            },
            Err(_) if run.cancel.load(Ordering::Relaxed) => ("cancelled", None),
            Err(error) => ("failed", Some(error)),
        };
        let elapsed_s = run.started.elapsed().as_secs_f64();
        run.update(|state| {
            state.status = status;
            state.error = error;
            state.elapsed_s = elapsed_s;
        });
    })
    .map_err(|error| format!("cannot start the run thread: {error}"))?;
    server.runs.worker = Some(worker);
    server.runs.runs.push(Arc::clone(&run));
    Ok(run.status())
}

fn run_status(runs: &RunManager, args: &Value) -> Result<Value, String> {
    match args.get("run_id").and_then(Value::as_str) {
        Some(id) => Ok(runs.find(id)?.status()),
        None => Ok(json!({ "runs": runs.runs.iter().map(|run| run.status()).collect::<Vec<_>>() })),
    }
}

fn cancel_run(runs: &RunManager, args: &Value) -> Result<Value, String> {
    let run = runs.find(str_arg(args, "run_id")?)?;
    run.cancel.store(true, Ordering::Relaxed);
    let mut status = run.status();
    status["note"] = json!("cancellation takes effect at the next stage boundary");
    Ok(status)
}

/// The facts a client needs from a result, as JSON (the pipeline result
/// itself is not serializable).
fn results_value(result: &PipelineResult) -> Value {
    let report = result
        .optimized_report
        .as_ref()
        .or(result.baseline_analysis.as_ref());
    let delivery = result.delivery();
    let findings: Vec<Value> = result
        .feasibility
        .findings
        .iter()
        .map(|finding| {
            json!({
                "code": format!("{:?}", finding.code),
                "severity": format!("{:?}", finding.severity),
                "message": finding.message,
                "actual": finding.actual,
                "limit": finding.limit,
                "unit": finding.unit,
            })
        })
        .collect();
    let stages: Vec<Value> = result
        .feasibility
        .mass_balance
        .as_ref()
        .and_then(|balance| balance.history.as_ref())
        .map(|history| {
            history
                .stages
                .iter()
                .map(|stage| {
                    json!({
                        "id": stage.id,
                        "kind": stage.label,
                        "fuel_kg": stage.fuel_kg,
                        "time_s": stage.time_s,
                        "distance_m": stage.distance_m,
                        "mass_start_kg": stage.mass_start_kg,
                        "mass_end_kg": stage.mass_end_kg,
                        "cg_end_pct_mac": stage.cg_end_pct_mac,
                        "cg_range_pct_mac": stage.cg_range_pct_mac,
                        "within_envelope": stage.within_envelope,
                        "field_check": stage.field.as_ref().map(|field| json!({
                            "field": field.field,
                            "mass_kg": field.mass_kg,
                            "required_m": field.required_m,
                            "available_m": field.available_m,
                            "feasible": field.feasible,
                        })),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    json!({
        "schema": RESULTS_SCHEMA,
        "preset": result.config.preset,
        "summary": report.map(|report| alas_pipeline::format_summary(report, Some(&result.config))),
        "feasibility": alas_pipeline::format_feasibility(&result.feasibility),
        "delivery": {
            "verdict": delivery.verdict.label(),
            "accepted": delivery.is_success(),
            "provenance": delivery.provenance.label(),
            "blockers": delivery.blockers.iter().map(|blocker| blocker.message()).collect::<Vec<_>>(),
            "advisories": delivery.advisories,
        },
        "findings": findings,
        "mission_stages": stages,
        "baseline_analysis_error": result.baseline_analysis_error,
    })
}

fn write_results(output: &Path, outcome: &RunOutcome) -> Result<(), String> {
    let mut value = results_value(&outcome.result);
    value["run_manifest_error"] = json!(outcome.manifest.as_ref().and_then(|m| m.as_ref().err()));
    value["plot_error"] = json!(outcome.plots.as_ref().and_then(|p| p.as_ref().err()));
    let text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    workspace::write_text(&output.join(RESULTS_FILE), &text)
}

fn list_files(ws: &Workspace, dir: &Path, out: &mut Vec<Value>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if out.len() >= MAX_LISTED_FILES {
            return;
        }
        let path = entry.path();
        match entry.metadata() {
            Ok(meta) if meta.is_dir() => list_files(ws, &path, out),
            Ok(meta) => out.push(json!({ "path": ws.relative(&path), "bytes": meta.len() })),
            Err(_) => {}
        }
    }
}

fn get_results(ws: &Workspace, args: &Value) -> Result<Value, String> {
    let dir = ws.resolve(&format!("{RUNS_DIR}/{}", str_arg(args, "run_name")?))?;
    if !dir.is_dir() {
        return Err(format!("{} does not exist", ws.relative(&dir)));
    }
    let read_json = |file: &str| -> Value {
        fs::read_to_string(dir.join(file))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Value::Null)
    };
    let mut files = Vec::new();
    list_files(ws, &dir, &mut files);
    Ok(json!({
        "output_dir": ws.relative(&dir),
        "results": read_json(RESULTS_FILE),
        "run_manifest": read_json(alas_pipeline::run_manifest::RUN_MANIFEST_FILE),
        "files": files,
        "files_truncated": files.len() >= MAX_LISTED_FILES,
    }))
}

fn read_output_file(ws: &Workspace, args: &Value) -> Result<Value, String> {
    let path = ws.resolve(str_arg(args, "path")?)?;
    let bytes = fs::metadata(&path)
        .map_err(|error| format!("{}: {error}", ws.relative(&path)))?
        .len();
    if bytes > MAX_READ_BYTES {
        return Err(format!(
            "{} is {bytes} bytes; the limit is {MAX_READ_BYTES}",
            ws.relative(&path)
        ));
    }
    let text = fs::read_to_string(&path)
        .map_err(|_| format!("{} is not a UTF-8 text file", ws.relative(&path)))?;
    Ok(json!({ "path": ws.relative(&path), "bytes": bytes, "text": text }))
}

/// The run tools' entries in the catalogue.
pub fn catalogue(path: &Value, config_path: &Value) -> Vec<Value> {
    vec![
        tool(
            "start_run",
            "Start a run of a configuration in the background and return its run_id at once; outputs go to runs/<run_name>. One run at a time. A full optimisation can take many minutes; optimize and baseline must be stated.",
            json!({
                "config_path": config_path,
                "run_name": { "type": "string", "description": "New folder name under runs/: letters, digits, '-', '_', '.'" },
                "optimize": { "type": "boolean", "description": "Run the design-space optimisation" },
                "baseline": { "type": "boolean", "description": "Run the baseline high-fidelity comparison" },
                "mission": { "type": "boolean", "description": "Fly the native mission (default true)" },
                "plots": { "type": "boolean", "description": "Write SVG figures (default true)" },
                "plot_data": { "type": "boolean", "description": "Write each figure's data as CSV and JSON (default true)" },
                "parallel": { "type": "boolean", "description": "Run independent stages in parallel (default true)" },
                "seed": { "type": "integer", "minimum": 0 },
                "aero_solver": { "type": "string", "enum": ["vlm", "avl", "both"] },
                "optimization_solver": { "type": "string", "enum": ["vlm", "avl", "both"] },
            }),
            &["config_path", "run_name", "optimize", "baseline"],
        ),
        tool(
            "run_status",
            "Status of a run (running, completed, failed, cancelled) with its latest stage events; without run_id, every run of this session.",
            json!({ "run_id": { "type": "string" } }),
            &[],
        ),
        tool(
            "cancel_run",
            "Ask a run to stop at its next stage boundary.",
            json!({ "run_id": { "type": "string" } }),
            &["run_id"],
        ),
        tool(
            "get_results",
            "A run's results.json (summary, findings, per-stage mission table), run manifest and the list of files it wrote (figures, plot data, reports).",
            json!({ "run_name": { "type": "string" } }),
            &["run_name"],
        ),
        tool(
            "read_output_file",
            "Read a text file in the workspace (SVG, CSV, JSON, Markdown), up to 1 MiB.",
            json!({ "path": path }),
            &["path"],
        ),
    ]
}
