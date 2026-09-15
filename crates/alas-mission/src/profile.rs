// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from the legacy mission-request integration (`build_mission_request`).
// Reference: alas @ rust-port-baseline.

//! The mission half of the mission request.
//!
//! Cruise altitude comes from the design requirements; the departure and
//! arrival field elevations and the departure ISA deviation come from the two
//! airports the route runs between; the route distance is supplied by whoever
//! computed the route (see `alas-route`). The flown profile, every climb,
//! cruise and descent speed and rate, is the user-editable
//! [`MissionProfileConfig`] carried straight through, because those numbers are
//! a configuration the operator tunes rather than anything this builder
//! derives.
//!
//! One field the reference deliberately does *not* read is worth naming, since
//! its presence in the vehicle request invites the assumption: cruise Mach is
//! not part of the mission request. Every cruise segment's air speed is an
//! explicit true airspeed taken from the profile above, never derived from
//! Mach; the vehicle request carries its own cruise Mach for engine sizing, and
//! the two do not meet here.

use alas_config::airports::Airport;
use alas_config::{AlasConfig, MissionProfileConfig};
use serde::Serialize;

/// The mission half of the request handed to the segment network.
///
/// The field names are the JSON keys the reference's subprocess boundary used,
/// so the document this serializes to is the one the mission evaluator reads.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MissionRequest {
    /// `"{origin_icao}_to_{dest_icao}"`, the tag the mission is logged under.
    pub mission_tag: String,
    /// The design cruise altitude, in metres.
    pub cruise_altitude_m: f64,
    /// The departure field's elevation, in metres.
    pub departure_elevation_m: f64,
    /// The arrival field's elevation, in metres.
    pub arrival_elevation_m: f64,
    /// The departure field's ISA temperature deviation, in Celsius.
    pub departure_isa_deviation_c: f64,
    /// The total route distance, in metres, after which the profile's climb
    /// and descent legs leave a remainder for the cruise legs.
    pub route_distance_m: f64,
    /// The flown speed, rate and altitude schedule, carried through unchanged.
    pub profile: MissionProfileConfig,
}

/// Build the mission half of the request for a route between two airports.
///
/// `route_distance_m` is supplied rather than computed here: the great-circle
/// or filed distance is `alas-route`'s concern, and the cruise legs split
/// whatever it hands in.
pub fn build_mission_request(
    config: &AlasConfig,
    origin: &Airport,
    dest: &Airport,
    route_distance_m: f64,
) -> MissionRequest {
    let req = &config.requirements;
    MissionRequest {
        mission_tag: format!("{}_to_{}", origin.icao, dest.icao),
        cruise_altitude_m: req.cruise_altitude_m,
        departure_elevation_m: origin.elevation_m,
        arrival_elevation_m: dest.elevation_m,
        departure_isa_deviation_c: origin.isa_deviation_c,
        route_distance_m,
        profile: config.mission.profile.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn airport(icao: &str, elevation_m: f64, isa_deviation_c: f64) -> Airport {
        Airport {
            name: icao.to_owned(),
            icao: icao.to_owned(),
            elevation_m,
            toda_m: 0.0,
            lda_m: 0.0,
            isa_deviation_c,
            notes: String::new(),
            latitude_deg: 0.0,
            longitude_deg: 0.0,
        }
    }

    // What parity cannot see: the tag is assembled from the two ICAO codes in
    // departure-then-arrival order, so a request whose endpoints were swapped
    // reads differently even when every distance is the same.
    #[test]
    fn mission_tag_reads_origin_then_destination() {
        let config = AlasConfig::default();
        let request = build_mission_request(
            &config,
            &airport("LEMD", 0.0, 0.0),
            &airport("EGLL", 0.0, 0.0),
            1000.0,
        );
        assert_eq!(request.mission_tag, "LEMD_to_EGLL");
    }

    // The departure ISA deviation and elevation are read off the *origin*, and
    // the arrival elevation off the *destination*: a builder that read either
    // from the wrong airport would still produce a well-formed request.
    #[test]
    fn elevations_and_deviation_come_from_the_named_airports() {
        let config = AlasConfig::default();
        let request = build_mission_request(
            &config,
            &airport("AAAA", 610.0, 12.0),
            &airport("BBBB", 4.0, -3.0),
            4_242_424.0,
        );
        assert_eq!(request.departure_elevation_m, 610.0);
        assert_eq!(request.departure_isa_deviation_c, 12.0);
        assert_eq!(request.arrival_elevation_m, 4.0);
        assert_eq!(request.route_distance_m, 4_242_424.0);
    }
}
