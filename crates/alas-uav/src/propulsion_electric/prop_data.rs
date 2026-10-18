// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The user's local propeller performance data directory.
//!
//! ALAS ships no propeller performance tables.  Tables are plain `PER3_*.dat`
//! files kept in one directory, resolved in this order:
//!
//! 1. [`set_prop_data_dir`], when a host application has configured one;
//! 2. the `ALAS_PROP_DATA_DIR` environment variable;
//! 3. the per-user data directory: `%LOCALAPPDATA%\ALAS\prop-data\apc` on
//!    Windows, `~/Library/Application Support/ALAS/prop-data/apc` on macOS,
//!    and `$XDG_DATA_HOME` (or `~/.local/share`)`/ALAS/prop-data/apc` elsewhere.
//!
//! The directory is read once per process and directory; call
//! [`reload_prop_data`] after files are added.  A missing or empty directory is
//! not an error of its own: lookups report
//! `ElectricPropulsionError::PropellerDataUnavailable` with the reason and the
//! way to obtain the data.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError, RwLock};

use super::fixture::PropellerPerformanceMap;
use super::per3::parse_per3;
use crate::catalog::{Catalog, CatalogError};

/// Environment variable naming the propeller data directory.
pub const PROP_DATA_DIR_ENV: &str = "ALAS_PROP_DATA_DIR";

static OVERRIDE: RwLock<Option<PathBuf>> = RwLock::new(None);
static CACHE: Mutex<Option<&'static LoadedTables>> = Mutex::new(None);

/// Tables read from one directory, with the files that were rejected.
#[derive(Debug)]
pub struct LoadedTables {
    pub(crate) dir: Option<PathBuf>,
    pub(crate) tables: Vec<PropellerPerformanceMap>,
    pub(crate) skipped: Vec<(String, String)>,
    pub(crate) catalog: OnceLock<Result<Catalog, CatalogError>>,
}

impl LoadedTables {
    /// Usable tables, sorted by identifier.
    pub fn tables(&self) -> &[PropellerPerformanceMap] {
        &self.tables
    }

    /// Files that were present but rejected, with the reason.
    pub fn skipped(&self) -> &[(String, String)] {
        &self.skipped
    }
}

/// What is installed, for status lines in an application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropDataStatus {
    /// Directory that was read, when one could be determined.
    pub directory: Option<PathBuf>,
    /// Number of usable tables.
    pub table_count: usize,
    /// Files that were present but rejected, with the reason.
    pub skipped: Vec<(String, String)>,
}

/// Configure the data directory for this process (`None` clears it).
pub fn set_prop_data_dir(dir: Option<PathBuf>) {
    *OVERRIDE.write().unwrap_or_else(PoisonError::into_inner) = dir;
}

/// The directory that will be read: override, environment, then default.
pub fn prop_data_dir() -> Option<PathBuf> {
    let configured = OVERRIDE
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if configured.is_some() {
        return configured;
    }
    if let Some(value) = std::env::var_os(PROP_DATA_DIR_ENV).filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(value));
    }
    default_prop_data_dir()
}

/// The per-user default directory, without consulting the override or env.
pub fn default_prop_data_dir() -> Option<PathBuf> {
    let env_path = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty());
    let base = if cfg!(windows) {
        PathBuf::from(env_path("LOCALAPPDATA")?)
    } else if cfg!(target_os = "macos") {
        PathBuf::from(env_path("HOME")?).join("Library/Application Support")
    } else if let Some(xdg) = env_path("XDG_DATA_HOME") {
        PathBuf::from(xdg)
    } else {
        PathBuf::from(env_path("HOME")?).join(".local/share")
    };
    Some(base.join("ALAS").join("prop-data").join("apc"))
}

/// Forget the cached tables so the next lookup re-reads the directory.
pub fn reload_prop_data() {
    *CACHE.lock().unwrap_or_else(PoisonError::into_inner) = None;
}

/// Summary of the installed tables.
pub fn prop_data_status() -> PropDataStatus {
    let loaded = loaded_prop_tables();
    PropDataStatus {
        directory: loaded.dir.clone(),
        table_count: loaded.tables.len(),
        skipped: loaded.skipped.clone(),
    }
}

/// The reason propeller maps are unavailable, or `None` when tables are installed.
pub fn prop_data_unavailable_reason() -> Option<String> {
    let loaded = loaded_prop_tables();
    loaded.tables.is_empty().then(|| unavailable_reason(loaded))
}

pub(super) fn unavailable_reason(loaded: &LoadedTables) -> String {
    let how = "ALAS does not distribute propeller performance tables; install them with \
               `cargo run -p alas-uav --release --example fetch_props` or import your own PER3 \
               files (see docs/propeller-data.md)";
    match &loaded.dir {
        None => format!(
            "no propeller data directory could be determined (set {PROP_DATA_DIR_ENV}); {how}"
        ),
        Some(dir) if loaded.skipped.is_empty() => {
            format!("no PER3 tables were found in {}; {how}", dir.display())
        }
        Some(dir) => format!(
            "none of the {} PER3 files in {} could be read (first problem: {}); {how}",
            loaded.skipped.len(),
            dir.display(),
            loaded.skipped[0].1
        ),
    }
}

