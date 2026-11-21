// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reproduction (declared cruise level): the SUAVE provenance mission flown with the
//! `Declared` cruise policy (argument `declared`, default) or the default
//! `OptimumStep` (`optimum`). Before `alas_opt` `prescribed_level` the `Declared`
//! sizing closure of this vehicle reported `NotConverged` (358,670 kg climb-limited
//! against a mission that needs 17,409 kg more); it now closes with the level
//! lowered to the climb ceiling. `cargo run -p alas-pipeline --example declared_level_probe -- declared`.

#![allow(clippy::print_stdout, clippy::expect_used, clippy::unwrap_used)]

use alas_config::airports::get as get_airport;
use alas_config::mission::CruiseAltitudePolicy;
use alas_config::AlasConfig;
use alas_exec::RunEnvironment;
use alas_pipeline::{DesignPipeline, PipelineOptions};
use alas_route::route::{Route, RouteSource, Waypoint, EARTH_RADIUS_M};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    inputs: Inputs,
}
#[derive(Deserialize)]
struct Inputs {
    route_distance_m: f64,
}

fn main() {
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(run)
        .unwrap()
        .join()
        .unwrap();
}

fn run() {
    let fixture: Fixture = alas_testkit::load("mission", "w64_provenance");
    let policy = std::env::args().nth(1).unwrap_or_else(|| "declared".into());
    let mut config = AlasConfig {
        departure_airport: "LEMD".to_owned(),
        arrival_airport: "HKJK".to_owned(),
        ..AlasConfig::default()
    };
    config.structures.run_nastran = false;
    config.structures.run_patran_export = false;
    config.geometry.wing.side_of_body_span_fraction = None;
    config.geometry.wing.side_of_body_chord_ratio = None;
    config.geometry.wing.kink_span_fraction = None;
    config.geometry.wing.outboard_le_sweep_deg = None;
    config.mission.profile.takeoff_altitude_gain_m = 3048.0;
    config.mission.profile.descent_4_altitude_ft = 6500.0;
    if policy == "declared" {
        config.mission.profile.cruise_altitude_policy = CruiseAltitudePolicy::Declared;
    }
    let origin = get_airport(&config.departure_airport).unwrap();
    let destination = get_airport(&config.arrival_airport).unwrap();
    let latitude = origin.latitude_deg.to_radians();
    let central_angle = fixture.inputs.route_distance_m / EARTH_RADIUS_M;
    let longitude_delta = 2.0
        * ((central_angle / 2.0).sin() / latitude.cos())
            .asin()
            .to_degrees();
    let route = Route {
        waypoints: vec![
            Waypoint::named(
                origin.latitude_deg,
                origin.longitude_deg,
                origin.icao.clone(),
            ),
            Waypoint::named(
                origin.latitude_deg,
                origin.longitude_deg + longitude_delta,
                "SUAVE_FIXTURE",
            ),
        ],
        source: RouteSource::SimbriefApi,
        origin_airport: Some(origin.clone()),
        dest_airport: Some(destination.clone()),
    };
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: None,
        quiet: true,
    };
    let result = DesignPipeline::new(config).run_with_environment_and_route(
        &options,
        &RunEnvironment::default(),
        Some(route),
    );
    match result {
        Ok(result) => {
            let dispatch = result.feasibility.fuel_loading.dispatch.as_ref();
            println!(
                "policy {policy}: dispatch {:?}",
                dispatch.map(|d| (
                    d.outcome.as_str(),
                    d.takeoff_mass_kg,
                    d.plan.map(|p| p.trip.kg)
                ))
            );
            println!("native error {:?}", result.feasibility.native_mission_error);
            if let Some(mission) = result.mission_result.as_ref() {
                let top = mission
                    .segments
                    .iter()
                    .flat_map(|s| s.conditions.altitude_m.iter().copied())
                    .fold(0.0_f64, f64::max);
                println!(
                    "native top {top:.1} m, fuel {:.0} kg",
                    mission.fuel_burned_kg()
                );
            }
        }
        Err(e) => println!("policy {policy}: pipeline error {e}"),
    }
}
