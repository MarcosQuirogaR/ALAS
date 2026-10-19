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
    /// The record these fields were loaded from, if any. While the fields still
    /// equal it, saving keeps its provenance and declared distances instead of
    /// re-registering it as user-entered.
    pub origin: Option<CustomAirport>,
}

impl CustomAirportDraft {
    /// Fill every field from a registered or freshly imported airport, so the
    /// value-entry fields show exactly what the record holds.
    pub fn from_airport(airport: &CustomAirport) -> Self {
        Self {
            icao: airport.icao.clone(),
            name: airport.name.clone(),
            latitude_deg: airport.latitude_deg.to_string(),
            longitude_deg: airport.longitude_deg.to_string(),
            isa_delta_c: airport.isa_delta_c.to_string(),
            altitude_m: airport.altitude_m.to_string(),
            runway_lengths_m: airport
                .runway_lengths_m
                .iter()
                .map(f64::to_string)
                .collect::<Vec<_>>()
                .join(", "),
            origin: Some(airport.clone()),
        }
    }

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
        if let Some(origin) = &self.origin {
            let unchanged = origin.icao == airport.icao
                && origin.name == airport.name
                && origin.latitude_deg == airport.latitude_deg
                && origin.longitude_deg == airport.longitude_deg
                && origin.isa_delta_c == airport.isa_delta_c
                && origin.altitude_m == airport.altitude_m
                && origin.runway_lengths_m == airport.runway_lengths_m;
            if unchanged {
                airport.declared_toda_m = origin.declared_toda_m;
                airport.declared_lda_m = origin.declared_lda_m;
                airport.provenance = origin.provenance.clone();
            }
        }
        Ok(airport)
    }
}

fn parse_number(label: &str, value: &str) -> Result<f64, String> {
    value
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("{label} must be a finite number"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn imported() -> CustomAirport {
        CustomAirport {
            icao: "LEXX".to_owned(),
            name: "Test Field".to_owned(),
            latitude_deg: 40.5,
            longitude_deg: -3.25,
            isa_delta_c: 15.0,
            altitude_m: 610.0,
            runway_lengths_m: vec![3500.0, 4100.5],
            declared_toda_m: Some(3600.0),
            declared_lda_m: None,
            provenance: AirportProvenance {
                kind: AirportProvenanceKind::DatImport,
                source: Some("lexx.dat".to_owned()),
                note: None,
            },
        }
    }

    #[test]
    fn an_unmodified_imported_draft_keeps_its_provenance_and_an_edited_one_does_not() {
        let mut draft = CustomAirportDraft::from_airport(&imported());
        let kept = draft.to_airport().expect("valid");
        assert_eq!(kept.provenance.kind, AirportProvenanceKind::DatImport);
        assert_eq!(kept.provenance.source.as_deref(), Some("lexx.dat"));
        assert_eq!(kept.declared_toda_m, Some(3600.0));
        draft.altitude_m = "611".to_owned();
        let edited = draft.to_airport().expect("valid");
        assert_eq!(edited.provenance.kind, AirportProvenanceKind::UserEntered);
        assert_eq!(edited.declared_toda_m, None);
    }

    #[test]
    fn draft_shows_every_imported_value_and_round_trips() {
        let draft = CustomAirportDraft::from_airport(&imported());
        assert_eq!(draft.altitude_m, "610");
        assert_eq!(draft.isa_delta_c, "15");
        assert_eq!(draft.runway_lengths_m, "3500, 4100.5");
        let airport = draft.to_airport().expect("imported values stay valid");
        assert_eq!(airport.runway_lengths_m, vec![3500.0, 4100.5]);
        assert_eq!(airport.altitude_m, 610.0);
    }
}
