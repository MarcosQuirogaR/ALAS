// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What loading a configuration document changes about its meaning, and
//! the one rule enforced at that boundary: mission analysis is part of
//! every normal full run (clarified GUI section 5), so a saved
//! `mission.enabled = false` is ignored and reported rather than honoured.
//! The flag stays an in-memory diagnostic knob for library and
//! command-line callers (`--no-mission`).

use super::AlasConfig;
use crate::overlay::OverlayError;
use crate::{MassArchitectureMigration, LEGACY_METHOD_TOKENS};

/// What loading a configuration document changed about its meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConfigLoadNotes {
    /// The mass-method migration, if any.
    pub mass_architecture: MassArchitectureMigration,
    /// The document said `mission.enabled = false`; the loaded configuration
    /// runs the mission anyway.
    pub mission_forced_on: bool,
    /// The saved `optimizer.solver.method` token, when it named an algorithm
    /// this build no longer implements as a separate kernel and the loaded
    /// configuration was migrated to `differential_evolution` instead.
    pub legacy_solver_method: Option<&'static str>,
}

impl ConfigLoadNotes {
    /// The sentence shown when a saved file tried to switch the mission off.
    pub const MISSION_FORCED_ON: &'static str = "Mission analysis is part of every full run; the saved 'mission.enabled = false' was ignored.";

    /// The sentence shown when a saved file named a retired solver method.
    pub fn legacy_solver_method_message(token: &str) -> String {
        format!(
            "The saved optimizer method '{token}' is no longer a separate algorithm; this configuration now runs differential evolution (L-SHADE, epsilon-constrained) instead."
        )
    }

    /// One sentence per change, for logs and the run log.
    pub fn messages(&self) -> Vec<String> {
        let mut messages = Vec::new();
        if let Some(message) = self.mass_architecture.message() {
            messages.push(message);
        }
        if self.mission_forced_on {
            messages.push(Self::MISSION_FORCED_ON.to_owned());
        }
        if let Some(token) = self.legacy_solver_method {
            messages.push(Self::legacy_solver_method_message(token));
        }
        messages
    }
}

/// Whether `data` (a configuration document) asks to skip the mission.
pub fn legacy_mission_disabled(data: &serde_json::Value) -> bool {
    data.get("mission")
        .and_then(|mission| mission.get("enabled"))
        .and_then(serde_json::Value::as_bool)
        == Some(false)
}

/// The legacy solver method token `data` names, if any, matched against the
/// stable list rather than the run's own default so a document that simply
/// omits the field is never reported as migrated.
pub fn legacy_solver_method(data: &serde_json::Value) -> Option<&'static str> {
    let method = data
        .get("optimizer")?
        .get("solver")?
        .get("method")?
        .as_str()?;
    LEGACY_METHOD_TOKENS
        .iter()
        .copied()
        .find(|&token| token == method)
}

/// Apply the loading-boundary rules to an overlaid configuration and
/// assemble its notes.
pub(super) fn finish(
    mut loaded: AlasConfig,
    mass_architecture: MassArchitectureMigration,
    data: &serde_json::Value,
) -> (AlasConfig, ConfigLoadNotes) {
    let mission_forced_on = legacy_mission_disabled(data);
    if mission_forced_on {
        loaded.mission.enabled = true;
        tracing::warn!("{}", ConfigLoadNotes::MISSION_FORCED_ON);
    }
    let legacy_solver_method = legacy_solver_method(data);
    if let Some(token) = legacy_solver_method {
        loaded.optimizer.solver.method = "differential_evolution".to_owned();
        tracing::warn!("{}", ConfigLoadNotes::legacy_solver_method_message(token));
    }
    (
        loaded,
        ConfigLoadNotes {
            mass_architecture,
            mission_forced_on,
            legacy_solver_method,
        },
    )
}

