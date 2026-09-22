// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The airfoil-screening sweep's own background-run state.
//!
//! Kept out of [`crate::state`] so that module stays about the design
//! configuration; this one is about running `alas-screen`'s multi-stage sweep
//! and holding its result, the same separation the reference desktop app drew
//! by lifting `AirfoilSweepScreen`'s run state up into `App.tsx` rather than
//! leaving it component-local, so navigating away mid-sweep does not lose it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use alas_config::{AlasConfig, DesignVector};
use alas_screen::runner::run_airfoil_screening_product;
use alas_screen::types::{AirfoilScreeningOptions, AirfoilScreeningResult};

mod figure_cache;
use figure_cache::ScreeningFigureCache;

#[path = "screening_readiness.rs"]
mod readiness;
pub(crate) use readiness::MsesReadiness;

enum ScreeningMessage {
    Progress(String),
    Finished(Box<Result<AirfoilScreeningResult, String>>),
}

/// The airfoil-screening page's run state.
pub struct ScreeningState {
    /// Whether the screening workspace is shown in its detached native window.
    ///
    /// The screening page remains available through the navigation tree for
    /// compatibility; the top-bar action uses this flag to show the same
    /// renderer in a separate viewport.
    pub window_open: bool,
    /// Whether the detached custom-airfoil importer was requested from the
    /// Advanced Settings > Airfoil Screening selector.
    pub custom_airfoil_import_open: bool,
    pub(crate) mses_readiness: MsesReadiness,
    /// Inspection state only: never applied to the aircraft configuration.
    pub preview: ScreeningPreview,
    /// The sweep's configured options.
    pub options: AirfoilScreeningOptions,
    /// Whether a sweep is currently running.
    pub running: bool,
    /// Whether the running sweep has been asked to stop at its next
    /// checkpoint. Kept separate from `running` because the worker only ends
    /// at a stage boundary, so the window must be able to say "cancelling"
    /// without pretending the sweep has already stopped.
    cancel_requested: bool,
    /// Whether the sweep that produced [`Self::result`] ended through
    /// cancellation. A cancelled sweep keeps the candidates it did evaluate,
    /// but it is never reported as a completed screening.
    pub last_run_cancelled: bool,
    /// The most recent progress message.
    pub status: String,
    /// The finished result, if any.
    pub result: Option<AirfoilScreeningResult>,
    /// Revision of the completed result, independent of live aircraft edits.
    pub(crate) result_revision: u64,
    /// Localized completed-result scenes reused between frames.
    pub(crate) figure_cache: ScreeningFigureCache,
    /// The error a failed sweep ended with, if any.
    pub error: Option<String>,
    rx: Option<Receiver<ScreeningMessage>>,
    cancel_flag: Arc<AtomicBool>,
}

