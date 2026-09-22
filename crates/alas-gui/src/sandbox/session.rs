// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The workspace mode contract: one guided case, one sandbox case, and the
//! explicit transitions between them.
//!
//! Entering the sandbox moves the complete guided case (configuration,
//! design vector, run log, results, result caches, page and run options)
//! into [`SandboxSession::prior_case`] and installs the sandbox case in the
//! same state fields, so every existing view keeps reading one authoritative
//! configuration. Leaving with *discard* moves the guided case back
//! untouched. Leaving with *promote* keeps the sandbox aircraft as the
//! guided workspace's custom baseline: the preset provenance is cleared, the
//! design mode becomes clean-sheet with the fuselage no longer sized from
//! the cabin, and the prior case's results are dropped because they belong
//! to a different aircraft. *Cancel* changes nothing.
//!
//! The first entry starts from the AVE reference. A later entry resumes the
//! last sandbox or promoted custom design unless the caller asks for a new
//! sandbox from AVE. Undo history is per session and never persisted.

use std::collections::BTreeMap;

use alas_config::optimizer::DesignMode;
use alas_config::{presets, validate, AlasConfig, DESIGN_VARIABLE_SPECS};
use alas_report::families::geometry::{FramingReference, SandboxSceneModel, SceneFraming};
use alas_report::scene::Scene;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config_edit::full_config_values;
use crate::state::{AppState, LogKind};
use crate::views::results_view::SolverResultView;
use crate::views::tr;

pub use super::case::{CaseSnapshot, SandboxLayout};
use super::fields::{Discipline, REFERENCE_PRESET};
use super::quick::QuickEstimates;
use super::undo::{EditSnapshot, UndoStack};

/// Which workspace the shell renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceMode {
    /// The standard guided pages.
    #[default]
    Guided,
    /// The full-window clean-sheet sandbox.
    Sandbox,
}

/// The starting-design choice shown at the top of Inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartingDesign {
    /// Design an aircraft from scratch in the sandbox (or its promoted result).
    CleanSheet,
    /// Analyse or adapt a registered aircraft with protected geometry.
    PresetAircraft,
}

/// The persisted part of a sandbox aircraft.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SandboxDesign {
    /// The configuration edit buffer.
    pub config_values: Value,
    /// The design vector by variable name.
    pub design_values: BTreeMap<String, f64>,
}

/// What the user chose when asked how to leave the sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitChoice {
    /// Restore the prior case and drop the sandbox aircraft.
    Discard,
    /// Keep the sandbox aircraft as the guided workspace's custom baseline.
    Promote,
    /// Stay in the sandbox.
    Cancel,
}

/// The sandbox's session state.
#[derive(Default)]
pub struct SandboxSession {
    /// Which workspace is active.
    pub mode: WorkspaceMode,
    /// The guided case preserved while the sandbox is active.
    pub prior_case: Option<CaseSnapshot>,
    /// The last sandbox or promoted custom design, for resuming.
    pub last_design: Option<SandboxDesign>,
    /// Monotonic identity of the sandbox aircraft; bumps on every
    /// model-affecting edit, including detached settings.
    pub revision: u64,
    /// Session-only undo history.
    pub undo: UndoStack,
    /// Quick Analysis results and worker.
    pub estimates: QuickEstimates,
    /// Persisted window layout.
    pub layout: SandboxLayout,
    /// Whether the leave-sandbox prompt is showing.
    pub exit_prompt: bool,
    /// The revision the last sandbox Full Analysis was launched for.
    pub full_analysis_revision: Option<u64>,
    /// The run the log window was auto-opened for, so closing it stays closed.
    pub log_auto_open_for_run: Option<u64>,
    /// The Advanced Settings tab shown, by page id.
    pub advanced_tab: String,
    /// Whether the Full Analysis results window is open.
    pub results_window_open: bool,
    /// The last successfully built aircraft, reprojected on camera motion.
    pub airplane: Option<alas_geom::aircraft::airplane::Airplane>,
    /// The field inventory, built once per sandbox case.
    pub fields: Vec<super::fields::SandboxField>,
    /// The AVE reference values every reset returns to.
    pub reference: Option<super::fields::ReferenceValues>,
    /// The last successfully built scene and its framing.
    pub scene: Option<(Scene, SceneFraming)>,
    /// Monotonic generation of `scene` for the viewport texture cache.
    pub scene_revision: u64,
    /// The last built aircraft prepared for drawing (faces and painter
    /// partition), reused for every camera frame.
    pub model: Option<SandboxSceneModel>,
    /// The framing kept across camera motion and geometry edits; `None`
    /// until the next redraw fits the shown components.
    pub framing: Option<FramingReference>,
    /// The viewport size the scene canvas is drawn for, in points.
    pub viewport_size: Option<(f32, f32)>,
    /// The message of the last rejected edit, shown until the next valid one.
    pub rejected_edit: Option<String>,
    /// Search text of the Parameter Panel.
    pub search: String,
    /// Direct-manipulation drag in progress.
    pub drag: Option<super::drag::ActiveDrag>,
    /// Undo stack capacity.
    pub undo_capacity: usize,
}

