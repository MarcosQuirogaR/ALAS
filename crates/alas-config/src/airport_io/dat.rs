// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Line-oriented DAT parsing and serialization for custom airports.

use std::collections::BTreeMap;
use std::path::Path;

use super::{
    annotate_import, normalize_and_validate, AirportIoError, AirportProvenance,
    AirportProvenanceKind, CustomAirport,
};

/// Parse one or more records in the line-oriented ALAS airport .dat format.
pub fn parse_dat(text: &str) -> Result<Vec<CustomAirport>, AirportIoError> {
    let mut records = Vec::new();
    let mut current = Vec::new();
    for (line_index, raw) in text.lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw.trim();
        if line.is_empty() {
            if !current.is_empty() {
                records.push(parse_dat_record(&current)?);
                current.clear();
            }
            continue;
        }
        if line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(AirportIoError::InvalidLine {
                format: "dat",
                line: line_number,
                reason: "expected key=value".to_owned(),
            });
        };
        let key = key.trim().to_ascii_lowercase();
        if key.is_empty() || value.trim().is_empty() {
            return Err(AirportIoError::InvalidLine {
                format: "dat",
                line: line_number,
                reason: "key and value must both be non-empty".to_owned(),
            });
        }
        if current.iter().any(|(_, existing, _)| existing == &key) {
            return Err(AirportIoError::InvalidLine {
                format: "dat",
                line: line_number,
                reason: format!("duplicate key '{key}' in one record"),
            });
        }
        current.push((line_number, key, value.trim().to_owned()));
    }
    if !current.is_empty() {
        records.push(parse_dat_record(&current)?);
    }
    if records.is_empty() {
        return Err(AirportIoError::Json(
            "airport dat document is empty".to_owned(),
        ));
    }
    normalize_and_validate(&mut records).map(|()| records)
}

/// Parse the .dat format and mark records with an import source.
pub fn parse_dat_with_source(
    text: &str,
    source: &str,
) -> Result<Vec<CustomAirport>, AirportIoError> {
    let mut airports = parse_dat(text)?;
    annotate_import(&mut airports, AirportProvenanceKind::DatImport, source)?;
    Ok(airports)
}

/// Import .dat or .json records from a path without mutating the registry.
pub fn import_file(path: impl AsRef<Path>) -> Result<Vec<CustomAirport>, AirportIoError> {
    let path = path.as_ref();
    let display = path.display().to_string();
    let text = std::fs::read_to_string(path).map_err(|error| AirportIoError::Read {
        path: display.clone(),
        detail: error.to_string(),
    })?;
    match extension(path).as_deref() {
        Some("dat") => super::parse_dat_with_source(&text, &display),
        Some("json") => super::parse_json_with_source(&text, &display),
        _ => Err(AirportIoError::UnsupportedExtension(display)),
    }
}

/// Export records to .dat or .json, validating before writing.
pub fn export_file(
    path: impl AsRef<Path>,
    airports: &[CustomAirport],
) -> Result<(), AirportIoError> {
    let path = path.as_ref();
    let display = path.display().to_string();
    let mut normalized = airports.to_vec();
    normalize_and_validate(&mut normalized)?;
    let text = match extension(path).as_deref() {
        Some("dat") => format_dat(&normalized),
        Some("json") => super::export_json(&normalized)?,
        _ => return Err(AirportIoError::UnsupportedExtension(display)),
    };
    std::fs::write(path, text).map_err(|error| AirportIoError::Write {
        path: display,
        detail: error.to_string(),
    })
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
}