impl AlasConfig {
    /// [`Self::from_value`], also reporting what loading did to the mass
    /// method.
    ///
    /// The mass architecture is the one configuration decision whose
    /// migration changes a published number (operating empty mass) so it
    /// is returned rather than logged. A front end shows it, an export
    /// records it, and a headless run can assert on it. Callers that also
    /// want the mission note use [`Self::from_value_with_notes`].
    ///
    /// # Errors
    ///
    /// As [`Self::from_value`].
    pub fn from_value_with_migration(
        data: &serde_json::Value,
    ) -> Result<(Self, MassArchitectureMigration), OverlayError> {
        Self::from_value_with_notes(data).map(|(config, notes)| (config, notes.mass_architecture))
    }
}

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod load_notes_tests {
    use super::*;

    #[test]
    fn a_saved_mission_disabled_flag_is_ignored_and_reported() {
        let data = serde_json::json!({ "mission": { "enabled": false } });
        assert!(legacy_mission_disabled(&data));
        let (config, notes) = AlasConfig::from_value_with_notes(&data).unwrap();
        assert!(config.mission.enabled);
        assert!(notes.mission_forced_on);
        assert_eq!(
            notes.messages(),
            vec![ConfigLoadNotes::MISSION_FORCED_ON.to_owned()]
        );
        assert!(AlasConfig::from_value(&data).unwrap().mission.enabled);

        let preset = serde_json::json!({ "preset": "A320-200", "mission": { "enabled": false } });
        let (config, notes) = AlasConfig::from_value_with_notes(&preset).unwrap();
        assert!(config.mission.enabled);
        assert!(notes.mission_forced_on);
    }

    #[test]
    fn documents_that_do_not_disable_the_mission_carry_no_mission_note() {
        for data in [
            serde_json::json!({}),
            serde_json::json!({ "mission": { "enabled": true } }),
            serde_json::json!({ "mission": { "timeout_s": 60.0 } }),
        ] {
            assert!(!legacy_mission_disabled(&data));
            let (config, notes) = AlasConfig::from_value_with_notes(&data).unwrap();
            assert!(config.mission.enabled);
            assert!(!notes.mission_forced_on);
            assert!(notes.messages().is_empty(), "{data}");
        }
    }

    #[test]
    fn a_saved_legacy_solver_method_is_migrated_and_reported() {
        for token in LEGACY_METHOD_TOKENS {
            let data = serde_json::json!({ "optimizer": { "solver": { "method": token } } });
            assert_eq!(legacy_solver_method(&data), Some(*token));
            let (config, notes) = AlasConfig::from_value_with_notes(&data).unwrap();
            assert_eq!(config.optimizer.solver.method, "differential_evolution");
            assert_eq!(notes.legacy_solver_method, Some(*token));
            assert_eq!(
                notes.messages(),
                vec![ConfigLoadNotes::legacy_solver_method_message(token)]
            );
            // The plain loader applies the same migration; only the note is
            // unavailable through it.
            let plain = AlasConfig::from_value(&data).unwrap();
            assert_eq!(plain.optimizer.solver.method, "differential_evolution");
        }
    }

    #[test]
    fn a_document_that_already_names_the_one_supported_method_carries_no_note() {
        for data in [
            serde_json::json!({}),
            serde_json::json!({ "optimizer": { "solver": { "method": "differential_evolution" } } }),
        ] {
            assert_eq!(legacy_solver_method(&data), None);
            let (config, notes) = AlasConfig::from_value_with_notes(&data).unwrap();
            assert_eq!(config.optimizer.solver.method, "differential_evolution");
            assert!(notes.legacy_solver_method.is_none());
            assert!(notes.messages().is_empty(), "{data}");
        }
    }

    #[test]
    fn an_unrecognized_method_is_left_untouched_for_the_dispatch_boundary_to_reject() {
        // A token that is neither the supported method nor a known legacy
        // one is not this loader's concern: it is passed through unchanged,
        // and `SolverSettings::is_supported_method` refuses it downstream.
        let data =
            serde_json::json!({ "optimizer": { "solver": { "method": "not_a_real_method" } } });
        assert_eq!(legacy_solver_method(&data), None);
        let config = AlasConfig::from_value(&data).unwrap();
        assert_eq!(config.optimizer.solver.method, "not_a_real_method");
        assert!(!crate::SolverSettings::is_supported_method(
            &config.optimizer.solver.method
        ));
    }

    #[test]
    fn the_in_memory_flag_stays_a_diagnostic_knob_and_saves_as_enabled() {
        let mut config = AlasConfig::default();
        config.mission.enabled = false;
        assert!(!config.mission.enabled);
        let saved = serde_json::to_value(AlasConfig::default()).unwrap();
        assert_eq!(saved["mission"]["enabled"], serde_json::Value::Bool(true));
        let reloaded = AlasConfig::from_value(&saved).unwrap();
        assert!(reloaded.mission.enabled);
    }
}
