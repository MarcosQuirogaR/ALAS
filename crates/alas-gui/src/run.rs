// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Launching and polling the background pipeline run.
//!
//! Split out of [`crate::state`] to keep that module under the line limit.

use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Instant;

use alas_pipeline::DesignPipeline;
use std::path::Path;

use crate::state::{AppState, LogKind, WorkerMessage};
use crate::views::{tr, tr_fields};
use alas_config::DesignMode;
use alas_exec::supervise::{launches_after, LaunchRecord};

#[path = "run/preset_barrier.rs"]
mod preset_barrier;
use alas_pipeline::{PipelineResult, RunEvent, RunEventKind, RunEventSeverity, RunObservers};

/// Prefix every pipeline stage uses for an error that is a cooperative
/// cancellation, not a failure (see `alas_pipeline::dual_solver`). The worker
/// uses it for a cancel seen before the pipeline starts, so both paths are
/// classified identically by `is_safe_cancellation`.
const CANCELLED_SAFELY: &str = "Cancelled safely";

/// Whether a run error reports a cooperative cancellation.
fn is_safe_cancellation(error: &str) -> bool {
    error.starts_with(CANCELLED_SAFELY)
}

impl AppState {
    /// Launch a full or baseline-only pipeline run in the background.
    pub fn start_pipeline(&mut self, baseline_only: bool) {
        if self.is_running {
            return;
        }
        // The "Analyze reference" button is a quick look: weight and balance,
        // stability and the report sheets need no external solver, so the
        // worker copy of the configuration drops them. A "Run" keeps them.
        let quick_look = baseline_only;
        // The baseline action is a fixed-aircraft sandbox by contract. Set the
        // typed design mode before decoding the worker config so the main
        // integrator receives the same mode the user just selected in the UI.
        if baseline_only {
            // The button implies the mode; it is not an edit of the case.
            self.without_marking_edit(|state| state.set_design_mode(DesignMode::BaselineSandbox));
        }
        let baseline_only = baseline_only || self.design_mode() == DesignMode::BaselineSandbox;
        self.refresh_clean_sheet_brief();
        self.enforce_design_space_fixed_variables();
        let mut config = match self.typed_config() {
            Some(c) => c,
            None => {
                self.log(
                    tr("Configuration is not currently valid; fix the highlighted fields."),
                    LogKind::Error,
                );
                return;
            }
        };
        let mut initial_design = match self.current_design() {
            Some(design) => design,
            None => {
                self.log(
                    tr("The initial design is incomplete; fix the Design Space values."),
                    LogKind::Error,
                );
                return;
            }
        };
        let mut bounds = match self.current_design_bounds() {
            Some(bounds) => bounds,
            None => {
                self.log(
                    tr("The optimizer bounds are incomplete; fix the Design Space values."),
                    LogKind::Error,
                );
                return;
            }
        };
        let Some(live_dispatch) = self.live_dispatch_allowed(&config, baseline_only) else {
            return;
        };
        let form_config = config.clone();
        self.apply_preset_dispatch_policy(&mut config, &mut initial_design, &mut bounds);
        if quick_look {
            skip_external_solvers(&mut config);
        }

        self.is_running = true;
        // The run log is not forced open: a failure opens it (see `log`), and
        // the menu toggles it, so a routine run no longer covers the page.
        self.run_started = Some(Instant::now());
        self.run_identity = self.run_identity.wrapping_add(1);
        self.status_message = "Running...".to_owned();
        self.stage.clear();
        self.pipeline_result = None;
        self.pipeline_result_design_values = Some(self.design_values.clone());
        self.pipeline_result_form_config = Some(form_config);
        self.pipeline_result_complete = false;
        self.clear_result_figures();
        self.patran_textures.clear();
        self.selected_solver_view = crate::views::results_view::SolverResultView::Vlm;
        self.cancel_flag.store(false, Ordering::Relaxed);
        alas_exec::supervise::block_launches(false);
        self.cancellation_requested = false;
        self.run_events.clear();
        self.log(
            if baseline_only {
                tr("Analyzing baseline (weight & balance + stability)...")
            } else {
                tr("Started pipeline execution.")
            },
            LogKind::Info,
        );
        if quick_look {
            self.log(
                "Reference analysis skips the external solvers (OpenVSP, VSPAERO, AVL, FLOWUnsteady, NASTRAN, Patran, MSES); Run executes the full chain.",
                LogKind::Info,
            );
        }

        let mut options = self.pipeline_options.clone();
        options.optimize = !baseline_only && self.run_options.optimize;
        options.compare_baseline = self.run_options.compare_baseline;
        options.parallel = self.run_options.parallel;
        options.save_plots = false;
        // The results view draws the structural-modes figure from this run.
        alas_struct::nastran95::request_modes_figure(true);
        if let Some(output_dir) = options.output_dir.take() {
            // The GUI can be launched from a read-only install directory.  A
            // relative path typed in Setup is resolved with the same policy as
            // preferences and navigation data, so creating the analysis
            // workspace never depends on the process current directory.
            let resolved = self.tool_locator.resolve_data_path(&output_dir);
            options.output_dir = Some(resolved.clone());
            // Keep the storage dialog and the path field aligned with the
            // actual run tree after a relative path is resolved.
            self.pipeline_options.output_dir = Some(resolved);
        }
        if !self.run_options.write_outputs {
            options.output_dir = None;
        }
        self.log(
            format!(
                "Run #{} | mode={} | optimizer={} | baseline={} | downstream={} | outputs={}",
                self.run_identity,
                if baseline_only { "baseline" } else { "full" },
                if options.optimize {
                    "enabled"
                } else {
                    "disabled"
                },
                if options.compare_baseline {
                    "enabled"
                } else {
                    "disabled"
                },
                if options.parallel {
                    "parallel"
                } else {
                    "sequential"
                },
                if options.output_dir.is_some() {
                    "retained"
                } else {
                    "temporary"
                },
            ),
            LogKind::Info,
        );

        self.record_addon_provenance(options.output_dir.as_deref());

        let (tx, rx): (Sender<WorkerMessage>, Receiver<WorkerMessage>) = channel();
        self.worker_rx = Some(rx);
        let cancel = self.cancel_flag.clone();
        let environment = self.tool_locator.resolve_environment(
            Path::new(&config.mses.mses_dir),
            Path::new(&config.structures.nastran_exe_path),
            Path::new(&config.structures.patran_exe_path),
            Path::new(self.tool_preferences.openvsp_dir.as_deref().unwrap_or("")),
            Path::new(self.tool_preferences.avl_exe.as_deref().unwrap_or("")),
        );

        let spawned = crate::worker_stack::spawn_analysis_thread("alas-run", move || {
            let pipeline = DesignPipeline::new(config).with_live_dispatch_fetch(live_dispatch);
            if cancel.load(Ordering::Relaxed) {
                let _ = tx.send(WorkerMessage::Finished(Box::new(Err(format!(
                    "{CANCELLED_SAFELY} before the run started"
                )))));
                return;
            }
            let event_tx = tx.clone();
            // Solver processes launched since the previous event are logged
            // ahead of it, so the run log ties every PID Task Manager shows to
            // the stage that started it.
            let launches_shown = std::sync::Mutex::new(0_u64);
            let report = move |event: RunEvent| {
                if !matches!(event.kind, RunEventKind::Progress) {
                    let mut shown = launches_shown
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let launches = launches_after(*shown);
                    if let Some(last) = launches.last() {
                        *shown = last.sequence;
                    }
                    for line in launch_events(&event, &launches) {
                        let _ = event_tx.send(WorkerMessage::Event(line));
                    }
                }
                let _ = event_tx.send(WorkerMessage::Event(event));
            };
            let snapshot_tx = tx.clone();
            let publish_snapshot = move |snapshot: PipelineResult| {
                let _ = snapshot_tx.send(WorkerMessage::Snapshot(Box::new(snapshot)));
            };
            let result = pipeline.run_with_design_space_events_and_snapshots(
                &options,
                &environment,
                &initial_design,
                &bounds,
                RunObservers {
                    events: &report,
                    snapshots: &publish_snapshot,
                    cancel: &cancel,
                },
            );
            let _ = tx.send(WorkerMessage::Finished(Box::new(result)));
        });
        // A failed spawn drops `tx`; the receiver sees the closed channel.
        drop(spawned);
    }

