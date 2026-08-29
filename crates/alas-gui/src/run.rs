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

impl AppState {
    /// Launch a full or baseline-only pipeline run in the background.
    pub fn start_pipeline(&mut self, baseline_only: bool) {
        if self.is_running {
            return;
        }
        let config = match self.typed_config() {
            Some(c) => c,
            None => {
                self.log(
                    tr("Configuration is not currently valid; fix the highlighted fields."),
                    LogKind::Error,
                );
                return;
            }
        };
        let initial_design = match self.current_design() {
            Some(design) => design,
            None => {
                self.log(
                    tr("The initial design is incomplete; fix the Design Space values."),
                    LogKind::Error,
                );
                return;
            }
        };
        let bounds = match self.current_design_bounds() {
            Some(bounds) => bounds,
            None => {
                self.log(
                    tr("The optimizer bounds are incomplete; fix the Design Space values."),
                    LogKind::Error,
                );
                return;
            }
        };

        self.is_running = true;
        self.run_started = Some(Instant::now());
        self.run_identity = self.run_identity.wrapping_add(1);
        self.status_message = "Running...".to_owned();
        self.stage.clear();
        self.pipeline_result = None;
        self.selected_solver_view = crate::views::results_view::SolverResultView::Vlm;
        self.cancel_flag.store(false, Ordering::Relaxed);
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
            let progress_tx = tx.clone();
            let report = move |message: &str| {
                let _ = progress_tx.send(WorkerMessage::Progress(message.to_owned()));
            };
            let result = pipeline.run_with_design_space_and_progress(
                &options,
                &environment,
                &initial_design,
                &bounds,
                &report,
            );
            let _ = tx.send(WorkerMessage::Finished(Box::new(result)));
        });
    }

    /// Drain any pending progress and update the UI when a run finishes.
    pub fn poll_worker(&mut self) {
        let mut completed = None;
        let mut progress = Vec::new();
        if let Some(rx) = &self.worker_rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    WorkerMessage::Progress(p) => progress.push(p),
                    WorkerMessage::Finished(res) => completed = Some(*res),
                }
            }
        }

        for p in progress {
            self.stage = p.clone();
            self.status_message = p.clone();
            self.log(p, LogKind::Info);
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
                    self.update_result_scene();
                    self.active_page = "results".to_owned();
                }
                Err(e) => {
                    self.status_message = "Failed.".to_owned();
                    self.log(
                        tr_fields("Run failed: {error}", &[("error", e)]),
                        LogKind::Error,
                    );
                }
            }
            self.run_started = None;
        }
    }
}
