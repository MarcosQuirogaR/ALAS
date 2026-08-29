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

enum ScreeningMessage {
    Progress(String),
    Finished(Box<Result<AirfoilScreeningResult, String>>),
}

/// The airfoil-screening page's run state.
pub struct ScreeningState {
    /// The sweep's configured options.
    pub options: AirfoilScreeningOptions,
    /// Whether a sweep is currently running.
    pub running: bool,
    /// The most recent progress message.
    pub status: String,
    /// The finished result, if any.
    pub result: Option<AirfoilScreeningResult>,
    /// The error a failed sweep ended with, if any.
    pub error: Option<String>,
    rx: Option<Receiver<ScreeningMessage>>,
    cancel_flag: Arc<AtomicBool>,
}

impl Default for ScreeningState {
    fn default() -> Self {
        Self {
            options: AirfoilScreeningOptions::default(),
            running: false,
            status: String::new(),
            result: None,
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
        self.error = None;
        self.result = None;
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
}