    /// Drain any pending progress and update the UI when a run finishes.
    pub fn poll_worker(&mut self) {
        let mut completed = None;
        let mut events = Vec::new();
        let mut latest_snapshot = None;
        if let Some(rx) = &self.worker_rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    WorkerMessage::Event(event) => events.push(event),
                    WorkerMessage::Snapshot(snapshot) => {
                        self.pipeline_result_complete = false;
                        latest_snapshot = Some(*snapshot);
                    }
                    WorkerMessage::Finished(res) => completed = Some(*res),
                }
            }
        }

        // Snapshots are immutable report boundaries. Keep the latest one so
        // the Results page can use its normal gallery while the worker still
        // runs downstream exports and optional analyses.
        if let Some(snapshot) = latest_snapshot {
            self.pipeline_result = Some(snapshot);
            self.update_result_scene();
        }

        let had_events = !events.is_empty();
        for event in events {
            if matches!(event.kind, RunEventKind::Progress) {
                self.stage = event.message.clone();
            }
            let kind = match event.severity {
                RunEventSeverity::Info => LogKind::Info,
                RunEventSeverity::Warning => LogKind::Warn,
                RunEventSeverity::Error => LogKind::Error,
            };
            if !matches!(event.kind, RunEventKind::Progress) {
                let label = event.stage.replace('_', " ");
                self.log(format!("{label}: {}", event.message), kind);
            }
            self.run_events.push(event);
        }
        if had_events && !self.cancellation_requested {
            if let Some(text) = active_stage_status(&self.run_events) {
                self.stage = text.clone();
                self.status_message = text;
            }
        }

        if let Some(res) = completed {
            alas_exec::supervise::block_launches(false);
            self.worker_rx = None;
            self.is_running = false;
            self.stage.clear();
            match res {
                Ok(result) => {
                    self.status_message = "Done.".to_owned();
                    self.log(
                        format!(
                            "Run finished successfully | findings={} | mission={} | structures={}",
                            result.feasibility.findings.len(),
                            if result.mission_result.is_some() {
                                "available"
                            } else {
                                "none"
                            },
                            if result.structural_result.is_some() {
                                "available"
                            } else {
                                "none"
                            },
                        ),
                        LogKind::Info,
                    );
                    if let Some(cpacs) = &result.cpacs_export {
                        self.log(
                            format!("CPACS {}: {}", cpacs.cpacs_version, cpacs.path.display()),
                            LogKind::Info,
                        );
                    }
                    self.pipeline_result = Some(result);
                    // A new result opens on its summary, not on the tab the
                    // previous run was left on.
                    self.results_tab = "summary".to_owned();
                    self.pipeline_result_complete = true;
                    self.update_result_scene();
                    self.begin_final_figure_check();
                    if self.sandbox.active() {
                        self.sandbox.results_window_open = true;
                    } else {
                        self.active_page = "results".to_owned();
                    }
                }
                Err(e) => {
                    // A snapshot may remain visible after cancellation or a
                    // downstream failure, but it is never a finalized report.
                    self.pipeline_result_complete = false;
                    // A solver killed by the cancel surfaces as a stage error;
                    // once the user asked to cancel it is a cancellation.
                    if is_safe_cancellation(&e) || self.cancellation_requested {
                        self.status_message = "Cancelled.".to_owned();
                        self.log("Run cancelled safely.", LogKind::Warn);
                    } else {
                        self.status_message = "Failed.".to_owned();
                        self.log(
                            tr_fields("Run failed: {error}", &[("error", e)]),
                            LogKind::Error,
                        );
                    }
                }
            }
            self.run_started = None;
            self.cancellation_requested = false;
        }
    }

    /// Request cancellation once and wait for the worker to reach a safe
    /// boundary before reporting the run as cancelled.
    pub fn request_pipeline_cancel(&mut self) {
        if !self.is_running || self.cancellation_requested {
            return;
        }
        self.cancellation_requested = true;
        self.cancel_flag.store(true, Ordering::Relaxed);
        alas_exec::supervise::block_launches(true);
        self.status_message = "Cancelling: stopping external tools...".to_owned();
        self.log(
            "Cancellation requested; external tools are being stopped by PID and the run ends at the next safe point.",
            LogKind::Warn,
        );
        // Kill the solver process trees off the UI thread (taskkill takes a
        // moment per tree); each blocked stage then returns and sees the flag.
        let since = std::time::SystemTime::now()
            - self
                .run_started
                .map_or(std::time::Duration::ZERO, |t| t.elapsed());
        let _ = std::thread::Builder::new()
            .name("alas-cancel-kill".to_owned())
            .spawn(move || {
                let _ = alas_exec::supervise::terminate_launched_since(since);
            });
    }
}

