// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Non-blocking CFD probe/run lifecycle and revision-gated result delivery.

use super::super::*;
use super::persistence::allocate_case_directory;
use alas_cfd::{run_study, AirfoilSnapshot, CfdOutcome, CfdRunEvent};
use alas_exec::openfoam::OpenFoamAdapter;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::mpsc::channel;
use std::thread;

impl AirfoilCfdState {
    /// Load actual, persisted OpenFOAM evidence from a prior isolated case.
    ///
    /// This does not reinterpret values or turn an unconverged result into a
    /// qualified one.  It restores the exact stored status, input provenance,
    /// force history and native field-artifact paths so the Results tab can
    /// inspect a completed production case after an application restart.
    pub fn load_result_json(&mut self, path: &Path) -> Result<(), String> {
        if self.running {
            return Err("Cannot load a CFD result while a study is running.".to_owned());
        }
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read CFD results {}: {error}", path.display()))?;
        let result: alas_cfd::CfdResults = serde_json::from_str(&text)
            .map_err(|error| format!("cannot decode CFD results {}: {error}", path.display()))?;
        if result.case_dir.as_os_str().is_empty() {
            return Err("The CFD result has no case directory provenance.".to_owned());
        }
        // Importing a persisted result replaces the study contract.  Advance
        // the same revision gate used by interactive edits so a queued worker
        // can never install evidence for the previous contract after this
        // load completes.
        self.input_revision = self.input_revision.wrapping_add(1);
        self.run_input_revision = self.input_revision;
        self.result_json_path = path.display().to_string();
        self.last_case_dir = Some(result.case_dir.clone());
        self.config = result.provenance.config.clone();
        self.refresh_preview();
        self.result = Some(result);
        self.sweep_results.clear();
        self.selected_field = None;
        self.contour_textures.clear();
        self.error = None;
        self.tab = CfdTab::Results;
        self.status = "Loaded persisted OpenFOAM result evidence.".to_owned();
        Ok(())
    }

    /// Start a non-blocking OpenFOAM utility/version probe.
    pub fn start_probe(&mut self) {
        if self.probing || self.running {
            return;
        }
        self.probing = true;
        self.status = "Checking OpenFOAM utilities...".to_owned();
        let preferences = self.openfoam_preferences.clone();
        let (tx, rx) = channel();
        self.probe_rx = Some(rx);
        thread::spawn(move || {
            let capabilities = OpenFoamAdapter::resolve(preferences).probe();
            let _ = tx.send(ProbeMessage::Finished(capabilities));
        });
    }

    /// Start a full CFD study in an isolated case directory.
    pub fn start_run(&mut self) -> Result<u64, String> {
        if self.running {
            return Err("An Airfoil CFD study is already running.".to_owned());
        }
        if let Err(errors) = self.config.validate() {
            self.error = Some(errors.join(" "));
            self.status = "CFD inputs are invalid; correct them before running.".to_owned();
            return Err(self.error.clone().unwrap_or_default());
        }
        let run_id = self.run_id.wrapping_add(1).max(1);
        let case_dir = match allocate_case_directory(&self.case_root, run_id) {
            Ok(path) => path,
            Err(error) => {
                self.error = Some(error.clone());
                self.status = "Unable to allocate an isolated CFD case directory.".to_owned();
                return Err(error);
            }
        };
        self.run_id = run_id;
        self.run_input_revision = self.input_revision;
        self.running = true;
        self.stage = None;
        self.events.clear();
        self.result = None;
        self.sweep_results.clear();
        self.sweep_running = false;
        self.error = None;
        self.selected_field = None;
        self.contour_textures.clear();
        self.status = format!("Starting Airfoil CFD run #{}...", self.run_id);
        self.cancel_flag.store(false, Ordering::Relaxed);
        let run_id = self.run_id;
        let input_revision = self.run_input_revision;
        let config = self.config.clone();
        let preferences = self.openfoam_preferences.clone();
        let cancel = self.cancel_flag.clone();
        let (tx, rx) = channel();
        self.rx = Some(rx);
        thread::spawn(move || {
            let adapter = OpenFoamAdapter::resolve(preferences);
            let event_tx = tx;
            let mut emit = |event: CfdRunEvent| {
                let _ = event_tx.send(CfdWorkerMessage::Event {
                    run_id,
                    input_revision,
                    event,
                });
            };
            let result = run_study(&config, &adapter, &case_dir, &cancel, &mut emit);
            let _ = event_tx.send(CfdWorkerMessage::Finished {
                run_id,
                input_revision,
                result,
            });
        });
        Ok(run_id)
    }

