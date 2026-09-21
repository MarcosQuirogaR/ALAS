// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! User airport entry, persistence, and import/export.
//!
//! A custom airport carries its identity, position, ISA delta, elevation,
//! runway lengths, and provenance as one value. Physical runway lengths are
//! deliberately kept separate from declared TODA/LDA: a length read from a
//! file is not silently promoted into a certified operational distance.

use serde::{Deserialize, Serialize};

/// The source category retained with a custom airport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AirportProvenanceKind {
    /// Entered directly in the application.
    UserEntered,
    /// Imported from the ALAS airport .dat format.
    DatImport,
    /// Imported from the ALAS airport JSON format.
    JsonImport,
    /// Restored from a saved workspace.
    Workspace,
}

impl Default for AirportProvenanceKind {
    fn default() -> Self {
        Self::UserEntered
    }
}

/// Source metadata retained with a custom airport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AirportProvenance {
    /// How the record entered the application.
    #[serde(default)]
    pub kind: AirportProvenanceKind,
    /// Optional source path, URL, or user-supplied label.
    #[serde(default)]
    pub source: Option<String>,
    /// Optional note explaining the provenance or limitations.
    #[serde(default)]
    pub note: Option<String>,
}

impl Default for AirportProvenance {
    fn default() -> Self {
        Self {
            kind: AirportProvenanceKind::UserEntered,
            source: None,
            note: None,
        }
    }
}

/// A user-entered or imported airport record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomAirport {
    /// Four-character ICAO-style identifier, normalized to uppercase.
    pub icao: String,
    /// Display name.
    pub name: String,
    /// Reference latitude, positive north, in degrees.
    pub latitude_deg: f64,
    /// Reference longitude, positive east, in degrees.
    pub longitude_deg: f64,
    /// ISA temperature offset at the airport, in degrees Celsius.
    #[serde(alias = "isa_deviation_c")]
    pub isa_delta_c: f64,
    /// Field elevation above mean sea level, in metres.
    #[serde(alias = "elevation_m")]
    pub altitude_m: f64,
    /// Physical runway lengths, in metres. These are not declared TODA/LDA.
    #[serde(alias = "runways_m", alias = "runway_lengths")]
    pub runway_lengths_m: Vec<f64>,
    /// Explicitly declared takeoff distance available, in metres, if known.
    #[serde(default)]
    pub declared_toda_m: Option<f64>,
    /// Explicitly declared landing distance available, in metres, if known.
    #[serde(default)]
    pub declared_lda_m: Option<f64>,
    /// Provenance retained in the workspace and export files.
    #[serde(default)]
    pub provenance: AirportProvenance,
}

impl CustomAirport {
    /// Validate and normalize an airport record in place.
    pub fn validate_and_normalize(&mut self) -> Result<(), AirportIoError> {
        self.icao = self.icao.trim().to_ascii_uppercase();
        self.name = self.name.trim().to_owned();
        if self.icao.len() != 4 || !self.icao.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
            return Err(AirportIoError::InvalidField {
                field: "icao".to_owned(),
                reason: "must contain exactly four ASCII letters or digits".to_owned(),
            });
        }
        if self.name.is_empty() || self.name.chars().any(char::is_control) {
            return Err(AirportIoError::InvalidField {
                field: "name".to_owned(),
                reason: "must be non-empty and contain no control characters".to_owned(),
            });
        }
        if !self.latitude_deg.is_finite() || !(-90.0..=90.0).contains(&self.latitude_deg) {
            return Err(AirportIoError::InvalidField {
                field: "latitude_deg".to_owned(),
                reason: "must be finite and within [-90, 90] degrees".to_owned(),
            });
        }
        if !self.longitude_deg.is_finite() || !(-180.0..=180.0).contains(&self.longitude_deg) {
            return Err(AirportIoError::InvalidField {
                field: "longitude_deg".to_owned(),
                reason: "must be finite and within [-180, 180] degrees".to_owned(),
            });
        }
        for (field, value) in [
            ("isa_delta_c", self.isa_delta_c),
            ("altitude_m", self.altitude_m),
        ] {
            if !value.is_finite() {
                return Err(AirportIoError::InvalidField {
                    field: field.to_owned(),
                    reason: "must be finite".to_owned(),
                });
            }
        }
        if self.runway_lengths_m.is_empty()
            || self
                .runway_lengths_m
                .iter()
                .any(|length| !length.is_finite() || *length <= 0.0)
        {
            return Err(AirportIoError::InvalidField {
                field: "runway_lengths_m".to_owned(),
                reason: "must contain at least one finite positive length".to_owned(),
            });
        }
        for (field, value) in [
            ("declared_toda_m", self.declared_toda_m),
            ("declared_lda_m", self.declared_lda_m),
        ] {
            if value.is_some_and(|value| !value.is_finite() || value <= 0.0) {
                return Err(AirportIoError::InvalidField {
                    field: field.to_owned(),
                    reason: "must be absent or finite and positive".to_owned(),
                });
            }
        }
        for (field, value) in [
            ("provenance.source", self.provenance.source.as_deref()),
            ("provenance.note", self.provenance.note.as_deref()),
        ] {
            if value.is_some_and(|value| value.chars().any(char::is_control)) {
                return Err(AirportIoError::InvalidField {
                    field: field.to_owned(),
                    reason: "must contain no control characters".to_owned(),
                });
            }
        }
        Ok(())
    }
}

