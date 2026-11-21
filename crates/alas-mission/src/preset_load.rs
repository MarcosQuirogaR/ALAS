// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one way a configuration document that names a preset is loaded.
//!
//! The desktop interface, the command line and the server all build their
//! starting case through [`load_document`], so the same preset and settings
//! describe the same analysed case in each of them. A preset's stored
//! operational profile is a generic three-leg schedule written for the
//! design range; the profile a case flies is the route-derived one
//! ([`propose_profile_for_route`] and [`configure_cruise_legs`]), exactly as
//! the Inputs card proposes it. A document that gives its own
//! `mission.profile` keeps it.

use alas_config::cabin::annotate_flops_cabin_resolution;
use alas_config::{airport_dataset, airports::Airport, AlasConfig, ConfigLoadNotes, OverlayError};
use serde_json::Value;

use crate::profile::{configure_cruise_legs, propose_profile_for_route, route_cruise_altitude_m};

const EARTH_RADIUS_M: f64 = 6_371_000.0;

fn airport(name: &str) -> Option<Airport> {
    let record = airport_dataset::resolve(name).ok()?;
    Some(Airport {
        name: record.name.value.unwrap_or_else(|| name.to_owned()),
        icao: record.icao.value.unwrap_or_default(),
        elevation_m: record.elevation_m.value?,
        toda_m: record.toda_m.value.unwrap_or_default(),
        lda_m: record.lda_m.value.unwrap_or_default(),
        isa_deviation_c: record.isa_deviation_c.value.unwrap_or_default(),
        notes: String::new(),
        latitude_deg: record.latitude_deg.value?,
        longitude_deg: record.longitude_deg.value?,
    })
}

/// Great-circle distance between two airports on a sphere of mean radius, m.
fn great_circle_m(origin: &Airport, destination: &Airport) -> f64 {
    let lat1 = origin.latitude_deg.to_radians();
    let lat2 = destination.latitude_deg.to_radians();
    let dlat = lat2 - lat1;
    let dlon = destination.longitude_deg.to_radians() - origin.longitude_deg.to_radians();
    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * a.sqrt().atan2((1.0 - a).sqrt())
}

/// Replace the cruise-leg count and the altitude ladder of `config`'s mission
/// profile with the ones the route supports. Aircraft-specific phase speeds
/// and rates stay as they are. Does nothing when an airport does not resolve
/// or the route admits no proposal.
pub fn apply_route_profile(config: &mut AlasConfig) {
    let (Some(origin), Some(destination)) = (
        airport(&config.departure_airport),
        airport(&config.arrival_airport),
    ) else {
        return;
    };
    let distance_m = great_circle_m(&origin, &destination);
    let Ok(proposal) = propose_profile_for_route(config, &origin, &destination, distance_m) else {
        return;
    };
    let cruise_altitude_m = route_cruise_altitude_m(config, &origin, &destination);
    configure_cruise_legs(
        &mut config.mission.profile,
        proposal.active_cruise_legs,
        cruise_altitude_m,
        origin.elevation_m,
    );
}

/// Show the FLOPS class counts the analysis uses. The analysis resolves them
/// from the canonical cabin and ignores the stored seed, so the stored counts
/// are written as the cabin resolves them and the cabin-evidence record says
/// so when the seed differed (`annotate_flops_cabin_resolution`). A preset
/// whose sourced split is the cabin's own keeps its source-backed evidence.
fn mirror_cabin_counts(config: &mut AlasConfig) {
    if config.requirements.aircraft_type != "passenger"
        || config.mass_model.flops_transport.manual_cabin_inputs
    {
        return;
    }
    let counts = config
        .cabin
        .passenger
        .canonicalized_for_product()
        .resolved_flops_counts(config.requirements.num_passengers);
    if !counts.is_nonempty() {
        return;
    }
    annotate_flops_cabin_resolution(&mut config.mass_model, counts);
    let to_count = |count: i64| usize::try_from(count.max(0)).unwrap_or(0);
    let transport = &mut config.mass_model.flops_transport;
    transport.first_class_passenger_count = Some(to_count(counts.first));
    transport.business_class_passenger_count = Some(to_count(counts.business));
    transport.tourist_class_passenger_count = Some(to_count(counts.tourist));
}

/// Load a configuration document with the load notes it produced. A document
/// that names a preset flies the route-derived profile unless it gives
/// `mission.profile`, and carries the FLOPS class counts of its cabin; every
/// other document loads as written.
///
/// # Errors
///
/// The document does not match the configuration schema.
pub fn load_document(data: &Value) -> Result<(AlasConfig, ConfigLoadNotes), OverlayError> {
    let (mut config, notes) = AlasConfig::from_value_with_notes(data)?;
    let names_preset = data.get("preset").and_then(Value::as_str).is_some();
    if names_preset {
        if data.pointer("/mission/profile").is_none() {
            apply_route_profile(&mut config);
        }
        mirror_cabin_counts(&mut config);
    }
    Ok((config, notes))
}
