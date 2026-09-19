// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Text draft used by the custom-airport editor.

use alas_config::airport_io::{AirportProvenance, AirportProvenanceKind, CustomAirport};

/// String-backed airport fields so an incomplete edit does not panic or
/// replace the last valid application state.
#[derive(Debug, Clone, Default)]
pub struct CustomAirportDraft {
    /// Four-character airport identifier.
    pub icao: String,
    /// Display name.
    pub name: String,
    /// Latitude in degrees.
    pub latitude_deg: String,
    /// Longitude in degrees.
    pub longitude_deg: String,
    /// ISA temperature delta in degrees Celsius.
    pub isa_delta_c: String,
    /// Field altitude in metres.
    pub altitude_m: String,
    /// Comma- or space-separated physical runway lengths in metres.
    pub runway_lengths_m: String,
    /// Optional declared TODA in metres.
    pub declared_toda_m: String,
    /// Optional declared LDA in metres.
    pub declared_lda_m: String,
}

impl CustomAirportDraft {
    /// Convert the draft into a validated custom airport.
    pub fn to_airport(&self) -> Result<CustomAirport, String> {
        let mut airport = CustomAirport {
            icao: self.icao.clone(),
            name: self.name.clone(),
            latitude_deg: parse_number("latitude", &self.latitude_deg)?,
            longitude_deg: parse_number("longitude", &self.longitude_deg)?,
            isa_delta_c: parse_number("ISA delta", &self.isa_delta_c)?,
            altitude_m: parse_number("altitude", &self.altitude_m)?,
            runway_lengths_m: self
                .runway_lengths_m
                .split(|character: char| character == ',' || character.is_whitespace())
                .filter(|value| !value.is_empty())
                .map(|value| parse_number("runway length", value))
                .collect::<Result<Vec<_>, _>>()?,
            declared_toda_m: parse_optional("declared TODA", &self.declared_toda_m)?,
            declared_lda_m: parse_optional("declared LDA", &self.declared_lda_m)?,
            provenance: AirportProvenance {
                kind: AirportProvenanceKind::UserEntered,
                source: Some("ALAS custom-airport editor".to_owned()),
                note: None,
            },
        };
        airport
            .validate_and_normalize()
            .map_err(|error| error.to_string())?;
        Ok(airport)
    }
}

fn parse_number(label: &str, value: &str) -> Result<f64, String> {
    value
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("{label} must be a finite number"))
}

fn parse_optional(label: &str, value: &str) -> Result<Option<f64>, String> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        parse_number(label, value).map(Some)
    }
}
