// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The airfoil-screening sweep's own background-run state.
//!
//! Kept out of [`crate::state`] so that module stays about the design
//! configuration; this one is about running `alas-screen`'s multi-stage sweep
//! and holding its result, the same separation the reference desktop app drew
//! by lifting `AirfoilSweepScreen`'s run state up into `App.tsx` rather than
//! leaving it component-local -- so navigating away mid-sweep does not lose it.

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
    pub(crate) mses_readiness: MsesReadiness,
    /// Inspection state only: never applied to the aircraft configuration.
    pub preview: ScreeningPreview,
    /// The sweep's configured options.
    pub options: AirfoilScreeningOptions,
    /// Whether a sweep is currently running.
    pub running: bool,
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
            mses_readiness: MsesReadiness::default(),
            preview: ScreeningPreview::default(),
            options: AirfoilScreeningOptions::default(),
            running: false,
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
        self.invalidate_result_figures();
        self.status = "Starting...".to_owned();
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
    pub fn cancel(&mut self) {
        self.cancel_flag.store(true, Ordering::Relaxed);
        self.status = "Cancelling...".to_owned();
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
            match res {
                Ok(result) => {
                    self.status = format!(
                        "Done: {} of {} candidates evaluated.",
                        result.n_ok, result.n_total
                    );
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
    /// The embedded library is immutable. Refilter only when the query changes.
    pub fn update_filter(&mut self, filter: &str) {
        if self.filter.as_deref() == Some(filter) {
            return;
        }
        static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
        let names =
            NAMES.get_or_init(alas_geom::airfoil_library::AirfoilLibrary::get_available_airfoils);
        self.filtered_names = alas_screen::runner::filter_names(names, filter);
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
