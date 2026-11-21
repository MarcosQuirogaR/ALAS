// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The planned standard mission is the flown standard mission.
//!
//! `planned_standard_mission` is the phase list the Mission page draws; the run
//! builds its schedules with the same constructor. After a full pipeline
//! evaluation the flown segments, grouped by the same rule, must match the plan
//! phase for phase: the kinds in order, the end altitudes to 1 m, the route
//! distance to the closure tolerance, and `N` legs must be exactly `N - 1` step
//! climbs.

// Fixture diagnostics fail immediately when their curated inputs are invalid.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use alas_config::airports::get as get_airport;
use alas_config::AlasConfig;
use alas_exec::RunEnvironment;
use alas_pipeline::{
    flown_phases, planned_standard_mission, DesignPipeline, Phase, PhaseKind, PipelineOptions,
    Scope,
};

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

fn config(preset: &str, legs: usize) -> AlasConfig {
    let mut config = AlasConfig::from_value(&serde_json::json!({"preset": preset})).unwrap();
    config.structures.run_nastran = false;
    config.structures.run_patran_export = false;
    let origin = get_airport(&config.departure_airport).unwrap();
    let destination = get_airport(&config.arrival_airport).unwrap();
    let cruise_m = alas_mission::route_cruise_altitude_m(&config, origin, destination);
    // Zero legs: the preset's own ladder, as the user finds it.
    if legs > 0 {
        alas_mission::configure_cruise_legs(
            &mut config.mission.profile,
            legs,
            cruise_m,
            origin.elevation_m,
        );
    }
    config
}

fn kinds(phases: &[Phase]) -> Vec<PhaseKind> {
    phases.iter().map(|phase| phase.kind).collect()
}