impl SandboxSession {
    /// Whether the sandbox workspace is active.
    pub fn active(&self) -> bool {
        self.mode == WorkspaceMode::Sandbox
    }

    /// The focused discipline, if any.
    pub fn focus(&self) -> Option<Discipline> {
        let id = self.layout.focus.as_deref()?;
        Discipline::ALL.into_iter().find(|d| d.id() == id)
    }

    /// Focus one discipline (isolating it in the preview) or return to the
    /// whole-aircraft overview.
    pub fn set_focus(&mut self, discipline: Option<Discipline>) {
        self.layout.focus = discipline.map(|d| d.id().to_owned());
    }
}

/// Undo steps retained per session.
pub const UNDO_CAPACITY: usize = 200;

impl AppState {
    /// The starting-design choice the current guided case represents.
    pub fn starting_design(&self) -> StartingDesign {
        if self.sandbox.active() || self.active_preset.is_empty() {
            StartingDesign::CleanSheet
        } else {
            StartingDesign::PresetAircraft
        }
    }

    /// Whether the guided case is a promoted or loaded custom design.
    pub fn has_custom_design(&self) -> bool {
        self.sandbox.last_design.is_some()
    }

    /// The sandbox aircraft as a fresh AVE case.
    fn ave_sandbox_design() -> Option<SandboxDesign> {
        let preset = presets::get(REFERENCE_PRESET).ok()?;
        let operational = preset.operational_mission_defaults();
        let mut config = AlasConfig::default();
        config.preset.clear();
        config.geometry = preset.geometry.clone();
        config.geometry.engine.apply_engine_spec();
        config.requirements = preset.requirements.clone();
        config.cabin = preset.planning_cabin_config();
        config.landing_gear = preset.landing_gear.clone();
        if let Some(mass_model) = &preset.mass_model {
            config.mass_model = mass_model.clone();
        }
        if let Some(performance) = &preset.performance {
            config.performance = performance.clone();
        }
        config.departure_airport = operational.departure_airport.to_owned();
        config.arrival_airport = operational.arrival_airport.to_owned();
        config.mission.profile = operational.profile;
        config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
        let design_values = serde_json::to_value(preset.design_vector)
            .ok()?
            .as_object()?
            .iter()
            .filter_map(|(k, v)| v.as_f64().map(|v| (k.clone(), v)))
            .collect();
        Some(SandboxDesign {
            config_values: full_config_values(&config),
            design_values,
        })
    }

