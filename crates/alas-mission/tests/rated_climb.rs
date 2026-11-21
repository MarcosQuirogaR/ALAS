// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A climb flown at the maximum-climb rating (`SegmentSpec::rating_limited`): the command is the
//! rating at every point and the vertical rate comes out of the force balance; the scheduled rate
//! is only the first guess.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod support;

use alas_mission::segments::{Segment, SegmentKind};
use alas_mission::solve::converge_root;
use serde::Deserialize;
use support::{SurrogateTraining, Vehicle};

#[derive(Debug, Deserialize)]
struct Fixture {
    inputs: Inputs,
    segments: Vec<Recorded>,
}

#[derive(Debug, Deserialize)]
struct Inputs {
    vehicle: Vehicle,
    surrogate_training: SurrogateTraining,
}

#[derive(Debug, Deserialize)]
struct Recorded {
    spec: support::Spec,
}

#[test]
fn a_climb_at_the_rating_climbs_at_its_excess_power_whatever_its_first_guess() {
    let fixture: Fixture = alas_testkit::load("mission", "mission");
    let analyses = support::analyses(&fixture.inputs.vehicle, &fixture.inputs.surrogate_training);
    let spec = fixture
        .segments
        .iter()
        .map(|recorded| recorded.spec.to_spec())
        .find(|spec| {
            matches!(
                spec.kind,
                SegmentKind::Climb {
                    altitude_start_m: Some(_),
                    ..
                }
            )
        })
        .expect("a climb with its own start altitude");
    let SegmentKind::Climb {
        altitude_start_m,
        altitude_end_m,
        climb_rate_m_s,
    } = spec.kind
    else {
        unreachable!()
    };

    let mut commanded = Segment::new(spec.clone(), None).unwrap();
    let solution = converge_root(&mut commanded, &analyses).unwrap();
    assert!(solution.converged, "the commanded climb flies");
    assert!(
        commanded.throttle.iter().all(|&t| t < 1.0),
        "the commanded rate is inside the rating"
    );

    // The same climb at the rating, from the commanded rate and from four times it as the first
    // guess: both settle on the rating, faster than the commanded climb, at the same rates.
    let at_rating = |first_guess_m_s: f64| {
        let mut rated_spec = spec.clone();
        rated_spec.rating_limited = true;
        rated_spec.kind = SegmentKind::Climb {
            altitude_start_m,
            altitude_end_m,
            climb_rate_m_s: first_guess_m_s,
        };
        let mut rated = Segment::new(rated_spec, None).unwrap();
        let solution = converge_root(&mut rated, &analyses).unwrap();
        assert!(solution.converged, "the climb at the rating converges");
        for (point, &throttle) in rated.conditions.throttle.iter().enumerate() {
            assert!(
                (throttle - 1.0).abs() < 1.0e-3,
                "point {point}: the command {throttle} is the rating"
            );
        }
        rated
    };
    let from_commanded = at_rating(climb_rate_m_s);
    let from_steep = at_rating(4.0 * climb_rate_m_s);
    let rates = from_commanded.vertical_rates_m_s();
    for (point, (&rate, &other)) in rates
        .iter()
        .zip(&from_steep.vertical_rates_m_s())
        .enumerate()
    {
        assert!(rate > climb_rate_m_s, "point {point}: {rate} m/s");
        assert!(
            (rate - other).abs() <= 1.0e-2 * rate,
            "point {point}: {rate} vs {other} m/s"
        );
    }
    let duration = |segment: &Segment| {
        segment.conditions.time_s.last().unwrap() - segment.conditions.time_s.first().unwrap()
    };
    assert!(duration(&from_commanded) < duration(&commanded));
}

#[test]
fn a_climb_at_the_rating_that_also_accelerates_settles_on_the_rating_at_every_point() {
    let fixture: Fixture = alas_testkit::load("mission", "mission");
    let analyses = support::analyses(&fixture.inputs.vehicle, &fixture.inputs.surrogate_training);
    let mut spec = fixture
        .segments
        .iter()
        .map(|recorded| recorded.spec.to_spec())
        .find(|spec| {
            matches!(
                spec.kind,
                SegmentKind::Climb {
                    altitude_start_m: Some(_),
                    ..
                }
            )
        })
        .expect("a climb with its own start altitude");
    spec.rating_limited = true;
    let mut steady = Segment::new(spec.clone(), None).unwrap();
    assert!(converge_root(&mut steady, &analyses).unwrap().converged);

    // The same climb entered 10 % slower: it accelerates to its schedule along the path, as the
    // entry and exit pieces of a path change do. The settling holds the rating at every point,
    // the ends included, and the speed it gains is paid for in vertical rate.
    spec.entry_speed_m_s = 0.9 * spec.air_speed_m_s;
    let mut blended = Segment::new(spec, None).unwrap();
    let solution = converge_root(&mut blended, &analyses).unwrap();
    assert!(
        solution.converged,
        "the accelerating climb at the rating converges"
    );
    for (point, &throttle) in blended.conditions.throttle.iter().enumerate() {
        assert!(
            (throttle - 1.0).abs() < 1.0e-3,
            "point {point}: the command {throttle} is the rating"
        );
    }
    let duration = |segment: &Segment| {
        segment.conditions.time_s.last().unwrap() - segment.conditions.time_s.first().unwrap()
    };
    assert!(
        duration(&blended) > duration(&steady),
        "part of the excess power accelerates"
    );
}
