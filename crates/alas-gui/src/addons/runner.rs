// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Starting, streaming, cancelling and collecting add-on runs.
//!
//! A run happens on a worker thread; progress and stderr lines reach the page
//! through a channel while it executes, and the cancel flag kills the add-on's
//! process tree.

use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, TryRecvError};
use std::sync::Arc;
use std::time::Instant;

use alas_addons::{RunControl, RunEvent};

use super::LastRun;
use crate::state::{AppState, LogKind};

/// Lines of live output kept for the progress view.
const LOG_LINES: usize = 400;

/// What the progress view shows for the current (or last) run.
#[derive(Default)]
pub(crate) struct RunView {
    /// Completion reported by the program, 0 to 1.
    pub(crate) fraction: Option<f64>,
    /// The latest progress text.
    pub(crate) text: String,
    /// Live stderr lines, oldest first.
    pub(crate) log: VecDeque<String>,
    /// When the run started.
    pub(crate) started: Option<Instant>,
}

impl RunView {
    fn start(&mut self) {
        *self = Self {
            started: Some(Instant::now()),
            ..Self::default()
        };
    }

    fn apply(&mut self, event: RunEvent) {
        match event {
            RunEvent::Progress { fraction, text } => {
                if fraction.is_some() {
                    self.fraction = fraction;
                }
                self.text = text;
            }
            RunEvent::Log(line) => {
                if self.log.len() >= LOG_LINES {
                    self.log.pop_front();
                }
                self.log.push_back(line);
            }
        }
    }
}

