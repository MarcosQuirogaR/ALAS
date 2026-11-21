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

use super::session::{SandboxDesign, SandboxLayout, SandboxSession, WorkspaceMode};

impl SandboxSession {
    /// Whether the sandbox aircraft has edits that were neither saved nor
    /// replaced since it was installed or last saved. It compares revision
    /// numbers, not content, so undoing back to the saved state still reads as
    /// dirty (the safe side), and a file saved from the Sandbox and loaded over
    /// a dirty guided case asks although the guided case is only held.
    pub fn dirty(&self) -> bool {
        self.active() && self.revision != self.saved_revision
    }

    /// The sandbox aircraft was saved or replaced as it stands.
    pub fn mark_saved(&mut self) {
        self.saved_revision = self.revision;
    }
}

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
        let mut envelope: Option<WorkspaceEnvelope> = document
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
        let (config, notes) =
            AlasConfig::from_value_with_notes(document).map_err(|error| error.to_string())?;
        let mut load_notes = notes.retired_keys.messages();
        let canonical = full_config_values(&config);
        if let Some(design) = envelope
            .as_mut()
            .and_then(|saved| saved.sandbox_design.as_mut())
        {
            let (config, notes) = AlasConfig::from_value_with_notes(&design.config_values)
                .map_err(|error| format!("saved sandbox configuration: {error}"))?;
            for message in notes.retired_keys.messages() {
                if !load_notes.contains(&message) {
                    load_notes.push(message);
                }
            }
            design.config_values = full_config_values(&config);
        }
        // What loading changed about the saved file is said, not silent: a
        // retired viscous margin reset to the class default, dropped keys.
        for message in load_notes {
            self.log(crate::views::tr(message), crate::state::LogKind::Warn);
        }
        // A loaded non-default profile may be a deliberate case-file or
        // Advanced Settings edit. Preset operational profiles are also
        // non-default, though, and must still receive their route-aware
        // cruise-leg count. Preserve profiles that differ from both generic
        // and preset operational defaults.
        let automatic_profile = crate::views::mission_profile_inputs::is_automatic_profile(&config);
        self.mission_profile_manual_edit = !automatic_profile;
        self.mission_profile_route_signature.clear();
        self.mission_profile_regeneration_prompt = false;
        self.mission_profile_retained_validation = None;
        // A file with no design vector of its own names the aircraft by its
        // preset, so it is that aircraft's vector, not the one left on screen.
        let design_values = envelope
            .as_ref()
            .filter(|e| !e.design_vector.is_empty())
            .map(|e| e.design_vector.clone())
            .or_else(|| preset_design_values(&config.preset));

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
            // The loaded document is the aircraft to open, not the guided case.
            self.sandbox.resume_exact = true;
            let entered = self.enter_sandbox(false);
            self.sandbox.resume_exact = false;
            if !entered {
                return Err("the sandbox could not be opened for the loaded file".to_owned());
            }
            return Ok(());
        }

        self.config_values = canonical;
        self.bump_config_revision();
        if let Some(design_values) = design_values {
            self.design_values = design_values;
        }
        self.active_preset = config.preset.clone();
        if automatic_profile {
            crate::views::mission_profile_inputs::initialize_route_profile(self);
        }
        if self.sandbox.active() {
            self.sandbox.undo.clear();
            self.sandbox.estimates.abandon();
            self.on_sandbox_model_changed();
            // A loaded document is the sandbox aircraft as saved.
            self.sandbox.mark_saved();
        } else {
            self.keep_design_and_reset_bounds();
            self.on_config_modified();
        }
        Ok(())
    }
}

