// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Airport data with an explicit source for every field.
//!
//! The curated table in [`crate::airports`] contains planning distances copied
//! from published airport material. The optional OurAirports import contains
//! physical runway lengths, which are useful for routing and screening but do
//! not become TORA/TODA/ASDA/LDA by implication.

use serde::{Deserialize, Serialize};

use crate::airport_io::{self, CustomAirport};
use crate::airports::{self, Airport};

const OURAIRPORTS_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/data/airports_ourairports.json"
));

/// Origin of one airport datum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldSource {
    /// Curated value tied to a published airport chart or airport data sheet.
    CuratedChartTable,
    /// Public-domain OurAirports value, normally a physical runway length.
    OurAirportsPublicDomain,
    /// Value explicitly supplied by the user.
    UserOverride,
    /// No value is available.
    Missing,
}

/// Whether a runway length can be used as an operational field distance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunwayDataKind {
    /// TORA/TODA/ASDA/LDA or an equivalent declared performance distance.
    DeclaredOperationalDistance,
    /// Physical runway length only; not a declared operational distance.
    PhysicalRunwayLength,
    /// No runway distance is available.
    Missing,
}

/// One field and the source that supplied it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirportField<T> {
    /// Value, absent when the source did not provide it.
    pub value: Option<T>,
    /// Source of the value.
    pub source: FieldSource,
}

impl<T> AirportField<T> {
    /// Construct an explicitly missing field.  Keeping the source as
    /// [`FieldSource::Missing`] makes an absent datum distinguishable from a
    /// value that was supplied by a public import and later rejected for the
    /// requested calculation.
    pub fn missing() -> Self {
        Self {
            value: None,
            source: FieldSource::Missing,
        }
    }
}

/// Provenance of the bundled, deliberately small OurAirports snapshot.
///
/// The source is public-domain open data, but the provider disclaims accuracy
/// and fitness for use.  The runway column is a physical runway length in m;
/// it is never treated as TORA, TODA, ASDA or LDA by this crate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetProvenance {
    /// Dataset provider.
    pub provider: String,
    /// Airport-record download URL.
    pub airport_url: String,
    /// Runway-record download URL.
    pub runway_url: String,
    /// Provider's field documentation.
    pub documentation_url: String,
    /// UTC date on which the local snapshot was retrieved.
    pub retrieved_utc: String,
    /// License and use limitation copied into the run metadata.
    pub license_and_limitation: String,
}

impl Default for DatasetProvenance {
    fn default() -> Self {
        Self {
            provider: "OurAirports".to_owned(),
            airport_url:
                "https://davidmegginson.github.io/ourairports-data/airports.csv".to_owned(),
            runway_url:
                "https://davidmegginson.github.io/ourairports-data/runways.csv".to_owned(),
            documentation_url: "https://ourairports.com/help/data-dictionary.html".to_owned(),
            retrieved_utc: "unknown".to_owned(),
            license_and_limitation:
                "Public Domain; provider gives no guarantee of accuracy or fitness for use. Physical runway length only, never declared performance distance.".to_owned(),
        }
    }
}

/// Airport data after source resolution, before a performance model decides
/// which fields it can use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProvenancedAirport {
    /// ICAO identifier, when known.
    pub icao: AirportField<String>,
    /// Human-readable name, when known.
    pub name: AirportField<String>,
    /// Elevation above mean sea level, m.
    pub elevation_m: AirportField<f64>,
    /// Takeoff distance available, m, only when explicitly declared.
    pub toda_m: AirportField<f64>,
    /// Landing distance available, m, only when explicitly declared.
    pub lda_m: AirportField<f64>,
    /// Reference latitude, positive north, deg.
    pub latitude_deg: AirportField<f64>,
    /// Reference longitude, positive east, deg.
    pub longitude_deg: AirportField<f64>,
    /// ISA temperature deviation at the field, in degrees Celsius.
    pub isa_deviation_c: AirportField<f64>,
    /// Meaning of the runway values in this record.
    pub runway_data_kind: RunwayDataKind,
}

