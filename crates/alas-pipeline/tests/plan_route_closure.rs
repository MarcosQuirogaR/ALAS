// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A mission plan's flight covers the distances the plan states: the route by the legs of the
//! stages that advance along it, and a diversion by its own legs. The closure sizes each cruise
//! against the footprint it computes for the climbs and descents; the flown climb at the
//! maximum-climb rating has the rate the force balance gives, so the closing target is moved by
//! the excess the flight covered until the flight covers the stated distance.
//!
//! Heavy (a full pipeline run of a plan on the A320-200's own 546 km sector).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_exec::RunEnvironment;
use alas_pipeline::{DesignPipeline, LoadCaseSelection, PipelineOptions, PlanClosure};
use alas_route::route::{Route, RouteSource};

/// The commercial-transport plan template of the mission editor (a Raymer-style profile, Aircraft
/// Design: A Conceptual Approach, 5th ed., p. 32, for its 10,000 ft hold): climb, cruise to the
/// route, descent to the hold,
/// loiter, approach, go-around, 200 nmi diversion, 30 min loiter, landing.
fn commercial_transport_plan() -> serde_json::Value {
    let stage = |id: &str, kind: serde_json::Value, reserve: bool| serde_json::json!({"id": id, "kind": kind, "reserve": reserve});
    serde_json::json!({
        "name": "Commercial Transport",
        "stages": [
            stage("takeoff", serde_json::json!({"type": "takeoff"}), false),
            stage("climb", serde_json::json!({"type": "climb", "altitude_ft": 37001.3}), false),
            stage("cruise-out", serde_json::json!({"type": "cruise", "extent": "remainder"}), false),
            stage("descend", serde_json::json!({"type": "descent", "altitude_ft": 10000.0}), false),
            stage("loiter", serde_json::json!({"type": "loiter", "altitude_ft": 10000.0, "time_min": 30.0}), true),
            stage("attempt-to-land", serde_json::json!({"type": "approach", "decision_height_ft": 200.0}), false),
            stage("climb-after-go-around", serde_json::json!({"type": "go_around", "agl": true, "altitude_ft": 3000.0}), true),
            stage("divert", serde_json::json!({"type": "diversion", "cruise_altitude_ft": 37001.3, "distance_km": 370.4}), true),
            stage("loiter-at-alternate", serde_json::json!({"type": "loiter", "time_min": 30.0}), true),
            stage("land", serde_json::json!({"type": "landing"}), true),
        ]
    })
}

#[test]
fn a_plan_flight_covers_the_route_and_the_diversion_it_states() {
    let mut config = alas_config::AlasConfig::from_value(&serde_json::json!({
        "preset": "A320-200",
        "mission": {"plan": commercial_transport_plan()},
    }))
    .expect("config loads");
    config.structures.run_nastran = false;
    config.structures.run_patran_export = false;
    // The preset's own sector, LEMD-LEPA.
    let origin = alas_config::airports::get(&config.departure_airport).unwrap();
    let destination = alas_config::airports::get(&config.arrival_airport).unwrap();
    let mut route = Route::great_circle(
        origin,
        destination,
        config.mission.great_circle_points as usize,
    );
    route.source = RouteSource::SimbriefApi;
    let route_m = route.total_distance_m();
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
    // The segment network recurses deeply: a thread with room for it.
    let result = std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            DesignPipeline::new(config)
                .run_with_environment_and_route(&options, &RunEnvironment::default(), Some(route))
                .expect("the plan flies")
        })
        .unwrap()
        .join()
        .unwrap();
    let load_case = result.mission_load_case.as_ref().expect("a load case");
    let LoadCaseSelection::Plan(case) = &load_case.selection else {
        panic!("a plan load case");
    };
    assert!(
        matches!(case.closure, PlanClosure::Closed),
        "{:?}",
        case.closure
    );
    let history = result
        .feasibility
        .mass_balance
        .as_ref()
        .and_then(|balance| balance.history.as_ref())
        .expect("a per-stage history");
    let stage_m = |id: &str| {
        history
            .stages
            .iter()
            .find(|stage| stage.id == id)
            .unwrap_or_else(|| panic!("stage {id}"))
            .distance_m
    };
    // The route is the legs of the stages that advance along it (not the hold's).
    let trip_m: f64 = [
        "takeoff",
        "climb",
        "cruise-out",
        "descend",
        "attempt-to-land",
    ]
    .iter()
    .map(|id| stage_m(id))
    .sum();
    assert!(
        (trip_m - route_m).abs() < 50.0,
        "trip {trip_m:.0} m against the route of {route_m:.0} m"
    );
    assert!(
        (stage_m("divert") - 370_400.0).abs() < 50.0,
        "diversion {:.0} m against its stated 370,400 m",
        stage_m("divert")
    );
}