    /// Install `design` as the active sandbox case.
    fn install_sandbox_design(&mut self, design: SandboxDesign, opening_line: String) {
        self.config_values = design.config_values;
        self.design_values = design.design_values;
        // Every design variable is pinned: the drawn aircraft is analysed as
        // drawn, never redesigned by a run.
        self.bounds = DESIGN_VARIABLE_SPECS
            .iter()
            .filter_map(|spec| {
                self.design_values
                    .get(spec.name)
                    .map(|v| (spec.name.to_owned(), (*v, *v)))
            })
            .collect();
        self.active_preset.clear();
        self.selected_aux_preset.clear();
        self.pipeline_result = None;
        self.run_events.clear();
        self.logs = Vec::new();
        self.result_scene = None;
        self.result_figure_cache.clear();
        self.selected_result_id = "polar_comparison".to_owned();
        self.results_tab = "summary".to_owned();
        self.selected_solver_view = SolverResultView::Vlm;
        self.active_page = "inputs".to_owned();
        self.run_options.optimize = false;
        self.run_options.compare_baseline = false;
        self.status_message = tr("Sandbox ready.");
        self.stage.clear();
        self.log(opening_line, LogKind::Info);
        self.sandbox.undo = UndoStack::new(UNDO_CAPACITY);
        self.sandbox.full_analysis_revision = None;
        self.sandbox.results_window_open = false;
        self.sandbox.rejected_edit = None;
        self.sandbox.drag = None;
        self.sandbox.exit_prompt = false;
        self.sandbox.revision = self.sandbox.revision.wrapping_add(1);
        self.sandbox.estimates.invalidate();
        self.sandbox.fields = super::fields::inventory(&self.schema);
        self.sandbox.reference = super::fields::reference_values();
        if let Some(config) = self.typed_config() {
            self.validation_findings = validate(&config);
        }
        // A replaced aircraft is framed afresh; edits keep the framing.
        self.sandbox.framing = None;
        self.refresh_sandbox_scene();
    }

    /// Enter the sandbox. `fresh` starts from AVE; otherwise the last
    /// sandbox or custom design is resumed when one exists.
    ///
    /// Returns `false` when the sandbox could not open (a run is in flight
    /// or the reference preset is unavailable).
    pub fn enter_sandbox(&mut self, fresh: bool) -> bool {
        if self.sandbox.active() {
            if fresh {
                return self.new_sandbox_from_reference();
            }
            return true;
        }
        if self.is_running {
            self.status_message =
                tr("Finish or cancel the current run before entering the sandbox.");
            return false;
        }
        let resumed = if fresh {
            None
        } else if self.active_preset.is_empty() && self.sandbox.last_design.is_some() {
            // The guided case is itself the promoted custom design; carry
            // its latest edits rather than an older copy.
            Some(SandboxDesign {
                config_values: self.config_values.clone(),
                design_values: self.design_values.clone(),
            })
        } else {
            self.sandbox.last_design.clone()
        };
        let (design, line) = match resumed {
            Some(mut design) => {
                if let Some(mode) = design
                    .config_values
                    .pointer_mut("/optimizer/design_space/mode")
                {
                    *mode = Value::String(DesignMode::BaselineSandbox.as_str().to_owned());
                }
                (design, tr("Sandbox resumed from the last custom design."))
            }
            None => match Self::ave_sandbox_design() {
                Some(design) => (design, tr("Sandbox opened from the AVE reference.")),
                None => {
                    self.status_message = tr("The AVE reference preset is unavailable.");
                    return false;
                }
            },
        };
        let prior = self.take_case();
        self.sandbox.prior_case = Some(prior);
        self.sandbox.mode = WorkspaceMode::Sandbox;
        // The estimates strip never appears on entry; Quick Analysis opens it.
        self.sandbox.layout.estimates_open = false;
        self.install_sandbox_design(design, line);
        true
    }

    /// Replace the active sandbox aircraft with a fresh AVE reference.
    pub fn new_sandbox_from_reference(&mut self) -> bool {
        if !self.sandbox.active() || self.is_running {
            return false;
        }
        match Self::ave_sandbox_design() {
            Some(design) => {
                self.install_sandbox_design(design, tr("New sandbox from the AVE reference."));
                true
            }
            None => false,
        }
    }

