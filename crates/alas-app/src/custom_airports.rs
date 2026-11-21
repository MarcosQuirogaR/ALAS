// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The user's custom airports for a headless run.
//!
//! The desktop application restores the persisted store at start-up and the
//! airports a saved workspace carries when it opens one
//! (`alas_gui::config_edit`). A headless run reads the same two sources, so a
//! case between user-entered aerodromes runs the same way without the GUI.

use std::path::Path;

use alas_config::airport_io::{self, CustomAirport};
use serde_json::Value;

/// What [`install`] registered.
#[derive(Debug, Default)]
pub(crate) struct InstallReport {
    /// Airports now registered, store and workspace together.
    pub count: usize,
    /// Problems that did not stop the install (a damaged store is skipped).
    pub warnings: Vec<String>,
}

/// The airports `document` carries in its workspace envelope
/// (`alas_workspace.custom_airports`), if any.
///
/// # Errors
///
/// The envelope lists airports that do not parse.
pub(crate) fn from_document(document: &Value) -> Result<Vec<CustomAirport>, String> {
    document
        .get(alas_config::WORKSPACE_ENVELOPE_KEY)
        .and_then(|envelope| envelope.get("custom_airports"))
        .map(|value| airport_io::parse_value(value.clone()).map_err(|error| error.to_string()))
        .transpose()
        .map(Option::unwrap_or_default)
}

/// Register the persisted store at `store` plus the airports `document`
/// carries. A workspace airport replaces a stored one with the same ICAO
/// code: the case file is what the run is about.
///
/// # Errors
///
/// The workspace envelope does not parse, or the combined list is refused by
/// the registry (an ICAO code that collides); the registry is then unchanged.
pub(crate) fn install(store: &Path, document: Option<&Value>) -> Result<InstallReport, String> {
    let mut report = InstallReport::default();
    let mut airports = match alas_gui::config_edit::read_custom_airports(store) {
        Ok(stored) => stored.unwrap_or_default(),
        Err(error) => {
            report.warnings.push(format!(
                "custom airport store {} skipped: {error}",
                store.display()
            ));
            Vec::new()
        }
    };
    if let Some(document) = document {
        for airport in from_document(document)? {
            airports.retain(|stored| !stored.icao.eq_ignore_ascii_case(&airport.icao));
            airports.push(airport);
        }
    }
    report.count = airports.len();
    airport_io::replace_custom_airports(airports).map_err(|error| error.to_string())?;
    Ok(report)
}

/// Add `airport` to the persisted store at `store` and to the registry,
/// replacing a stored airport with the same ICAO code.
///
/// # Errors
///
/// The airport is invalid or collides with a curated one, or the store
/// cannot be read or written; neither the store nor the registry changes.
pub(crate) fn add_to_store(store: &Path, airport: CustomAirport) -> Result<usize, String> {
    let mut airports = alas_gui::config_edit::read_custom_airports(store)?.unwrap_or_default();
    airports.retain(|stored| !stored.icao.eq_ignore_ascii_case(&airport.icao));
    airports.push(airport);
    let previous = airport_io::registered_custom_airports();
    airport_io::replace_custom_airports(airports.clone()).map_err(|error| error.to_string())?;
    if let Err(error) = alas_gui::config_edit::write_custom_airports(store, &airports) {
        // The store is the record; a registry ahead of it would vanish at the
        // next start, so the registry goes back to what it was.
        let _ = airport_io::replace_custom_airports(previous);
        return Err(error);
    }
    Ok(airports.len())
}