/// Tables of the current data directory, read on first use.
pub(crate) fn loaded_prop_tables() -> &'static LoadedTables {
    let dir = prop_data_dir();
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(existing) = *cache {
        if existing.dir == dir {
            return existing;
        }
    }
    // A superseded snapshot stays allocated: references to it may be live.
    let fresh: &'static LoadedTables = Box::leak(Box::new(read_directory(dir)));
    *cache = Some(fresh);
    fresh
}

/// True for `PER3_*.dat` (case-insensitive) file names.
pub(super) fn is_per3_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("per3_") && lower.ends_with(".dat") && is_safe_name(name)
}

/// File names are restricted so imports can never escape the directory.
pub(super) fn is_safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 96
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'(' | b')'))
        && !name.starts_with('.')
}

/// Read and parse every PER3 file of a directory.
pub fn read_directory(dir: Option<PathBuf>) -> LoadedTables {
    let mut tables: Vec<PropellerPerformanceMap> = Vec::new();
    let mut skipped = Vec::new();
    if let Some(path) = &dir {
        let mut names: Vec<String> = std::fs::read_dir(path)
            .map(|entries| {
                entries
                    .flatten()
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .filter(|name| is_per3_name(name))
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        for name in names {
            match std::fs::read(path.join(&name))
                .map_err(|error| error.to_string())
                .and_then(|bytes| {
                    parse_per3(&String::from_utf8_lossy(&bytes), &name)
                        .map_err(|error| error.to_string())
                }) {
                Ok(table)
                    if tables
                        .iter()
                        .any(|t| t.propeller_id() == table.propeller_id()) =>
                {
                    skipped.push((name, "duplicate propeller identifier".to_owned()));
                }
                Ok(table) => tables.push(table),
                Err(reason) => skipped.push((name, reason)),
            }
        }
    }
    tables.sort_by(|a, b| a.propeller_id().cmp(b.propeller_id()));
    LoadedTables {
        dir,
        tables,
        skipped,
        catalog: OnceLock::new(),
    }
}

/// Outcome of an import into the data directory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// File names written to the data directory.
    pub imported: Vec<String>,
    /// Source entries that were ignored or rejected, with the reason.
    pub rejected: Vec<(String, String)>,
}

/// Why an import could not start.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// A file-system operation failed.
    #[error("{0}")]
    Io(String),
    /// The archive is not a readable zip.
    #[error("{0}")]
    Archive(String),
}

/// Validate one PER3 file and store it, atomically, in `dest`.
///
/// The name must be a plain `PER3_*.dat` file name and the contents must parse
/// as a performance table; anything else is rejected without touching `dest`.
pub fn store_per3_file(dest: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    if !is_per3_name(name) {
        return Err("not a PER3_*.dat file name".to_owned());
    }
    let text = String::from_utf8_lossy(bytes);
    parse_per3(&text, name).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(dest)
        .map_err(|error| format!("cannot create {}: {error}", dest.display()))?;
    let partial = dest.join(format!("{name}.part"));
    std::fs::write(&partial, bytes).map_err(|error| error.to_string())?;
    std::fs::rename(&partial, dest.join(name)).map_err(|error| {
        let _ = std::fs::remove_file(&partial);
        error.to_string()
    })
}

/// Import every `PER3_*.dat` file of a folder (not recursive) into `dest`.
pub fn import_prop_folder(source: &Path, dest: &Path) -> Result<ImportReport, ImportError> {
    let entries = std::fs::read_dir(source)
        .map_err(|error| ImportError::Io(format!("cannot read {}: {error}", source.display())))?;
    let mut report = ImportReport::default();
    let mut files: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| Some((entry.file_name().into_string().ok()?, entry.path())))
        .filter(|(name, path)| is_per3_name(name) && path.is_file())
        .collect();
    files.sort();
    for (name, path) in files {
        let outcome = std::fs::read(&path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| store_per3_file(dest, &name, &bytes));
        match outcome {
            Ok(()) => report.imported.push(name),
            Err(reason) => report.rejected.push((name, reason)),
        }
    }
    reload_prop_data();
    Ok(report)
}

/// Import every `PER3_*.dat` entry of a zip archive into `dest`.
///
/// Directory components of entry names are ignored, and each entry must pass
/// [`store_per3_file`], so a crafted archive cannot write outside `dest`.
pub fn import_prop_zip(archive: &Path, dest: &Path) -> Result<ImportReport, ImportError> {
    let bytes = std::fs::read(archive)
        .map_err(|error| ImportError::Io(format!("cannot read {}: {error}", archive.display())))?;
    let mut report = ImportReport::default();
    for entry in super::prop_zip::read_entries(&bytes).map_err(ImportError::Archive)? {
        let name = entry
            .name
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or_default()
            .to_owned();
        if !is_per3_name(&name) {
            continue;
        }
        match entry
            .data
            .and_then(|data| store_per3_file(dest, &name, &data))
        {
            Ok(()) => report.imported.push(name),
            Err(reason) => report.rejected.push((name, reason)),
        }
    }
    reload_prop_data();
    Ok(report)
}

/// Import a folder or a zip file, chosen by what `source` is.
pub fn import_prop_data(source: &Path, dest: &Path) -> Result<ImportReport, ImportError> {
    if source.is_dir() {
        import_prop_folder(source, dest)
    } else if source
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
    {
        import_prop_zip(source, dest)
    } else {
        Err(ImportError::Io(format!(
            "{} is neither a folder nor a .zip file",
            source.display()
        )))
    }
}
