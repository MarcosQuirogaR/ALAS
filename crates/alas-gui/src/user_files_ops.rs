// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `AppState` operations on the saved-aircraft folder (save, load, import,
//! rename, delete) with their log and status messages.

use std::path::Path;

use serde_json::Value;

use crate::state::{AppState, LogKind};
use crate::user_files::{
    delete_aircraft, import_aircraft, list_aircraft, now_unix, read_aircraft, rename_aircraft,
    save_aircraft, UserAircraft,
};
use crate::views::tr_fields;

impl AppState {
    /// Re-read the saved-aircraft folder.
    pub fn refresh_user_aircraft(&mut self) {
        self.files.aircraft = list_aircraft(&self.data_dir());
        self.files.aircraft_loaded = true;
    }

    /// Saved aircraft, read from disk on first use.
    pub fn user_aircraft(&mut self) -> &[UserAircraft] {
        if !self.files.aircraft_loaded {
            self.refresh_user_aircraft();
        }
        &self.files.aircraft
    }

    /// A starting name for the save window: the aircraft and its route.
    pub fn default_user_aircraft_name(&self) -> String {
        let aircraft = self
            .preset_names
            .iter()
            .find(|(name, _)| *name == self.active_preset)
            .map(|(_, display)| display.clone())
            .or_else(|| self.files.active_aircraft.clone())
            .unwrap_or_else(|| "My aircraft".to_owned());
        let route = |key: &str| {
            self.config_values
                .get(key)
                .and_then(Value::as_str)
                .and_then(|name| alas_config::airports::get(name).ok())
                .map(|airport| airport.icao.clone())
                .filter(|icao| !icao.is_empty())
        };
        match (route("departure_airport"), route("arrival_airport")) {
            (Some(from), Some(to)) => format!("{aircraft} {from}-{to}"),
            _ => aircraft,
        }
    }

    /// Save the current aircraft + mission configuration as `name`.
    pub fn save_current_as_user_aircraft(&mut self, name: &str) -> bool {
        let workspace = self.workspace_document();
        let base = self.active_preset.clone();
        match save_aircraft(&self.data_dir(), name, &base, &workspace, now_unix()) {
            Ok(path) => {
                self.refresh_user_aircraft();
                self.files.active_aircraft = Some(name.trim().to_owned());
                let message = tr_fields(
                    "Saved aircraft \"{name}\" to {path}.",
                    &[
                        ("name", name.trim().to_owned()),
                        ("path", path.display().to_string()),
                    ],
                );
                self.files.status = Some(message.clone());
                self.log(message, LogKind::Info);
                true
            }
            Err(error) => {
                self.files.status = Some(error.clone());
                self.log(
                    tr_fields("Aircraft not saved: {error}", &[("error", error)]),
                    LogKind::Warn,
                );
                false
            }
        }
    }

    /// Make a saved aircraft the active case. This machine's tool locations
    /// are kept; everything else comes from the file.
    pub fn load_user_aircraft(&mut self, path: &Path) {
        let (header, workspace) = match read_aircraft(path) {
            Ok(parts) => parts,
            Err(error) => {
                self.files.status = Some(error.clone());
                self.log(
                    tr_fields("Aircraft not loaded: {error}", &[("error", error)]),
                    LogKind::Error,
                );
                return;
            }
        };
        let previous = self.typed_config();
        if let Err(error) = self.apply_workspace_document(&workspace) {
            self.files.status = Some(error.clone());
            self.log(
                tr_fields("Aircraft not loaded: {error}", &[("error", error)]),
                LogKind::Error,
            );
            return;
        }
        if let (Some(previous), Some(mut config)) = (previous, self.typed_config()) {
            crate::config_edit::preserve_machine_locations(&mut config, &previous);
            self.config_values = crate::config_edit::full_config_values(&config);
            self.on_config_modified();
        }
        self.files.active_aircraft = Some(header.name.clone());
        self.log(
            tr_fields("Loaded aircraft: {name}", &[("name", header.name)]),
            LogKind::Info,
        );
    }

    /// Import an aircraft file into the saved-aircraft folder.
    pub fn import_user_aircraft(&mut self, source: &Path) {
        match import_aircraft(&self.data_dir(), source) {
            Ok(_) => {
                self.refresh_user_aircraft();
                let message = tr_fields(
                    "Imported aircraft from {path}.",
                    &[("path", source.display().to_string())],
                );
                self.files.status = Some(message.clone());
                self.log(message, LogKind::Info);
            }
            Err(error) => {
                self.files.status = Some(error.clone());
                self.log(
                    tr_fields("Aircraft import failed: {error}", &[("error", error)]),
                    LogKind::Error,
                );
            }
        }
    }

    /// Rename a saved aircraft.
    pub fn rename_user_aircraft(&mut self, path: &Path, new_name: &str) -> bool {
        match rename_aircraft(&self.data_dir(), path, new_name) {
            Ok(_) => {
                self.refresh_user_aircraft();
                self.files.status = Some(tr_fields(
                    "Renamed aircraft to \"{name}\".",
                    &[("name", new_name.trim().to_owned())],
                ));
                true
            }
            Err(error) => {
                self.files.status = Some(error);
                false
            }
        }
    }

    /// Delete a saved aircraft.
    pub fn delete_user_aircraft(&mut self, path: &Path) {
        match delete_aircraft(path) {
            Ok(()) => {
                self.refresh_user_aircraft();
                self.files.status = Some(tr_fields(
                    "Deleted aircraft file {path}.",
                    &[("path", path.display().to_string())],
                ));
            }
            Err(error) => self.files.status = Some(error),
        }
    }
}
