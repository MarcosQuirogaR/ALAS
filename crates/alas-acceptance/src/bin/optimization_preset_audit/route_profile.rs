// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Route-profile initialization shared by optimizer acceptance experiments.

use alas_config::{airport_dataset, airports::Airport, AlasConfig};

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

/// Same endpoint resolution, great-circle radius, proposal and cruise-leg
/// configuration as GUI `mission_profile_inputs::initialize_route_profile`.
/// Aircraft-specific speeds and rates remain unchanged.
pub(crate) fn initialize_route_profile(config: &mut AlasConfig) {
    let Some(origin) = airport(&config.departure_airport) else {
        return;
    };
    let Some(destination) = airport(&config.arrival_airport) else {
        return;
    };
    let lat1 = origin.latitude_deg.to_radians();
    let lat2 = destination.latitude_deg.to_radians();
    let dlat = lat2 - lat1;
    let dlon = destination.longitude_deg.to_radians() - origin.longitude_deg.to_radians();
    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    let distance_m = 2.0 * 6_371_000.0 * a.sqrt().atan2((1.0 - a).sqrt());
    let Ok(proposal) =
        alas_mission::propose_profile_for_route(config, &origin, &destination, distance_m)
    else {
        return;
    };
    let cruise_altitude_m = alas_mission::route_cruise_altitude_m(config, &origin, &destination);
    alas_mission::configure_cruise_legs(
        &mut config.mission.profile,
        proposal.active_cruise_legs,
        cruise_altitude_m,
        origin.elevation_m,
    );
}
