// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! JSON parsing and serialization for custom airports.

use super::CustomAirport;
use super::{annotate_import, normalize_and_validate, AirportIoError, AirportProvenanceKind};

/// Parse an ALAS airport JSON document without changing the custom registry.
pub fn parse_json(text: &str) -> Result<Vec<CustomAirport>, AirportIoError> {
    let value = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|error| AirportIoError::Json(error.to_string()))?;
    parse_value(value)
}

/// Parse an ALAS airport JSON document and mark records as JSON imports.
pub fn parse_json_with_source(
    text: &str,
    source: &str,
) -> Result<Vec<CustomAirport>, AirportIoError> {
    let mut airports = parse_json(text)?;
    annotate_import(&mut airports, AirportProvenanceKind::JsonImport, source)?;
    Ok(airports)
}

/// Parse a JSON value used by a saved workspace, preserving its provenance.
pub fn parse_value(value: serde_json::Value) -> Result<Vec<CustomAirport>, AirportIoError> {
    let mut airports = if let Some(array) = value.as_array() {
        serde_json::from_value::<Vec<CustomAirport>>(serde_json::Value::Array(array.clone()))
            .map_err(|error| AirportIoError::Json(error.to_string()))?
    } else if let Some(object) = value.as_object() {
        if let Some(format) = object.get("format").and_then(serde_json::Value::as_str) {
            if format != "alas-airports" {
                return Err(AirportIoError::Json(format!(
                    "unsupported airport document format '{format}'"
                )));
            }
        }
        if let Some(version) = object.get("version").and_then(serde_json::Value::as_u64) {
            if version != 1 {
                return Err(AirportIoError::Json(format!(
                    "unsupported airport document version {version}"
                )));
            }
        }
        if let Some(records) = object.get("airports") {
            serde_json::from_value::<Vec<CustomAirport>>(records.clone())
                .map_err(|error| AirportIoError::Json(error.to_string()))?
        } else if object.get("icao").is_some() {
            vec![serde_json::from_value::<CustomAirport>(value)
                .map_err(|error| AirportIoError::Json(error.to_string()))?]
        } else {
            return Err(AirportIoError::Json(
                "expected an 'airports' array or one airport object".to_owned(),
            ));
        }
    } else {
        return Err(AirportIoError::Json(
            "expected an airport object or array".to_owned(),
        ));
    };
    normalize_and_validate(&mut airports).map(|()| airports)
}

/// Export custom airports as a versioned JSON document.
pub fn export_json(airports: &[CustomAirport]) -> Result<String, AirportIoError> {
    let mut normalized = airports.to_vec();
    normalize_and_validate(&mut normalized)?;
    serde_json::to_string_pretty(&serde_json::json!({
        "format": "alas-airports",
        "version": 1,
        "airports": normalized,
    }))
    .map_err(|error| AirportIoError::Json(error.to_string()))
}
