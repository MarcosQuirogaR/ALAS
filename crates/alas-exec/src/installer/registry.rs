// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Record of what the installer put on disk, in `installed-tools.json`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::InstallError;

const FILE_NAME: &str = "installed-tools.json";

/// One installed tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledRecord {
    /// Version installed.
    pub version: String,
    /// Install directory relative to the user data root.
    pub directory: String,
    /// Executable relative to the install directory.
    pub executable: String,
    /// SHA-256 of the archive that was installed.
    pub archive_sha256: String,
    /// Source URL.
    pub url: String,
    /// Seconds since the Unix epoch.
    pub installed_at: u64,
    /// Whether the health check passed when the record was written.
    pub healthy: bool,
}

/// Installed tools by id.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registry {
    /// Records by tool id.
    #[serde(default)]
    pub tools: BTreeMap<String, InstalledRecord>,
}

impl Registry {
    /// Load from the user data root; a missing or unreadable file is empty.
    pub fn load(data_root: &Path) -> Self {
        fs::read_to_string(data_root.join(FILE_NAME))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Write atomically (temporary file, then rename).
    pub fn save(&self, data_root: &Path) -> Result<(), InstallError> {
        fs::create_dir_all(data_root).map_err(|error| {
            InstallError::Io(format!("cannot create {}: {error}", data_root.display()))
        })?;
        let text = serde_json::to_string_pretty(self)
            .map_err(|error| InstallError::Io(error.to_string()))?;
        let temporary = data_root.join(format!(".{FILE_NAME}.tmp"));
        fs::write(&temporary, text)
            .and_then(|()| fs::rename(&temporary, data_root.join(FILE_NAME)))
            .map_err(|error| InstallError::Io(format!("cannot write {FILE_NAME}: {error}")))
    }

    /// Absolute executable path of an installed tool, if it still exists.
    pub fn executable(&self, data_root: &Path, id: &str) -> Option<PathBuf> {
        let record = self.tools.get(id)?;
        let path = data_root.join(&record.directory).join(&record.executable);
        path.is_file().then_some(path)
    }
}
