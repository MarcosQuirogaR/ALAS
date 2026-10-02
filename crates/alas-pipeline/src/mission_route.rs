// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The route the mission stage flies, planned once for the full analysis
//! and for the sandbox.
//!
//! An aircraft is sized on its design mission (the still-air great circle
//! between the declared airports); the route it is then flown over is the
//! best one the planner finds (`alas_route::planner`): a dispatched plan, a
//! hand-exported KML plan, the airway graph of the open navigation data
//! within the configured detour limit, else the great circle. Both the full
//! analysis and the sandbox plan it here, so they fly one route.

use std::path::Path;

use alas_config::airports::{get as get_airport, Airport};
use alas_config::AlasConfig;
use alas_exec::ToolLocator;
use alas_route::planner::{
    load_navdata_with_airway_coordinates, plan_route_with_max_stretch, RouteSources,
};
use alas_route::route::Route;

/// The mission route between the configured airports, `dispatched` first
/// when the caller supplies one, then the offline tiers. `None` when an
/// airport is not in the registry.
pub(crate) fn plan_mission_route(config: &AlasConfig, dispatched: Option<Route>) -> Option<Route> {
    let origin = get_airport(&config.departure_airport).ok()?;
    let dest = get_airport(&config.arrival_airport).ok()?;
    let locator = ToolLocator::for_current_process();
    let routes_dir = locator.resolve_data_path(Path::new(&config.mission.routes_dir));
    let navdata_dir = locator.resolve_data_path(Path::new(&config.mission.navdata_dir));
    let navdata = load_navdata_with_airway_coordinates(
        &navdata_dir,
        config.mission.use_airway_endpoint_coordinates,
    );
    let sources = RouteSources {
        dispatched,
        routes_dir: Some(routes_dir.as_path()),
        navdata: navdata.as_ref(),
        great_circle_points: config.mission.great_circle_points.max(1) as usize,
    };
    Some(plan_route_with_max_stretch(
        origin,
        dest,
        sources,
        config.mission.max_airway_stretch,
    ))
}

/// The airports `route` is flown between: the endpoint records the planner
/// attached, else the configured airports (a great-circle plan carries
/// none).
pub(crate) fn route_endpoints<'a>(
    config: &AlasConfig,
    route: &'a Route,
) -> Result<(&'a Airport, &'a Airport), String> {
    let origin = match route.origin_airport.as_ref() {
        Some(airport) => airport,
        None => get_airport(&config.departure_airport)
            .map_err(|error| format!("mission departure airport could not be resolved: {error}"))?,
    };
    let destination = match route.dest_airport.as_ref() {
        Some(airport) => airport,
        None => get_airport(&config.arrival_airport)
            .map_err(|error| format!("mission arrival airport could not be resolved: {error}"))?,
    };
    Ok((origin, destination))
}

/// Great-circle distance between two airports, m: the still-air design
/// mission distance the route is compared against.
pub(crate) fn great_circle_m(origin: &Airport, destination: &Airport) -> f64 {
    alas_route::route::haversine_m(
        origin.latitude_deg,
        origin.longitude_deg,
        destination.latitude_deg,
        destination.longitude_deg,
    )
}