/// Fly `preset` at `legs` cruise legs and compare the flight with the plan; returns the largest
/// relative leg-length error (cruise distance, other legs duration) of the nominal plan.
fn check(preset: &str, legs: usize) -> f64 {
    {
        let config = config(preset, legs);
        let legs = if legs == 0 {
            let p = &config.mission.profile;
            [
                p.cruise_1_distance_fraction,
                p.cruise_2_distance_fraction,
                p.cruise_3_distance_fraction,
            ]
            .iter()
            .filter(|share| **share > 0.0)
            .count()
        } else {
            legs
        };
        let result = DesignPipeline::new(config.clone())
            .run(&options(), &RunEnvironment::default())
            .unwrap();
        let origin = get_airport(&config.departure_airport).unwrap();
        let destination = get_airport(&config.arrival_airport).unwrap();
        let route_m = result
            .route
            .as_ref()
            .expect("the run planned a route")
            .total_distance_m();
        let planned = planned_standard_mission(&config, origin, destination, route_m).unwrap();
        let mission = result
            .mission_result
            .as_ref()
            .expect("the native mission was flown");
        let load_case = result.mission_load_case.as_ref().unwrap();
        assert_eq!(load_case.native_error, None, "{legs} legs");
        assert!(
            mission.completed_summary().is_some(),
            "{preset} {legs} legs"
        );
        let capped = load_case
            .native_supplement
            .as_ref()
            .is_some_and(|supplement| supplement.reach_note.is_some());

        // The trip: the planned phases without the ground time and the reserve.
        let planned_trip: Vec<Phase> = planned
            .phases
            .iter()
            .filter(|phase| !phase.kind.is_ground() && !phase.is_reserve())
            .cloned()
            .collect();
        let flown_trip = flown_phases(mission, Scope::Trip);
        assert_eq!(kinds(&flown_trip), kinds(&planned_trip), "{legs} legs");
        // N legs are exactly N - 1 step climbs, at the configured levels.
        let steps = |phases: &[Phase]| {
            phases
                .iter()
                .filter(|phase| phase.kind == PhaseKind::StepClimb)
                .count()
        };
        // The step climbs the profile defines: one per active share after the first level.
        let expected_steps = usize::from(config.mission.profile.cruise_2_distance_fraction > 0.0)
            + usize::from(config.mission.profile.cruise_3_distance_fraction > 0.0);
        assert_eq!(steps(&flown_trip), expected_steps);
        // Every step climb is a real climb, also under a lowered top level.
        for step in flown_trip
            .iter()
            .filter(|phase| phase.kind == PhaseKind::StepClimb)
        {
            assert!(
                step.end_altitude_m - step.start_altitude_m > 100.0,
                "{preset} {legs} legs: a step climb of {} m",
                step.end_altitude_m - step.start_altitude_m
            );
        }
        assert_eq!(
            flown_trip
                .iter()
                .filter(|phase| phase.kind == PhaseKind::Cruise)
                .count(),
            legs
        );
        for (plan, fly) in planned_trip.iter().zip(&flown_trip) {
            // The configured levels are reached to 1 m, or the lowering is said so (`capped`: the
            // flown top level is below the configured one and the run reports it).
            assert!(
                capped
                    || !matches!(
                        plan.kind,
                        PhaseKind::Climb | PhaseKind::StepClimb | PhaseKind::Cruise
                    )
                    || (fly.end_altitude_m - plan.end_altitude_m).abs() < 1.0,
                "{legs} legs, {:?}: end altitude planned {} m, flown {} m",
                plan.kind,
                plan.end_altitude_m,
                fly.end_altitude_m
            );
        }
        let flown_m: f64 = flown_trip.iter().map(|phase| phase.distance_m).sum();
        assert!(
            (flown_m - route_m).abs() <= 1.0e-6 * route_m,
            "{legs} legs: the trip covers {flown_m} m of {route_m} m"
        );
        let error = planned_trip
            .iter()
            .zip(&flown_trip)
            .map(|(plan, fly)| {
                let (p, f) = if plan.kind == PhaseKind::Cruise {
                    (plan.distance_m, fly.distance_m)
                } else {
                    (plan.duration_s, fly.duration_s)
                };
                (f - p).abs() / p.max(1.0)
            })
            .fold(0.0, f64::max);
        println!(
            "{preset} {legs} legs: max leg-length error {:.1} %, capped {capped}",
            100.0 * error
        );

        // The ground time and the reserve program.
        let supplement = load_case
            .native_supplement
            .as_ref()
            .expect("the ground phases and reserves were flown");
        // The flight the Mission page draws before any run is the flight of the run: same report,
        // same route tier, same dispatch and sizing passes, so every phase agrees exactly.
        let design = alas_config::presets::get(preset).unwrap().design_vector;
        let page = alas_pipeline::quick_analysis::planned_flight(
            &config,
            &design,
            None,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap_or_else(|error| panic!("{preset} {legs} legs: the page flight: {error}"));
        let run_phases = supplement.phases();
        assert_eq!(
            kinds(&page.phases),
            kinds(&run_phases),
            "{preset} {legs} legs: page vs run"
        );
        for (shown, flown) in page.phases.iter().zip(&run_phases) {
            let close = |a: f64, b: f64| (a - b).abs() <= 1.0e-6 * a.abs().max(b.abs()).max(1.0);
            assert!(
                close(shown.start_altitude_m, flown.start_altitude_m)
                    && close(shown.end_altitude_m, flown.end_altitude_m)
                    && close(shown.distance_m, flown.distance_m)
                    && close(shown.duration_s, flown.duration_s)
                    && shown
                        .fuel_kg
                        .zip(flown.fuel_kg)
                        .map_or(shown.fuel_kg == flown.fuel_kg, |(a, b)| close(a, b)),
                "{preset} {legs} legs, {:?}: page {shown:?} against run {flown:?}",
                shown.kind
            );
        }
        let close = |a: f64, b: f64| (a - b).abs() <= 1.0e-6 * a.abs().max(b.abs()).max(1.0);
        assert!(close(
            page.load_case.takeoff_mass_kg,
            load_case.takeoff_mass_kg
        ));
        assert!(close(
            page.load_case.zero_fuel_mass_kg,
            load_case.zero_fuel_mass_kg
        ));
        assert!(close(
            page.landing_mass_kg,
            mission.completed_summary().unwrap().landing_mass_kg
        ));
        // The sizing on the flown mission converged: the fuel on board covers the trip, the flown
        // reserves, the taxi-in and the contingency.
        let sizing = supplement.sizing.expect("the sizing outcome is reported");
        // A takeoff mass pinned by the mass or tank limit cannot carry the flown mission's fuel: the
        // route is then infeasible (the policy case says so) and the sizing reports it as not
        // converged; everywhere else the sizing converges and the fuel covers the flight.
        let limited = load_case
            .route_case()
            .is_some_and(|case| format!("{:?}", case.analytic.status).contains("Limited"));
        assert!(
            sizing.reserves_flown && !sizing.clamped,
            "{preset} {legs} legs: {sizing:?}"
        );
        if limited {
            assert!(
                sizing.converged || sizing.fuel_residual_kg < 0.0,
                "{preset} {legs} legs: {sizing:?}"
            );
        } else {
            assert!(sizing.converged, "{preset} {legs} legs: {sizing:?}");
            assert!(
                sizing.fuel_residual_kg >= -2.0e-4 * load_case.takeoff_mass_kg,
                "{preset} {legs} legs: fuel short by {} kg",
                -sizing.fuel_residual_kg
            );
        }
        let planned_ground: Vec<&Phase> = planned
            .phases
            .iter()
            .filter(|phase| phase.kind.is_ground())
            .collect();
        assert_eq!(planned_ground.len(), 2);
        assert_eq!(planned_ground[0].kind, PhaseKind::TaxiOut);
        assert_eq!(planned_ground[1].kind, PhaseKind::TaxiIn);
        assert!(
            (supplement.taxi_out.duration_s - planned_ground[0].duration_s).abs() < 1.0e-9
                && (supplement.taxi_in.duration_s - planned_ground[1].duration_s).abs() < 1.0e-9
        );
        assert!(supplement.taxi_out.fuel_kg.unwrap() > 0.0);
        let reserves = supplement
            .reserves
            .as_ref()
            .unwrap_or_else(|| panic!("{legs} legs: {:?}", supplement.reserve_error));
        let planned_reserve: Vec<Phase> = planned
            .phases
            .iter()
            .filter(|phase| phase.is_reserve())
            .cloned()
            .collect();
        assert_eq!(
            kinds(&reserves.phases),
            kinds(&planned_reserve),
            "{legs} legs"
        );
        for (plan, fly) in planned_reserve.iter().zip(&reserves.phases) {
            if plan.kind == PhaseKind::Hold {
                assert!((plan.duration_s - fly.duration_s).abs() < 1.0e-6 * plan.duration_s);
            }
            // The diversion's climb and cruise level follow the altitude fit of the short alternate
            // distance after the guidance revises the rates (the footprint moves); the holding
            // altitude and the touchdown are fixed by the field.
            if matches!(plan.kind, PhaseKind::Hold | PhaseKind::DiversionLanding) {
                assert!(
                    (plan.end_altitude_m - fly.end_altitude_m).abs() < 1.0,
                    "{legs} legs, {:?}: end altitude planned {} m, flown {} m",
                    plan.kind,
                    plan.end_altitude_m,
                    fly.end_altitude_m
                );
            }
        }
        assert!(reserves.alternate_fuel_kg > 0.0 && reserves.hold_fuel_kg() > 0.0);
        error
    }
}

#[test]
fn after_a_full_evaluation_the_flown_phases_are_the_planned_phases() {
    for legs in 1..=3 {
        check("A320-200", legs);
    }
}

/// A turboprop and the two aircraft whose default ladder is above what they hold at the dispatched
/// mass (B787-9, A400M): the ladder is refitted under the level they hold with real step climbs.
#[test]
fn the_page_flight_is_the_run_flight_on_a_turboprop_and_on_lowered_ladders() {
    for name in ["ATR72-600", "B787-9", "A400M"] {
        check(name, 3);
    }
}

/// The page flies the route the run flies: with a dispatched route (a detour of the great circle)
/// both resolve it once, with the same planner.
/// The preset's own 3-leg ladder on the two aircraft that cannot hold its top level at the dispatched
/// mass: the ladder is refitted under the level they hold, with N - 1 real step climbs (checked in
/// `check`), and the lowering is reported.
#[test]
fn the_default_ladder_is_refitted_with_real_step_climbs_on_the_b787_and_the_a400m() {
    for name in ["B787-9", "A400M"] {
        check(name, 0);
    }
}

#[test]
fn the_page_and_the_run_fly_one_dispatched_route() {
    use alas_route::route::{Route, RouteSource};
    let config = config("A320-200", 3);
    let origin = get_airport(&config.departure_airport).unwrap();
    let destination = get_airport(&config.arrival_airport).unwrap();
    let mut route = Route::great_circle(
        origin,
        destination,
        config.mission.great_circle_points as usize,
    );
    route.source = RouteSource::SimbriefApi;
    let middle = route.waypoints.len() / 2;
    route.waypoints[middle].lat += 1.0;
    let design = alas_config::presets::get("A320-200").unwrap().design_vector;
    let page = alas_pipeline::quick_analysis::planned_flight(
        &config,
        &design,
        Some(route.clone()),
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    let result = DesignPipeline::new(config.clone())
        .run_with_environment_and_route(&options(), &RunEnvironment::default(), Some(route.clone()))
        .unwrap();
    let supplement = result
        .mission_load_case
        .as_ref()
        .and_then(|case| case.native_supplement.as_deref())
        .expect("the run flew the standard mission");
    let run_phases = supplement.phases();
    assert_eq!(kinds(&page.phases), kinds(&run_phases));
    for (shown, flown) in page.phases.iter().zip(&run_phases) {
        assert!(
            (shown.distance_m - flown.distance_m).abs() <= 1.0e-6 * flown.distance_m.max(1.0)
                && (shown.duration_s - flown.duration_s).abs()
                    <= 1.0e-6 * flown.duration_s.max(1.0),
            "{:?}: page {shown:?} against run {flown:?}",
            shown.kind
        );
    }
    // And it is the detour, not the great circle.
    let trip_m: f64 = page
        .phases
        .iter()
        .filter(|p| !p.is_reserve())
        .map(|p| p.distance_m)
        .sum();
    assert!((trip_m - route.total_distance_m()).abs() <= 1.0e-6 * trip_m);
}

/// Every preset at one, two and three legs (slow: nightly).
#[test]
#[ignore = "flies every preset three times; run with --ignored"]
fn every_preset_completes_and_its_flight_is_its_plan() {
    for name in alas_config::presets::available() {
        for legs in 1..=3 {
            check(name, legs);
        }
    }
}
