// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Persistent and environment state for the standalone Airfoil CFD study.

use super::super::*;
use alas_cfd::CfdStudyConfig;
use alas_exec::openfoam::OpenFoamPreferences;
use alas_exec::ToolLocator;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};
/// Extra settings persisted beside the study contract.
///
/// OpenFOAM preferences belong to the execution environment rather than to
/// an airfoil.  The legacy top-level Gmsh key is retained for one-way
/// migration from older GUI settings while the typed OpenFOAM preference is
/// now the source consumed by the worker.  The wrapper is deliberately
/// versionless and `serde(default)` keeps older files readable.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(super) struct CfdEnvironmentPreferences {
    pub(super) openfoam: OpenFoamPreferences,
    pub(super) gmsh_executable: Option<String>,
    pub(super) paraview_executable: Option<String>,
}

impl AirfoilCfdState {
    /// Construct state and restore the user's last study/environment settings.
    pub fn new(locator: &ToolLocator) -> Self {
        let environment_path = environment_preferences_path(locator);
        let environment = read_environment_preferences(&environment_path);
        let mut openfoam_preferences = environment.openfoam;
        // Migrate the first GUI version's top-level Gmsh path into the typed
        // OpenFOAM preference consumed by the worker.  Keeping this migration
        // here means an existing user's configured executable is never silently
        // ignored after the execution crate gained its canonical field.
        if openfoam_preferences.gmsh_executable.is_none() {
            openfoam_preferences.gmsh_executable = environment.gmsh_executable;
        }
        let saved_study_path = study_preferences_path(locator);
        let saved_sweep_path = sweep_preferences_path(locator);
        let mut config = read_study_config(&saved_study_path).unwrap_or_default();
        let sweep_settings = read_sweep_settings(&saved_sweep_path);
        // Resolve the section after loading so a saved name always gets an
        // immediate outline.  The CFD runner still performs its exact lookup
        // and reports a failure for an invalid/stale name; the preview never
        // substitutes a different section.
        if config.airfoil_name.trim().is_empty() {
            config.airfoil_name = CfdStudyConfig::default().airfoil_name;
        }
        let mut state = Self {
            config,
            case_root: locator.resolve_data_path(Path::new("outputs/airfoil-cfd")),
            saved_study_path,
            saved_sweep_path,
            airfoil_filter: String::new(),
            filtered_airfoils: Vec::new(),
            preview_coordinates: None,
            window_open: false,
            tab: CfdTab::Study,
            running: false,
            probing: false,
            status: "OpenFOAM connection has not been checked.".to_owned(),
            stage: None,
            events: Vec::new(),
            result: None,
            sweep_settings,
            sweep_results: Vec::new(),
            sweep_running: false,
            error: None,
            capabilities: None,
            gmsh_executable: openfoam_preferences.gmsh_executable.clone(),
            openfoam_preferences,
            paraview_executable: environment.paraview_executable,
            last_case_dir: None,
            selected_field: None,
            contour_textures: BTreeMap::new(),
            result_json_path: String::new(),
            input_revision: 0,
            run_id: 0,
            run_input_revision: 0,
            rx: None,
            probe_rx: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        };
        state.refresh_airfoil_filter();
        state.refresh_preview();
        state
    }
}

