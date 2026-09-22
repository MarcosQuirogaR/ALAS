// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the routing parity test reads its fixture with, and how it compares one
//! route against another.
//!
//! The record types mirror `gen_route.py`'s own output shape, and
//! [`compare_route`] is the one place that decides which parts of a route are
//! discrete and which are numerical, so every tier is held to the same
//! standard rather than to whichever assertions its own test happened to make.

// A test binary's failed unwrap or expect is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]
// Each test binary compiles its own copy and uses the part it needs.
#![allow(dead_code)]

use std::path::PathBuf;

use alas_config::airports::Airport;
use alas_route::route::Route;
use alas_testkit::Comparison;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct AirportRecord {
    pub name: String,
    pub icao: String,
    pub elevation_m: f64,
    pub toda_m: f64,
    pub lda_m: f64,
    pub isa_deviation_c: f64,
    pub notes: String,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
}

impl AirportRecord {
    pub fn airport(&self) -> Airport {
        Airport {
            name: self.name.clone(),
            icao: self.icao.clone(),
            elevation_m: self.elevation_m,
            toda_m: self.toda_m,
            lda_m: self.lda_m,
            isa_deviation_c: self.isa_deviation_c,
            notes: self.notes.clone(),
            latitude_deg: self.latitude_deg,
            longitude_deg: self.longitude_deg,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct WaypointRecord {
    pub lat: f64,
    pub lon: f64,
    pub alt_m: f64,
    pub ident: String,
}

#[derive(Debug, Deserialize)]
pub struct RouteRecord {
    pub source: String,
    pub waypoints: Vec<WaypointRecord>,
    pub cumulative_distance_m: Vec<f64>,
    pub total_distance_m: f64,
    pub origin_airport: Option<AirportRecord>,
    pub dest_airport: Option<AirportRecord>,
}

/// The inputs the generator authored and checked in beside the fixture.
pub fn inputs_dir() -> PathBuf {
    alas_testkit::golden_dir().join("route").join("inputs")
}

pub fn read_input(name: &str) -> String {
    let path = inputs_dir().join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("the authored input {} is missing: {error}", path.display()))
}

/// Compare a route against what the reference produced for it.
pub fn compare_route(
    discrete: &mut Comparison,
    numeric: &mut Comparison,
    at: &dyn Fn(&str) -> String,
    actual: &Route,
    expected: &RouteRecord,
) {
    discrete
        .exact(
            &at("source"),
            &actual.source.as_str().to_owned(),
            &expected.source,
        )
        .exact(
            &at("waypoints.len"),
            &actual.waypoints.len(),
            &expected.waypoints.len(),
        )
        .exact(
            &at("idents"),
            &actual
                .waypoints
                .iter()
                .map(|waypoint| waypoint.ident.clone())
                .collect::<Vec<String>>(),
            &expected
                .waypoints
                .iter()
                .map(|waypoint| waypoint.ident.clone())
                .collect::<Vec<String>>(),
        );

    for (index, (waypoint, record)) in actual.waypoints.iter().zip(&expected.waypoints).enumerate()
    {
        numeric
            .scalar(
                &at(&format!("waypoints[{index}].lat")),
                waypoint.lat,
                record.lat,
            )
            .scalar(
                &at(&format!("waypoints[{index}].lon")),
                waypoint.lon,
                record.lon,
            )
            .scalar(
                &at(&format!("waypoints[{index}].alt_m")),
                waypoint.alt_m,
                record.alt_m,
            );
    }

    numeric
        .slice(
            &at("cumulative_distance_m"),
            &actual.cumulative_distance_m(),
            &expected.cumulative_distance_m,
        )
        .scalar(
            &at("total_distance_m"),
            actual.total_distance_m(),
            expected.total_distance_m,
        );

    for (what, actual, expected) in [
        (
            "origin_airport",
            &actual.origin_airport,
            &expected.origin_airport,
        ),
        ("dest_airport", &actual.dest_airport, &expected.dest_airport),
    ] {
        discrete.exact(
            &at(&format!("{what}.present")),
            &actual.is_some(),
            &expected.is_some(),
        );
        let (Some(actual), Some(expected)) = (actual, expected) else {
            continue;
        };
        discrete
            .exact(&at(&format!("{what}.icao")), &actual.icao, &expected.icao)
            .exact(&at(&format!("{what}.name")), &actual.name, &expected.name)
            .exact(
                &at(&format!("{what}.notes")),
                &actual.notes,
                &expected.notes,
            );
        numeric
            .scalar(
                &at(&format!("{what}.elevation_m")),
                actual.elevation_m,
                expected.elevation_m,
            )
            .scalar(
                &at(&format!("{what}.toda_m")),
                actual.toda_m,
                expected.toda_m,
            )
            .scalar(&at(&format!("{what}.lda_m")), actual.lda_m, expected.lda_m)
            .scalar(
                &at(&format!("{what}.isa_deviation_c")),
                actual.isa_deviation_c,
                expected.isa_deviation_c,
            )
            .scalar(
                &at(&format!("{what}.latitude_deg")),
                actual.latitude_deg,
                expected.latitude_deg,
            )
            .scalar(
                &at(&format!("{what}.longitude_deg")),
                actual.longitude_deg,
                expected.longitude_deg,
            );
    }
}