/// Turn off every stage that launches an external program, for the quick
/// reference analysis. Sizing, weight and balance and stability are native.
fn skip_external_solvers(config: &mut alas_config::AlasConfig) {
    config.downstream.openvsp = false;
    config.downstream.vspaero = false;
    config.downstream.avl = false;
    config.downstream.flowunsteady = false;
    config.structures.run_nastran = false;
    config.structures.run_patran_export = false;
    config.mses.enabled = false;
}

/// The status text for the stages running right now, derived from the run
/// events: a stage is active from its `StageStarted` until its
/// `StageCompleted`, so a finished stage never leaves its name on screen while
/// a later one works. Parallel stages are listed together.
fn active_stage_status(events: &[RunEvent]) -> Option<String> {
    let mut active: Vec<(&str, &str)> = Vec::new();
    let mut last_progress: Option<&str> = None;
    for event in events {
        match event.kind {
            RunEventKind::StageStarted => {
                active.retain(|(stage, _)| *stage != event.stage);
                active.push((&event.stage, &event.message));
            }
            RunEventKind::Progress => {
                if let Some(entry) = active.iter_mut().find(|(stage, _)| *stage == event.stage) {
                    entry.1 = &event.message;
                } else {
                    last_progress = Some(&event.message);
                }
            }
            RunEventKind::StageCompleted => active.retain(|(stage, _)| *stage != event.stage),
            RunEventKind::Diagnostic => {}
        }
    }
    if active.is_empty() {
        return last_progress.map(str::to_owned);
    }
    let shown: Vec<&str> = active.iter().rev().take(3).map(|(_, m)| *m).collect();
    Some(shown.join(" + "))
}

