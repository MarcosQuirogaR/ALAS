// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Saving and loading the workspace.
//!
//! A saved file is the active case's configuration, exactly as before, plus
//! one `alas_workspace` envelope carrying what the configuration alone does
//! not: the workspace mode, the active design vector, the last sandbox or
//! promoted custom design so a later entry can resume it, and the sandbox
//! window layout. The envelope is versioned; the configuration loader strips
//! it, so the file stays a valid configuration for every other consumer, and
//! a file without an envelope loads exactly as it always did. Undo history is
//! never written.

use std::collections::BTreeMap;

use alas_config::{AlasConfig, WORKSPACE_ENVELOPE_KEY};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config_edit::full_config_values;
use crate::state::AppState;

use super::session::{SandboxDesign, SandboxLayout, WorkspaceMode};

/// The envelope format version this build writes.
pub const WORKSPACE_VERSION: u32 = 1;

/// The desktop session stored beside the configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceEnvelope {
    /// Format version.
    pub version: u32,
    /// Which workspace the file was saved from.
    #[serde(default)]
    pub mode: WorkspaceMode,
    /// The active case's design vector by variable name.
    #[serde(default)]
    pub design_vector: BTreeMap<String, f64>,
    /// The last sandbox or promoted custom design, for resuming.
    #[serde(default)]
    pub sandbox_design: Option<SandboxDesign>,
    /// Sandbox window layout.
    #[serde(default)]
    pub layout: SandboxLayout,
}

impl AppState {
    /// The document `save_config` writes.
    pub fn workspace_document(&self) -> Value {
        let mut document = self.config_values.clone();
        let envelope = WorkspaceEnvelope {
            version: WORKSPACE_VERSION,
            mode: self.sandbox.mode,
            design_vector: self.design_values.clone(),
            sandbox_design: if self.sandbox.active() {
                Some(SandboxDesign {
                    config_values: self.config_values.clone(),
                    design_values: self.design_values.clone(),
                })
            } else {
                self.sandbox.last_design.clone()
            },
            layout: self.sandbox.layout.clone(),
        };
        if let (Some(map), Ok(envelope)) =
            (document.as_object_mut(), serde_json::to_value(envelope))
        {
            map.insert(WORKSPACE_ENVELOPE_KEY.to_owned(), envelope);
        }
        document
    }

