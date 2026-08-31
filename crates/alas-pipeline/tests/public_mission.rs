// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Public-path evidence for the native mission stage.
//!
//! Lower-level mission fixtures can agree with SUAVE while the application
//! never publishes their result. These runs exercise the same pipeline entry
//! used by the CLI and desktop worker, with a deterministic dispatched route.

use alas_config::airports::get as get_airport;
use alas_config::AlasConfig;
use alas_exec::RunEnvironment;
use alas_pipeline::{DesignPipeline, PipelineOptions};
use alas_route::route::{Route, RouteSource};
use alas_route::SimbriefFetchStatus;

fn config_for_preset(name: &str) -> AlasConfig {
    AlasConfig::from_value(&serde_json::json!({"preset": name}))
        .unwrap_or_else(|error| panic!("preset {name}: {error}"))
}

fn dispatched_route(config: &AlasConfig) -> Route {
    let origin = get_airport(&config.departure_airport)
        .unwrap_or_else(|error| panic!("configured origin: {error}"));
    let destination = get_airport(&config.arrival_airport)
        .unwrap_or_else(|error| panic!("configured destination: {error}"));
    let mut route = Route::great_circle(
        origin,
        destination,
        config.mission.great_circle_points as usize,
    );
    route.source = RouteSource::SimbriefApi;
    route
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
fn public_pipeline_reports_complete_or_explicitly_partial_preset_missions() {
    for name in [
        "AVE",
        "A220-300",
        "A320-200",
        "A340-300",
        "A380-800",
        "ATR72-600",
        "B787-9",
        "DC-10",
    ] {
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
        assert!(!mission.segments.is_empty(), "native schedule for {name}");
        assert!(
            mission.completed_summary().is_some(),
            "validated preset {name} must retain a fully completed public-path mission"
        );
        assert_eq!(
            mission.solutions.len(),
            mission.segments.len(),
            "missing segment solutions for {name}"
        );
        if mission.completed_summary().is_some() {
            assert_eq!(mission.segments.len(), mission.scheduled_segment_count);
            assert!(mission
                .solutions
                .iter()
                .all(|solution| solution.converged && !solution.throttle_limited));
            assert_eq!(
                result.feasibility.fuel_loading.mission.status,
                alas_pipeline::MissionFuelStatus::Completed
            );
            assert!(mission.segments.iter().all(|segment| {
                segment
                    .conditions
                    .total_mass_kg
                    .windows(2)
                    .all(|pair| pair[0] >= pair[1])
            }));
        } else {
            assert_ne!(
                result.feasibility.fuel_loading.mission.status,
                alas_pipeline::MissionFuelStatus::Completed,
                "partial trajectory was mislabeled complete for {name}"
            );
            assert!(
                mission.segments.len() < mission.scheduled_segment_count
                    || mission.fuel_exhaustion.is_some(),
                "partial mission has no explicit stopping condition for {name}"
            );
        }
        assert!(expected_distance_m > 0.0);
    }
}

#[test]
fn narrowbody_operational_routes_fly_full_trajectory_with_explicit_fuel_status() {
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
        assert_eq!(mission.segments.len(), mission.scheduled_segment_count);
        assert_eq!(mission.solutions.len(), mission.segments.len());
        assert!(mission
            .solutions
            .iter()
            .all(|solution| solution.converged && !solution.throttle_limited));
        if mission.completed_summary().is_some() {
            assert_eq!(
                result.feasibility.fuel_loading.mission.status,
                alas_pipeline::MissionFuelStatus::Completed
            );
            assert!(result
                .feasibility
                .fuel_loading
                .mission
                .completed_trip_burn_kg
                .is_some());
        } else {
            assert!(mission
                .fuel_exhaustion
                .as_ref()
                .is_some_and(|crossing| crossing.segment_tag == "final_landing"));
            assert_eq!(
                result.feasibility.fuel_loading.mission.status,
                alas_pipeline::MissionFuelStatus::Exhausted
            );
        }
    }
}
