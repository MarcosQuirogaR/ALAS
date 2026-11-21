// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Configuration-document loading for the command line, reporting what
//! loading changed about the document's meaning.

use alas_config::AlasConfig;

/// Load a parsed configuration document, logging every load note (the
/// mass-architecture migration and an ignored `mission.enabled = false`).
///
/// # Errors
///
/// The document does not match the configuration schema.
pub fn load_config_value(value: &serde_json::Value) -> Result<AlasConfig, String> {
    let (config, notes) = alas_mission::preset_load::load_document(value)
        .map_err(|error| format!("invalid config structure: {error}"))?;
    for message in notes.messages() {
        tracing::warn!("{message}");
    }
    Ok(config)
}

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_that_disables_the_mission_loads_with_the_mission_on() {
        let value = serde_json::json!({ "preset": "AVE", "mission": { "enabled": false } });
        let config = load_config_value(&value).expect("loads");
        assert!(config.mission.enabled);
        assert_eq!(config.preset, "AVE");
        assert!(load_config_value(&serde_json::json!({ "no_such_group": 1 })).is_err());
    }

    #[test]
    fn a_rejected_document_is_reported_in_words_not_as_a_debug_dump() {
        let message = load_config_value(&serde_json::json!({
            "preset": "A320-200",
            "requirements": { "mtow_kgg": 1 }
        }))
        .expect_err("misspelled key is refused");
        assert!(message.contains("mtow_kgg"), "{message}");
        assert!(!message.contains("Rejected {"), "{message}");
    }
}
