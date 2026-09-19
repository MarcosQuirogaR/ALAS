// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Custom airport and airfoil persistence operations for [`AppState`].

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use alas_config::airport_io::CustomAirport;

use crate::state::{AppState, LogKind};
use crate::views::tr_fields;

/// The user-level file the custom-airport registry is kept in.
///
/// The registry itself is in-memory, and a saved workspace carries a copy, but
/// entering an airport in the application is expected to survive a restart on
/// its own — without the user having to remember to save a case file. This
/// sits beside the tool preferences the same installation already writes, so
/// one user-data location holds both.
const CUSTOM_AIRPORT_STORE: &str = "custom-airports.json";

/// Serialize the registry to `path` without exposing a partly written file to
/// the next launch, the same temporary-then-rename discipline the tool
/// preferences use.
fn write_custom_airports(path: &Path, airports: &[CustomAirport]) -> Result<(), String> {
    let text = alas_config::airport_io::export_json(airports).map_err(|error| error.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let temporary = path.with_file_name(format!(".custom-airports-{}.tmp", std::process::id()));
    fs::write(&temporary, text)
        .map_err(|error| format!("cannot write {}: {error}", temporary.display()))?;
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("cannot replace {}: {error}", path.display())
    })
}

/// Read a persisted registry.
///
/// `Ok(None)` means there is nothing stored yet, which is the ordinary state
/// of a fresh installation. An unreadable or invalid file is an error rather
/// than an empty registry, so a damaged store is reported instead of silently
/// discarding the user's airports. Provenance is read back as it was written:
/// this is the same registry, not a fresh JSON import.
fn read_custom_airports(path: &Path) -> Result<Option<Vec<CustomAirport>>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    alas_config::airport_io::parse_value(value)
        .map(Some)
        .map_err(|error| error.to_string())
}

impl AppState {
    /// Where this installation keeps its custom airports.
    fn custom_airport_store(&self) -> PathBuf {
        self.tool_locator
            .preferences_path()
            .with_file_name(CUSTOM_AIRPORT_STORE)
    }

    /// Restore the persisted custom airports at start-up.
    ///
    /// A damaged store is reported and left on disk rather than overwritten,
    /// so the user can recover it; the session simply starts with the curated
    /// database only.
    pub fn load_persisted_custom_airports(&mut self) {
        let path = self.custom_airport_store();
        match read_custom_airports(&path) {
            Ok(None) => {}
            Ok(Some(airports)) => {
                let count = airports.len();
                match alas_config::airport_io::replace_custom_airports(airports) {
                    Ok(()) => {
                        self.refresh_airport_names();
                        if count > 0 {
                            self.log(
                                format!(
                                    "Restored {count} custom airport(s) from {}.",
                                    path.display()
                                ),
                                LogKind::Info,
                            );
                        }
                    }
                    Err(error) => self.log(
                        format!("Custom airport store rejected: {error}"),
                        LogKind::Warn,
                    ),
                }
            }
            Err(error) => self.log(
                format!(
                    "Custom airport store at {} is unreadable: {error}",
                    path.display()
                ),
                LogKind::Warn,
            ),
        }
    }

    /// Persist the current registry after it changed.
    ///
    /// A failure here never rejects the change the user already made in this
    /// session; it is reported so they know the entry is not yet durable.
    fn persist_custom_airports(&mut self) {
        let path = self.custom_airport_store();
        let airports = alas_config::airport_io::registered_custom_airports();
        if let Err(error) = write_custom_airports(&path, &airports) {
            let status =
                format!("Custom airports could not be saved for the next session: {error}");
            self.custom_airport_status = Some(status.clone());
            self.log(status, LogKind::Warn);
        }
    }

    /// Refresh airport selector choices after a custom entry or import.
    pub fn refresh_airport_names(&mut self) {
        self.airport_names = alas_config::airports::database_with_custom()
            .into_iter()
            .map(|airport| airport.name)
            .collect();
    }

    /// Validate and register the airport currently entered in the editor.
    ///
    /// Returns the registered display name, which is the identity the route
    /// selectors and the airport database use. `None` means the draft or the
    /// registry write was rejected and nothing was registered, so a caller
    /// must not attach the entry to a route.
    pub fn save_custom_airport(&mut self) -> Option<String> {
        let airport = match self.custom_airport_draft.to_airport() {
            Ok(airport) => airport,
            Err(error) => {
                self.custom_airport_status = Some(error.clone());
                self.log(format!("Custom airport rejected: {error}"), LogKind::Error);
                return None;
            }
        };
        let mut airports = alas_config::airport_io::registered_custom_airports();
        airports.retain(|existing| {
            !existing.icao.eq_ignore_ascii_case(&airport.icao)
                && !existing.name.eq_ignore_ascii_case(&airport.name)
        });
        airports.push(airport.clone());
        match alas_config::airport_io::replace_custom_airports(airports) {
            Ok(()) => {
                let status = format!(
                    "Saved custom airport {} (physical runway lengths retained; declared TODA/LDA only when supplied).",
                    airport.icao
                );
                self.custom_airport_status = Some(status.clone());
                self.refresh_airport_names();
                self.persist_custom_airports();
                self.log(status, LogKind::Info);
                Some(airport.name)
            }
            Err(error) => {
                let status = error.to_string();
                self.custom_airport_status = Some(status.clone());
                self.log(format!("Custom airport rejected: {status}"), LogKind::Error);
                None
            }
        }
    }

