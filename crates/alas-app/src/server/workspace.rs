// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The folder the server reads and writes in, and configuration files in it.

use std::fs;
use std::path::{Component, Path, PathBuf};

use alas_config::AlasConfig;
use alas_exec::ToolLocator;
use serde_json::Value;

/// Default workspace folder name under the per-user data folder.
pub const DEFAULT_WORKSPACE_DIR: &str = "automation";

/// Sub-folder of the workspace that holds run outputs.
pub const RUNS_DIR: &str = "runs";

/// The server's folder. Every path a tool takes is relative to it.
#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
    locator: ToolLocator,
}

impl Workspace {
    /// `root`, or `<user data>/automation` when `None`, with `locator`'s tool
    /// preferences and custom-airport store.
    pub fn for_locator(locator: &ToolLocator, root: Option<PathBuf>) -> Self {
        let root = root.unwrap_or_else(|| locator.user_data_root().join(DEFAULT_WORKSPACE_DIR));
        Self {
            root,
            locator: locator.clone(),
        }
    }

    /// The workspace folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The locator runs resolve external tools with.
    pub fn locator(&self) -> &ToolLocator {
        &self.locator
    }

    /// The persisted custom-airport store (shared with the desktop
    /// application, outside the workspace).
    pub fn airport_store(&self) -> PathBuf {
        self.locator.custom_airports_path()
    }

    /// Resolve `relative` inside the workspace.
    ///
    /// # Errors
    ///
    /// The path is empty, absolute, or has a `..`, `.` or drive component; or
    /// an existing part of it leads outside the workspace (a link).
    pub fn resolve(&self, relative: &str) -> Result<PathBuf, String> {
        let path = Path::new(relative);
        let plain = !relative.trim().is_empty()
            && path
                .components()
                .all(|component| matches!(component, Component::Normal(_)));
        if !plain {
            return Err(format!(
                "path {relative:?} must be relative to the workspace, without '..'"
            ));
        }
        let joined = self.root.join(path);
        // An existing ancestor that resolves elsewhere is a link out.
        if let Some(existing) = joined.ancestors().find(|ancestor| ancestor.exists()) {
            let root = fs::canonicalize(&self.root).unwrap_or_else(|_| self.root.clone());
            let real = fs::canonicalize(existing).map_err(|error| error.to_string())?;
            if existing.starts_with(&self.root) && !real.starts_with(&root) {
                return Err(format!("path {relative:?} leads outside the workspace"));
            }
        }
        Ok(joined)
    }

    /// The path of `absolute` relative to the workspace, with `/` separators.
    /// Callers pass paths from [`Self::resolve`]; any other path is never
    /// shown, so a reply cannot pass an outside path off as a workspace one.
    pub fn relative(&self, absolute: &Path) -> String {
        let Ok(relative) = absolute.strip_prefix(&self.root) else {
            return "(outside the workspace)".to_owned();
        };
        relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
    }
}

/// Whether `path` names a configuration file: `.json`, `.yaml` or `.yml`.
pub fn config_extension_ok(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("json" | "yaml" | "yml")
    )
}

/// Read a configuration document.
///
/// # Errors
///
/// The file is missing or does not parse.
pub fn read_document(path: &Path) -> Result<Value, String> {
    crate::cli::read_config_document(path)
}

/// Write a configuration document, JSON or YAML by the file's extension,
/// creating its folder. The file is replaced through a temporary sibling, so
/// a failed write leaves the previous one intact.
///
/// # Errors
///
/// The extension is not a configuration one, or the file cannot be written.
pub fn write_document(path: &Path, document: &Value) -> Result<(), String> {
    if !config_extension_ok(path) {
        return Err(format!(
            "{} must end in .json, .yaml or .yml",
            path.display()
        ));
    }
    let text = if path.extension().and_then(|e| e.to_str()) == Some("json") {
        serde_json::to_string_pretty(document).map_err(|e| e.to_string())? + "\n"
    } else {
        serde_yaml::to_string(document).map_err(|e| e.to_string())?
    };
    write_text(path, &text)
}

/// Write `text` to `path` through a temporary sibling, creating the folder.
///
/// # Errors
///
/// The folder or the file cannot be written.
pub fn write_text(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    fs::write(&temporary, text)
        .map_err(|error| format!("cannot write {}: {error}", temporary.display()))?;
    fs::rename(&temporary, path)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

/// Load a document through the configuration schema, as the command line
/// does, with the load notes (migrations) it produced.
///
/// # Errors
///
/// The document does not match the schema.
pub fn load(document: &Value) -> Result<(AlasConfig, Vec<String>), String> {
    let (config, notes) = alas_mission::preset_load::load_document(document)
        .map_err(|error| format!("invalid config structure: {error}"))?;
    Ok((config, notes.messages()))
}

/// The configuration fields that name a program or a folder: tool
/// installations and the navigation data, route and texture folders.
fn locations(config: &AlasConfig) -> [(&'static str, Option<&str>); 11] {
    let structures = &config.structures;
    let mission = &config.mission;
    [
        ("mses.mses_dir", Some(&config.mses.mses_dir)),
        ("mses.osmap_path", config.mses.osmap_path.as_ref()),
        (
            "structures.nastran_exe_path",
            Some(&structures.nastran_exe_path),
        ),
        (
            "structures.nastran_solver_path",
            Some(&structures.nastran_solver_path),
        ),
        (
            "structures.nastran95_dir_path",
            Some(&structures.nastran95_dir_path),
        ),
        (
            "structures.nastran95_runtime_path",
            Some(&structures.nastran95_runtime_path),
        ),
        (
            "structures.nastran95_rf_stage_path",
            Some(&structures.nastran95_rf_stage_path),
        ),
        (
            "structures.patran_exe_path",
            Some(&structures.patran_exe_path),
        ),
        ("mission.navdata_dir", Some(&mission.navdata_dir)),
        ("mission.routes_dir", Some(&mission.routes_dir)),
        ("mission.texture_path", Some(&mission.texture_path)),
    ]
    .map(|(field, value)| (field, value.map(String::as_str)))
}

/// Refuse a configuration that sets a program or folder location.
///
/// A configuration is data the server's client writes; a location in it would
/// let a run start any program or read any folder. The server leaves every
/// such field at its default and takes tool and data locations from the user's
/// tool preferences, as the command line does without a configuration file.
///
/// # Errors
///
/// A location field differs from its default; the message names each one.
pub fn check_locations(config: &AlasConfig) -> Result<(), String> {
    let defaults = AlasConfig::default();
    let changed: Vec<&str> = locations(config)
        .into_iter()
        .zip(locations(&defaults))
        .filter(|((_, value), (_, default))| value != default)
        .map(|((field, _), _)| field)
        .collect();
    if changed.is_empty() {
        return Ok(());
    }
    Err(format!(
        "the server takes program and data locations from the tool preferences, never from a \
         configuration; remove {} (set it to null with edit_config)",
        changed.join(", ")
    ))
}

/// RFC 7386 JSON merge patch: objects merge key by key, `null` removes a
/// key, anything else (arrays included) replaces the target.
pub fn merge_patch(target: &mut Value, patch: &Value) {
    let Value::Object(patch) = patch else {
        *target = patch.clone();
        return;
    };
    if !target.is_object() {
        *target = Value::Object(serde_json::Map::new());
    }
    if let Value::Object(target) = target {
        for (key, value) in patch {
            if value.is_null() {
                target.remove(key);
            } else {
                merge_patch(target.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
    }
}