fn parse_dat_record(entries: &[(usize, String, String)]) -> Result<CustomAirport, AirportIoError> {
    let values: BTreeMap<&str, (&usize, &String)> = entries
        .iter()
        .map(|(line, key, value)| (key.as_str(), (line, value)))
        .collect();
    let required = |key: &str| {
        values
            .get(key)
            .map(|(_, value)| value.as_str())
            .ok_or_else(|| AirportIoError::InvalidLine {
                format: "dat",
                line: entries.first().map_or(1, |entry| entry.0),
                reason: format!("missing required key '{key}'"),
            })
    };
    for key in values.keys() {
        if !matches!(
            *key,
            "icao"
                | "name"
                | "latitude_deg"
                | "lat"
                | "longitude_deg"
                | "lon"
                | "isa_delta_c"
                | "isa_deviation_c"
                | "altitude_m"
                | "elevation_m"
                | "runway_lengths_m"
                | "runways_m"
                | "declared_toda_m"
                | "declared_lda_m"
                | "source"
                | "note"
        ) {
            let line = values[key].0;
            return Err(AirportIoError::InvalidLine {
                format: "dat",
                line: *line,
                reason: format!("unknown key '{key}'"),
            });
        }
    }
    let parse_number = |key: &str, aliases: &[&str]| -> Result<f64, AirportIoError> {
        let (line, value) = aliases
            .iter()
            .find_map(|alias| values.get(alias).copied())
            .ok_or_else(|| AirportIoError::InvalidLine {
                format: "dat",
                line: entries.first().map_or(1, |entry| entry.0),
                reason: format!("missing required key '{key}'"),
            })?;
        value
            .parse::<f64>()
            .map_err(|_| AirportIoError::InvalidLine {
                format: "dat",
                line: *line,
                reason: format!("'{key}' must be a number"),
            })
    };
    let parse_optional_number = |key: &str| -> Result<Option<f64>, AirportIoError> {
        let Some((line, value)) = values.get(key).copied() else {
            return Ok(None);
        };
        value
            .parse::<f64>()
            .map(Some)
            .map_err(|_| AirportIoError::InvalidLine {
                format: "dat",
                line: *line,
                reason: format!("'{key}' must be a number"),
            })
    };
    let runway_line = values
        .get("runway_lengths_m")
        .or_else(|| values.get("runways_m"))
        .map(|(line, _)| **line)
        .unwrap_or_else(|| entries.first().map_or(1, |entry| entry.0));
    let runway_value = values
        .get("runway_lengths_m")
        .or_else(|| values.get("runways_m"))
        .map(|(_, value)| value.as_str())
        .ok_or_else(|| AirportIoError::InvalidLine {
            format: "dat",
            line: runway_line,
            reason: "missing required key 'runway_lengths_m'".to_owned(),
        })?;
    let mut runway_lengths_m = Vec::new();
    for value in runway_value
        .split([',', ';', ' '])
        .filter(|value| !value.is_empty())
    {
        runway_lengths_m.push(
            value
                .parse::<f64>()
                .map_err(|_| AirportIoError::InvalidLine {
                    format: "dat",
                    line: runway_line,
                    reason: "runway_lengths_m must be comma-separated numbers".to_owned(),
                })?,
        );
    }
    let latitude_key = if values.contains_key("latitude_deg") {
        "latitude_deg"
    } else {
        "lat"
    };
    let longitude_key = if values.contains_key("longitude_deg") {
        "longitude_deg"
    } else {
        "lon"
    };
    let isa_key = if values.contains_key("isa_delta_c") {
        "isa_delta_c"
    } else {
        "isa_deviation_c"
    };
    let altitude_key = if values.contains_key("altitude_m") {
        "altitude_m"
    } else {
        "elevation_m"
    };
    let mut airport = CustomAirport {
        icao: required("icao")?.to_owned(),
        name: required("name")?.to_owned(),
        latitude_deg: parse_number("latitude_deg", &[latitude_key])?,
        longitude_deg: parse_number("longitude_deg", &[longitude_key])?,
        isa_delta_c: parse_number("isa_delta_c", &[isa_key])?,
        altitude_m: parse_number("altitude_m", &[altitude_key])?,
        runway_lengths_m,
        declared_toda_m: parse_optional_number("declared_toda_m")?,
        declared_lda_m: parse_optional_number("declared_lda_m")?,
        provenance: AirportProvenance {
            kind: AirportProvenanceKind::UserEntered,
            source: values.get("source").map(|(_, value)| (*value).clone()),
            note: values.get("note").map(|(_, value)| (*value).clone()),
        },
    };
    airport.validate_and_normalize()?;
    Ok(airport)
}

pub(super) fn format_dat(airports: &[CustomAirport]) -> String {
    let mut output = String::from("# ALAS-AIRPORT-DAT v1\n");
    for (index, airport) in airports.iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        output.push_str(&format!(
            "icao={}\nname={}\nlatitude_deg={}\nlongitude_deg={}\nisa_delta_c={}\naltitude_m={}\nrunway_lengths_m={}\n",
            airport.icao,
            airport.name,
            airport.latitude_deg,
            airport.longitude_deg,
            airport.isa_delta_c,
            airport.altitude_m,
            airport
                .runway_lengths_m
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
        ));
        if let Some(value) = airport.declared_toda_m {
            output.push_str(&format!("declared_toda_m={value}\n"));
        }
        if let Some(value) = airport.declared_lda_m {
            output.push_str(&format!("declared_lda_m={value}\n"));
        }
        if let Some(source) = &airport.provenance.source {
            output.push_str(&format!("source={source}\n"));
        }
        if let Some(note) = &airport.provenance.note {
            output.push_str(&format!("note={note}\n"));
        }
    }
    output
}