    /// Import custom airports atomically from the path in the editor.
    pub fn import_custom_airports(&mut self) {
        let path = self.custom_airport_file_path.clone();
        match alas_config::airport_io::import_file(&path)
            .and_then(alas_config::airport_io::replace_custom_airports)
        {
            Ok(()) => {
                let status = format!(
                    "Imported {} custom airport(s) from {path}.",
                    alas_config::airport_io::registered_custom_airports().len()
                );
                self.custom_airport_status = Some(status.clone());
                self.refresh_airport_names();
                self.persist_custom_airports();
                self.log(status, LogKind::Info);
            }
            Err(error) => {
                let status = error.to_string();
                self.custom_airport_status = Some(status.clone());
                self.log(
                    format!("Custom airport import rejected: {status}"),
                    LogKind::Error,
                );
            }
        }
    }

    /// Export the registered custom airports to the path in the editor.
    pub fn export_custom_airports(&mut self) {
        let path = self.custom_airport_file_path.clone();
        let airports = alas_config::airport_io::registered_custom_airports();
        match alas_config::airport_io::export_file(&path, &airports) {
            Ok(()) => {
                let status = format!("Exported {} custom airport(s) to {path}.", airports.len());
                self.custom_airport_status = Some(status.clone());
                self.log(status, LogKind::Info);
            }
            Err(error) => {
                let status = error.to_string();
                self.custom_airport_status = Some(status.clone());
                self.log(
                    format!("Custom airport export failed: {status}"),
                    LogKind::Error,
                );
            }
        }
    }

    /// Import a validated custom airfoil and make it available to geometry
    /// lookup without changing the current preset selection.
    pub fn import_custom_airfoil(&mut self) {
        let path = self.custom_airfoil_file_path.clone();
        let result = alas_geom::airfoil_io::import_dat(&path).and_then(|imported| {
            let mut records = alas_geom::airfoil_io::records();
            records.retain(|record| !record.name.eq_ignore_ascii_case(&imported.airfoil.name));
            records.push(alas_geom::airfoil_io::AirfoilRecord::from(&imported));
            alas_geom::airfoil_io::replace_records(records)
        });
        match result {
            Ok(()) => {
                let status = format!("Imported custom airfoil from {path}.");
                self.custom_airfoil_status = Some(status.clone());
                self.screening.preview.invalidate_filter();
                self.log(status, LogKind::Info);
            }
            Err(error) => {
                let status = error.to_string();
                self.custom_airfoil_status = Some(status.clone());
                self.log(
                    format!("Custom airfoil import rejected: {status}"),
                    LogKind::Error,
                );
            }
        }
    }

    /// Add custom airport and airfoil records to the desktop workspace
    /// envelope while leaving the generic configuration representation intact.
    pub(super) fn workspace_document_with_custom_data(&self) -> Value {
        let mut document = self.workspace_document();
        if let Some(envelope) = document
            .get_mut(alas_config::WORKSPACE_ENVELOPE_KEY)
            .and_then(Value::as_object_mut)
        {
            if let Ok(value) =
                serde_json::to_value(alas_config::airport_io::registered_custom_airports())
            {
                envelope.insert("custom_airports".to_owned(), value);
            }
            if let Ok(value) = serde_json::to_value(alas_geom::airfoil_io::records()) {
                envelope.insert("custom_airfoils".to_owned(), value);
            }
        }
        document
    }

