// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The force the native mission asks of the deck below a zero command.
//!
//! The deck's command interval is `[0, 1]`; where the flight-idle net force is negative (ram
//! drag above the idle gross thrust: a high-speed descent) the deck's zero is zero net force
//! and a command below zero was clamped to it. Every negative trial command then returned the
//! same force, the root find met a plateau, and the descent rate search refused rates the
//! engine can fly (AVE `descent_1`, 6.7 m/s and above against a 3 degree path of 13 m/s). The
//! relation `F = f F(1)` is carried below zero as it is above the rating.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::full_analysis::FullAnalysis;
use alas_config::airports::get as get_airport;
use alas_mission::segments::{Segment, SegmentKind};

/// At a state whose flight-idle net force is negative: the force at command zero is zero, a
/// negative command continues `f F(1)`, nothing at or above zero moves, and fuel flow stays the
/// deck's idle value.
#[test]
fn a_command_below_zero_continues_the_force_where_the_flight_idle_net_force_is_negative() {
    let config = AlasConfig::default();
    let origin = get_airport(&config.departure_airport).unwrap();
    let destination = get_airport(&config.arrival_airport).unwrap();
    let report = FullAnalysis::new(config.clone())
        .run(
            &alas_config::design_variables::DesignVector::default(),
            true,
        )
        .unwrap();
    let analyses = build_analyses(&config, &report).unwrap();
    let request = build_mission_request(&config, origin, destination, 5_000_000.0);
    let mut descent = build_schedule(&request)
        .unwrap()
        .into_iter()
        .find(|spec| matches!(spec.kind, SegmentKind::Descent { .. }))
        .expect("a descent leg");
    // The upper descent: high speed, thin air, a large ram drag against the idle thrust.
    if let SegmentKind::Descent {
        altitude_end_m,
        descent_rate_m_s,
        ..
    } = descent.kind
    {
        descent.kind = SegmentKind::Descent {
            altitude_start_m: Some(10_500.0),
            altitude_end_m,
            descent_rate_m_s,
        };
    }
    let probe_at = |command: f64| -> (f64, f64, f64) {
        let mut probe = Segment::new(descent.clone(), None).unwrap();
        for throttle in &mut probe.throttle {
            *throttle = command;
        }
        probe.iterate(&analyses);
        (
            probe.conditions.thrust_force_vector_n[0][0],
            probe.conditions.available_throttle_floor[0],
            probe.conditions.vehicle_mass_rate_kg_s[0],
        )
    };
    let (rated, _, _) = probe_at(1.0);
    let (at_zero, floor, idle_fuel_flow) = probe_at(0.0);
    assert!(rated.is_finite() && rated > 0.0, "rated force {rated} N");
    assert!(
        floor == 0.0 && at_zero.abs() < 1.0e-6 * rated,
        "the probe state must be one whose idle net force is negative: floor {floor}, \
         force at zero {at_zero} N, rated {rated} N"
    );
    let (negative, floor_below, fuel_below) = probe_at(-0.25);
    assert!(
        (negative + 0.25 * rated).abs() < 1.0e-9 * rated,
        "a command of -0.25 must continue f F(1): {negative} N against {} N",
        -0.25 * rated
    );
    let (more_negative, _, _) = probe_at(-0.5);
    assert!(more_negative < negative && negative < at_zero);
    // The engine's flight-idle net force is a drag here: the lowest command is its fraction of F(1).
    assert!(
        floor_below < 0.0 && floor_below > -0.25,
        "floor {floor_below}"
    );
    assert!(
        (fuel_below - idle_fuel_flow).abs() <= 1.0e-12 * idle_fuel_flow.abs().max(1.0),
        "fuel flow stays the idle value below zero: {fuel_below} against {idle_fuel_flow} kg/s"
    );
    // Nothing inside the envelope moves.
    let (half, _, _) = probe_at(0.5);
    assert!(at_zero < half && half < rated);
}

/// On a take-off-rated leg (the take-off acceleration) a command is the share of the take-off
/// rating at each point: the force and the fuel the leg books are the deck's at that share,
/// and one is the rating itself.
#[test]
fn a_take_off_leg_books_the_fuel_of_the_take_off_rating() {
    let config = AlasConfig::default();
    let origin = get_airport(&config.departure_airport).unwrap();
    let destination = get_airport(&config.arrival_airport).unwrap();
    let report = FullAnalysis::new(config.clone())
        .run(
            &alas_config::design_variables::DesignVector::default(),
            true,
        )
        .unwrap();
    let analyses = build_analyses(&config, &report).unwrap();
    let request = build_mission_request(&config, origin, destination, 5_000_000.0);
    let base = build_schedule(&request)
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let probe_at = |command: f64, rated: bool| {
        let mut spec = base.clone();
        spec.takeoff_rated = rated;
        let mut probe = Segment::new(spec, None).unwrap();
        for throttle in &mut probe.throttle {
            *throttle = command;
        }
        probe.iterate(&analyses);
        probe
    };
    let rated_at = |probe: &Segment, share: f64, point: usize| {
        let c = &probe.conditions;
        let atmosphere = analyses.atmosphere(c.altitude_m[point], 0.0);
        analyses.thrust_at_demand(
            &atmosphere,
            c.altitude_m[point],
            c.velocity_m_s[point],
            c.mach[point],
            c.gravity_m_s2[point],
            alas_prop::system::PropulsionDemand::RatedFraction {
                rating: alas_prop::system::PropulsionRating::TakeoffGoAround,
                fraction: share,
            },
        )
    };
    // The command is the share of the take-off rating at each point: force and fuel are the
    // deck's at that share, and a command of one is exactly the rating, never more.
    for command in [0.3, 0.6, 0.8, 1.0] {
        let flown = probe_at(command, true);
        for point in 0..flown.conditions.len() {
            let expected = rated_at(&flown, command, point);
            let c = &flown.conditions;
            assert!(
                (c.vehicle_mass_rate_kg_s[point] - expected.fuel_flow_rate_kg_s).abs()
                    <= 1.0e-9 * expected.fuel_flow_rate_kg_s,
                "command {command} point {point}: booked {} kg/s, the rating burns {} kg/s",
                c.vehicle_mass_rate_kg_s[point],
                expected.fuel_flow_rate_kg_s
            );
            assert!((c.thrust_force_vector_n[point][0] - expected.thrust_n).abs() < 1.0e-6);
        }
    }
    let at_rating = probe_at(1.0, true);
    let full = rated_at(&at_rating, 1.0, 0);
    assert!(at_rating.conditions.thrust_force_vector_n[0][0] <= full.thrust_n * (1.0 + 1.0e-12));
    // A leg that does not admit the take-off rating books the maximum-climb deck as before,
    // and its command just above one books no less fuel than just below it.
    let below = probe_at(1.0 - 1.0e-6, false);
    let above = probe_at(1.0 + 1.0e-6, false);
    let (b, a) = (
        below.conditions.vehicle_mass_rate_kg_s[0],
        above.conditions.vehicle_mass_rate_kg_s[0],
    );
    assert!(
        (a - b).abs() <= 1.0e-4 * b,
        "fuel steps from {b} to {a} kg/s at the ceiling"
    );
}