impl ProvenancedAirport {
    /// Whether all fields needed for a route and declared field-performance
    /// check are present and finite.
    pub fn is_complete_for_declared_performance(&self) -> bool {
        self.runway_data_kind == RunwayDataKind::DeclaredOperationalDistance
            && finite_elevation(self.elevation_m.value)
            && finite_positive(self.toda_m.value)
            && finite_positive(self.lda_m.value)
            && finite_latitude(self.latitude_deg.value)
            && finite_longitude(self.longitude_deg.value)
    }

    /// Convert the record to the legacy airport value only when its required
    /// fields are complete. Physical-runway records intentionally return
    /// `None`; callers may opt into a planning approximation explicitly.
    pub fn declared_airport(&self) -> Option<Airport> {
        if !self.is_complete_for_declared_performance() {
            return None;
        }
        Some(Airport {
            name: self.name.value.clone()?,
            icao: self.icao.value.clone()?,
            elevation_m: self.elevation_m.value?,
            toda_m: self.toda_m.value?,
            lda_m: self.lda_m.value?,
            isa_deviation_c: 0.0,
            notes: "Resolved with per-field provenance".to_owned(),
            latitude_deg: self.latitude_deg.value?,
            longitude_deg: self.longitude_deg.value?,
        })
    }
}

/// Why an airport could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AirportDataError {
    /// No curated or imported record matched the user key.
    #[error("airport '{0}' is unresolved; no airport data record matched")]
    Unknown(String),
    /// A record exists but cannot support the requested operation.
    #[error("airport '{0}' has incomplete or non-declared field-performance data")]
    Incomplete(String),
}

/// Resolve a display name or ICAO code from the curated table, then the local
/// OurAirports import. Resolution never invents missing fields.
pub fn resolve(name_or_icao: &str) -> Result<ProvenancedAirport, AirportDataError> {
    if let Some(airport) = airport_io::find_custom(name_or_icao) {
        return Ok(from_custom(&airport));
    }
    if let Ok(airport) = airports::get(name_or_icao) {
        return Ok(from_curated(airport));
    }
    imported_records()
        .iter()
        .find(|record| record.icao == name_or_icao || record.name == name_or_icao)
        .map(from_import)
        .ok_or_else(|| AirportDataError::Unknown(name_or_icao.to_owned()))
}

/// Resolve and require declared operational distances.
pub fn resolve_for_declared_performance(
    name_or_icao: &str,
) -> Result<ProvenancedAirport, AirportDataError> {
    let record = resolve(name_or_icao)?;
    if record.is_complete_for_declared_performance() {
        Ok(record)
    } else {
        Err(AirportDataError::Incomplete(name_or_icao.to_owned()))
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ImportedAirport {
    icao: String,
    name: String,
    elevation_m: Option<f64>,
    physical_runway_length_m: Option<f64>,
    latitude_deg: Option<f64>,
    longitude_deg: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ImportedTable {
    #[serde(default)]
    source: DatasetProvenance,
    airports: Vec<ImportedAirport>,
}

fn imported_table() -> &'static ImportedTable {
    use std::sync::OnceLock;
    static TABLE: OnceLock<ImportedTable> = OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str::<ImportedTable>(OURAIRPORTS_JSON).unwrap_or_else(|_| ImportedTable {
            source: DatasetProvenance::default(),
            airports: Vec::new(),
        })
    })
}

/// Provenance metadata for the bundled imported records.
pub fn dataset_provenance() -> &'static DatasetProvenance {
    &imported_table().source
}

fn imported_records() -> &'static [ImportedAirport] {
    &imported_table().airports
}

fn from_curated(airport: &Airport) -> ProvenancedAirport {
    let source = FieldSource::CuratedChartTable;
    ProvenancedAirport {
        icao: field(airport.icao.clone(), source),
        name: field(airport.name.clone(), source),
        elevation_m: field(airport.elevation_m, source),
        toda_m: field(airport.toda_m, source),
        lda_m: field(airport.lda_m, source),
        latitude_deg: field(airport.latitude_deg, source),
        longitude_deg: field(airport.longitude_deg, source),
        isa_deviation_c: field(airport.isa_deviation_c, source),
        runway_data_kind: RunwayDataKind::DeclaredOperationalDistance,
    }
}

