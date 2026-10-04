// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The configured route as the sizing loop flies it: aerodromes, distances,
//! elevations and the cruise and holding altitudes. Distances and altitudes
//! are metres.

use alas_config::{airport_dataset, airports, AlasConfig};

use crate::mdo::range::mission_range_from_coordinates;

/// The resolved route of one candidate configuration.
pub(super) struct Route {
    pub departure_record: Option<airport_dataset::ProvenancedAirport>,
    pub arrival_record: Option<airport_dataset::ProvenancedAirport>,
    pub departure: Option<&'static airports::Airport>,
    pub arrival: Option<&'static airports::Airport>,
    /// Whether the distance was explicit or computed from two finite
    /// source-resolved coordinates.
    pub mission_distance_known: bool,
    /// Sizing still-air distance: the declared design range when positive,
    /// otherwise the great-circle route.
    pub range_m: f64,
    /// The selected route alone, flown off-design by a closed aircraft: the
    /// run's planned route (`MissionConfig::planned_route_distance_m`), else
    /// the great circle (zero when unknown).
    pub route_distance_m: f64,
    pub departure_elevation_m: f64,
    pub arrival_elevation_m: f64,
    /// Holding altitude above mean sea level.
    pub holding_altitude_m: f64,
    /// The altitude the route is actually flown at.
    pub flown_cruise_altitude_m: f64,
}

impl Route {
    /// Whether both configured aerodrome identifiers resolved to records.
    pub fn records_resolved(&self) -> bool {
        self.departure_record.is_some() && self.arrival_record.is_some()
    }

    /// Whether both records carry declared operational runway distances.
    pub fn declared_data_complete(&self) -> bool {
        let complete = |record: &Option<airport_dataset::ProvenancedAirport>| {
            record.as_ref().is_some_and(
                airport_dataset::ProvenancedAirport::is_complete_for_declared_performance,
            )
        };
        complete(&self.departure_record) && complete(&self.arrival_record)
    }
}

/// Resolve the route of `config`.
pub(super) fn resolve(config: &AlasConfig) -> Route {
    let departure_record = airport_dataset::resolve(&config.departure_airport).ok();
    let arrival_record = airport_dataset::resolve(&config.arrival_airport).ok();
    let departure = airports::get(&config.departure_airport).ok();
    let arrival = airports::get(&config.arrival_airport).ok();
    let explicit_range = config.optimizer.objective.design_range_nmi > 0.0;
    let coordinates = |record: &Option<airport_dataset::ProvenancedAirport>| {
        record
            .as_ref()
            .and_then(|airport| Some((airport.latitude_deg.value?, airport.longitude_deg.value?)))
    };
    let departure_coordinates = coordinates(&departure_record);
    let arrival_coordinates = coordinates(&arrival_record);
    let coordinate_range = departure_coordinates.is_some() && arrival_coordinates.is_some();
    // The aircraft is sized on its design mission, the declared design
    // range or the great circle. The route it is flown over is the one the
    // run planned, which its published mission and its delivery check fly;
    // the great circle stands in only when none was planned.
    let range_m = mission_range_from_coordinates(
        config.optimizer.objective.design_range_nmi,
        departure_coordinates,
        arrival_coordinates,
    );
    let planned_m = config.mission.planned_route_distance_m();
    let route_distance_m = planned_m.unwrap_or_else(|| {
        mission_range_from_coordinates(0.0, departure_coordinates, arrival_coordinates)
    });
    let elevation = |record: &Option<airport_dataset::ProvenancedAirport>,
                     airport: Option<&'static airports::Airport>| {
        record
            .as_ref()
            .and_then(|airport| airport.elevation_m.value)
            .or_else(|| airport.map(|airport| airport.elevation_m))
            .unwrap_or(0.0)
    };
    let departure_elevation_m = elevation(&departure_record, departure);
    let arrival_elevation_m = elevation(&arrival_record, arrival);
    let holding_altitude_m =
        arrival_elevation_m + config.fuel_policy.holding_altitude_ft * alas_units::FOOT;
    // The altitude the configured route is actually flown at, resolved by the
    // same rule the published mission uses
    // (`alas_mission::route_cruise_altitude_m`). `requirements.cruise_altitude_m`
    // is the *sizing* cruise altitude - the design point the wing, the engine
    // deck and the drag table are built at - and it stays that everywhere
    // else in the sizing, including the propulsion deck's reference point. It
    // is not the flight level a dispatcher files for a short declared sector,
    // and using it as one is what made the A320-200 and the A220-300 reject
    // every candidate on `mission_profile_range`: the climb-cruise-descent
    // ladder to 11 278 m needs 743 km of still air and the declared LEMD-LEPA
    // sector is 546 km. The published mission was corrected to fly the
    // preset's own declared operational altitude; this is the same correction
    // on the optimizer's side, so the two models size and fly one mission
    // instead of two.
    let flown_cruise_altitude_m = match (departure, arrival) {
        (Some(origin), Some(destination)) => {
            alas_mission::route_cruise_altitude_m(config, origin, destination)
        }
        _ => config.requirements.cruise_altitude_m,
    };
    Route {
        departure_record,
        arrival_record,
        departure,
        arrival,
        mission_distance_known: explicit_range || coordinate_range,
        range_m,
        route_distance_m,
        departure_elevation_m,
        arrival_elevation_m,
        holding_altitude_m,
        flown_cruise_altitude_m,
    }
}

#[cfg(test)]
// A registered preset that fails to load is the assertion failing.
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use alas_units::NAUTICAL_MILE;

    #[test]
    fn the_planned_route_is_flown_and_the_design_mission_still_sizes() {
        let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "DC-10"})).unwrap();
        let great_circle = resolve(&config);
        assert!(great_circle.route_distance_m > 0.0);
        assert_eq!(great_circle.range_m, great_circle.route_distance_m);

        // A planned airway route 19 % longer than the great circle.
        let planned_m = 1.19 * great_circle.route_distance_m;
        config.mission.route_distance_m = planned_m;
        let planned = resolve(&config);
        assert_eq!(planned.route_distance_m, planned_m);
        assert_eq!(planned.range_m, great_circle.range_m);

        config.optimizer.objective.design_range_nmi = 4_120.0;
        let designed = resolve(&config);
        assert_eq!(designed.range_m, 4_120.0 * NAUTICAL_MILE);
        assert_eq!(designed.route_distance_m, planned_m);

        // Nothing planned, or a non-physical record: the great circle.
        config.optimizer.objective.design_range_nmi = 0.0;
        for unset in [0.0, -1.0, f64::NAN] {
            config.mission.route_distance_m = unset;
            assert_eq!(
                resolve(&config).route_distance_m,
                great_circle.route_distance_m
            );
        }
    }
}