/// Through `converge_root`: a descent steeper than the zero-force path but within the engine's
/// flight-idle net drag is flown (its command is negative), and one steeper than flight idle
/// allows is refused as below the idle floor.
#[test]
fn a_descent_may_use_the_idle_net_drag_but_not_go_below_it() {
    let config = AlasConfig::default();
    let origin = get_airport(&config.departure_airport).unwrap();
    let destination = get_airport(&config.arrival_airport).unwrap();
    let report = FullAnalysis::new(config.clone())
        .run(
            &alas_config::design_variables::DesignVector::default(),
            true,
        )
        .unwrap();
    let analyses = build_analyses(&config, &report).unwrap();
    let request = build_mission_request(&config, origin, destination, 5_000_000.0);
    let mut descent = build_schedule(&request)
        .unwrap()
        .into_iter()
        .find(|spec| matches!(spec.kind, SegmentKind::Descent { .. }))
        .expect("a descent leg");
    let mut fly = |rate_m_s: f64| {
        if let SegmentKind::Descent { altitude_end_m, .. } = descent.kind {
            descent.kind = SegmentKind::Descent {
                altitude_start_m: Some(10_500.0),
                altitude_end_m,
                descent_rate_m_s: rate_m_s,
            };
        }
        let mut segment = Segment::new(descent.clone(), None).unwrap();
        let solution = alas_mission::converge_root(&mut segment, &analyses).unwrap();
        // Every point against its own floor: the lowest margin over the floor, and whether any
        // floor is a drag (negative).
        let c = &segment.conditions;
        let margin = c
            .throttle
            .iter()
            .zip(&c.available_throttle_floor)
            .map(|(t, floor)| t - floor.min(0.0))
            .fold(f64::INFINITY, f64::min);
        let drag_floor = c.available_throttle_floor.iter().any(|floor| *floor < 0.0);
        let lowest = c.throttle.iter().copied().fold(f64::INFINITY, f64::min);
        (solution, lowest, margin, drag_floor)
    };
    let mut flown_below_zero = false;
    let mut refused_below_negative_floor = false;
    let mut last_flown_rate = 0.0;
    let mut first_refused_rate = f64::INFINITY;
    for step in 1..=60 {
        let (solution, lowest, margin, drag_floor) = fly(0.5 * f64::from(step));
        if solution.converged && !solution.throttle_limited && !solution.idle_floor_limited {
            flown_below_zero |= lowest < -1.0e-3;
            last_flown_rate = 0.5 * f64::from(step);
            assert!(
                margin >= -alas_mission::ENVELOPE_TOLERANCE,
                "a flown point {margin} below its own floor"
            );
        } else if solution.idle_floor_limited && drag_floor {
            if first_refused_rate.is_infinite() {
                first_refused_rate = 0.5 * f64::from(step);
            }
            // Refused with a negative floor in force: the request is below that floor.
            refused_below_negative_floor |= margin < -alas_mission::ENVELOPE_TOLERANCE;
        }
    }
    // The negative floor of this engine is narrow (about -0.02 of F(1)), narrower than the
    // 0.5 m/s grid step moves the command, so the grid can jump from a positive command straight
    // to a refused one. Bisect the last flown and first refused rates to resolve the band.
    if !flown_below_zero && first_refused_rate.is_finite() {
        let (mut flown, mut refused) = (last_flown_rate, first_refused_rate);
        for _ in 0..12 {
            let mid = 0.5 * (flown + refused);
            let (solution, lowest, _, _) = fly(mid);
            if solution.converged && !solution.throttle_limited && !solution.idle_floor_limited {
                flown_below_zero |= lowest < -1.0e-3;
                flown = mid;
            } else {
                refused = mid;
            }
        }
    }
    assert!(
        flown_below_zero,
        "no rate used the negative flight-idle net force"
    );
    assert!(
        refused_below_negative_floor,
        "no rate was refused below the negative idle floor"
    );
}
