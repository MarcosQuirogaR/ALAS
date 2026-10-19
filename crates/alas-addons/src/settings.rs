// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Per-user add-on settings: which add-ons are enabled and which Python
//! interpreter process add-ons use.
//!
//! Script add-ons are sandboxed and start enabled. Process add-ons run
//! arbitrary code and start disabled; enabling is recorded against
//! `<folder>@<version>`, so a changed version has to be enabled again.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AddonError;
use crate::manifest::AddonKind;

/// File name of the settings store inside the add-ons folder.
pub const SETTINGS_FILE: &str = "addon-settings.json";

/// Persisted add-on settings.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AddonSettings {
    /// `<folder>@<version>` to an explicit enabled/disabled choice.
    #[serde(default)]
    pub enabled: BTreeMap<String, bool>,
    /// Python interpreter for `.py` process add-ons.
    #[serde(default)]
    pub python: Option<String>,
}

impl AddonSettings {
    fn key(id: &str, version: &str) -> String {
        format!("{id}@{version}")
    }

    /// Whether the add-on may run now.
    pub fn is_enabled(&self, id: &str, version: &str, kind: AddonKind) -> bool {
        self.enabled
            .get(&Self::key(id, version))
            .copied()
            .unwrap_or(kind == AddonKind::Script)
    }

    /// Record the user's choice.
    pub fn set_enabled(&mut self, id: &str, version: &str, enabled: bool) {
        self.enabled.insert(Self::key(id, version), enabled);
    }

    /// The interpreter path, if configured and non-empty.
    pub fn python_path(&self) -> Option<PathBuf> {
        self.python
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(PathBuf::from)
    }

    /// Load from `<addons_dir>/addon-settings.json`; a missing file is the
    /// default, a damaged one is an error so it is not silently overwritten.
    pub fn load(addons_dir: &Path) -> Result<Self, AddonError> {
        let path = addons_dir.join(SETTINGS_FILE);
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|error| AddonError::Io(format!("{}: {error}", path.display()))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(AddonError::Io(format!("{}: {error}", path.display()))),
        }
    }

    /// Save to `<addons_dir>/addon-settings.json`.
    pub fn save(&self, addons_dir: &Path) -> Result<(), AddonError> {
        std::fs::create_dir_all(addons_dir)
            .map_err(|error| AddonError::Io(format!("{}: {error}", addons_dir.display())))?;
        let path = addons_dir.join(SETTINGS_FILE);
        let text = serde_json::to_string_pretty(self)
            .map_err(|error| AddonError::Io(error.to_string()))?;
        std::fs::write(&path, text)
            .map_err(|error| AddonError::Io(format!("{}: {error}", path.display())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_default_on_processes_default_off_and_version_changes_reset() {
        let mut settings = AddonSettings::default();
        assert!(settings.is_enabled("a", "1", AddonKind::Script));
        assert!(!settings.is_enabled("a", "1", AddonKind::Process));
        settings.set_enabled("a", "1", true);
        assert!(settings.is_enabled("a", "1", AddonKind::Process));
        assert!(!settings.is_enabled("a", "2", AddonKind::Process));
        settings.set_enabled("b", "1", false);
        assert!(!settings.is_enabled("b", "1", AddonKind::Script));
    }

    #[test]
    fn settings_round_trip_and_a_damaged_file_is_an_error() {
        let dir = std::env::temp_dir().join(format!("alas-addon-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(AddonSettings::load(&dir).unwrap(), AddonSettings::default());
        let mut settings = AddonSettings::default();
        settings.set_enabled("a", "1", true);
        settings.python = Some("py".into());
        settings.save(&dir).unwrap();
        assert_eq!(AddonSettings::load(&dir).unwrap(), settings);
        std::fs::write(dir.join(SETTINGS_FILE), "{ nope").unwrap();
        assert!(AddonSettings::load(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
