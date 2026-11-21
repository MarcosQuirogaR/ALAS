// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The partial-run report cache: one YAML entry per effective
//! configuration and design vector (the module docs of [`super`]).

use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::full_analysis::AnalysisReport;

/// The cached report and where it came from.
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct CacheEntry {
    pub(super) program_version: String,
    pub(super) built: String,
    pub(super) computed_s: f64,
    pub(super) report: AnalysisReport,
}

pub(super) fn digest(text: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub(super) fn stamp() -> String {
    std::env::current_exe()
        .and_then(std::fs::metadata)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or_else(
            || "unknown".to_owned(),
            |age| format!("exe mtime {}", age.as_secs()),
        )
}

pub(super) fn read_cached(path: &Path) -> Option<CacheEntry> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_yaml::from_str::<CacheEntry>(&text) {
        Ok(entry) => Some(entry),
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "partial-run cache entry unreadable; recomputing");
            None
        }
    }
}

pub(super) fn write_cached(path: &Path, entry: &CacheEntry) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cache folder {}: {error}", parent.display()))?;
    }
    // YAML keeps NaN and infinities, which several report fields carry, and
    // round-trips every f64 exactly.
    let text = serde_yaml::to_string(entry).map_err(|error| format!("cache encode: {error}"))?;
    let partial = path.with_extension("yaml.part");
    std::fs::write(&partial, text)
        .and_then(|()| std::fs::rename(&partial, path))
        .map_err(|error| format!("cache write {}: {error}", path.display()))
}
