// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Public-path evidence for the native mission stage.
//!
//! Lower-level mission fixtures can agree with SUAVE while the application
//! never publishes their result. These runs exercise the same pipeline entry
//! used by the CLI and desktop worker, with a deterministic dispatched route.

use alas_config::airports::get as get_airport;
use alas_config::presets;
use alas_config::AlasConfig;
use alas_exec::RunEnvironment;
use alas_pipeline::{DesignPipeline, PipelineOptions};
use alas_route::route::{Route, RouteSource, Waypoint};
use alas_route::SimbriefFetchStatus;

fn config_for_preset(name: &str) -> AlasConfig {
    let preset = presets::get(name).unwrap_or_else(|error| panic!("preset {name}: {error}"));
    let mut config = AlasConfig {
        preset: name.to_owned(),
        geometry: preset.geometry.clone(),
        requirements: preset.requirements.clone(),
        landing_gear: preset.landing_gear.clone(),
        ..AlasConfig::default()
    };
    if let Some(mass_model) = preset.mass_model.clone() {
        config.mass_model = mass_model;
    }
    if let Some(performance) = preset.performance.clone() {
        config.performance = performance;
    }
    config
}

fn dispatched_route(config: &AlasConfig) -> Route {
    let origin = get_airport(&config.departure_airport)
        .unwrap_or_else(|error| panic!("configured origin: {error}"));
    let destination = get_airport(&config.arrival_airport)
        .unwrap_or_else(|error| panic!("configured destination: {error}"));
    Route {
        waypoints: vec![
            Waypoint::named(
                origin.latitude_deg,
                origin.longitude_deg,
                origin.icao.clone(),
            ),
            Waypoint::named(45.0, -2.0, "RECOVERY_FIX"),
            Waypoint::named(
                destination.latitude_deg,
                destination.longitude_deg,
                destination.icao.clone(),
            ),
        ],
        source: RouteSource::SimbriefApi,
        origin_airport: Some(origin.clone()),
        dest_airport: Some(destination.clone()),
    }
}

fn options() -> PipelineOptions {
    PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: None,
        quiet: true,
    }
}

#[test]
fn public_pipeline_publishes_converged_mission_telemetry_for_representative_presets() {
    for name in ["AVE", "A380-800", "B787-9"] {
        let config = config_for_preset(name);
        let route = dispatched_route(&config);
        let expected_distance_m = route.total_distance_m();
        let result = DesignPipeline::new(config)
            .run_with_environment_and_route(
                &options(),
                &RunEnvironment::default(),
                Some(route.clone()),
            )
            .unwrap_or_else(|error| panic!("public mission run for {name}: {error}"));

        assert_eq!(
            result.route,
            Some(route),
            "route must survive the public path for {name}"
        );
        let route_status = result
            .route_status
            .as_ref()
            .unwrap_or_else(|| panic!("route status missing for {name}"));
        assert_eq!(route_status.selected_source, RouteSource::SimbriefApi);
        assert_eq!(route_status.simbrief, SimbriefFetchStatus::SuppliedByCaller);
        let mission = result
            .mission_result
            .as_ref()
            .unwrap_or_else(|| panic!("mission telemetry missing for {name}"));
        assert_eq!(mission.segments.len(), 12, "native schedule for {name}");
        assert_eq!(mission.solutions.len(), mission.segments.len());
        assert!(
            mission.solutions.iter().all(|solution| solution.converged),
            "mission did not converge for {name}: {:?}",
            mission.solutions
        );
        assert!(mission.initial_mass_kg() > mission.final_mass_kg());
        assert!(mission.fuel_burned_kg() > 0.0);
        assert!(mission.block_time_s() > 0.0);
        assert_eq!(mission.fuel_exhaustion, None, "fuel endurance for {name}");
        assert_eq!(
            result.feasibility.fuel_loading.mission.status,
            alas_pipeline::MissionFuelStatus::Completed
        );
        assert_eq!(
            result
                .feasibility
                .fuel_loading
                .mission
                .required_trip_fuel_kg,
            Some(mission.fuel_burned_kg())
        );
        assert!(expected_distance_m > 0.0);
        assert!(mission.segments.iter().all(|segment| segment
            .conditions
            .total_mass_kg
            .windows(2)
            .all(|pair| pair[0] >= pair[1])));
    }
}

#[test]
fn public_pipeline_stops_when_a_preset_consumes_its_usable_fuel() {
    for name in ["A320-200", "A220-300"] {
        let config = config_for_preset(name);
        let route = dispatched_route(&config);
        let result = DesignPipeline::new(config)
            .run_with_environment_and_route(&options(), &RunEnvironment::default(), Some(route))
            .unwrap_or_else(|error| panic!("public mission run for {name}: {error}"));
        let mission = result
            .mission_result
            .as_ref()
            .unwrap_or_else(|| panic!("mission telemetry missing for {name}"));
        let exhaustion = mission
            .fuel_exhaustion
            .as_ref()
            .unwrap_or_else(|| panic!("{name} was propagated past usable-fuel exhaustion"));

        assert!(mission.segments.len() < 12, "later segments ran for {name}");
        assert_eq!(mission.solutions.len(), mission.segments.len());
        assert_eq!(exhaustion.segment_index, mission.segments.len());
        assert!(exhaustion.burned_fuel_kg > exhaustion.available_fuel_kg);
        assert!(result
            .feasibility
            .findings
            .iter()
            .any(|finding| { finding.code == alas_pipeline::FindingCode::MissionFuelShortfall }));
        assert_eq!(
            result.feasibility.fuel_loading.mission.status,
            alas_pipeline::MissionFuelStatus::Exhausted
        );
        assert!(
            (result.feasibility.fuel_loading.analyzed_carried_fuel_kg
                - exhaustion.available_fuel_kg)
                .abs()
                < 1.0e-9
        );
        assert_eq!(
            result
                .feasibility
                .fuel_loading
                .mission
                .required_trip_fuel_kg,
            None,
            "partial telemetry cannot establish the total trip-fuel requirement"
        );
    }
}