    /// Restore custom data from a workspace envelope before its aircraft
    /// configuration is applied. Registry changes are reverted if either data
    /// family is invalid, so a rejected load cannot leave a half-imported case.
    pub(super) fn restore_custom_data_from_workspace(
        &mut self,
        document: &Value,
    ) -> Result<(), String> {
        let Some(envelope) = document
            .get(alas_config::WORKSPACE_ENVELOPE_KEY)
            .and_then(Value::as_object)
        else {
            return Ok(());
        };
        let airports = envelope
            .get("custom_airports")
            .map(|value| alas_config::airport_io::parse_value(value.clone()))
            .transpose()
            .map_err(|error| error.to_string())?;
        let airfoils = envelope
            .get("custom_airfoils")
            .map(|value| {
                serde_json::from_value::<Vec<alas_geom::airfoil_io::AirfoilRecord>>(value.clone())
                    .map_err(|error| error.to_string())
            })
            .transpose()?;
        let old_airports = alas_config::airport_io::registered_custom_airports();
        let old_airfoils = alas_geom::airfoil_io::records();
        if let Some(airports) = airports {
            alas_config::airport_io::replace_custom_airports(airports)
                .map_err(|error| error.to_string())?;
        }
        if let Some(airfoils) = airfoils {
            if let Err(error) = alas_geom::airfoil_io::replace_records(airfoils) {
                let _ = alas_config::airport_io::replace_custom_airports(old_airports);
                let _ = alas_geom::airfoil_io::replace_records(old_airfoils);
                return Err(error.to_string());
            }
        }
        self.refresh_airport_names();
        // A workspace load replaces the registry, so the persisted store has
        // to follow it; otherwise the next launch would restore the airports
        // of whichever case was open before.
        self.persist_custom_airports();
        self.screening.preview.invalidate_filter();
        Ok(())
    }

    /// Save the current configuration with custom records in its workspace
    /// envelope. The generic configuration remains backward-compatible.
    pub fn save_config(&mut self) {
        let text = match serde_json::to_string_pretty(&self.workspace_document_with_custom_data()) {
            Ok(t) => t,
            Err(e) => {
                self.log(
                    tr_fields("Save failed: {error}", &[("error", e.to_string())]),
                    LogKind::Error,
                );
                return;
            }
        };
        let path = self.config_path.clone();
        match std::fs::write(&path, text) {
            Ok(()) => self.log(
                tr_fields("Saved configuration to {path}.", &[("path", path)]),
                LogKind::Info,
            ),
            Err(e) => self.log(
                tr_fields("Save failed: {error}", &[("error", e.to_string())]),
                LogKind::Error,
            ),
        }
    }
}

#[cfg(test)]
mod custom_airport_store_tests {
    use super::*;
    use alas_config::airport_io::{AirportProvenance, AirportProvenanceKind};

    fn temporary_store(name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir()
            .join(format!("alas-airport-store-{}-{stamp}", std::process::id()))
            .join(name)
    }

    fn entered_airport() -> CustomAirport {
        CustomAirport {
            icao: "LEXX".to_owned(),
            name: "Test Field".to_owned(),
            latitude_deg: 40.0,
            longitude_deg: -3.0,
            isa_delta_c: 0.0,
            altitude_m: 600.0,
            runway_lengths_m: vec![3200.0],
            declared_toda_m: None,
            declared_lda_m: None,
            provenance: AirportProvenance {
                kind: AirportProvenanceKind::UserEntered,
                source: Some("ALAS custom-airport editor".to_owned()),
                note: None,
            },
        }
    }

    #[test]
    fn a_missing_store_is_an_empty_installation_not_an_error() {
        let path = temporary_store("absent.json");
        assert_eq!(read_custom_airports(&path), Ok(None));
    }

    #[test]
    fn the_store_round_trips_provenance_and_missing_declarations() {
        let path = temporary_store("round-trip.json");
        write_custom_airports(&path, &[entered_airport()]).expect("the store is writable");

        let restored = read_custom_airports(&path)
            .expect("the store is readable")
            .expect("the store exists");
        let _ = fs::remove_dir_all(path.parent().expect("the store has a directory"));

        assert_eq!(restored.len(), 1);
        let airport = &restored[0];
        // Restoring is not re-importing: a user-entered record stays one.
        assert_eq!(airport.provenance.kind, AirportProvenanceKind::UserEntered);
        // Physical lengths are kept as physical lengths; no declaration is
        // invented for the operational distances the user never supplied.
        assert_eq!(airport.runway_lengths_m, vec![3200.0]);
        assert_eq!(airport.declared_toda_m, None);
        assert_eq!(airport.declared_lda_m, None);
    }

    #[test]
    fn a_damaged_store_is_reported_rather_than_read_as_no_airports() {
        let path = temporary_store("damaged.json");
        fs::create_dir_all(path.parent().expect("the store has a directory"))
            .expect("the directory is creatable");
        fs::write(&path, "{ not json").expect("the damaged store is writable");

        let outcome = read_custom_airports(&path);
        let _ = fs::remove_dir_all(path.parent().expect("the store has a directory"));

        assert!(
            outcome.is_err(),
            "a damaged store must not look like an empty one: {outcome:?}"
        );
    }

    #[test]
    fn a_rejected_record_never_becomes_a_silently_emptied_store() {
        let path = temporary_store("invalid-record.json");
        let mut broken = entered_airport();
        broken.latitude_deg = 120.0;

        let outcome = write_custom_airports(&path, &[broken]);

        assert!(outcome.is_err(), "an invalid record must not be written");
        assert!(!path.exists(), "no partial store may be left behind");
        let _ = fs::remove_dir_all(path.parent().expect("the store has a directory"));
    }
}
