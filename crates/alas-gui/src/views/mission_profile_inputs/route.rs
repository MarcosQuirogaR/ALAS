// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Route identity, distance and airport resolution for the mission profile.

use super::*;

/// Stable route identity used by the explicit regeneration policy.
pub(crate) fn route_signature(values: &Value) -> String {
    format!(
        "{}|{}",
        airport_signature(
            values
                .get("departure_airport")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        airport_signature(
            values
                .get("arrival_airport")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )
    )
}

/// Include the resolved physical endpoint in the route identity. A custom
/// airport can retain its ICAO/name while its coordinates or field climate
/// are edited; the profile policy must see that as a route change rather than
/// silently retaining a schedule derived from the old endpoint.
fn airport_signature(name: &str) -> String {
    let resolved = airport_dataset::resolve(name).ok().map(|airport| {
        (
            airport.icao.value,
            airport.latitude_deg.value,
            airport.longitude_deg.value,
            airport.elevation_m.value,
            airport.isa_deviation_c.value,
        )
    });
    format!("{name}|{resolved:?}")
}

pub(crate) fn route_distance_m(values: &Value) -> Option<f64> {
    let departure = values.get("departure_airport").and_then(Value::as_str)?;
    let arrival = values.get("arrival_airport").and_then(Value::as_str)?;
    let departure = airport_dataset::resolve(departure).ok()?;
    let arrival = airport_dataset::resolve(arrival).ok()?;
    let lat1 = departure.latitude_deg.value?.to_radians();
    let lat2 = arrival.latitude_deg.value?.to_radians();
    let dlat = lat2 - lat1;
    let lon1 = departure.longitude_deg.value?.to_radians();
    let lon2 = arrival.longitude_deg.value?.to_radians();
    let dlon = lon2 - lon1;
    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    Some(2.0 * 6_371_000.0 * a.sqrt().atan2((1.0 - a).sqrt()))
}

/// Resolve route endpoints into the minimal airport values needed by
/// the shared route-aware mission proposal. Route distance and cruise-altitude
/// selection must use the same endpoint records as the Inputs card.
pub(crate) fn resolved_route_airports(config: &AlasConfig) -> Option<(Airport, Airport)> {
    fn resolve(name: &str) -> Option<Airport> {
        let airport = airport_dataset::resolve(name).ok()?;
        Some(Airport {
            name: airport.name.value.unwrap_or_else(|| name.to_owned()),
            icao: airport.icao.value.unwrap_or_default(),
            elevation_m: airport.elevation_m.value?,
            toda_m: airport.toda_m.value.unwrap_or_default(),
            lda_m: airport.lda_m.value.unwrap_or_default(),
            isa_deviation_c: airport.isa_deviation_c.value.unwrap_or_default(),
            notes: String::new(),
            latitude_deg: airport.latitude_deg.value?,
            longitude_deg: airport.longitude_deg.value?,
        })
    }

    Some((
        resolve(&config.departure_airport)?,
        resolve(&config.arrival_airport)?,
    ))
}

/// Whether the profile still matches a registered preset's unedited
/// operational schedule (or the generic default), so its route-based cruise
/// count may be initialized automatically.
pub(crate) fn is_automatic_profile(config: &AlasConfig) -> bool {
    config.mission.profile == MissionProfileConfig::default()
        || alas_config::presets::get(&config.preset)
            .ok()
            .is_some_and(|preset| {
                config.mission.profile == preset.operational_mission_defaults().profile
            })
}