/// One run-log line per task that launched solver processes before `event`:
/// the program, the PIDs Task Manager shows, and whether they end with ALAS.
fn launch_events(event: &RunEvent, launches: &[LaunchRecord]) -> Vec<RunEvent> {
    let mut groups: Vec<(&LaunchRecord, Vec<u32>)> = Vec::new();
    for launch in launches {
        match groups.iter_mut().find(|(first, _)| {
            first.role == launch.role
                && first.supervised == launch.supervised
                && first.stage == launch.stage
        }) {
            Some((_, pids)) => pids.push(launch.pid),
            None => groups.push((launch, vec![launch.pid])),
        }
    }
    groups
        .into_iter()
        .map(|(first, pids)| {
            let program = Path::new(&first.program)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| first.program.clone());
            let pids = pids
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let lifetime = if first.supervised {
                "ends with ALAS"
            } else {
                "not supervised; outlives a forced ALAS exit"
            };
            RunEvent {
                // The stage the launching thread declared; the event that
                // happens to follow the launch may belong to a parallel one.
                stage: first.stage.clone().unwrap_or_else(|| event.stage.clone()),
                message: format!("{} launched {program}, PID {pids} ({lifetime})", first.role),
                fraction: None,
                kind: RunEventKind::Diagnostic,
                severity: RunEventSeverity::Info,
                stage_index: None,
                stage_count: None,
                elapsed_ms: event.elapsed_ms,
                duration_ms: None,
            }
        })
        .collect()
}

#[cfg(test)]
mod pre_run_error_visibility_tests {
    use crate::state::AppState;

    #[test]
    fn an_invalid_configuration_surfaces_the_run_log_instead_of_doing_nothing() {
        let mut state = AppState {
            run_log_open: false,
            config_values: serde_json::Value::Null,
            ..Default::default()
        };

        state.start_pipeline(false);

        assert!(!state.is_running);
        assert!(
            state.run_log_open,
            "an invalid configuration must surface a visible error, not silently no-op"
        );
    }

    #[test]
    fn an_incomplete_design_vector_surfaces_the_run_log() {
        let mut state = AppState {
            run_log_open: false,
            ..Default::default()
        };
        state.design_values.clear();

        state.start_pipeline(false);

        assert!(!state.is_running);
        assert!(state.run_log_open);
    }

    #[test]
    fn incomplete_optimizer_bounds_surface_the_run_log() {
        let mut state = AppState {
            run_log_open: false,
            ..Default::default()
        };
        state.bounds.clear();

        state.start_pipeline(false);

        assert!(!state.is_running);
        assert!(state.run_log_open);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn launch(sequence: u64, pid: u32, role: &str, supervised: bool) -> LaunchRecord {
        LaunchRecord {
            sequence,
            pid,
            role: role.to_owned(),
            program: format!("C:/tools/{}.exe", role.split(' ').next().unwrap_or("tool")),
            started: SystemTime::UNIX_EPOCH,
            supervised,
            stage: None,
        }
    }

    fn stage_completed(stage: &str) -> RunEvent {
        RunEvent {
            stage: stage.to_owned(),
            message: "Completed in 1.000 s".to_owned(),
            fraction: Some(1.0),
            kind: RunEventKind::StageCompleted,
            severity: RunEventSeverity::Info,
            stage_index: None,
            stage_count: None,
            elapsed_ms: 1_000,
            duration_ms: Some(1_000),
        }
    }

    #[test]
    fn launches_are_grouped_by_task_and_listed_by_pid() {
        let event = stage_completed("downstream/mses");
        let launches = [
            launch(1, 100, "MSES mset", true),
            launch(2, 104, "MSES mses", true),
            launch(3, 110, "MSES mses", true),
            launch(4, 120, "MSES mses", false),
        ];
        let lines = launch_events(&event, &launches);
        let messages: Vec<&str> = lines.iter().map(|line| line.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "MSES mset launched MSES.exe, PID 100 (ends with ALAS)",
                "MSES mses launched MSES.exe, PID 104, 110 (ends with ALAS)",
                "MSES mses launched MSES.exe, PID 120 (not supervised; outlives a forced ALAS exit)",
            ]
        );
        assert!(lines.iter().all(|line| line.stage == "downstream/mses"
            && line.kind == RunEventKind::Diagnostic
            && line.elapsed_ms == 1_000));
    }

