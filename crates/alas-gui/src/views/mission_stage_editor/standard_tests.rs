// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Mission page draws the phase list the pipeline builds, leg for leg.

use super::standard::{self, LegKind};
use super::standard_note;
use alas_config::{AlasConfig, CruiseExtent, StageKind};
use alas_mission::segments::SegmentKind;
use serde_json::json;

const METRES_PER_FOOT: f64 = 0.3048;

fn case(departure: &str, arrival: &str, legs: usize) -> (AlasConfig, f64) {
    let mut config =
        AlasConfig::from_value(&json!({"preset": "AVE"})).expect("the AVE preset loads");
    config.departure_airport = departure.to_owned();
    config.arrival_airport = arrival.to_owned();
    let values = serde_json::to_value(&config).expect("configuration serializes");
    let distance_m = crate::views::mission_profile_inputs::route_distance_m(&values)
        .expect("both airports resolve");
    let (origin, destination) =
        crate::views::mission_profile_inputs::resolved_route_airports(&config)
            .expect("both airports resolve");
    let cruise_m = alas_mission::route_cruise_altitude_m(&config, &origin, &destination);
    alas_mission::configure_cruise_legs(
        &mut config.mission.profile,
        legs,
        cruise_m,
        origin.elevation_m,
    );
    (config, distance_m)
}

fn schedule_of(config: &AlasConfig, distance_m: f64) -> Vec<alas_mission::segments::SegmentSpec> {
    let (origin, destination) =
        crate::views::mission_profile_inputs::resolved_route_airports(config)
            .expect("both airports resolve");
    alas_pipeline::planned_standard_schedule(config, &origin, &destination, distance_m)
        .expect("the pipeline plans the route")
}

#[test]
fn the_plot_sequence_equals_the_pipeline_schedule_at_one_two_and_three_legs() {
    // A long route (EGLL-RJTT, 9,592 km): the planner flies
    // only two cruise legs on EGLL-OMDB (5,497 km), three from about 9,000 km.
    for legs in 1..=3 {
        let (config, distance_m) = case("EGLL", "RJTT", legs);
        let schedule = schedule_of(&config, distance_m);
        let flight = standard::planned(&config, distance_m).expect("the page plans the route");

        // The pipeline's cruise legs, in order, with their distances.
        let cruises: Vec<(f64, f64)> = {
            let mut level_m = 0.0;
            let mut out = Vec::new();
            for segment in &schedule {
                match &segment.kind {
                    SegmentKind::Climb { altitude_end_m, .. }
                    | SegmentKind::Descent { altitude_end_m, .. } => level_m = *altitude_end_m,
                    SegmentKind::Cruise {
                        altitude_m,
                        distance_m,
                    } if segment.tag.starts_with("cruise_step") => {
                        out.push((altitude_m.unwrap_or(level_m), *distance_m));
                    }
                    SegmentKind::Cruise { .. } => {}
                }
            }
            out
        };
        assert_eq!(cruises.len(), legs, "{legs} legs are flown, none dropped");
        assert_eq!(flight.cruise_legs, legs);

        // The page's trip cruise stages, in order, with their distances.
        let drawn: Vec<(f64, f64)> = flight
            .plan
            .stages
            .iter()
            .zip(&flight.legs)
            .filter(|(_, leg)| leg.kind == LegKind::Cruise)
            .filter_map(|(stage, _)| match &stage.kind {
                StageKind::Cruise {
                    extent: CruiseExtent::DistanceKm(km),
                    altitude_ft: Some(ft),
                    ..
                } => Some((ft * METRES_PER_FOOT, km * 1000.0)),
                _ => None,
            })
            .collect();
        assert_eq!(drawn.len(), cruises.len());
        let pipeline_total: f64 = cruises.iter().map(|c| c.1).sum();
        let drawn_total: f64 = drawn.iter().map(|c| c.1).sum();
        for (flown, shown) in cruises.iter().zip(&drawn) {
            assert!(
                (flown.0 - shown.0).abs() < 1.0,
                "level {flown:?} vs {shown:?}"
            );
            assert!(
                (flown.1 / pipeline_total - shown.1 / drawn_total).abs() < 1.0e-6,
                "distance share {flown:?} vs {shown:?}"
            );
        }

        // The kinds in order: taxi, take-off, climb, then a step climb before
        // every further cruise leg, the descent, the landing and the taxi-in,
        // then the reserve program: N legs are exactly N - 1 step climbs.
        let kinds: Vec<LegKind> = flight.legs.iter().map(|leg| leg.kind).collect();
        let mut expected = vec![
            LegKind::TaxiOut,
            LegKind::Takeoff,
            LegKind::Climb,
            LegKind::Cruise,
        ];
        for _ in 1..legs {
            expected.extend([LegKind::StepClimb, LegKind::Cruise]);
        }
        expected.extend([
            LegKind::Descent,
            LegKind::Landing,
            LegKind::TaxiIn,
            LegKind::MissedApproach,
            LegKind::DiversionClimb,
            LegKind::DiversionCruise,
            LegKind::DiversionDescent,
            LegKind::Hold,
            LegKind::DiversionLanding,
        ]);
        assert_eq!(kinds, expected, "{legs} legs");
        assert!(flight.warnings.is_empty(), "{:?}", flight.warnings);
        // The reserve legs, and only they, are reserve stages.
        for (stage, leg) in flight.plan.stages.iter().zip(&flight.legs) {
            assert_eq!(stage.reserve, leg.reserve);
        }
        assert_eq!(flight.legs.iter().filter(|leg| leg.reserve).count(), 6);

        // Every trip leg of the plot ends where the pipeline's leg ends.
        let landing = flight
            .legs
            .iter()
            .find(|leg| leg.kind == LegKind::Landing)
            .expect("the landing");
        assert!((landing.end_m - schedule.last().map_or(0.0, end_m)).abs() < 1.0);
        let steps: Vec<f64> = flight
            .legs
            .iter()
            .filter(|leg| leg.kind == LegKind::StepClimb)
            .map(|leg| leg.end_m)
            .collect();
        assert_eq!(steps.len(), legs - 1);
        if legs > 1 {
            // The last step reaches the cruise altitude; the level below sits
            // about 2,000 ft under it.
            let top_m = cruises[legs - 1].0;
            assert!((steps[steps.len() - 1] - top_m).abs() < 1.0);
            for pair in cruises.windows(2) {
                assert!((pair[1].0 - pair[0].0 - 2_000.0 * METRES_PER_FOOT).abs() < 5.0);
            }
        }

        // The stage plot is built from the same plan.
        let segments = super::model::profile(&flight.plan, 0.0, 0.0, 3_000.0);
        assert_eq!(segments.len(), flight.legs.len());
        assert_eq!(flight.rows.len(), flight.legs.len());
        assert!(segments
            .iter()
            .zip(&flight.legs)
            .all(|(s, l)| s.reserve == l.reserve));
    }
}

