// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Scratch probe: planned airway route distance against the great-circle arc.
#![allow(clippy::print_stdout, missing_docs)]

use std::path::Path;

use alas_config::airports::get as get_airport;
use alas_config::{presets, AlasConfig};
use alas_exec::ToolLocator;
use alas_route::planner::{
    load_navdata_with_airway_coordinates, plan_route_with_max_stretch, RouteSources,
    DEFAULT_GREAT_CIRCLE_POINTS,
};
use alas_route::route::Route;

fn main() {
    let config = AlasConfig::default();
    let locator = ToolLocator::for_current_process();
    let navdata_dir = locator.resolve_data_path(Path::new(&config.mission.navdata_dir));
    let navdata = load_navdata_with_airway_coordinates(
        &navdata_dir,
        config.mission.use_airway_endpoint_coordinates,
    );
    println!(
        "navdata at {}: {}",
        navdata_dir.display(),
        navdata.is_some()
    );
    println!(
        "{:<11} {:<24} {:<24} {:>9} {:>10} {:>7} {:>5}",
        "preset", "from", "to", "gc_km", "planned_km", "ratio", "wpts"
    );
    for name in presets::available() {
        let p = presets::get(name).unwrap();
        let d = p.operational_mission_defaults();
        let (Ok(origin), Ok(dest)) = (
            get_airport(d.departure_airport),
            get_airport(d.arrival_airport),
        ) else {
            println!("{name:<11} airport lookup failed");
            continue;
        };
        let gc = Route::great_circle(origin, dest, DEFAULT_GREAT_CIRCLE_POINTS).total_distance_m()
            / 1000.0;
        let planned = plan_route_with_max_stretch(
            origin,
            dest,
            RouteSources {
                dispatched: None,
                routes_dir: None,
                navdata: navdata.as_ref(),
                great_circle_points: DEFAULT_GREAT_CIRCLE_POINTS,
            },
            config.mission.max_airway_stretch,
        );
        let pd = planned.total_distance_m() / 1000.0;
        println!(
            "{:<11} {:<24} {:<24} {:>9.1} {:>10.1} {:>7.2} {:>5}",
            name,
            d.departure_airport.chars().take(23).collect::<String>(),
            d.arrival_airport.chars().take(23).collect::<String>(),
            gc,
            pd,
            pd / gc,
            planned.waypoints.len()
        );
    }
}
