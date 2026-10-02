// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What loading a configuration document changes about its meaning, and
//! the one rule enforced at that boundary: mission analysis is part of
//! every normal full run (clarified GUI section 5), so a saved
//! `mission.enabled = false` is ignored and reported rather than honoured.
//! The flag stays an in-memory diagnostic knob for library and
//! command-line callers (`--no-mission`).

use std::borrow::Cow;

use super::AlasConfig;
use crate::overlay::OverlayError;
use crate::retired_keys::{LegacySolverBudget, RetiredKeysDropped};
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
    /// The generation-count budget a saved file stated, converted to the
    /// refinement evaluation budget.
    pub legacy_solver_budget: Option<LegacySolverBudget>,
    /// Retired optimizer keys the document carried and the load dropped.
    pub retired_keys: RetiredKeysDropped,
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

    /// The sentence shown when a saved file stated the budget in generations.
    pub fn legacy_solver_budget_message(budget: &LegacySolverBudget) -> String {
        format!(
            "The saved optimizer budget of {} generations with population multiplier {} was converted to a refinement budget of {} evaluations.",
            budget.max_iterations, budget.population_size, budget.refinement_max_evaluations
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
        if let Some(budget) = &self.legacy_solver_budget {
            messages.push(Self::legacy_solver_budget_message(budget));
        }
        messages.extend(self.retired_keys.messages().into_iter().map(str::to_owned));
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

/// `data` as the strict overlay should see it: without the desktop session
/// envelope a workspace file carries next to the aircraft configuration, which
/// is not aircraft data, and without the optimizer keys this build retired.
/// A document with neither is borrowed unchanged. A retired generation-count
/// budget is converted to the refinement evaluation budget and returned for
/// the load note.
pub(super) fn aircraft_document(
    data: &serde_json::Value,
) -> (Cow<'_, serde_json::Value>, Option<LegacySolverBudget>) {
    let (migrated, budget) = crate::retired_keys::with_migrated_solver_budget(data);
    let mut document = match migrated {
        Cow::Borrowed(data) => crate::retired_keys::without_retired_optimizer_keys(data),
        Cow::Owned(value) => {
            Cow::Owned(crate::retired_keys::without_retired_optimizer_keys(&value).into_owned())
        }
    };
    if document
        .as_object()
        .is_some_and(|map| map.contains_key(super::WORKSPACE_ENVELOPE_KEY))
    {
        if let Some(map) = document.to_mut().as_object_mut() {
            map.remove(super::WORKSPACE_ENVELOPE_KEY);
        }
    }
    (document, budget)
}

/// Apply the loading-boundary rules to an overlaid configuration and
/// assemble its notes.
pub(super) fn finish(
    mut loaded: AlasConfig,
    mass_architecture: MassArchitectureMigration,
    data: &serde_json::Value,
    legacy_solver_budget: Option<LegacySolverBudget>,
) -> (AlasConfig, ConfigLoadNotes) {
    if let Some(budget) = &legacy_solver_budget {
        tracing::warn!("{}", ConfigLoadNotes::legacy_solver_budget_message(budget));
    }
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
            legacy_solver_budget,
            retired_keys: RetiredKeysDropped::default(),
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
    fn a_saved_generation_budget_becomes_a_refinement_evaluation_budget() {
        let variables = crate::DESIGN_VARIABLE_SPECS.len() as i64;
        let data = serde_json::json!({
            "optimizer": { "solver": { "max_iterations": 4, "population_size": 2 } }
        });
        let (config, notes) = AlasConfig::from_value_with_notes(&data).unwrap();
        let budget = notes.legacy_solver_budget.unwrap();
        assert_eq!(budget.refinement_max_evaluations, 5 * 2 * variables);
        assert_eq!(
            config.optimizer.solver.refinement.max_evaluations,
            5 * 2 * variables
        );
        assert_eq!(
            notes.messages(),
            vec![ConfigLoadNotes::legacy_solver_budget_message(&budget)]
        );

        // An explicit new budget wins over the converted one, and a document
        // without the retired keys carries no note.
        let explicit = serde_json::json!({
            "optimizer": { "solver": { "max_iterations": 4, "refinement": { "max_evaluations": 77 } } }
        });
        let (config, _) = AlasConfig::from_value_with_notes(&explicit).unwrap();
        assert_eq!(config.optimizer.solver.refinement.max_evaluations, 77);
        let (_, notes) = AlasConfig::from_value_with_notes(&serde_json::json!({})).unwrap();
        assert!(notes.legacy_solver_budget.is_none());
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
    fn an_old_file_with_the_retired_default_method_and_its_keys_loads_and_migrates() {
        let mut value = serde_json::to_value(AlasConfig::default()).unwrap();
        value["optimizer"]["solver"]["method"] = serde_json::json!("scipy_legacy");
        value["optimizer"]["solver"]["strategy"] = serde_json::json!("best1bin");
        value["optimizer"]["weights"]["cg_penalty_scale"] = serde_json::json!(200.0);
        value["optimizer"]["weights"]["ld_weight"] = serde_json::json!(1.0);

        let (config, notes) = AlasConfig::from_value_with_notes(&value).unwrap();

        assert_eq!(config.optimizer.solver.method, "differential_evolution");
        assert_eq!(notes.legacy_solver_method, Some("scipy_legacy"));
        assert!(notes.retired_keys.solver && notes.retired_keys.weights);
        assert!(!notes.retired_keys.span);
        let messages = notes.messages();
        assert_eq!(messages.len(), 3, "{messages:?}");
        assert_eq!(
            messages[0],
            ConfigLoadNotes::legacy_solver_method_message("scipy_legacy")
        );
        // What the retired keys carried is dropped, not reinterpreted.
        let saved = serde_json::to_value(&config).unwrap();
        assert!(saved["optimizer"]["solver"].get("strategy").is_none());
        assert!(saved["optimizer"]["weights"]
            .get("cg_penalty_scale")
            .is_none());
    }

    #[test]
    fn the_supported_method_or_an_omitted_method_carries_no_note() {
        for (data, expected_method) in [
            (serde_json::json!({}), "differential_evolution"),
            (
                serde_json::json!({ "optimizer": { "solver": { "method": "differential_evolution" } } }),
                "differential_evolution",
            ),
        ] {
            assert_eq!(legacy_solver_method(&data), None);
            let (config, notes) = AlasConfig::from_value_with_notes(&data).unwrap();
            assert_eq!(config.optimizer.solver.method, expected_method);
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