impl AppState {
    /// The environment every process add-on receives: where this ALAS is and
    /// the tool locations it knows, so an add-on need not guess them.
    fn addon_environment(&self) -> Vec<(String, String)> {
        let mut env = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            env.push(("ALAS_EXE".to_owned(), exe.display().to_string()));
        }
        let text = |pointer: &str| {
            self.config_values
                .pointer(pointer)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_owned)
        };
        if let Some(dir) = text("/mses/mses_dir")
            .and_then(|configured| self.tool_locator.resolve_mses_dir(Path::new(&configured)))
        {
            env.push(("ALAS_MSES_DIR".to_owned(), dir.display().to_string()));
        }
        if let Some(osmap) = text("/mses/osmap_path") {
            env.push(("ALAS_OSMAP".to_owned(), osmap));
        }
        env
    }

    /// Start the selected add-on on a worker thread.
    pub(crate) fn run_selected_addon(&mut self) {
        if self.addons.running.is_some() {
            self.addons.status = Some("An add-on is already running.".to_owned());
            return;
        }
        let Some(id) = self.addons.selected.clone() else {
            self.addons.status = Some("Select an add-on to run.".to_owned());
            return;
        };
        let Some(addon) = self
            .addons
            .discovery
            .addons
            .iter()
            .find(|addon| addon.id == id)
            .cloned()
        else {
            self.addons.status = Some(format!("Add-on `{id}` is no longer in the add-ons folder."));
            return;
        };
        let mut inputs = BTreeMap::new();
        for spec in &addon.manifest.inputs {
            let text = self
                .addons
                .input_text
                .get(&format!("{id}/{}", spec.name))
                .map(|text| text.trim().to_owned())
                .unwrap_or_default();
            if text.is_empty() {
                continue;
            }
            match crate::user_number::parse_user_number(&text).ok_or(()) {
                Ok(value) => {
                    inputs.insert(spec.name.clone(), value);
                }
                Err(_) => {
                    self.addons.status = Some(format!("`{}` is not a number: {text}", spec.name));
                    return;
                }
            }
        }
        let design = self.addon_design_summary();
        let settings = self.addons.settings.clone();
        let env = self.addon_environment();
        let (tx, rx) = channel();
        let (event_tx, event_rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let control = RunControl {
            cancel: Arc::clone(&cancel),
            events: Some(event_tx),
        };
        let spawned = std::thread::Builder::new()
            .name("alas-addon".to_owned())
            .spawn(move || {
                let _ = tx.send(alas_addons::run_addon_with(
                    &addon, &inputs, &design, &settings, &env, &control,
                ));
            });
        if let Err(error) = spawned {
            self.addons.status = Some(format!("cannot start the add-on worker: {error}"));
            return;
        }
        self.addons.run_view.start();
        self.addons.preview = None;
        self.addons.status = None;
        self.addons.cancel = Some(cancel);
        self.addons.events = Some(event_rx);
        self.addons.running = Some((id, rx));
    }

    /// Ask the running add-on to stop; its process tree is killed.
    pub(crate) fn cancel_running_addon(&mut self) {
        if let Some(cancel) = &self.addons.cancel {
            cancel.store(true, Ordering::Relaxed);
            self.addons.status = Some("Cancelling...".to_owned());
        }
    }

    fn drain_addon_events(&mut self) {
        let Some(events) = &self.addons.events else {
            return;
        };
        while let Ok(event) = events.try_recv() {
            self.addons.run_view.apply(event);
        }
    }

    /// Collect a finished add-on run and its live output; called every frame.
    pub fn poll_addons(&mut self) {
        self.drain_addon_events();
        let Some((id, rx)) = self.addons.running.as_ref() else {
            return;
        };
        let finished = match rx.try_recv() {
            Ok(result) => Some(result.map_err(|error| error.to_string())),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("the add-on worker stopped".to_owned())),
        };
        let Some(result) = finished else {
            return;
        };
        let id = id.clone();
        self.drain_addon_events();
        self.addons.running = None;
        self.addons.events = None;
        self.addons.cancel = None;
        let name = self
            .addons
            .discovery
            .addons
            .iter()
            .find(|addon| addon.id == id)
            .map(|addon| addon.manifest.name.clone())
            .unwrap_or_else(|| id.clone());
        match &result {
            Ok(outcome) => {
                let summary: Vec<String> = outcome
                    .outputs
                    .iter()
                    .map(|(key, value)| format!("{key} = {value}"))
                    .chain(
                        outcome
                            .texts
                            .iter()
                            .map(|(key, value)| format!("{key}: {value}")),
                    )
                    .collect();
                self.log(
                    format!("Add-on {name}: {}", summary.join(", ")),
                    LogKind::Info,
                );
                if !outcome.log.trim().is_empty() {
                    self.log(
                        format!("Add-on {name} log: {}", outcome.log.trim()),
                        LogKind::Info,
                    );
                }
            }
            Err(error) if error == "cancelled" => {
                self.log(format!("Add-on {name} cancelled."), LogKind::Warn);
            }
            Err(error) => self.log(format!("Add-on {name} failed: {error}"), LogKind::Warn),
        }
        self.addons.last = Some(LastRun { id, result });
        self.addons.scroll_to_result = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_and_log_events_update_the_view_and_the_log_is_bounded() {
        let mut view = RunView::default();
        view.start();
        view.apply(RunEvent::Progress {
            fraction: Some(0.25),
            text: "meshing".to_owned(),
        });
        view.apply(RunEvent::Progress {
            fraction: None,
            text: "solving".to_owned(),
        });
        assert_eq!(
            view.fraction,
            Some(0.25),
            "text-only keeps the last fraction"
        );
        assert_eq!(view.text, "solving");
        for i in 0..(LOG_LINES + 10) {
            view.apply(RunEvent::Log(format!("line {i}")));
        }
        assert_eq!(view.log.len(), LOG_LINES);
        assert_eq!(view.log.front().map(String::as_str), Some("line 10"));
    }

    #[test]
    fn the_environment_names_the_alas_executable() {
        let state = AppState::default();
        let env = state.addon_environment();
        assert!(env.iter().any(|(key, _)| key == "ALAS_EXE"));
    }
}

#[cfg(test)]
mod run_tests {
    use super::*;

    #[test]
    fn run_handler_starts_the_bundled_script_and_stores_a_result() {
        let dir = std::env::temp_dir().join(format!("alas-run-handler-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        alas_addons::install_examples(&dir).unwrap();
        let mut state = AppState::default();
        state.addons.dir = dir.clone();
        state.addons.discovery = alas_addons::discover(&dir);
        state.addons.selected = Some("raymer_wing_weight".to_owned());
        // No Run click yet: a missing selection must be reported as text.
        let mut empty = AppState::default();
        empty.run_selected_addon();
        assert!(empty.addons.status.is_some());

        state.run_selected_addon();
        assert!(state.addons.is_running(), "{:?}", state.addons.status);
        let start = Instant::now();
        while state.addons.is_running() && start.elapsed().as_secs() < 20 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            state.poll_addons();
        }
        let last = state.addons.last.as_ref().expect("a result is stored");
        let outcome = last.result.as_ref().expect("the script ran");
        let kg = outcome.outputs["wing_mass_kg"];
        assert!(kg.is_finite() && kg > 1000.0, "{kg}");
        assert!(state.addons.scroll_to_result);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
