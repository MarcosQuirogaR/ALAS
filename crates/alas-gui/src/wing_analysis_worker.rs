// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Geometry snapshot, live preview and the off-thread Wing Analysis run.
//!
//! The worker holds its own copy of the analysed surfaces and the inputs, so
//! nothing it touches can be edited from the UI thread while it solves. It
//! reports once, carrying the run identifier and the input revision it started
//! under; [`WingAnalysisState::poll`] installs the outcome only while that
//! revision is still current.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::Ordering;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;

use alas_aero::wing_analysis::{analyse, build_wing_model, WingAnalysisOutcome};
use alas_config::{AlasConfig, DesignVector};
use alas_report::families::geometry::figure_exterior_3d;

use super::{RunPhase, WingAnalysisState, WingAnalysisTab};

/// The single message a Wing Analysis worker sends.
pub enum WingAnalysisRun {
    /// The run ended, with an outcome or the reason there is none.
    Finished {
        /// Identifier of the run that produced this message.
        run_id: u64,
        /// Input revision the run started under.
        input_revision: u64,
        /// The solved outcome, or the error text to show.
        result: Box<Result<WingAnalysisOutcome, String>>,
    },
}

impl WingAnalysisState {
    /// Snapshot the analysed surfaces from the live configuration.
    ///
    /// The snapshot is rebuilt only when the geometry, the design point or the
    /// selected surface set actually changed, so holding the window open does
    /// not rebuild a lattice every frame. A changed snapshot invalidates the
    /// preview and any installed result through the geometry revision.
    pub fn refresh_geometry(
        &mut self,
        config: &AlasConfig,
        design: &DesignVector,
        source_name: &str,
        theme: &str,
    ) {
        let signature = geometry_signature(config, design, self.includes_empennage());
        let unchanged = self.geometry_signature == Some(signature);
        if unchanged && self.model.is_some() && self.preview.is_some() {
            if self.preview_theme != theme {
                self.rebuild_preview(theme);
            }
            return;
        }
        self.geometry_signature = Some(signature);
        self.source_name = source_name.to_owned();
        if !unchanged || self.model.is_none() {
            match build_wing_model(
                &config.geometry,
                design,
                self.inputs.surfaces,
                self.moment_reference_manual
                    .then_some(self.inputs.moment_reference_m),
            ) {
                Ok(model) => {
                    if !self.moment_reference_manual {
                        self.inputs.moment_reference_m = model.reference().moment_reference_m;
                    }
                    self.model = Some(model);
                    self.geometry_error = None;
                }
                Err(error) => {
                    self.model = None;
                    self.geometry_error = Some(error.to_string());
                }
            }
            self.invalidate_geometry();
        }
        self.rebuild_preview(theme);
    }

    /// Redraw the live preview of the analysed surfaces.
    ///
    /// The scene is the shared exterior wireframe renderer over a model that
    /// contains only the analysed surfaces, so the preview cannot show a
    /// component the run does not model.
    pub fn rebuild_preview(&mut self, theme: &str) {
        self.preview_theme = theme.to_owned();
        self.preview = self.model.as_ref().map(|model| {
            Arc::new(crate::scene::localize_scene_for_display(
                figure_exterior_3d(model.airplane(), Some(self.camera.into()), Some(theme)),
            ))
        });
    }

    /// Reset the moment reference to the snapshot's quarter mean-aerodynamic
    /// chord, the geometric default a wing analysis starts from.
    pub fn reset_moment_reference(&mut self) {
        let Some(model) = &self.model else {
            return;
        };
        let Some(wing) = model.airplane().wings.first() else {
            return;
        };
        let reference = wing.aerodynamic_center(0.25);
        self.moment_reference_manual = false;
        if self.inputs.moment_reference_m != reference {
            self.inputs.moment_reference_m = reference;
            self.invalidate_inputs();
        }
    }
}

