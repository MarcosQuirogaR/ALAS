// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Public-path evidence for the native mission stage.
//!
//! Lower-level mission fixtures can agree with SUAVE while the application
//! never publishes their result. These runs exercise the same pipeline entry
//! used by the CLI and desktop worker, with a deterministic dispatched route.

// Standalone fixture diagnostics fail immediately when their curated inputs are invalid.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use alas_config::airports::get as get_airport;
use alas_config::AlasConfig;
use alas_exec::RunEnvironment;
use alas_pipeline::{DesignPipeline, PipelineOptions};
use alas_route::route::{Route, RouteSource};
use alas_route::SimbriefFetchStatus;

fn config_for_preset(name: &str) -> AlasConfig {
    let mut config = AlasConfig::from_value(&serde_json::json!({"preset": name}))
        .unwrap_or_else(|error| panic!("preset {name}: {error}"));
    config.structures.run_nastran = false;
    config.structures.run_patran_export = false;
    config
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

/// The run records the route it planned on the configuration it runs with,
/// so the search flies the route the published mission flies, off-design
/// beside its great-circle sizing mission: a dispatched route with a detour,
/// longer than the great circle.
#[test]
fn the_run_flies_its_planned_route_in_the_sizing_loop() {
    let config = config_for_preset("A220-300");
    let mut route = dispatched_route(&config);
    let middle = route.waypoints.len() / 2;
    route.waypoints[middle].lat += 1.0;
    let great_circle_m = dispatched_route(&config).total_distance_m();
    let planned_m = route.total_distance_m();
    assert!(planned_m > 1.001 * great_circle_m);
    assert_eq!(config.mission.route_distance_m, 0.0);
    let result = DesignPipeline::new(config)
        .run_with_environment_and_route(&options(), &RunEnvironment::default(), Some(route))
        .expect("A220 public mission run");
    assert_eq!(result.config.mission.route_distance_m, planned_m);
    let design = alas_config::presets::get("A220-300").unwrap().design_vector;
    let assessment = alas_opt::assess_product_candidate(&result.config, &design).unwrap();
    let sizing = assessment
        .residuals
        .iter()
        .find(|row| row.id == "mission_profile_range")
        .expect("the sizing loop reports its mission distance");
    // The sizing mission is the great circle between the airport records
    // the sizing loop reads, which agree with the registry's to well inside
    // the detour.
    assert!((sizing.actual - great_circle_m).abs() <= 1.0e-3 * great_circle_m);
    let route = assessment
        .sized
        .mtow
        .offdesign
        .as_ref()
        .expect("the planned route is flown off-design");
    assert_eq!(route.range_m, planned_m);
    assert_eq!(
        assessment.sized.flown_dispatch(),
        &route.dispatch,
        "the flown load is the route's"
    );
    let load_case = result.mission_load_case.as_ref().expect("a load case");
    let priced = load_case.route_case().expect("the route is priced");
    assert_eq!(priced.route_distance_m, planned_m);
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
        let run = DesignPipeline::new(config).run_with_environment_and_route(
            &options(),
            &RunEnvironment::default(),
            Some(route.clone()),
        );
        let result = match run {
            Ok(result) => result,
            // The ATR's documented public-path failure is now the station
            // refusal, which is raised while the lumped groups are being
            // placed and therefore *before* the propulsion buildup that used
            // to stop it. Both are missing data on the same aircraft; this
            // pins the one the pipeline actually reports, classified and with
            // the evidence that decided it, so the failure cannot silently
            // become a green run or a different blocker.
            Err(error) if name == "ATR72-600" => {
                assert!(
                    error.contains(
                        alas_pipeline::full_analysis::StationPlacementFailure::MainGearStationNotMeasured
                            .as_str()
                    ),
                    "the ATR public failure must carry its stable classification: {error}"
                );
                assert!(
                    error.contains("no main-gear longitudinal station is available"),
                    "the ATR public failure must name the missing main-gear station: {error}"
                );
                assert!(
                    error.contains("above the fuselage crown"),
                    "the ATR public failure must keep the evidence that refused the wing-mounted fallback: {error}"
                );
                assert!(
                    !error.contains(
                        alas_pipeline::full_analysis::StationPlacementFailure::MassCoordinates
                            .as_str()
                    ),
                    "a missing gear datum must not be reported as a generic coordinate failure: {error}"
                );
                continue;
            }
            Err(error) => panic!("public mission run for {name}: {error}"),
        };

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
        assert_eq!(
            mission.solutions.len(),
            mission.segments.len(),
            "missing segment solutions for {name}"
        );
        assert!(mission.segments.iter().all(|segment| {
            segment
                .conditions
                .total_mass_kg
                .windows(2)
                .all(|pair| pair[0] >= pair[1])
        }));
        if mission.completed_summary().is_some() {
            assert!(
                mission.figure_data_ready(),
                "complete mission {name} must expose figure telemetry"
            );
            assert_eq!(mission.segments.len(), mission.scheduled_segment_count);
            assert!(mission
                .solutions
                .iter()
                .all(|solution| solution.converged && !solution.throttle_limited));
            assert_eq!(
                result.feasibility.fuel_loading.mission.native.status,
                alas_pipeline::MissionFuelStatus::Completed
            );
        } else {
            assert!(
                !mission.figure_data_ready(),
                "partial mission {name} must not publish figure telemetry"
            );
            assert_ne!(
                result.feasibility.fuel_loading.mission.native.status,
                alas_pipeline::MissionFuelStatus::Completed
            );
            assert!(
                mission.fuel_exhaustion.is_some()
                    || mission.segments.len() < mission.scheduled_segment_count
                    || mission.solutions.iter().any(|solution| !solution.converged),
                "partial mission {name} must carry an explicit stopping condition"
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
        assert!(mission.figure_data_ready());
        assert_eq!(mission.segments.len(), mission.scheduled_segment_count);
        assert_eq!(mission.solutions.len(), mission.segments.len());
        assert!(mission
            .solutions
            .iter()
            .all(|solution| solution.converged && !solution.throttle_limited));
        assert_eq!(
            result.feasibility.fuel_loading.mission.status,
            alas_pipeline::MissionFuelStatus::Completed
        );
        assert!(result
            .feasibility
            .fuel_loading
            .mission
            .required_trip_fuel_kg
            .is_some());
    }
}

#[test]
fn b787_regression_does_not_cut_off_after_takeoff_or_publish_untrimmed_cruise() {
    let config = config_for_preset("B787-9");
    let route = dispatched_route(&config);
    let result = DesignPipeline::new(config)
        .run_with_environment_and_route(&options(), &RunEnvironment::default(), Some(route))
        .expect("B787 public mission run");
    let mission = result
        .mission_result
        .as_ref()
        .expect("B787 mission telemetry");
    let report = result.optimized_report.as_ref().expect("B787 report");

    assert!(mission.figure_data_ready());
    assert!(mission.fuel_exhaustion.is_none());
    assert_eq!(mission.segments.len(), mission.scheduled_segment_count);
    assert!(mission
        .solutions
        .iter()
        .all(|solution| solution.converged && !solution.throttle_limited));
    assert_eq!(
        result.feasibility.fuel_loading.mission.status,
        alas_pipeline::MissionFuelStatus::Completed
    );
    let trim = report
        .trimmed_design_point
        .as_ref()
        .expect("B787 trimmed cruise point");
    assert!(trim.cm_residual.abs() < 1.0e-6);
    assert!(trim.trim_ih_deg.is_finite());
}
