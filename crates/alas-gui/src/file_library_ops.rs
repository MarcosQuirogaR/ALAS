// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `AppState` side of the file library: what each kind captures from the
//! current case and how opening a file of that kind changes the case.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::file_library::{self, LibraryEntry, ObjectKind};
use crate::state::{AppState, LogKind};
use crate::user_files::now_unix;
use crate::views::tr_fields;

/// What a library action did, for its status line.
#[derive(Clone, Copy)]
pub(crate) enum Verb {
    Saved,
    Opened,
    Imported,
}

/// Engine installation keys that say where the engines are mounted: they belong
/// to the airframe, so an engine file neither stores nor restores them.
const ENGINE_PLACEMENT_KEYS: [&str; 7] = [
    "spanwise_positions_m",
    "z_m",
    "z_per_engine_m",
    "inlet_x_offset_m",
    "inlet_x_per_engine_m",
    "centre_engine",
    "aft_fuselage",
];

/// Engine keys shown locked on the Installation card while a registered aircraft
/// is the case.
const ENGINE_LOCKED_KEYS: [&str; 2] = ["nacelle_profile", "radius_scale_m"];

/// Configuration keys a mission plan owns.
const MISSION_KEYS: [&str; 3] = ["mission", "departure_airport", "arrival_airport"];
/// Requirement keys a payload layout owns next to the cabin group.
const PAYLOAD_REQUIREMENT_KEYS: [&str; 5] = [
    "aircraft_type",
    "cabin_preset",
    "num_passengers",
    "cargo_payload_kg",
    "optimize_passenger_capacity",
];

#[cfg(test)]
#[path = "file_library_ops_tests.rs"]
mod tests;