/// An actionable airport import, validation, or persistence failure.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AirportIoError {
    /// A field did not satisfy its physical or format contract.
    #[error("invalid airport field '{field}': {reason}")]
    InvalidField {
        /// Field name.
        field: String,
        /// Actionable reason for rejection.
        reason: String,
    },
    /// A text row was malformed.
    #[error("invalid airport {format} record at line {line}: {reason}")]
    InvalidLine {
        /// Input format.
        format: &'static str,
        /// One-based input line.
        line: usize,
        /// Actionable reason for rejection.
        reason: String,
    },
    /// A JSON value could not be decoded.
    #[error("invalid airport JSON: {0}")]
    Json(String),
    /// A record is duplicated inside an import or registry.
    #[error("duplicate custom airport '{0}'")]
    Duplicate(String),
    /// A custom airport would shadow a curated or registered airport.
    #[error("custom airport '{0}' conflicts with an existing airport")]
    Collision(String),
    /// A file could not be read.
    #[error("could not read airport file '{path}': {detail}")]
    Read {
        /// Display path.
        path: String,
        /// Underlying I/O message.
        detail: String,
    },
    /// A file could not be written.
    #[error("could not write airport file '{path}': {detail}")]
    Write {
        /// Display path.
        path: String,
        /// Underlying I/O message.
        detail: String,
    },
    /// The path extension is not supported.
    #[error("unsupported airport file extension for '{0}'; expected .dat or .json")]
    UnsupportedExtension(String),
}

mod dat;
mod json;
mod registry;

pub use dat::{export_file, import_file, parse_dat, parse_dat_with_source};
pub use json::{export_json, parse_json, parse_json_with_source, parse_value};
pub(crate) use registry::legacy_by_name_or_icao;
pub use registry::{
    find_custom, register_custom_airport, registered_custom_airports, replace_custom_airports,
};

fn normalize_and_validate(airports: &mut [CustomAirport]) -> Result<(), AirportIoError> {
    let mut seen = Vec::with_capacity(airports.len());
    for airport in airports {
        airport.validate_and_normalize()?;
        if seen.iter().any(|existing: &CustomAirport| {
            existing.icao.eq_ignore_ascii_case(&airport.icao)
                || existing.name.eq_ignore_ascii_case(&airport.name)
        }) {
            return Err(AirportIoError::Duplicate(airport.icao.clone()));
        }
        seen.push(airport.clone());
    }
    Ok(())
}

fn annotate_import(
    airports: &mut [CustomAirport],
    kind: AirportProvenanceKind,
    source: &str,
) -> Result<(), AirportIoError> {
    let source = (!source.trim().is_empty()).then(|| source.trim().to_owned());
    for airport in airports {
        airport.provenance.kind = kind;
        if airport.provenance.source.is_none() {
            airport.provenance.source = source.clone();
        }
        airport.validate_and_normalize()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(icao: &str) -> CustomAirport {
        CustomAirport {
            icao: icao.to_owned(),
            name: format!("Custom {icao}"),
            latitude_deg: 40.0,
            longitude_deg: -3.0,
            isa_delta_c: 8.0,
            altitude_m: 610.0,
            runway_lengths_m: vec![2400.0, 1800.0],
            declared_toda_m: None,
            declared_lda_m: None,
            provenance: AirportProvenance::default(),
        }
    }

    #[test]
    fn dat_round_trip_preserves_physical_runways_and_provenance() {
        let mut airport = sample("ZZ01");
        airport.provenance.source = Some("survey.dat".to_owned());
        let text = super::dat::format_dat(&[airport.clone()]);
        let parsed = parse_dat_with_source(&text, "survey.dat").expect("dat parses");
        assert_eq!(parsed[0].icao, "ZZ01");
        assert_eq!(parsed[0].runway_lengths_m, airport.runway_lengths_m);
        assert_eq!(parsed[0].provenance.kind, AirportProvenanceKind::DatImport);
    }

    #[test]
    fn json_round_trip_has_a_versioned_envelope() {
        let text = export_json(&[sample("ZZ02")]).expect("json exports");
        let parsed = parse_json(&text).expect("json parses");
        assert_eq!(parsed[0].icao, "ZZ02");
        assert!(text.contains("\"version\": 1"));
    }

    #[test]
    fn physical_lengths_supply_conservative_legacy_distances() {
        let airport = sample("ZZ03");
        replace_custom_airports(vec![airport]).expect("registry accepts sample");
        let legacy = legacy_by_name_or_icao("ZZ03").expect("legacy lookup");
        assert_eq!(legacy.toda_m, 2400.0);
        assert_eq!(legacy.lda_m, 2400.0);
        replace_custom_airports(Vec::new()).expect("registry clears");
    }

    #[test]
    fn invalid_coordinates_are_rejected_with_a_field_name() {
        let mut airport = sample("ZZ04");
        airport.latitude_deg = 100.0;
        let error = airport.validate_and_normalize().unwrap_err();
        assert!(
            matches!(error, AirportIoError::InvalidField { field, .. } if field == "latitude_deg")
        );
    }

    #[test]
    fn an_import_source_with_control_text_is_rejected_after_annotation() {
        let text = super::dat::format_dat(&[sample("ZZ05")]);
        let error = parse_dat_with_source(&text, "survey\nforged").unwrap_err();
        assert!(matches!(
            error,
            AirportIoError::InvalidField { field, .. } if field == "provenance.source"
        ));
    }
}