impl Default for ScreeningState {
    fn default() -> Self {
        Self {
            window_open: false,
            custom_airfoil_import_open: false,
            mses_readiness: MsesReadiness::default(),
            preview: ScreeningPreview::default(),
            options: AirfoilScreeningOptions::default(),
            running: false,
            cancel_requested: false,
            last_run_cancelled: false,
            status: String::new(),
            result: None,
            result_revision: 0,
            figure_cache: ScreeningFigureCache::default(),
            error: None,
            rx: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl ScreeningState {
    /// Launch a sweep against `config`/`design` in the background.
    pub fn start(&mut self, config: AlasConfig, design: DesignVector, mses_dir: Option<PathBuf>) {
        if self.running {
            return;
        }
        self.running = true;
        self.options.objective = alas_screen::types::ScreeningObjective::Balanced;
        self.error = None;
        self.result = None;
        self.last_run_cancelled = false;
        self.invalidate_result_figures();
        self.status = "Starting...".to_owned();
        self.cancel_requested = false;
        self.cancel_flag.store(false, Ordering::Relaxed);

        let (tx, rx): (Sender<ScreeningMessage>, Receiver<ScreeningMessage>) = channel();
        self.rx = Some(rx);
        let options = self.options.clone();
        let cancel = self.cancel_flag.clone();

        thread::spawn(move || {
            let cancel_check = cancel.clone();
            let should_cancel = move || cancel_check.load(Ordering::Relaxed);
            let tx_progress = tx.clone();
            let mut on_progress = move |msg: &str| {
                let _ = tx_progress.send(ScreeningMessage::Progress(msg.to_owned()));
            };
            let result = run_airfoil_screening_product(
                &config,
                Some(&design),
                &options,
                mses_dir.as_deref(),
                Some(&mut on_progress),
                Some(&should_cancel),
            );
            let _ = tx.send(ScreeningMessage::Finished(Box::new(result)));
        });
    }

    /// Ask a running sweep to stop at its next checkpoint.
    ///
    /// Cancelling an idle screening is a no-op: without this guard a stray
    /// click would leave a "Cancelling..." status over a finished result and
    /// arm the flag for the next sweep.
    pub fn cancel(&mut self) {
        if !self.running {
            return;
        }
        self.cancel_flag.store(true, Ordering::Relaxed);
        self.cancel_requested = true;
        self.status = "Cancelling...".to_owned();
    }

    /// Whether a cancellation has been requested and the worker has not yet
    /// reached the checkpoint where it can stop.
    pub fn is_cancelling(&self) -> bool {
        self.running && self.cancel_requested
    }

    /// Drain progress messages and pick up the result once the sweep ends.
    pub fn poll(&mut self) {
        let mut finished = None;
        let mut progress = None;
        if let Some(rx) = &self.rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    ScreeningMessage::Progress(p) => progress = Some(p),
                    ScreeningMessage::Finished(res) => finished = Some(*res),
                }
            }
        }
        if let Some(p) = progress {
            self.status = p;
        }
        if let Some(res) = finished {
            self.invalidate_result_figures();
            self.running = false;
            self.cancel_requested = false;
            self.cancel_flag.store(false, Ordering::Relaxed);
            self.rx = None;
            match res {
                Ok(result) => {
                    // A cancelled sweep returns whatever it had evaluated. It
                    // keeps that partial ranking, but its status must not read
                    // like a completed screening: the remaining candidates were
                    // never examined, so "Done: n of N" would understate the
                    // ranking's scope.
                    self.last_run_cancelled = result.cancelled;
                    self.status = if result.cancelled {
                        "Cancelled: partial results are from the candidates evaluated before stopping.".to_owned()
                    } else {
                        format!(
                            "Done: {} of {} candidates evaluated.",
                            result.n_ok, result.n_total
                        )
                    };
                    self.result = Some(result);
                }
                Err(e) => {
                    self.status.clear();
                    self.error = Some(e);
                }
            }
        }
    }

    /// Invalidate scene and raster revisions when replacing or clearing results.
    pub(crate) fn invalidate_result_figures(&mut self) {
        self.result_revision = self.result_revision.wrapping_add(1);
        self.figure_cache.clear();
    }
}

/// A stable library name, independent of ranking position and run lifecycle.
#[derive(Default)]
pub struct ScreeningPreview {
    selected: Option<String>,
    coordinates: Option<Vec<(f64, f64)>>,
    filter: Option<String>,
    filtered_names: Vec<String>,
}

impl ScreeningPreview {
    /// Discard the filtered-name cache after a custom airfoil is imported.
    pub(crate) fn invalidate_filter(&mut self) {
        self.filter = None;
        self.filtered_names.clear();
    }

    /// Refilter when the query changes. The library is read here rather than
    /// cached globally because the user can add validated airfoils at runtime.
    pub fn update_filter(&mut self, filter: &str) {
        if self.filter.as_deref() == Some(filter) {
            return;
        }
        let names = alas_geom::airfoil_library::AirfoilLibrary::get_available_airfoils();
        self.filtered_names = alas_screen::runner::filter_names(&names, filter);
        self.filter = Some(filter.to_owned());
    }

    pub fn filtered_names(&self) -> &[String] {
        &self.filtered_names
    }

    pub fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    pub fn coordinates(&self) -> Option<&[(f64, f64)]> {
        self.coordinates.as_deref()
    }