impl Default for AirfoilCfdState {
    fn default() -> Self {
        let mut state = Self {
            config: CfdStudyConfig::default(),
            case_root: PathBuf::from("outputs/airfoil-cfd"),
            saved_study_path: PathBuf::from("airfoil-cfd-study.json"),
            saved_sweep_path: PathBuf::from("airfoil-cfd-sweep.json"),
            airfoil_filter: String::new(),
            filtered_airfoils: Vec::new(),
            preview_coordinates: None,
            window_open: false,
            tab: CfdTab::Study,
            running: false,
            probing: false,
            status: "OpenFOAM connection has not been checked.".to_owned(),
            stage: None,
            events: Vec::new(),
            result: None,
            sweep_settings: CfdSweepSettings::default(),
            sweep_results: Vec::new(),
            sweep_running: false,
            error: None,
            capabilities: None,
            openfoam_preferences: OpenFoamPreferences::default(),
            gmsh_executable: None,
            paraview_executable: None,
            last_case_dir: None,
            selected_field: None,
            contour_textures: BTreeMap::new(),
            result_json_path: String::new(),
            input_revision: 0,
            run_id: 0,
            run_input_revision: 0,
            rx: None,
            probe_rx: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        };
        state.refresh_airfoil_filter();
        state.refresh_preview();
        state
    }
}
impl AirfoilCfdState {
    /// Save the effective study contract to the user's settings directory.
    pub fn save_study(&self) -> Result<PathBuf, String> {
        if let Some(parent) = self
            .saved_study_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
        if let Some(sweep_parent) = self
            .saved_sweep_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            std::fs::create_dir_all(sweep_parent)
                .map_err(|error| format!("cannot create {}: {error}", sweep_parent.display()))?;
        }
        let text = serde_json::to_string_pretty(&self.config)
            .map_err(|error| format!("cannot encode CFD study settings: {error}"))?;
        std::fs::write(&self.saved_study_path, text).map_err(|error| {
            format!(
                "cannot write CFD study settings {}: {error}",
                self.saved_study_path.display()
            )
        })?;
        let sweep_text = serde_json::to_string_pretty(&self.sweep_settings)
            .map_err(|error| format!("cannot encode CFD sweep settings: {error}"))?;
        std::fs::write(&self.saved_sweep_path, sweep_text).map_err(|error| {
            format!(
                "cannot write CFD sweep settings {}: {error}",
                self.saved_sweep_path.display()
            )
        })?;
        Ok(self.saved_study_path.clone())
    }

    /// Reload the saved study contract and invalidate dependent outputs.
    pub fn reload_study(&mut self) -> Result<PathBuf, String> {
        let config = read_study_config(&self.saved_study_path).ok_or_else(|| {
            format!(
                "No saved CFD study settings were found at {}.",
                self.saved_study_path.display()
            )
        })?;
        self.config = config;
        if let Some(sweep_settings) = read_optional_sweep_settings(&self.saved_sweep_path) {
            self.sweep_settings = sweep_settings;
        }
        self.refresh_preview();
        self.mark_inputs_changed();
        Ok(self.saved_study_path.clone())
    }

    /// Save OpenFOAM/Gmsh environment settings in the user's preferences tree.
    pub fn save_environment_preferences(&self, locator: &ToolLocator) -> Result<PathBuf, String> {
        let path = environment_preferences_path(locator);
        let parent = path.parent().ok_or_else(|| {
            "The CFD environment settings path has no parent directory.".to_owned()
        })?;
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        let preferences = CfdEnvironmentPreferences {
            openfoam: self.openfoam_preferences.clone(),
            // Retain the legacy top-level key for one-way migration while the
            // typed preference remains the source used by OpenFoamAdapter.
            gmsh_executable: self.openfoam_preferences.gmsh_executable.clone(),
            paraview_executable: self.paraview_executable.clone(),
        };
        let text = serde_json::to_string_pretty(&preferences)
            .map_err(|error| format!("cannot encode CFD environment settings: {error}"))?;
        std::fs::write(&path, text)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
        Ok(path)
    }
}

/// Path for the persisted study contract.
pub fn study_preferences_path(locator: &ToolLocator) -> PathBuf {
    locator
        .preferences_path()
        .with_file_name("airfoil-cfd-study.json")
}

/// Path for the persisted AoA/Reynolds sweep definition.
pub fn sweep_preferences_path(locator: &ToolLocator) -> PathBuf {
    locator
        .preferences_path()
        .with_file_name("airfoil-cfd-sweep.json")
}

/// Path for persisted OpenFOAM/Gmsh environment settings.
pub fn environment_preferences_path(locator: &ToolLocator) -> PathBuf {
    locator
        .preferences_path()
        .with_file_name("airfoil-cfd-environment.json")
}

fn read_study_config(path: &Path) -> Option<CfdStudyConfig> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn read_sweep_settings(path: &Path) -> CfdSweepSettings {
    read_optional_sweep_settings(path).unwrap_or_default()
}

fn read_optional_sweep_settings(path: &Path) -> Option<CfdSweepSettings> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn read_environment_preferences(path: &Path) -> CfdEnvironmentPreferences {
    let Ok(text) = std::fs::read_to_string(path) else {
        return CfdEnvironmentPreferences::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// Allocate an empty case directory with an atomic filesystem operation.
///
/// A session-local `run_id` is useful for correlating worker messages, but it
/// is not a durable case identity: it starts again at zero after an
/// application restart.  Combining a UTC epoch timestamp, process id and
/// atomic counter keeps every launch in its own directory.  `create_dir`
/// (rather than `create_dir_all`) is the final collision check, so an existing
/// case can never be silently reused by a new study.
pub(super) fn allocate_case_directory(root: &Path, run_id: u64) -> Result<PathBuf, String> {
    std::fs::create_dir_all(root)
        .map_err(|error| format!("cannot create CFD case root {}: {error}", root.display()))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?
        .as_nanos();
    let process_id = std::process::id();
    for _ in 0..128 {
        let counter = NEXT_CASE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = root.join(format!(
            "run-{timestamp}-p{process_id}-c{counter}-r{run_id}"
        ));
        match std::fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "cannot allocate isolated CFD case {}: {error}",
                    candidate.display()
                ));
            }
        }
    }
    Err(format!(
        "could not allocate an unused CFD case directory under {}",
        root.display()
    ))
}