    /// Ask how to leave the sandbox.
    pub fn request_leave_sandbox(&mut self) {
        if self.sandbox.active() {
            self.sandbox.exit_prompt = true;
        }
    }

    /// Resolve the leave-sandbox prompt.
    ///
    /// Returns `false` when the choice could not be applied (a sandbox Full
    /// Analysis is still running); the sandbox then stays open.
    pub fn resolve_leave_sandbox(&mut self, choice: ExitChoice) -> bool {
        self.sandbox.exit_prompt = false;
        if !self.sandbox.active() {
            return false;
        }
        match choice {
            ExitChoice::Cancel => true,
            ExitChoice::Discard => {
                if self.is_running {
                    self.status_message = tr("Cancel the running sandbox analysis before leaving.");
                    return false;
                }
                self.sandbox.estimates.abandon();
                self.sandbox.undo.clear();
                self.sandbox.drag = None;
                self.sandbox.last_design = None;
                self.sandbox.mode = WorkspaceMode::Guided;
                if let Some(prior) = self.sandbox.prior_case.take() {
                    self.restore_case(prior);
                }
                self.log(
                    tr("Sandbox discarded; the previous case is active again."),
                    LogKind::Info,
                );
                self.on_config_modified();
                true
            }
            ExitChoice::Promote => {
                if self.is_running {
                    self.status_message = tr("Cancel the running sandbox analysis before leaving.");
                    return false;
                }
                let Some(mut config) = self.typed_config() else {
                    self.status_message = tr(
                        "The sandbox configuration is not valid; fix the highlighted fields first.",
                    );
                    self.sandbox.exit_prompt = true;
                    return false;
                };
                config.preset.clear();
                config.optimizer.design_space.mode = DesignMode::CleanSheet;
                // The promoted geometry is the baseline as drawn; sizing the
                // fuselage from the cabin would silently redraw it.
                config.optimizer.design_space.fuselage_sized_by_cabin = false;
                self.config_values = full_config_values(&config);
                self.sandbox.last_design = Some(SandboxDesign {
                    config_values: self.config_values.clone(),
                    design_values: self.design_values.clone(),
                });
                // Results that do not match the promoted aircraft are dropped:
                // the prior case's outright, the sandbox's own when edits
                // followed its Full Analysis.
                let prior = self.sandbox.prior_case.take();
                let prior_run_identity =
                    prior.as_ref().map(CaseSnapshot::run_identity).unwrap_or(0);
                self.run_identity = self.run_identity.max(prior_run_identity);
                if self.sandbox.full_analysis_revision != Some(self.sandbox.revision) {
                    self.pipeline_result = None;
                    self.run_events.clear();
                    self.result_scene = None;
                }
                self.result_figure_cache.clear();
                self.sandbox.estimates.abandon();
                self.sandbox.undo.clear();
                self.sandbox.drag = None;
                self.sandbox.mode = WorkspaceMode::Guided;
                self.active_preset.clear();
                self.active_page = "inputs".to_owned();
                self.run_options.optimize = true;
                self.reset_design_space_bounds_to_mode();
                self.log(
                    tr("Sandbox promoted: the custom aircraft is now the guided baseline."),
                    LogKind::Info,
                );
                self.on_config_modified();
                true
            }
        }
    }

    /// The current editable state, for undo bookkeeping.
    pub fn edit_snapshot(&self) -> EditSnapshot {
        EditSnapshot {
            config_values: self.config_values.clone(),
            design_values: self.design_values.clone(),
        }
    }

    /// Apply a snapshot from the undo history.
    fn apply_edit_snapshot(&mut self, snapshot: EditSnapshot) {
        self.config_values = snapshot.config_values;
        self.design_values = snapshot.design_values;
        self.on_sandbox_model_changed();
    }

