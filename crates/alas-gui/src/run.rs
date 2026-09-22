// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Launching and polling the background pipeline run.
//!
//! Split out of [`crate::state`] to keep that module under the line limit.

use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
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

impl AppState {
    /// Launch a full or baseline-only pipeline run in the background.
    pub fn start_pipeline(&mut self, baseline_only: bool) {
        if self.is_running {
            return;
        }
        // The baseline action is a fixed-aircraft sandbox by contract. Set the
        // typed design mode before decoding the worker config so the main
        // integrator receives the same mode the user just selected in the UI.
        if baseline_only {
            self.set_design_mode(DesignMode::BaselineSandbox);
        }
        let baseline_only = baseline_only || self.design_mode() == DesignMode::BaselineSandbox;
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
        self.apply_preset_dispatch_policy(&mut config, &mut initial_design, &mut bounds);

        self.is_running = true;
        self.run_log_open = true;
        self.run_started = Some(Instant::now());
        self.run_identity = self.run_identity.wrapping_add(1);
        self.status_message = "Running...".to_owned();
        self.stage.clear();
        self.pipeline_result = None;
        self.pipeline_result_complete = false;
        self.result_figure_cache.clear();
        self.patran_textures.clear();
        self.selected_solver_view = crate::views::results_view::SolverResultView::Vlm;
        self.cancel_flag.store(false, Ordering::Relaxed);
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

        let mut options = self.pipeline_options.clone();
        options.optimize = !baseline_only && self.run_options.optimize;
        options.compare_baseline = self.run_options.compare_baseline;
        options.parallel = self.run_options.parallel;
        options.save_plots = false;
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

        thread::spawn(move || {
            let pipeline = DesignPipeline::new(config);
            if cancel.load(Ordering::Relaxed) {
                let _ = tx.send(WorkerMessage::Finished(Box::new(Err(
                    "Cancelled".to_owned()
                ))));
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
    }

    /// Drain any pending progress and update the UI when a run finishes.
    pub fn poll_worker(&mut self) {
        let mut completed = None;
        let mut events = Vec::new();
        let mut snapshots = Vec::new();
        if let Some(rx) = &self.worker_rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    WorkerMessage::Event(event) => events.push(event),
                    WorkerMessage::Snapshot(snapshot) => {
                        self.pipeline_result_complete = false;
                        snapshots.push(*snapshot);
                    }
                    WorkerMessage::Finished(res) => completed = Some(*res),
                }
            }
        }

        // Snapshots are immutable report boundaries. Keep the latest one so
        // the Results page can use its normal gallery while the worker still
        // runs downstream exports and optional analyses.
        if let Some(snapshot) = snapshots.into_iter().last() {
            self.pipeline_result = Some(snapshot);
            self.update_result_scene();
        }

        for event in events {
            if matches!(
                event.kind,
                RunEventKind::StageStarted | RunEventKind::Progress
            ) {
                self.stage = event.message.clone();
                self.status_message = event.message.clone();
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

        if let Some(res) = completed {
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
                    self.pipeline_result_complete = true;
                    self.update_result_scene();
                    self.verify_final_result_figures();
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
                    if e.starts_with("Cancelled safely") {
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
        self.status_message = "Cancellation requested; finishing current safe unit...".to_owned();
        self.log(
            "Cancellation requested; the active stage or supervised external tool will finish first.",
            LogKind::Warn,
        );
    }
}

/// One run-log line per task that launched solver processes before `event`:
/// the program, the PIDs Task Manager shows, and whether they end with ALAS.
fn launch_events(event: &RunEvent, launches: &[LaunchRecord]) -> Vec<RunEvent> {
    let mut groups: Vec<(&LaunchRecord, Vec<u32>)> = Vec::new();
    for launch in launches {
        match groups
            .iter_mut()
            .find(|(first, _)| first.role == launch.role && first.supervised == launch.supervised)
        {
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
                stage: event.stage.clone(),
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
    fn no_launches_means_no_lines() {
        assert!(launch_events(&stage_completed("baseline"), &[]).is_empty());
    }
}