/// The design vector of a registered preset, by design variable; `None` when
/// the name is empty or not registered.
fn preset_design_values(preset: &str) -> Option<std::collections::BTreeMap<String, f64>> {
    let design = alas_config::presets::get(preset).ok()?.design_vector;
    let value = serde_json::to_value(design).ok()?;
    let values = value
        .as_object()?
        .iter()
        .filter_map(|(key, value)| value.as_f64().map(|value| (key.clone(), value)))
        .collect();
    Some(values)
}

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_preset_only_file_brings_the_preset_design_vector() {
        let mut opened = state();
        opened
            .apply_workspace_document(&json!({ "preset": "A320-200" }))
            .unwrap();
        let expected = alas_config::presets::get("A320-200").unwrap().design_vector;
        assert_eq!(opened.active_preset, "A320-200");
        assert_eq!(opened.design_values["span_m"], expected.span_m);
    }

    #[test]
    fn the_sandbox_opens_on_the_loaded_preset_not_on_ave() {
        let mut opened = state();
        opened.load_preset("A320-200");
        let expected = alas_config::presets::get("A320-200").unwrap().design_vector;
        assert!(opened.enter_sandbox(false));
        assert_eq!(opened.design_values["span_m"], expected.span_m);
        let config = opened.typed_config().unwrap();
        assert!(config.preset.is_empty());
        assert_eq!(
            config.optimizer.design_space.mode,
            alas_config::DesignMode::BaselineSandbox
        );
        // Asked for afresh, it is still the reference aircraft.
        let mut fresh = state();
        fresh.load_preset("A320-200");
        assert!(fresh.enter_sandbox(true));
        let ave = alas_config::presets::get("AVE").unwrap().design_vector;
        assert_eq!(fresh.design_values["span_m"], ave.span_m);
    }

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
    fn every_load_note_has_a_spanish_catalog_entry() {
        let all = alas_config::RetiredKeysDropped {
            solver: true,
            weights: true,
            span: true,
            constraint_allowances: true,
            preference_weight: true,
            korn_technology_factor: true,
            engine_scale: true,
            fan_face_mach: true,
            requested_count_budget: true,
            requested_count_replay: true,
            fitted_viscous_margin: true,
        };
        let messages = all.messages();
        assert_eq!(messages.len(), 11);
        let catalog = alas_i18n::es::native_desktop_catalog();
        for message in messages {
            assert!(catalog.contains_key(message), "no ES entry for: {message}");
        }
    }

    #[test]
    fn loading_a_file_saved_before_the_class_margin_says_it_reset_the_retired_value() {
        let mut loaded = state();
        let mut old = serde_json::to_value(AlasConfig::default()).expect("config");
        old["drag_model"]
            .as_object_mut()
            .expect("drag model")
            .remove("schema_version");
        old["drag_model"]["viscous_margin"] = json!(1.0);
        loaded.apply_workspace_document(&old).expect("loads");
        assert!(loaded
            .logs
            .iter()
            .any(|line| line.text.contains("propulsion class")));
        assert!(loaded.config_values["drag_model"]["viscous_margin"].is_null());
        // A v1.3 file (1.10, no fraction, no marker) is reset with the same note.
        let mut v13 = state();
        let mut older = serde_json::to_value(AlasConfig::default()).expect("config");
        older["drag_model"]
            .as_object_mut()
            .expect("drag model")
            .remove("schema_version");
        older["drag_model"]["viscous_margin"] = json!(1.10);
        older["drag_model"]["lift_dependent_viscous_fraction"] = json!(0.0);
        v13.apply_workspace_document(&older).expect("loads");
        assert!(v13
            .logs
            .iter()
            .any(|line| line.text.contains("propulsion class")));
        assert!(v13.config_values["drag_model"]["viscous_margin"].is_null());
        // The same file with a non-zero fraction states its 1.10: kept, silent.
        let mut kept = state();
        older["drag_model"]["lift_dependent_viscous_fraction"] = json!(0.25);
        kept.apply_workspace_document(&older).expect("loads");
        assert!(!kept
            .logs
            .iter()
            .any(|line| line.text.contains("propulsion class")));
        assert_eq!(
            kept.config_values["drag_model"]["viscous_margin"],
            json!(1.10)
        );
        // The same value stated on a current build is kept and silent.
        let mut current = state();
        let mut stated = serde_json::to_value(AlasConfig::default()).expect("config");
        stated["drag_model"]["viscous_margin"] = json!(1.0);
        current.apply_workspace_document(&stated).expect("loads");
        assert!(!current
            .logs
            .iter()
            .any(|line| line.text.contains("propulsion class")));
        assert_eq!(
            current.config_values["drag_model"]["viscous_margin"],
            json!(1.0)
        );
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

    #[test]
    fn a_guided_project_migrates_its_saved_sandbox_before_resuming() {
        let mut saved = state();
        assert!(saved.enter_sandbox(false));
        saved.design_values.insert("span_m".to_owned(), 62.0);
        assert!(saved.resolve_leave_sandbox(super::super::session::ExitChoice::Promote));
        let mut document = saved.workspace_document();
        let nested = &mut document[WORKSPACE_ENVELOPE_KEY]["sandbox_design"]["config_values"];
        nested["optimizer"]["relaxation"] = json!({"enabled": true, "allowed_violated_groups": 4});
        nested["optimizer"]["objective"]["mass_constraints"] = json!("soft");
        nested["optimizer"]["objective"]["soft_penalty_weight"] = json!(99.0);
        let mut loaded = state();
        loaded
            .apply_workspace_document(&document)
            .expect("legacy project loads");
        assert!(!loaded.sandbox.active());
        let migrated = &loaded.sandbox.last_design.as_ref().unwrap().config_values;
        assert!(migrated.pointer("/optimizer/relaxation").is_none());
        assert!(migrated
            .pointer("/optimizer/objective/mass_constraints")
            .is_none());
        assert!(migrated
            .pointer("/optimizer/objective/soft_penalty_weight")
            .is_none());
        assert!(loaded.enter_sandbox(false));
        assert!(loaded.typed_config().is_some());
        assert_eq!(loaded.design_values["span_m"], 62.0);
    }
}