    /// Start a sequential AoA/Reynolds sweep in isolated per-point cases.
    ///
    /// The worker intentionally processes one point at a time.  This keeps
    /// cancellation and resource usage deterministic and lets the Results tab
    /// retain a separate numerical/provenance record for every point.
    pub fn start_sweep(&mut self) -> Result<u64, String> {
        if self.running {
            return Err("An Airfoil CFD study is already running.".to_owned());
        }
        if let Err(errors) = self.config.validate() {
            self.error = Some(errors.join(" "));
            self.status = "CFD inputs are invalid; correct them before running.".to_owned();
            return Err(self.error.clone().unwrap_or_default());
        }
        let values = self.sweep_settings.values()?;
        let point_configs = values
            .iter()
            .map(|value| self.sweep_settings.config_for_value(&self.config, *value))
            .collect::<Vec<_>>();

        let run_id = self.run_id.wrapping_add(1).max(1);
        let run_case_dir = match allocate_case_directory(&self.case_root, run_id) {
            Ok(path) => path,
            Err(error) => {
                self.error = Some(error.clone());
                self.status = "Unable to allocate isolated CFD sweep cases.".to_owned();
                return Err(error);
            }
        };
        self.run_id = run_id;
        self.run_input_revision = self.input_revision;
        self.running = true;
        self.sweep_running = true;
        self.stage = None;
        self.events.clear();
        self.result = None;
        self.sweep_results = values
            .iter()
            .zip(point_configs.iter())
            .enumerate()
            .map(|(index, (value, config))| {
                CfdSweepPointResult::pending(index, *value, config.clone())
            })
            .collect();
        self.error = None;
        self.selected_field = None;
        self.contour_textures.clear();
        self.status = format!(
            "Starting {} sweep with {} points...",
            self.sweep_settings.variable.label(),
            values.len()
        );
        self.cancel_flag.store(false, Ordering::Relaxed);

        let run_id = self.run_id;
        let input_revision = self.run_input_revision;
        let base_config = self.config.clone();
        let settings = self.sweep_settings.clone();
        let preferences = self.openfoam_preferences.clone();
        let cancel = self.cancel_flag.clone();
        let (tx, rx) = channel();
        self.rx = Some(rx);
        thread::spawn(move || {
            let adapter = OpenFoamAdapter::resolve(preferences);
            for (index, value) in values.into_iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let config = settings.config_for_value(&base_config, value);
                if tx
                    .send(CfdWorkerMessage::SweepPointStarted {
                        run_id,
                        input_revision,
                        index,
                        value,
                        config: config.clone(),
                    })
                    .is_err()
                {
                    return;
                }
                let case_dir = run_case_dir.join(format!("point-{:03}", index + 1));
                if let Err(error) = std::fs::create_dir(&case_dir) {
                    let _ = tx.send(CfdWorkerMessage::SweepPointFinished {
                        run_id,
                        input_revision,
                        index,
                        result: Err(format!(
                            "cannot allocate isolated sweep point {}: {error}",
                            case_dir.display()
                        )),
                    });
                    break;
                }
                let event_tx = &tx;
                let mut emit = |event: CfdRunEvent| {
                    let _ = event_tx.send(CfdWorkerMessage::Event {
                        run_id,
                        input_revision,
                        event,
                    });
                };
                let result = run_study(&config, &adapter, &case_dir, &cancel, &mut emit);
                let stop_after_point = cancel.load(Ordering::Relaxed)
                    || matches!(
                        result.as_ref(),
                        Ok(result) if result.outcome == CfdOutcome::Cancelled
                    );
                if tx
                    .send(CfdWorkerMessage::SweepPointFinished {
                        run_id,
                        input_revision,
                        index,
                        result,
                    })
                    .is_err()
                {
                    return;
                }
                if stop_after_point {
                    break;
                }
            }
            let _ = tx.send(CfdWorkerMessage::SweepFinished {
                run_id,
                input_revision,
            });
        });
        Ok(run_id)
    }

    /// Request cancellation of the owned OpenFOAM process tree.
    pub fn cancel_run(&mut self) {
        if !self.running {
            return;
        }
        self.cancel_flag.store(true, Ordering::Relaxed);
        self.status = "Cancellation requested; stopping the active CFD stage...".to_owned();
    }

    /// Launch the configured ParaView executable on a case marker file.
    ///
    /// ParaView owns all field/streamline rendering; ALAS only creates the
    /// standard empty `.foam` marker and passes the isolated case directory.
    pub fn open_case_in_paraview(&self, case_dir: &Path) -> Result<(), String> {
        let executable = self
            .paraview_executable
            .as_deref()
            .filter(|path| !path.trim().is_empty())
            .ok_or_else(|| "ParaView is not configured under External Tools.".to_owned())?;
        if !Path::new(executable).is_file() {
            return Err(format!(
                "ParaView executable was not found at {executable}."
            ));
        }
        std::fs::create_dir_all(case_dir)
            .map_err(|error| format!("cannot prepare the case directory: {error}"))?;
        let marker = case_dir.join("case.foam");
        if !marker.exists() {
            std::fs::write(&marker, b"# ALAS OpenFOAM case marker\n")
                .map_err(|error| format!("cannot write {}: {error}", marker.display()))?;
        }
        std::process::Command::new(executable)
            .arg(&marker)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("cannot launch ParaView: {error}"))
    }

    /// Drain probe/run messages without blocking the UI.
    ///
    /// Returned events are intended for the application run log.  Events and
    /// results from an older run or input revision are ignored deliberately.
    pub fn poll(&mut self) -> Vec<CfdRunEvent> {
        let mut emitted = Vec::new();
        if let Some(rx) = &self.probe_rx {
            match rx.try_recv() {
                Ok(ProbeMessage::Finished(capabilities)) => {
                    self.probing = false;
                    self.status = capabilities.summary();
                    self.capabilities = Some(capabilities);
                    self.probe_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.probing = false;
                    self.status = "OpenFOAM connection probe stopped unexpectedly.".to_owned();
                    self.probe_rx = None;
                }
            }
        }

        let mut finished = None;
        let mut sweep_finished = None;
        if let Some(rx) = &self.rx {
            while let Ok(message) = rx.try_recv() {
                match message {
                    CfdWorkerMessage::Event {
                        run_id,
                        input_revision,
                        event,
                    } if run_id == self.run_id && input_revision == self.input_revision => {
                        self.stage = Some(event.stage);
                        self.status = event.message.clone();
                        self.events.push(event.clone());
                        emitted.push(event);
                    }
                    CfdWorkerMessage::Finished {
                        run_id,
                        input_revision,
                        result,
                    } => {
                        finished = Some((run_id, input_revision, result));
                    }
                    CfdWorkerMessage::SweepPointStarted {
                        run_id,
                        input_revision,
                        index,
                        value: _value,
                        config,
                    } if run_id == self.run_id && input_revision == self.input_revision => {
                        if let Some(point) = self.sweep_results.get_mut(index) {
                            point.status = CfdSweepPointStatus::Running;
                            point.config = config;
                            point.error = None;
                        }
                        self.status = format!(
                            "Running {} sweep point {}/{}...",
                            self.sweep_settings.variable.label(),
                            index + 1,
                            self.sweep_results.len()
                        );
                    }
                    CfdWorkerMessage::SweepPointFinished {
                        run_id,
                        input_revision,
                        index,
                        result,
                    } if run_id == self.run_id && input_revision == self.input_revision => {
                        if let Some(point) = self.sweep_results.get_mut(index) {
                            match result {
                                Ok(result) => {
                                    point.status = CfdSweepPointStatus::Finished(result.outcome);
                                    point.result = Some(result);
                                    point.error = None;
                                }
                                Err(error) => {
                                    point.status = CfdSweepPointStatus::Failed;
                                    point.result = None;
                                    point.error = Some(error);
                                }
                            }
                        }
                    }
                    CfdWorkerMessage::SweepFinished {
                        run_id,
                        input_revision,
                    } => {
                        sweep_finished = Some((run_id, input_revision));
                    }
                    _ => {}
                }
            }
        }
        if let Some((run_id, input_revision, result)) = finished {
            if run_id == self.run_id {
                // A run can finish after an input edit requested its
                // cancellation.  It still belongs to the active worker
                // channel, so clear the busy state, but never install its
                // result against the newer input revision.
                self.rx = None;
                self.running = false;
                self.stage = None;
                if input_revision == self.input_revision {
                    match result {
                        Ok(result) => {
                            self.status =
                                format!("Airfoil CFD finished: {}.", result.outcome.as_str());
                            self.last_case_dir = Some(result.case_dir.clone());
                            self.result = Some(result);
                        }
                        Err(error) => {
                            self.status = "Airfoil CFD failed.".to_owned();
                            self.error = Some(error);
                        }
                    }
                } else {
                    self.status =
                        "Previous CFD run discarded because its inputs changed.".to_owned();
                }
            }
        }
        if let Some((run_id, input_revision)) = sweep_finished {
            if run_id == self.run_id {
                self.rx = None;
                self.running = false;
                self.sweep_running = false;
                self.stage = None;
                if input_revision == self.input_revision {
                    if self.cancel_flag.load(Ordering::Relaxed) {
                        for point in &mut self.sweep_results {
                            if matches!(
                                point.status,
                                CfdSweepPointStatus::Pending | CfdSweepPointStatus::Running
                            ) {
                                point.status = CfdSweepPointStatus::Cancelled;
                            }
                        }
                        self.status = "Airfoil CFD sweep cancelled.".to_owned();
                    } else {
                        let completed = self
                            .sweep_results
                            .iter()
                            .filter(|point| {
                                !matches!(
                                    point.status,
                                    CfdSweepPointStatus::Pending | CfdSweepPointStatus::Running
                                )
                            })
                            .count();
                        self.status = format!(
                            "Airfoil CFD sweep finished: {completed}/{} points returned.",
                            self.sweep_results.len()
                        );
                    }
                } else {
                    self.status =
                        "Previous CFD sweep discarded because its inputs changed.".to_owned();
                }
            }
        }
        emitted
    }

    /// Translate a result classification to a compact user-facing status.
    pub fn outcome_label(outcome: CfdOutcome) -> &'static str {
        match outcome {
            CfdOutcome::Failed => "Failed",
            CfdOutcome::Cancelled => "Cancelled",
            CfdOutcome::Unconverged => "Unconverged",
            CfdOutcome::NumericallyConverged => "Numerically converged",
        }
    }

    /// Resolve the selected section to the core's exact provenance snapshot.
    pub fn selected_snapshot(&self) -> Result<AirfoilSnapshot, String> {
        alas_cfd::resolve_airfoil(&self.config.airfoil_name)
    }
}

impl crate::state::AppState {
    /// Open an independent CFD study for a screening candidate.
    ///
    /// This is intentionally separate from the aircraft configuration and
    /// therefore cannot silently apply the candidate to a preset or sandbox.
    pub fn open_cfd_for_airfoil(&mut self, name: &str) {
        if self.cfd.select_airfoil(name) {
            self.cfd.window_open = true;
            self.cfd.tab = CfdTab::Study;
        } else {
            self.log(
                format!("Cannot open CFD: database airfoil '{name}' is unavailable."),
                crate::state::LogKind::Error,
            );
        }
    }
}