    /// Apply a loaded document to the workspace.
    ///
    /// The configuration replaces the active case. A file saved from the
    /// sandbox opens the sandbox (preserving the current guided case) when
    /// the guided workspace is active; a file saved from the guided
    /// workspace loaded while the sandbox is open replaces the sandbox
    /// aircraft and stays in the sandbox.
    ///
    /// # Errors
    ///
    /// The configuration loader's message when the file is not a valid
    /// configuration.
    pub fn apply_workspace_document(&mut self, document: &Value) -> Result<(), String> {
        let envelope: Option<WorkspaceEnvelope> = document
            .get(WORKSPACE_ENVELOPE_KEY)
            .map(|value| serde_json::from_value(value.clone()))
            .transpose()
            .map_err(|error| format!("workspace envelope: {error}"))?;
        if let Some(envelope) = &envelope {
            if envelope.version > WORKSPACE_VERSION {
                return Err(format!(
                    "workspace envelope version {} is newer than this build supports ({WORKSPACE_VERSION})",
                    envelope.version
                ));
            }
        }
        let config = AlasConfig::from_value(document).map_err(|error| error.to_string())?;
        let canonical = full_config_values(&config);
        let design_values = envelope
            .as_ref()
            .filter(|e| !e.design_vector.is_empty())
            .map(|e| e.design_vector.clone());

        let saved_from_sandbox = envelope
            .as_ref()
            .is_some_and(|e| e.mode == WorkspaceMode::Sandbox);
        if let Some(envelope) = &envelope {
            self.sandbox.layout = envelope.layout.clone();
            if envelope.sandbox_design.is_some() {
                self.sandbox.last_design = envelope.sandbox_design.clone();
            }
        }
        if saved_from_sandbox && !self.sandbox.active() {
            let design = SandboxDesign {
                config_values: canonical,
                design_values: design_values.unwrap_or_else(|| self.design_values.clone()),
            };
            self.sandbox.last_design = Some(design);
            if !self.enter_sandbox(false) {
                return Err("the sandbox could not be opened for the loaded file".to_owned());
            }
            return Ok(());
        }

        self.config_values = canonical;
        if let Some(design_values) = design_values {
            self.design_values = design_values;
        }
        self.active_preset = config.preset.clone();
        if self.sandbox.active() {
            self.sandbox.undo.clear();
            self.sandbox.estimates.abandon();
            self.on_sandbox_model_changed();
        } else {
            self.reset_design_space_bounds_to_mode();
            self.on_config_modified();
        }
        Ok(())
    }
}

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn state() -> AppState {
        let mut state = AppState::default();
        state.finish_walkthrough();
        state
    }

    #[test]
    fn a_sandbox_round_trips_through_the_envelope_into_a_fresh_state() {
        let mut saved = state();
        assert!(saved.enter_sandbox(false));
        saved.design_values.insert("span_m".to_owned(), 64.5);
        saved.config_values["geometry"]["wing"]["root_z_m"] = Value::from(-0.75);
        saved.sandbox.layout.estimates_open = false;
        saved
            .sandbox
            .set_focus(Some(super::super::fields::Discipline::Wing));
        saved.on_sandbox_model_changed();
        let document = saved.workspace_document();
        assert!(document.get(WORKSPACE_ENVELOPE_KEY).is_some());
        assert!(document[WORKSPACE_ENVELOPE_KEY].get("undo").is_none());

        let mut loaded = state();
        loaded.load_preset("A380-800");
        let prior = loaded.config_values.clone();
        loaded.apply_workspace_document(&document).expect("loads");
        assert!(loaded.sandbox.active());
        assert_eq!(loaded.design_values["span_m"], 64.5);
        assert_eq!(
            loaded.config_values["geometry"]["wing"]["root_z_m"],
            Value::from(-0.75)
        );
        assert!(!loaded.sandbox.layout.estimates_open);
        assert_eq!(
            loaded.sandbox.focus(),
            Some(super::super::fields::Discipline::Wing)
        );
        assert!(!loaded.sandbox.undo.can_undo());
        // The guided case that was active before the load is preserved.
        assert!(loaded.resolve_leave_sandbox(super::super::session::ExitChoice::Discard));
        assert_eq!(loaded.config_values, prior);
    }

    #[test]
    fn an_old_configuration_without_an_envelope_loads_into_the_guided_workspace() {
        let mut loaded = state();
        let mut old = serde_json::to_value(AlasConfig::default()).expect("config");
        old["requirements"]["cruise_mach"] = json!(0.78);
        loaded
            .apply_workspace_document(&old)
            .expect("old file loads");
        assert!(!loaded.sandbox.active());
        assert_eq!(
            loaded.config_values["requirements"]["cruise_mach"],
            json!(0.78)
        );
    }

    #[test]
    fn a_promoted_design_is_remembered_by_a_guided_save() {
        let mut saved = state();
        assert!(saved.enter_sandbox(false));
        saved.design_values.insert("span_m".to_owned(), 62.0);
        saved.on_sandbox_model_changed();
        assert!(saved.resolve_leave_sandbox(super::super::session::ExitChoice::Promote));
        let document = saved.workspace_document();
        let envelope: WorkspaceEnvelope =
            serde_json::from_value(document[WORKSPACE_ENVELOPE_KEY].clone()).expect("envelope");
        assert_eq!(envelope.mode, WorkspaceMode::Guided);
        assert_eq!(envelope.design_vector["span_m"], 62.0);
        assert!(envelope.sandbox_design.is_some());

        let mut loaded = state();
        loaded.apply_workspace_document(&document).expect("loads");
        assert!(!loaded.sandbox.active());
        assert_eq!(loaded.design_values["span_m"], 62.0);
        assert!(loaded.has_custom_design());
        assert!(loaded.enter_sandbox(false));
        assert_eq!(loaded.design_values["span_m"], 62.0);
    }

    #[test]
    fn a_newer_envelope_version_is_refused_honestly() {
        let mut loaded = state();
        let mut document = serde_json::to_value(AlasConfig::default()).expect("config");
        document[WORKSPACE_ENVELOPE_KEY] = json!({ "version": WORKSPACE_VERSION + 1 });
        assert!(loaded.apply_workspace_document(&document).is_err());
    }
}
