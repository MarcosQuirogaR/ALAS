// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/airports.py
// Reference: alas @ rust-port-baseline.

//! Aerodromes the field-performance check and the route are evaluated at.
//!
//! Ten major hubs and ten airports that are hard to operate out of: high
//! elevation, short runways, hot days, because a design that meets its
//! field length at sea level on a standard day may not meet it anywhere
//! interesting. The figures come from published aerodrome charts.
//!
//! As with [`crate::materials`] and [`crate::engines`], the table is data
//! rather than code: `data/airports.json`, embedded and parsed once, with
//! `golden/config/airports.json` as the parity copy.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

// See `crate::materials` for why the manifest directory is spelled out.
const AIRPORTS_JSON: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/airports.json"));

/// The code a hand-entered aerodrome carries, since it has no real one.
const CUSTOM_ICAO: &str = "CUST";

/// An aerodrome that was asked for and is not in the table.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("airport '{0}' not found in the database")]
pub struct UnknownAirport(pub String);

/// One aerodrome's performance-relevant figures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Airport {
    /// Display name, including the code in parentheses.
    pub name: String,
    /// Four-letter ICAO code.
    pub icao: String,
    /// Field elevation above mean sea level, in metres.
    pub elevation_m: f64,
    /// Takeoff distance available on the longest runway, in metres.
    pub toda_m: f64,
    /// Landing distance available on the longest runway, in metres.
    pub lda_m: f64,
    /// How far above the standard atmosphere a hot day here runs, in kelvin
    /// of temperature difference.
    pub isa_deviation_c: f64,
    /// What makes this aerodrome worth having in the table.
    pub notes: String,
    /// Reference-point latitude, positive north, used for routing.
    pub latitude_deg: f64,
    /// Reference-point longitude, positive east, used for routing.
    pub longitude_deg: f64,
}

impl Airport {
    /// A one-off aerodrome the user entered by hand.
    ///
    /// Carries the `CUST` code rather than a real one, which is how anything
    /// downstream tells a hand-entered field from a table entry.
    pub fn custom(
        name: impl Into<String>,
        elevation_m: f64,
        toda_m: f64,
        lda_m: f64,
        isa_deviation_c: f64,
        latitude_deg: f64,
        longitude_deg: f64,
    ) -> Self {
        Self {
            name: name.into(),
            icao: CUSTOM_ICAO.to_owned(),
            elevation_m,
            toda_m,
            lda_m,
            isa_deviation_c,
            notes: "Custom entry".to_owned(),
            latitude_deg,
            longitude_deg,
        }
    }
}

/// Every curated aerodrome, in the order the table lists them.
pub fn database() -> &'static [Airport] {
    static DATABASE: OnceLock<Vec<Airport>> = OnceLock::new();
    DATABASE.get_or_init(|| parse().airports)
}

/// Return the curated table plus any user-registered airport records.
///
/// The returned vector is a snapshot so callers may safely use it to populate
/// a selector while another workspace import replaces the registry.
pub fn database_with_custom() -> Vec<Airport> {
    let mut airports = database().to_vec();
    airports.extend(
        crate::airport_io::registered_custom_airports()
            .into_iter()
            .map(|airport| Airport {
                name: airport.name,
                icao: airport.icao,
                elevation_m: airport.altitude_m,
                toda_m: airport.declared_toda_m.unwrap_or(0.0),
                lda_m: airport.declared_lda_m.unwrap_or(0.0),
                isa_deviation_c: airport.isa_delta_c,
                notes: "Custom entry; physical runway lengths remain provenance-only".to_owned(),
                latitude_deg: airport.latitude_deg,
                longitude_deg: airport.longitude_deg,
            }),
    );
    airports
}

/// Look one aerodrome up by display name or by ICAO code.
///
/// Both are accepted because the configuration stores the display name while
/// a route stores the code, and requiring callers to know which they hold
/// would put the same lookup in two places.
///
/// # Errors
///
/// [`UnknownAirport`] when neither matches. Upstream raises for the same
/// input.
pub fn get(name_or_icao: &str) -> Result<&'static Airport, UnknownAirport> {
    if let Some(airport) = database()
        .iter()
        .find(|airport| airport.name == name_or_icao || airport.icao == name_or_icao)
    {
        return Ok(airport);
    }
    crate::airport_io::legacy_by_name_or_icao(name_or_icao)
        .ok_or_else(|| UnknownAirport(name_or_icao.to_owned()))
}

#[derive(Deserialize)]
struct Table {
    airports: Vec<Airport>,
}

/// Parse the embedded table. See [`crate::materials`] for why a parse failure
/// degrades rather than panicking.
fn parse() -> Table {
    serde_json::from_str(AIRPORTS_JSON).unwrap_or_else(|error| {
        tracing::error!(%error, "crates/alas-config/data/airports.json failed to parse");
        Table {
            airports: Vec::new(),
        }
    })
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_table_parses_into_the_curated_aerodromes() {
        assert_eq!(database().len(), 28);
    }

    #[test]
    fn an_aerodrome_is_found_by_either_its_name_or_its_code() {
        let by_code = get("EGLL").unwrap();
        let by_name = get(&by_code.name).unwrap();
        assert_eq!(by_code, by_name);
    }

    #[test]
    fn an_unknown_aerodrome_is_an_error_not_a_panic() {
        assert_eq!(get("ZZZZ").unwrap_err(), UnknownAirport("ZZZZ".to_owned()));
    }

    #[test]
    fn every_code_is_four_letters_and_unique() {
        let mut codes: Vec<&str> = database().iter().map(|a| a.icao.as_str()).collect();
        for code in &codes {
            assert_eq!(code.len(), 4, "{code} is not a four-letter ICAO code");
        }
        codes.sort_unstable();
        let count = codes.len();
        codes.dedup();
        assert_eq!(codes.len(), count, "two entries share an ICAO code");
    }

    #[test]
    fn every_aerodrome_has_a_runway_and_a_position_on_the_globe() {
        for airport in database() {
            assert!(
                airport.toda_m > 0.0,
                "{}: no takeoff distance",
                airport.name
            );
            assert!(airport.lda_m > 0.0, "{}: no landing distance", airport.name);
            assert!(
                (-90.0..=90.0).contains(&airport.latitude_deg),
                "{}: latitude {} is off the globe",
                airport.name,
                airport.latitude_deg
            );
            assert!(
                (-180.0..=180.0).contains(&airport.longitude_deg),
                "{}: longitude {} is off the globe",
                airport.name,
                airport.longitude_deg
            );
        }
    }

    #[test]
    fn the_table_includes_aerodromes_that_are_hard_to_leave() {
        // Half the point of the table is the demanding half; a table of sea
        // level hubs would let a design pass field length everywhere.
        let highest = database()
            .iter()
            .map(|airport| airport.elevation_m)
            .fold(f64::MIN, f64::max);
        assert!(highest > 2000.0, "the highest field is only {highest} m up");
    }

    #[test]
    fn a_hand_entered_aerodrome_is_marked_as_one() {
        let custom = Airport::custom("Somewhere", 100.0, 2500.0, 2400.0, 0.0, 10.0, 20.0);
        assert_eq!(custom.icao, "CUST");
        assert_eq!(custom.notes, "Custom entry");
    }
}