    #[test]
    fn a_cancel_before_the_worker_starts_is_reported_as_cancelled_not_failed() {
        let (tx, rx) = channel();
        let mut state = AppState {
            is_running: true,
            worker_rx: Some(rx),
            ..Default::default()
        };
        tx.send(WorkerMessage::Finished(Box::new(Err(format!(
            "{CANCELLED_SAFELY} before the run started"
        )))))
        .unwrap();
        state.poll_worker();
        assert!(!state.is_running);
        assert_eq!(state.status_message, "Cancelled.");
        assert!(is_safe_cancellation(
            "Cancelled safely: finalist replay interrupted"
        ));
        assert!(!is_safe_cancellation("Cancelled"));
    }

    fn stage_event(stage: &str, kind: RunEventKind, message: &str) -> RunEvent {
        RunEvent {
            stage: stage.to_owned(),
            message: message.to_owned(),
            kind,
            ..stage_completed(stage)
        }
    }

    #[test]
    fn the_status_names_only_the_stages_that_are_still_running() {
        use RunEventKind::{Progress, StageCompleted, StageStarted};
        let events = vec![
            stage_event(
                "downstream/patran",
                StageStarted,
                "Patran deformation export",
            ),
            stage_event("downstream/vspaero", StageStarted, "VSPAERO analysis"),
            stage_event("downstream/patran", StageCompleted, "Skipped"),
        ];
        assert_eq!(
            active_stage_status(&events).as_deref(),
            Some("VSPAERO analysis")
        );
        let mut events = events;
        events.push(stage_event(
            "downstream/vspaero",
            Progress,
            "VSPAERO case 2/4",
        ));
        assert_eq!(
            active_stage_status(&events).as_deref(),
            Some("VSPAERO case 2/4")
        );
        events.push(stage_event("downstream/vspaero", StageCompleted, "Done"));
        assert_eq!(active_stage_status(&events), None);
    }

    #[test]
    fn a_stage_error_after_a_cancel_request_is_a_cancellation() {
        let (tx, rx) = channel();
        let mut state = AppState {
            is_running: true,
            cancellation_requested: true,
            worker_rx: Some(rx),
            ..Default::default()
        };
        tx.send(WorkerMessage::Finished(Box::new(Err(
            "Patran export failed: process killed".to_owned(),
        ))))
        .unwrap();
        state.poll_worker();
        assert!(!state.is_running);
        assert_eq!(state.status_message, "Cancelled.");
    }

    #[test]
    fn the_reference_quick_look_drops_every_external_solver() {
        let mut config = alas_config::AlasConfig::default();
        config.downstream.vspaero = true;
        config.structures.run_nastran = true;
        config.structures.run_patran_export = true;
        skip_external_solvers(&mut config);
        assert!(!config.downstream.openvsp && !config.downstream.vspaero);
        assert!(!config.downstream.avl && !config.downstream.flowunsteady);
        assert!(!config.structures.run_nastran && !config.structures.run_patran_export);
        assert!(!config.mses.enabled);
    }

    #[test]
    fn a_launch_is_logged_under_the_stage_that_made_it_not_the_next_event() {
        let event = stage_completed("downstream/structural");
        let mses = LaunchRecord {
            stage: Some("downstream/mses".to_owned()),
            ..launch(1, 100, "MSES mses", true)
        };
        let lines = launch_events(&event, &[mses, launch(2, 200, "NASTRAN solve", true)]);
        let stages: Vec<&str> = lines.iter().map(|line| line.stage.as_str()).collect();
        assert_eq!(stages, ["downstream/mses", "downstream/structural"]);
    }

    #[test]
    fn no_launches_means_no_lines() {
        assert!(launch_events(&stage_completed("baseline"), &[]).is_empty());
    }
}