fn from_import(record: &ImportedAirport) -> ProvenancedAirport {
    let source = FieldSource::OurAirportsPublicDomain;
    ProvenancedAirport {
        icao: field(record.icao.clone(), source),
        name: field(record.name.clone(), source),
        elevation_m: optional_field(record.elevation_m, source),
        toda_m: optional_field(record.physical_runway_length_m, source),
        lda_m: optional_field(record.physical_runway_length_m, source),
        latitude_deg: optional_field(record.latitude_deg, source),
        longitude_deg: optional_field(record.longitude_deg, source),
        isa_deviation_c: AirportField::missing(),
        runway_data_kind: if record.physical_runway_length_m.is_some() {
            RunwayDataKind::PhysicalRunwayLength
        } else {
            RunwayDataKind::Missing
        },
    }
}

fn from_custom(airport: &CustomAirport) -> ProvenancedAirport {
    let source = FieldSource::UserOverride;
    let declared = airport
        .declared_toda_m
        .zip(airport.declared_lda_m)
        .filter(|(toda, lda)| toda.is_finite() && *toda > 0.0 && lda.is_finite() && *lda > 0.0);
    let physical = airport
        .runway_lengths_m
        .iter()
        .copied()
        .filter(|length| length.is_finite() && *length > 0.0)
        .reduce(f64::max);
    let (toda_m, lda_m, runway_data_kind) = match declared {
        Some((toda, lda)) => (
            field(toda, source),
            field(lda, source),
            RunwayDataKind::DeclaredOperationalDistance,
        ),
        None => match physical {
            Some(length) => (
                field(length, source),
                field(length, source),
                RunwayDataKind::PhysicalRunwayLength,
            ),
            None => (
                AirportField::missing(),
                AirportField::missing(),
                RunwayDataKind::Missing,
            ),
        },
    };
    ProvenancedAirport {
        icao: field(airport.icao.clone(), source),
        name: field(airport.name.clone(), source),
        elevation_m: field(airport.altitude_m, source),
        toda_m,
        lda_m,
        latitude_deg: field(airport.latitude_deg, source),
        longitude_deg: field(airport.longitude_deg, source),
        isa_deviation_c: field(airport.isa_delta_c, source),
        runway_data_kind,
    }
}

fn field<T>(value: T, source: FieldSource) -> AirportField<T> {
    AirportField {
        value: Some(value),
        source,
    }
}

fn optional_field<T>(value: Option<T>, source: FieldSource) -> AirportField<T> {
    AirportField { value, source }
}

fn finite_positive(value: Option<f64>) -> bool {
    value.is_some_and(|value| value.is_finite() && value > 0.0)
}

fn finite_elevation(value: Option<f64>) -> bool {
    value.is_some_and(f64::is_finite)
}

fn finite_latitude(value: Option<f64>) -> bool {
    value.is_some_and(|value| value.is_finite() && (-90.0..=90.0).contains(&value))
}

fn finite_longitude(value: Option<f64>) -> bool {
    value.is_some_and(|value| value.is_finite() && (-180.0..=180.0).contains(&value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curated_fields_keep_declared_distance_provenance() {
        let airport = resolve("EGLL").unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            airport.runway_data_kind,
            RunwayDataKind::DeclaredOperationalDistance
        );
        assert_eq!(airport.elevation_m.source, FieldSource::CuratedChartTable);
        assert!(airport.declared_airport().is_some());
    }

    #[test]
    fn imported_physical_lengths_are_not_promoted_to_declared_distances() {
        let airport = resolve("LEBL").unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            airport.runway_data_kind,
            RunwayDataKind::PhysicalRunwayLength
        );
        assert!(airport.declared_airport().is_none());
        assert!(matches!(
            resolve_for_declared_performance("LEBL"),
            Err(AirportDataError::Incomplete(_))
        ));
    }

    #[test]
    fn unknown_airports_remain_explicitly_unresolved() {
        assert!(matches!(resolve("ZZZZ"), Err(AirportDataError::Unknown(_))));
    }

    #[test]
    fn missing_field_has_missing_provenance() {
        let value: AirportField<f64> = AirportField::missing();
        assert_eq!(value.value, None);
        assert_eq!(value.source, FieldSource::Missing);
    }
}