    pub fn select(&mut self, name: &str) {
        if self.selected() == Some(name) {
            return;
        }
        self.coordinates = alas_geom::airfoil_library::AirfoilLibrary::get(name)
            .map(|airfoil| airfoil.coordinates)
            .filter(|points| {
                points.len() >= 3 && points.iter().all(|(x, y)| x.is_finite() && y.is_finite())
            });
        self.selected = Some(name.to_owned());
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use alas_screen::types::ScreeningObjective;

    /// Hand a finished worker message to an otherwise-idle state, the way a
    /// real background sweep ends, so `poll` can be exercised without running
    /// the multi-minute screening itself.
    fn deliver(state: &mut ScreeningState, result: Result<AirfoilScreeningResult, String>) {
        let (tx, rx) = channel();
        tx.send(ScreeningMessage::Finished(Box::new(result)))
            .expect("the receiver is alive");
        state.running = true;
        state.rx = Some(rx);
        state.poll();
    }

    #[test]
    fn cancelled_sweep_is_not_reported_as_a_completed_screening() {
        let mut state = ScreeningState {
            cancel_requested: true,
            ..Default::default()
        };
        deliver(
            &mut state,
            Ok(AirfoilScreeningResult {
                n_ok: 3,
                n_total: 40,
                cancelled: true,
                ..AirfoilScreeningResult::default()
            }),
        );

        assert!(!state.running);
        assert!(state.last_run_cancelled);
        assert!(!state.is_cancelling());
        assert_eq!(
            state.status,
            "Cancelled: partial results are from the candidates evaluated before stopping."
        );
        // The partial ranking is kept: the evaluated candidates are real.
        assert!(state.result.is_some());
    }

    #[test]
    fn completed_sweep_reports_the_evaluated_counts() {
        let mut state = ScreeningState::default();
        deliver(
            &mut state,
            Ok(AirfoilScreeningResult {
                n_ok: 38,
                n_total: 40,
                cancelled: false,
                ..AirfoilScreeningResult::default()
            }),
        );

        assert!(!state.running);
        assert!(!state.last_run_cancelled);
        assert_eq!(state.status, "Done: 38 of 40 candidates evaluated.");
    }

    #[test]
    fn failed_sweep_keeps_the_error_and_clears_the_run() {
        let mut state = ScreeningState::default();
        deliver(&mut state, Err("worker failed".to_owned()));

        assert!(!state.running);
        assert!(!state.last_run_cancelled);
        assert!(state.status.is_empty());
        assert_eq!(state.error.as_deref(), Some("worker failed"));
    }

    #[test]
    fn cancel_is_ignored_while_no_sweep_is_running() {
        let mut state = ScreeningState {
            status: "Done: 38 of 40 candidates evaluated.".to_owned(),
            ..Default::default()
        };
        state.cancel();

        assert!(!state.is_cancelling());
        assert!(!state.cancel_flag.load(Ordering::Relaxed));
        assert_eq!(state.status, "Done: 38 of 40 candidates evaluated.");
    }

    #[test]
    fn cancel_arms_the_worker_flag_and_reports_the_pending_stop() {
        let mut state = ScreeningState {
            running: true,
            ..Default::default()
        };
        state.cancel();

        assert!(state.is_cancelling());
        assert!(state.cancel_flag.load(Ordering::Relaxed));
        assert_eq!(state.status, "Cancelling...");
    }

    #[test]
    fn a_finished_sweep_disarms_cancellation_for_the_next_run() {
        let mut state = ScreeningState {
            running: true,
            ..Default::default()
        };
        state.cancel();
        deliver(
            &mut state,
            Ok(AirfoilScreeningResult {
                cancelled: true,
                ..AirfoilScreeningResult::default()
            }),
        );

        assert!(!state.cancel_flag.load(Ordering::Relaxed));
        assert!(state.rx.is_none());
        assert!(!state.is_cancelling());
    }

    #[test]
    fn preview_selection_leaves_the_configuration_and_preset_untouched() {
        let mut state = crate::state::AppState {
            active_preset: "A320-200".to_owned(),
            ..Default::default()
        };
        let config_before = state.config_values.clone();
        let design_before = state.design_values.clone();
        let preset_before = state.active_preset.clone();

        state.screening.preview.select("naca2412");
        state.screening.preview.select("naca0012");

        assert_eq!(state.screening.preview.selected(), Some("naca0012"));
        assert_eq!(state.config_values, config_before);
        assert_eq!(state.design_values, design_before);
        assert_eq!(state.active_preset, preset_before);
    }

    /// The detached workspace has no ranking-objective selector, so Balanced
    /// must already be the state a fresh sweep starts from. `start` overrides
    /// it again; this guards the default a restored workspace would carry.
    #[test]
    fn the_default_ranking_objective_is_balanced() {
        assert_eq!(
            ScreeningState::default().options.objective,
            ScreeningObjective::Balanced
        );
    }
}