impl WingAnalysisState {
    /// Start a wing analysis on a worker thread.
    ///
    /// The window keeps no partial state while a run is active: the snapshot
    /// and the inputs are cloned into the worker, so an edit made during a run
    /// changes only the next run and the revision gate discards the one in
    /// flight.
    ///
    /// # Errors
    ///
    /// The validation text, when the inputs are outside the entry's validity
    /// domain, or the reason the analysed surfaces are unavailable.
    pub fn start(&mut self) -> Result<u64, String> {
        if self.running() {
            return Err("A wing analysis is already running.".to_owned());
        }
        let Some(model) = self.model.clone() else {
            let error = self
                .geometry_error
                .clone()
                .unwrap_or_else(|| "No wing geometry is available to analyse.".to_owned());
            self.error = Some(error.clone());
            self.phase = RunPhase::Failed;
            return Err(error);
        };
        let findings = self.inputs.validate();
        if !findings.is_empty() {
            let error = findings.join(" ");
            self.error = Some(error.clone());
            self.phase = RunPhase::Failed;
            self.status = "The wing analysis inputs are invalid.".to_owned();
            return Err(error);
        }
        self.run_id = self.run_id.wrapping_add(1).max(1);
        self.run_revision = self.input_revision;
        self.phase = RunPhase::Running;
        self.result = None;
        self.error = None;
        self.status = "Assembling the lattice...".to_owned();
        self.cancel_flag.store(false, Ordering::Relaxed);

        let run_id = self.run_id;
        let input_revision = self.run_revision;
        let inputs = self.inputs;
        let cancel = self.cancel_flag.clone();
        let (tx, rx) = channel();
        self.rx = Some(rx);
        thread::spawn(move || {
            let cancelled = move || cancel.load(Ordering::Relaxed);
            let result = analyse(&model, &inputs, &cancelled).map_err(|error| error.to_string());
            let _ = tx.send(WingAnalysisRun::Finished {
                run_id,
                input_revision,
                result: Box::new(result),
            });
        });
        Ok(run_id)
    }

    /// Install a finished run when it still belongs to the current inputs.
    ///
    /// Returns the status line worth logging, if any. A result whose revision
    /// has been superseded is dropped: the window says so rather than showing
    /// numbers for inputs the user has already changed.
    pub fn poll(&mut self) -> Option<String> {
        let Some(rx) = &self.rx else {
            return None;
        };
        let mut finished = None;
        while let Ok(message) = rx.try_recv() {
            let WingAnalysisRun::Finished {
                run_id,
                input_revision,
                result,
            } = message;
            if run_id == self.run_id {
                finished = Some((input_revision, *result));
            }
        }
        let (input_revision, result) = finished?;
        self.rx = None;
        let cancelled = self.cancel_flag.swap(false, Ordering::Relaxed);
        if input_revision != self.input_revision {
            self.phase = RunPhase::Idle;
            self.status =
                "The previous wing analysis was discarded because its inputs changed.".to_owned();
            return Some(self.status.clone());
        }
        match result {
            Ok(outcome) => {
                self.result_revision = input_revision;
                self.status = format!(
                    "Solved {} panels at alpha = {:.2} deg: CL = {:.4}, CDi = {:.5}.",
                    outcome.diagnostics.panel_count,
                    outcome.condition.alpha_deg,
                    outcome.cl,
                    outcome.cd_induced
                );
                self.result = Some(outcome);
                self.phase = RunPhase::Finished;
                self.tab = WingAnalysisTab::Results;
            }
            Err(error) => {
                self.phase = if cancelled {
                    RunPhase::Idle
                } else {
                    RunPhase::Failed
                };
                self.status = if cancelled {
                    "The wing analysis was cancelled; no result was produced.".to_owned()
                } else {
                    "The wing analysis failed.".to_owned()
                };
                self.error = (!cancelled).then_some(error);
            }
        }
        Some(self.status.clone())
    }
}

/// A stable signature of everything the analysed surfaces are built from.
///
/// Serializing the geometry scaffold and the design point is what the live
/// preview's cache key already does; hashing it keeps the comparison cheap
/// enough to run once per frame while the window is open.
fn geometry_signature(config: &AlasConfig, design: &DesignVector, empennage: bool) -> u64 {
    let mut hasher = DefaultHasher::new();
    serde_json::to_string(&config.geometry)
        .unwrap_or_default()
        .hash(&mut hasher);
    serde_json::to_string(design)
        .unwrap_or_default()
        .hash(&mut hasher);
    empennage.hash(&mut hasher);
    hasher.finish()
}