fn end_m(segment: &alas_mission::segments::SegmentSpec) -> f64 {
    match &segment.kind {
        SegmentKind::Climb { altitude_end_m, .. } | SegmentKind::Descent { altitude_end_m, .. } => {
            *altitude_end_m
        }
        SegmentKind::Cruise { altitude_m, .. } => altitude_m.unwrap_or(0.0),
    }
}

#[test]
fn two_legs_read_cruise_one_step_climb_cruise_two_with_flight_levels() {
    let (config, distance_m) = case("EGLL", "OMDB", 2);
    let flight = standard::planned(&config, distance_m).expect("the page plans the route");
    let rows = flight.rows.join(" | ");
    assert!(rows.contains("Cruise 1 \u{b7} FL"), "{rows}");
    assert!(rows.contains("Step climb \u{2192} FL"), "{rows}");
    assert!(rows.contains("Cruise 2 \u{b7} FL"), "{rows}");
    assert!(!rows.contains("rest of route"), "{rows}");
    // The ground time and the reserve program are listed, reserve marked.
    assert!(rows.contains("Taxi out"), "{rows}");
    assert!(rows.contains("Hold \u{b7} 30 min"), "{rows}");
    assert!(rows.contains("(Reserve)"), "{rows}");
}

#[test]
fn a_leg_count_the_route_cannot_carry_is_said_next_to_the_radios() {
    // A short sector cannot carry three cruise legs.
    let (config, distance_m) = case("LEMD", "LEBL", 3);
    let (origin, destination) =
        crate::views::mission_profile_inputs::resolved_route_airports(&config)
            .expect("both airports resolve");
    let supported =
        alas_mission::propose_profile_for_route(&config, &origin, &destination, distance_m)
            .expect("the route is proposed")
            .active_cruise_legs;
    assert!(supported < 3, "a 500 km sector carries {supported} legs");
    let planned = standard::planned(&config, distance_m);
    let flown = planned
        .as_ref()
        .expect("the page plans the route")
        .cruise_legs;
    let note = standard_note::leg_note(&config, distance_m, &planned).expect("a note is shown");
    // The planner flies at most the legs the sector carries, so
    // the note says how many of the set legs are flown.
    assert!(flown < 3, "a 500 km sector flies {flown} legs");
    assert_eq!(note, format!("3 legs set; {flown} flown on this route"));

    // The same count on a long route carries no note.
    let (config, distance_m) = case("EGLL", "OMDB", 2);
    let planned = standard::planned(&config, distance_m);
    assert_eq!(standard_note::leg_note(&config, distance_m, &planned), None);
}
