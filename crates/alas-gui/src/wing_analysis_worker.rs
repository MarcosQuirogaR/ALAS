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
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use alas_aero::wing_analysis::{analyse, build_wing_model, WingAnalysisOutcome, WingModel};
use alas_config::{AlasConfig, DesignVector};
use alas_report::families::geometry::{SandboxSceneModel, SandboxSceneOptions};
use alas_report::scene::Camera3D;

use crate::sandbox::scene::{LivePreviewFit, OrbitFit, SECTION_POINTS};

use super::{RunPhase, WingAnalysisState, WingAnalysisTab};

/// Quiet time after the last geometry edit before the surfaces and the live
/// preview are rebuilt, so a slider drag does not rebuild them every frame.
pub(crate) const PREVIEW_DEBOUNCE: Duration = Duration::from_millis(150);

/// A geometry edit waiting for its debounce to elapse.
pub(crate) struct PendingGeometry {
    signature: u64,
    since: Instant,
}

/// What the background rebuild produced for one geometry signature.
struct GeometryBuilt {
    signature: u64,
    model: Result<WingModel, String>,
    preview_model: Option<SandboxSceneModel>,
}

/// A background rebuild of the surfaces and the preview scene.
pub(crate) struct GeometryJob {
    signature: u64,
    source_name: String,
    rx: Receiver<GeometryBuilt>,
}

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
    ///
    /// The first snapshot and a surface-set change are built at once. An edit
    /// of the geometry or the design point while a snapshot and its preview
    /// are showing is debounced by [`PREVIEW_DEBOUNCE`] after the last edit
    /// and rebuilt on a background thread; the last preview stays on screen
    /// until the new one is ready.
    pub fn refresh_geometry(
        &mut self,
        config: &AlasConfig,
        design: &DesignVector,
        source_name: &str,
        theme: &str,
    ) {
        self.install_built_geometry(theme);
        let signature = geometry_signature(config, design, self.includes_empennage());
        let unchanged = self.geometry_signature == Some(signature);
        if unchanged && self.model.is_some() && self.preview.is_some() {
            self.pending_geometry = None;
            if self.preview_theme != theme {
                self.rebuild_preview(theme);
            }
            return;
        }
        if self.geometry_signature.is_some() && self.model.is_some() && self.preview.is_some() {
            self.debounce_geometry(config, design, source_name, theme, signature);
            return;
        }
        self.pending_geometry = None;
        self.geometry_job = None;
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
                    self.preview_model =
                        Some(SandboxSceneModel::new(model.airplane(), SECTION_POINTS));
                    self.model = Some(model);
                    self.geometry_error = None;
                }
                Err(error) => {
                    self.preview_model = None;
                    self.model = None;
                    self.geometry_error = Some(error.to_string());
                }
            }
            self.invalidate_geometry();
        }
        self.rebuild_preview(theme);
    }

    /// Note an edit and start the background rebuild once it has been quiet
    /// for [`PREVIEW_DEBOUNCE`] and no earlier rebuild is still running.
    fn debounce_geometry(
        &mut self,
        config: &AlasConfig,
        design: &DesignVector,
        source_name: &str,
        _theme: &str,
        signature: u64,
    ) {
        if self.pending_geometry.as_ref().map(|p| p.signature) != Some(signature) {
            self.pending_geometry = Some(PendingGeometry {
                signature,
                since: Instant::now(),
            });
        }
        let quiet = self
            .pending_geometry
            .as_ref()
            .is_some_and(|p| p.since.elapsed() >= PREVIEW_DEBOUNCE);
        if !quiet || self.geometry_job.is_some() {
            return;
        }
        let geometry = config.geometry.clone();
        let design = *design;
        let surfaces = self.inputs.surfaces;
        let moment_reference = self
            .moment_reference_manual
            .then_some(self.inputs.moment_reference_m);
        let (tx, rx) = channel();
        let spawned = crate::worker_stack::spawn_analysis_thread("alas-wing-preview", move || {
            let model = build_wing_model(&geometry, &design, surfaces, moment_reference)
                .map_err(|error| error.to_string());
            let preview_model = model
                .as_ref()
                .ok()
                .map(|model| SandboxSceneModel::new(model.airplane(), SECTION_POINTS));
            let _ = tx.send(GeometryBuilt {
                signature,
                model,
                preview_model,
            });
        });
        drop(spawned);
        self.geometry_job = Some(GeometryJob {
            signature,
            source_name: source_name.to_owned(),
            rx,
        });
    }

    /// Install a finished background rebuild when it still matches the latest
    /// edit; a rebuild for a superseded edit is dropped.
    fn install_built_geometry(&mut self, theme: &str) {
        let Some(job) = &self.geometry_job else {
            return;
        };
        let built = match job.rx.try_recv() {
            Ok(built) => built,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.geometry_job = None;
                return;
            }
        };
        let Some(job) = self.geometry_job.take() else {
            return;
        };
        debug_assert_eq!(job.signature, built.signature);
        if self.pending_geometry.as_ref().map(|p| p.signature) != Some(built.signature) {
            return;
        }
        self.pending_geometry = None;
        self.geometry_signature = Some(built.signature);
        self.source_name = job.source_name;
        match built.model {
            Ok(model) => {
                if !self.moment_reference_manual {
                    self.inputs.moment_reference_m = model.reference().moment_reference_m;
                }
                self.model = Some(model);
                self.geometry_error = None;
            }
            Err(error) => {
                self.model = None;
                self.geometry_error = Some(error);
            }
        }
        self.invalidate_geometry();
        self.preview_model = built.preview_model;
        self.rebuild_preview(theme);
    }

    /// Redraw the live preview of the analysed surfaces.
    ///
    /// The scene is the sandbox's shaded renderer over a model that contains
    /// only the analysed surfaces, so the preview cannot show a component the
    /// run does not model. The surfaces are lofted and partitioned once per
    /// geometry ([`Self::preview_model`]) and the fit is made once per pane
    /// size, so an orbit frame only reprojects them, about the centre of the
    /// surfaces' bounding box.
    pub fn rebuild_preview(&mut self, theme: &str) {
        self.preview_theme = theme.to_owned();
        let camera: Camera3D = self.camera.into();
        self.preview = self.preview_model.as_ref().map(|model| {
            let fit = match self
                .preview_fit
                .filter(|fit| fit.canvas == self.preview_canvas)
            {
                Some(fit) => fit,
                None => {
                    let fit = OrbitFit::new(
                        model,
                        LivePreviewFit {
                            canvas: self.preview_canvas,
                            elev_deg: camera.elev_deg,
                            azim_deg: camera.azim_deg,
                            reserved: (0.0, 0.0),
                        },
                    );
                    self.preview_fit = Some(fit);
                    fit
                }
            };
            let options = SandboxSceneOptions {
                isolate: None,
                max_section_points: SECTION_POINTS,
                canvas: self.preview_canvas,
                reference: Some(fit.reference(&camera)),
            };
            Arc::new(model.render(Some(camera), Some(theme), &options).0)
        });
        // Every call redraws the scene (a new geometry snapshot, an orbit
        // drag, a camera preset button, or a theme switch), so the cached
        // preview texture must be treated as new content every time, not
        // only when the analysed geometry itself changes.
        self.preview_revision = self.preview_revision.wrapping_add(1);
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
        let spawned = crate::worker_stack::spawn_analysis_thread("alas-wing", move || {
            let cancelled = move || cancel.load(Ordering::Relaxed);
            let result = analyse(&model, &inputs, &cancelled).map_err(|error| error.to_string());
            let _ = tx.send(WingAnalysisRun::Finished {
                run_id,
                input_revision,
                result: Box::new(result),
            });
        });
        drop(spawned);
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

#[cfg(test)]
mod preview_revision_tests {
    use super::WingAnalysisState;

    #[test]
    fn every_rebuild_advances_the_preview_revision_even_without_a_geometry_change() {
        let mut window = WingAnalysisState::default();
        let geometry_revision = window.geometry_revision;

        // A camera drag or preset button calls `rebuild_preview` without
        // touching the analysed geometry (no `invalidate_geometry` call), so
        // `geometry_revision` alone is not a safe `SceneView::cache_revision`:
        // it would leave a stale cached texture from before the redraw.
        window.rebuild_preview("Dark");
        let after_first = window.preview_revision;
        assert_ne!(after_first, 0);
        assert_eq!(
            window.geometry_revision, geometry_revision,
            "rebuild_preview alone must not touch geometry_revision"
        );

        window.camera.apply_orbit_motion(egui::vec2(10.0, 0.0));
        window.rebuild_preview("Dark");
        let after_camera_change = window.preview_revision;
        assert_ne!(
            after_camera_change, after_first,
            "a second redraw (a camera-only change here) must get a new revision"
        );

        window.rebuild_preview("Light");
        assert_ne!(
            window.preview_revision, after_camera_change,
            "a theme-only redraw must also get a new revision"
        );
    }
}
