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
            // Operational TODA/LDA values are derived by the application from
            // the physical runway lengths; the editor never accepts declared
            // values that would look like user-supplied regulatory data.
            declared_toda_m: None,
            declared_lda_m: None,
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