    /// Undo the last sandbox edit.
    pub fn sandbox_undo(&mut self) -> bool {
        if !self.sandbox.active() || self.sandbox.undo.in_transaction() {
            return false;
        }
        let current = self.edit_snapshot();
        match self.sandbox.undo.undo(current) {
            Some(previous) => {
                self.apply_edit_snapshot(previous);
                true
            }
            None => false,
        }
    }

    /// Redo the last undone sandbox edit.
    pub fn sandbox_redo(&mut self) -> bool {
        if !self.sandbox.active() || self.sandbox.undo.in_transaction() {
            return false;
        }
        let current = self.edit_snapshot();
        match self.sandbox.undo.redo(current) {
            Some(next) => {
                self.apply_edit_snapshot(next);
                true
            }
            None => false,
        }
    }

    /// Bookkeeping after any model-affecting sandbox change: a new revision,
    /// invalidated estimates, revalidation and a scene rebuild that keeps
    /// the last valid scene when the geometry no longer builds.
    pub fn on_sandbox_model_changed(&mut self) {
        if !self.sandbox.active() {
            return;
        }
        self.sandbox.revision = self.sandbox.revision.wrapping_add(1);
        self.sandbox.estimates.invalidate();
        if let Some(config) = self.typed_config() {
            self.validation_findings = validate(&config);
        }
        self.refresh_sandbox_scene();
    }

    /// Rebuild the sandbox aircraft from the committed model and redraw it,
    /// keeping the last valid aircraft when the geometry does not build.
    pub fn refresh_sandbox_scene(&mut self) {
        match super::scene::build_sandbox_airplane(self) {
            Some((plane, _)) => {
                self.sandbox.model = Some(super::scene::build_sandbox_model(&plane));
                self.sandbox.airplane = Some(plane);
                self.sandbox.rejected_edit = None;
                self.reproject_sandbox_scene();
            }
            None => {
                if self.sandbox.rejected_edit.is_none() {
                    self.sandbox.rejected_edit = Some(tr(
                        "The geometry does not build; the last valid shape is shown.",
                    ));
                }
            }
        }
    }

    /// Redraw the cached aircraft for the current camera, focus, theme and
    /// viewport without rebuilding its geometry. The framing in use is
    /// kept; a cleared framing is fitted to the shown components and kept
    /// from then on.
    pub fn reproject_sandbox_scene(&mut self) {
        if let Some(model) = &self.sandbox.model {
            let built = super::scene::project_sandbox_model(self, model);
            self.sandbox.framing = Some(built.1.reference());
            self.sandbox.scene = Some(built);
            self.sandbox.scene_revision = self.sandbox.scene_revision.wrapping_add(1);
        }
    }

    /// Fit the framing to the components now shown and redraw: the explicit
    /// Fit action, and the deliberate reframe on a focus change or a new
    /// design. Camera presets, orbit, zoom, resizing and geometry edits
    /// never call this, so the pixels per metre they show stay comparable.
    pub fn refit_sandbox_framing(&mut self) {
        self.sandbox.framing = None;
        self.reproject_sandbox_scene();
    }

    /// Focus one discipline (isolating it in the preview) or return to the
    /// overview, refitting the framing to what is now shown.
    pub fn set_sandbox_focus(&mut self, discipline: Option<Discipline>) {
        self.sandbox.set_focus(discipline);
        self.refit_sandbox_framing();
    }

    /// Record the viewport size the scene is drawn for. A changed size
    /// redraws the kept framing on the new canvas, so the model scales
    /// with the viewport's smaller side and nothing is refitted.
    pub fn set_sandbox_viewport_size(&mut self, size: (f32, f32)) -> bool {
        let changed = self
            .sandbox
            .viewport_size
            .is_none_or(|(w, h)| (w - size.0).abs() > 0.5 || (h - size.1).abs() > 0.5);
        if changed {
            self.sandbox.viewport_size = Some(size);
            self.reproject_sandbox_scene();
        }
        changed
    }
}

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
