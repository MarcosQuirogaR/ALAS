// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Camera, design, log and preview-scene accessors on the application state.

use super::types::*;
use super::{AppState, MAX_LOG_LINES};
pub use crate::viewport::PreviewCamera;
use alas_config::{DesignVector, Severity, DESIGN_VARIABLE_SPECS};
use alas_viz::SceneViewState;
use serde_json::Value;

impl AppState {
    /// Return the orbit state belonging to one three-dimensional preview.
    pub fn preview_camera_mut(&mut self, id: impl Into<String>) -> &mut PreviewCamera {
        self.preview_cameras.entry(id.into()).or_default()
    }

    /// Return the orbit state belonging to one three-dimensional result.
    pub fn result_camera_mut(&mut self, id: impl Into<String>) -> &mut PreviewCamera {
        self.result_cameras.entry(id.into()).or_default()
    }

    /// Return the camera shared by both modes of the unified aircraft viewer.
    pub fn active_preview_camera(&self) -> PreviewCamera {
        self.preview_cameras
            .get(AIRCRAFT_PREVIEW_CAMERA_ID)
            .copied()
            .unwrap_or_default()
    }

    /// Return the persistent state belonging to one interactive canvas.
    pub fn view_state_mut(&mut self, key: impl Into<String>) -> &mut SceneViewState {
        self.view_states.entry(key.into()).or_default()
    }

    /// The design point currently shown in the design-space editor.
    pub fn current_design(&self) -> Option<DesignVector> {
        serde_json::to_value(&self.design_values)
            .ok()
            .and_then(|value| serde_json::from_value(value).ok())
    }

    /// The edited optimizer bounds in design-vector order.
    pub fn current_design_bounds(&self) -> Option<Vec<(f64, f64)>> {
        DESIGN_VARIABLE_SPECS
            .iter()
            .map(|spec| self.bounds.get(spec.name).copied())
            .collect()
    }

    /// Append a run-log line, trimming the oldest once the cap is reached.
    ///
    /// An `Error`-severity line always opens the run log, independent of
    /// whatever the panel's current visibility was. Every pre-run validation
    /// failure and every Setup > Tools picker failure reports through this
    /// path, so a user who has closed the log (or never opened it) still sees
    /// why clicking Run or Browse did nothing. This only flips a visibility
    /// flag; it does not request keyboard focus.
    pub fn log(&mut self, text: impl Into<String>, kind: LogKind) {
        if matches!(kind, LogKind::Error) {
            self.run_log_open = true;
        }
        let elapsed = self.run_started.map(|started| started.elapsed());
        self.logs.push(LogLine {
            text: text.into(),
            kind,
            elapsed,
            run_id: if elapsed.is_some() {
                self.run_identity
            } else {
                0
            },
        });
        if self.logs.len() > MAX_LOG_LINES {
            let overflow = self.logs.len() - MAX_LOG_LINES;
            self.logs.drain(0..overflow);
        }
    }

    /// The mutable value of one top-level configuration group.
    pub fn group_mut(&mut self, group: &str) -> Option<&mut Value> {
        self.config_values.get_mut(group)
    }

    /// Whether a blocking (error-severity) validation issue exists, which
    /// disables the Run button.
    pub fn blocked(&self) -> bool {
        self.validation_findings
            .iter()
            .any(|i| i.severity == Severity::Error)
    }

    /// Update the live-preview scene from the current geometry and dock tab.
    pub fn update_preview_scene(&mut self) {
        // A text editor can temporarily hold an unreadable or physically
        // invalid configuration. Keep the last accepted scene visible until
        // the edit is valid again; replacing it with a blank scene makes an
        // ordinary mid-edit keystroke look like data loss.
        let invalid = self.typed_config().is_none_or(|config| {
            alas_config::validate(&config)
                .iter()
                .any(|issue| issue.severity == alas_config::Severity::Error)
        });
        if invalid {
            return;
        }
        let next_scene = crate::scene::build_preview_scene(self);
        if next_scene.is_none() && self.preview_scene.is_some() {
            return;
        }
        self.preview_scene = next_scene;
        self.preview_scene_revision = self.preview_scene_revision.wrapping_add(1);
    }

    /// Update the results-gallery scene from the current result and selection.
    pub fn update_result_scene(&mut self) {
        // A run identity is stable while new stages arrive. Both successful
        // scenes and cached `None`/unavailable scenes must be invalidated at
        // each data boundary, including the final result.
        self.result_figure_cache.clear();
        self.patran_textures.clear();
        self.result_scene = crate::scene::build_result_scene(self);
    }

    /// Elapsed milliseconds since the current run began, or zero when idle.
    pub fn elapsed_ms(&self) -> u128 {
        self.run_started
            .map(|t| t.elapsed().as_millis())
            .unwrap_or(0)
    }
}
