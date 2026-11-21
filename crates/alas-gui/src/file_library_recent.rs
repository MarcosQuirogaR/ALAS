// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Recently used files and the autosave slots of the file library.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::file_library::{
    read_document, write_document, ObjectHeader, ObjectKind, FORMAT_VERSION,
};

/// Recently used files kept.
pub const RECENT_LIMIT: usize = 12;

/// One recently used file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecentFile {
    /// The [`ObjectKind::id`].
    pub kind: String,
    /// The path used.
    pub path: PathBuf,
    /// When, seconds since the Unix epoch (UTC).
    pub used_unix: u64,
}

fn recent_path(data_dir: &Path) -> PathBuf {
    data_dir.join("recent-files.json")
}

/// The recently used files, newest first, without files that no longer exist.
pub fn recent(data_dir: &Path) -> Vec<RecentFile> {
    let Ok(text) = fs::read_to_string(recent_path(data_dir)) else {
        return Vec::new();
    };
    let mut files: Vec<RecentFile> = serde_json::from_str(&text).unwrap_or_default();
    files.retain(|file| file.path.exists());
    files
}

/// Record a file as just used.
pub fn note_recent(
    data_dir: &Path,
    kind: ObjectKind,
    path: &Path,
    used_unix: u64,
) -> Result<(), String> {
    let mut files = recent(data_dir);
    files.retain(|file| file.path != path);
    files.insert(
        0,
        RecentFile {
            kind: kind.id().to_owned(),
            path: path.to_owned(),
            used_unix,
        },
    );
    files.truncate(RECENT_LIMIT);
    let text = serde_json::to_string_pretty(&files).map_err(|error| error.to_string())?;
    crate::fs_atomic::write_atomic(&recent_path(data_dir), text)
        .map_err(|error| format!("cannot write the recent list: {error}"))
}

/// The autosave file of `kind`: one slot, overwritten by each autosave.
pub fn autosave_path(data_dir: &Path, kind: ObjectKind) -> PathBuf {
    data_dir
        .join("autosave")
        .join(format!("{}.json", kind.id()))
}

/// Write the autosave slot of `kind`.
pub fn autosave(
    data_dir: &Path,
    kind: ObjectKind,
    body: &Value,
    saved_unix: u64,
) -> Result<(), String> {
    let header = ObjectHeader {
        kind: kind.id().to_owned(),
        version: FORMAT_VERSION,
        name: "Autosave".to_owned(),
        saved_unix,
    };
    write_document(&autosave_path(data_dir, kind), &header, body)
}

/// Read the autosave slot of `kind`, when one exists.
pub fn read_autosave(data_dir: &Path, kind: ObjectKind) -> Option<(u64, Value)> {
    let path = autosave_path(data_dir, kind);
    let (header, body) = read_document(&path).ok()?;
    Some((header.saved_unix, body))
}

/// The Sandbox aircraft's crash-autosave file. It is its own slot, so a
/// Sandbox autosave never overwrites the guided case's.
pub fn sandbox_autosave_path(data_dir: &Path) -> PathBuf {
    data_dir.join("autosave").join("sandbox.json")
}

/// Write the Sandbox crash-autosave slot.
pub fn autosave_sandbox(data_dir: &Path, body: &Value, saved_unix: u64) -> Result<(), String> {
    let header = ObjectHeader {
        kind: ObjectKind::Aircraft.id().to_owned(),
        version: FORMAT_VERSION,
        name: "Sandbox autosave".to_owned(),
        saved_unix,
    };
    write_document(&sandbox_autosave_path(data_dir), &header, body)
}

/// Read the Sandbox crash-autosave slot, when one exists.
pub fn read_sandbox_autosave(data_dir: &Path) -> Option<(u64, Value)> {
    let (header, body) = read_document(&sandbox_autosave_path(data_dir)).ok()?;
    Some((header.saved_unix, body))
}