impl AppState {
    /// The names a kind offers to save from, for kinds that store one chosen
    /// item of a registry (custom airports and airfoils); empty for kinds
    /// that capture the current case.
    pub fn library_save_choices(&self, kind: ObjectKind) -> Vec<String> {
        match kind {
            ObjectKind::Airport => alas_config::airport_io::registered_custom_airports()
                .into_iter()
                .map(|airport| airport.name)
                .collect(),
            ObjectKind::Airfoil => alas_geom::airfoil_io::records()
                .into_iter()
                .map(|record| record.name)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// A suggested name for saving `kind` now.
    pub fn library_default_name(&self, kind: ObjectKind) -> String {
        match kind {
            ObjectKind::Aircraft => self.default_user_aircraft_name(),
            ObjectKind::Engine => self
                .config_values
                .pointer("/geometry/engine/engine_name")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .map_or_else(|| "My Engine".to_owned(), str::to_owned),
            ObjectKind::MissionPlan => {
                let name = self.default_user_aircraft_name();
                format!("{name} Mission")
            }
            ObjectKind::PayloadLayout => format!("{} Payload", self.default_user_aircraft_name()),
            ObjectKind::Results => format!("{} Results", self.default_user_aircraft_name()),
            ObjectKind::AddonSettings => "Add-on Settings".to_owned(),
            ObjectKind::Airport | ObjectKind::Airfoil => String::new(),
        }
    }

    /// What saving `kind` stores. `choice` names the registry item for the
    /// kinds that store one.
    pub fn library_capture(&self, kind: ObjectKind, choice: &str) -> Result<Value, String> {
        match kind {
            ObjectKind::Aircraft => Ok(self.workspace_document()),
            ObjectKind::Engine => {
                let engine = self
                    .config_values
                    .pointer("/geometry/engine")
                    .and_then(Value::as_object)
                    .ok_or_else(|| "The case has no engine installation.".to_owned())?;
                Ok(Value::Object(
                    engine
                        .iter()
                        .filter(|(key, _)| !ENGINE_PLACEMENT_KEYS.contains(&key.as_str()))
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect(),
                ))
            }
            ObjectKind::MissionPlan => {
                let mut plan = serde_json::Map::new();
                for key in MISSION_KEYS {
                    if let Some(value) = self.config_values.get(key) {
                        plan.insert(key.to_owned(), value.clone());
                    }
                }
                Ok(Value::Object(plan))
            }
            ObjectKind::PayloadLayout => {
                let mut requirements = serde_json::Map::new();
                for key in PAYLOAD_REQUIREMENT_KEYS {
                    if let Some(value) = self.config_values.pointer(&format!("/requirements/{key}"))
                    {
                        requirements.insert(key.to_owned(), value.clone());
                    }
                }
                Ok(json!({
                    "cabin": self.config_values.get("cabin").cloned().unwrap_or(Value::Null),
                    "requirements": Value::Object(requirements),
                    "layout_settings": crate::views::mission_stage_editor::capture_layout_settings(self),
                }))
            }
            ObjectKind::Airport => alas_config::airport_io::registered_custom_airports()
                .into_iter()
                .find(|airport| airport.name == choice)
                .ok_or_else(|| "Choose a custom airport to save.".to_owned())
                .and_then(|airport| {
                    serde_json::to_value(airport).map_err(|error| error.to_string())
                }),
            ObjectKind::Airfoil => alas_geom::airfoil_io::records()
                .into_iter()
                .find(|record| record.name == choice)
                .ok_or_else(|| "Choose a custom airfoil to save.".to_owned())
                .and_then(|record| serde_json::to_value(record).map_err(|error| error.to_string())),
            ObjectKind::AddonSettings => {
                serde_json::to_value(&self.addons.settings).map_err(|error| error.to_string())
            }
            ObjectKind::Results => {
                if self.pipeline_result.is_none() {
                    return Err("Run an analysis first: there are no results to save.".to_owned());
                }
                Ok(json!({
                    "summary": self.addon_design_summary(),
                    "workspace": self.workspace_document(),
                    "design_values": self.pipeline_result_design_values,
                }))
            }
        }
    }

    /// Open a library file: apply its content to the current case.
    pub fn library_apply(&mut self, kind: ObjectKind, body: &Value) -> Result<(), String> {
        match kind {
            ObjectKind::Aircraft | ObjectKind::Results => {
                let workspace = if kind == ObjectKind::Results {
                    body.get("workspace")
                        .ok_or_else(|| "The results file has no aircraft.".to_owned())?
                } else {
                    body
                };
                let previous = self.typed_config();
                self.apply_workspace_document(workspace)?;
                // A results file reopens at the design point its run ended on, so
                // running again reproduces it; set before the refresh below.
                if kind == ObjectKind::Results {
                    if let Some(values) = body
                        .get("design_values")
                        .and_then(|value| serde_json::from_value(value.clone()).ok())
                    {
                        self.design_values = values;
                    }
                }
                if let (Some(previous), Some(mut config)) = (previous, self.typed_config()) {
                    crate::config_edit::preserve_machine_locations(&mut config, &previous);
                    self.config_values = crate::config_edit::full_config_values(&config);
                    self.bump_config_revision();
                }
                self.on_config_modified();
                Ok(())
            }
            ObjectKind::Engine => {
                let engine = body.as_object().ok_or_else(|| "not an engine".to_owned())?;
                // A registered aircraft keeps the nacelle its Installation card shows locked.
                let locked = self.manual_geometry_locked();
                let slot = self
                    .config_values
                    .pointer_mut("/geometry/engine")
                    .and_then(Value::as_object_mut)
                    .ok_or_else(|| "The case has no engine installation.".to_owned())?;
                // The engine type replaces the current one; where it is mounted
                // stays with the airframe.
                let mut candidate = slot.clone();
                let mut kept: Vec<&str> = Vec::new();
                for (key, value) in engine {
                    let held = locked && ENGINE_LOCKED_KEYS.contains(&key.as_str());
                    if held && slot.get(key) != Some(value) {
                        kept.push(
                            ENGINE_LOCKED_KEYS
                                .iter()
                                .find(|k| **k == key)
                                .copied()
                                .unwrap_or_default(),
                        );
                    }
                    if !ENGINE_PLACEMENT_KEYS.contains(&key.as_str()) && !held {
                        candidate.insert(key.clone(), value.clone());
                    }
                }
                serde_json::from_value::<alas_config::EngineConfig>(Value::Object(
                    candidate.clone(),
                ))
                .map_err(|error| format!("not an engine: {error}"))?;
                *slot = candidate;
                self.on_config_modified();
                if !kept.is_empty() {
                    self.log(
                        tr_fields(
                            "Locked Nacelle Kept From The Registered Aircraft: {keys}",
                            &[("keys", kept.join(", "))],
                        ),
                        LogKind::Info,
                    );
                }
                Ok(())
            }
            ObjectKind::MissionPlan => {
                let plan = body
                    .as_object()
                    .ok_or_else(|| "not a mission plan".to_owned())?;
                let root = self
                    .config_values
                    .as_object_mut()
                    .ok_or_else(|| "The case has no configuration.".to_owned())?;
                for key in MISSION_KEYS {
                    if let Some(value) = plan.get(key) {
                        root.insert(key.to_owned(), value.clone());
                    }
                }
                self.mission_profile_manual_edit = true;
                self.on_config_modified();
                Ok(())
            }
            ObjectKind::PayloadLayout => {
                let layout = body
                    .as_object()
                    .ok_or_else(|| "not a payload layout".to_owned())?;
                if let Some(cabin) = layout.get("cabin").filter(|cabin| !cabin.is_null()) {
                    if let Some(slot) = self.config_values.get_mut("cabin") {
                        *slot = cabin.clone();
                    }
                }
                if let Some(requirements) = layout.get("requirements").and_then(Value::as_object) {
                    if let Some(target) = self
                        .config_values
                        .get_mut("requirements")
                        .and_then(Value::as_object_mut)
                    {
                        for (key, value) in requirements {
                            target.insert(key.clone(), value.clone());
                        }
                    }
                }
                if let Some(settings) = layout.get("layout_settings") {
                    crate::views::mission_stage_editor::store_layout_settings(
                        &mut self.config_values,
                        settings,
                    );
                }
                self.on_config_modified();
                Ok(())
            }
            ObjectKind::Airport => {
                let airport: alas_config::airport_io::CustomAirport =
                    serde_json::from_value(body.clone())
                        .map_err(|error| format!("not an airport: {error}"))?;
                let mut airports = alas_config::airport_io::registered_custom_airports();
                airports.retain(|existing| existing.icao != airport.icao);
                airports.push(airport);
                alas_config::airport_io::replace_custom_airports(airports)
                    .map_err(|error| error.to_string())?;
                self.refresh_airport_names();
                Ok(())
            }
            ObjectKind::Airfoil => {
                let record: alas_geom::airfoil_io::AirfoilRecord =
                    serde_json::from_value(body.clone())
                        .map_err(|error| format!("not an airfoil: {error}"))?;
                let mut records = alas_geom::airfoil_io::records();
                records.retain(|existing| !existing.name.eq_ignore_ascii_case(&record.name));
                records.push(record);
                alas_geom::airfoil_io::replace_records(records).map_err(|error| error.to_string())
            }
            ObjectKind::AddonSettings => {
                self.addons.settings = serde_json::from_value(body.clone())
                    .map_err(|error| format!("not add-on settings: {error}"))?;
                self.save_addon_settings();
                Ok(())
            }
        }
    }

    /// Save the current case as a `kind` file named `name`.
    pub fn library_save(&mut self, kind: ObjectKind, name: &str, choice: &str) -> bool {
        let result = self
            .library_capture(kind, choice)
            .and_then(|body| file_library::save(&self.data_dir(), kind, name, &body, now_unix()));
        self.library_report(kind, name, result, Verb::Saved)
    }

    /// Open a library file.
    pub fn library_open(&mut self, entry: &LibraryEntry) -> bool {
        let result = file_library::read(entry.kind, &entry.path).and_then(|(_, body)| {
            self.library_apply(entry.kind, &body)?;
            Ok(entry.path.clone())
        });
        if result.is_ok() {
            if entry.kind == ObjectKind::Aircraft {
                self.files.active_aircraft = Some(entry.name.clone());
            }
            let _ =
                file_library::note_recent(&self.data_dir(), entry.kind, &entry.path, now_unix());
        }
        self.library_report(entry.kind, &entry.name, result, Verb::Opened)
    }

    pub(crate) fn library_report(
        &mut self,
        kind: ObjectKind,
        name: &str,
        result: Result<PathBuf, String>,
        verb: Verb,
    ) -> bool {
        match result {
            Ok(path) => {
                let template = match verb {
                    Verb::Saved => "Saved {kind} \"{name}\" to {path}.",
                    Verb::Opened => "Opened {kind} \"{name}\".",
                    Verb::Imported => "Imported {kind} from {name}.",
                };
                let message = tr_fields(
                    template,
                    &[
                        ("kind", crate::views::tr(kind.name())),
                        ("name", name.trim().to_owned()),
                        ("path", path.display().to_string()),
                    ],
                );
                // Only a file that holds the whole case settles the unsaved edits.
                if matches!(kind, ObjectKind::Aircraft | ObjectKind::Results)
                    && !matches!(verb, Verb::Imported)
                {
                    self.settle_case();
                }
                self.files.status = Some(message.clone());
                self.log(message, LogKind::Info);
                true
            }
            Err(error) => {
                self.files.status = Some(error.clone());
                let template = match verb {
                    Verb::Saved => "File not saved: {error}",
                    Verb::Opened => "File not opened: {error}",
                    Verb::Imported => "File not imported: {error}",
                };
                self.log(tr_fields(template, &[("error", error)]), LogKind::Warn);
                false
            }
        }
    }

    /// Import any library file from `source`.
    pub fn library_import(&mut self, source: &Path) -> bool {
        match file_library::import(&self.data_dir(), source) {
            Ok((kind, path)) => {
                if kind == ObjectKind::Aircraft {
                    self.refresh_user_aircraft();
                }
                self.library_report(
                    kind,
                    &source.display().to_string(),
                    Ok(path),
                    Verb::Imported,
                )
            }
            Err(error) => self.library_report(ObjectKind::Aircraft, "", Err(error), Verb::Imported),
        }
    }
}

impl AppState {
    /// Open a file chosen from any path: a library file of any kind, or a plain
    /// ALAS configuration, which is loaded as a configuration.
    pub fn library_open_path(&mut self, path: &Path) -> bool {
        match file_library::file_kind(path) {
            Some(kind) => {
                let entry = LibraryEntry {
                    kind,
                    name: path
                        .file_stem()
                        .map(|stem| stem.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    saved_unix: 0,
                    path: path.to_owned(),
                };
                self.library_open(&entry)
            }
            None => {
                self.config_path = path.display().to_string();
                self.load_config();
                true
            }
        }
    }
}
