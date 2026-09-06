// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mission profile construction and route-distance closure.

use alas_mission::segments::{SegmentKind, SegmentSpec};
use alas_mission::MissionRequest;

mod closure;
use closure::profile_horizontal_distance_of_schedule;
pub(super) use closure::{
    close_schedule_distance, fit_altitude_profile, schedule_horizontal_distance,
};

const CONTROL_POINTS: usize = 16;
const METRES_PER_FOOT: f64 = 0.3048;
const DISTANCE_TOLERANCE_M: f64 = 1.0e-6;
const ALTITUDE_TOLERANCE_M: f64 = 1.0e-9;
const PROFILE_SCALE_ITERATIONS: usize = 64;

pub(super) fn build_schedule(request: &MissionRequest) -> Result<Vec<SegmentSpec>, String> {
    let p = &request.profile;
    if !request.route_distance_m.is_finite() || request.route_distance_m < 0.0 {
        return Err(format!(
            "mission route distance must be finite and non-negative, got {}",
            request.route_distance_m
        ));
    }
    validate_profile_inputs(request)?;
    // Every configured climb/descent has vertical rate below true airspeed,
    // so changing elevation requires positive horizontal distance. Check
    // this before altitude scaling can discard opposite-direction legs.
    if request.route_distance_m <= DISTANCE_TOLERANCE_M
        && (request.arrival_elevation_m - request.departure_elevation_m).abs()
            > ALTITUDE_TOLERANCE_M
    {
        return Err("mission route distance cannot connect the airport elevations".to_owned());
    }
    let cruise_fractions = [
        p.cruise_1_distance_fraction,
        p.cruise_2_distance_fraction,
        p.cruise_3_distance_fraction,
    ];
    if cruise_fractions
        .iter()
        .any(|fraction| !fraction.is_finite() || *fraction < 0.0)
    {
        return Err("mission cruise distance fractions must be finite and non-negative".to_owned());
    }
    let cruise_altitude = request.cruise_altitude_m;
    let first_level = (cruise_altitude * p.initial_climb_altitude_fraction)
        .max(request.departure_elevation_m + 3000.0);
    let second_level =
        (cruise_altitude * p.step_climb_1_altitude_fraction).max(first_level + 300.0);
    let temperature_deviation_k = request.departure_isa_deviation_c;
    let mut schedule = Vec::with_capacity(12);

    schedule.push(climb(
        "takeoff",
        Some(request.departure_elevation_m),
        request.departure_elevation_m + p.takeoff_altitude_gain_m,
        p.takeoff_air_speed_m_s,
        p.takeoff_climb_rate_m_s,
        temperature_deviation_k,
    ));
    schedule.push(climb(
        "initial_climb",
        None,
        first_level,
        p.initial_climb_air_speed_m_s,
        p.initial_climb_rate_m_s,
        temperature_deviation_k,
    ));
    let active_cruise = [
        p.cruise_1_distance_fraction > 0.0,
        p.cruise_2_distance_fraction > 0.0,
        p.cruise_3_distance_fraction > 0.0,
    ];
    if active_cruise[0] {
        schedule.push(cruise(
            "cruise_step_1",
            None,
            0.0,
            p.cruise_1_air_speed_m_s,
            temperature_deviation_k,
        ));
    }
    if active_cruise[1] {
        schedule.push(climb(
            "step_climb_1",
            None,
            second_level,
            p.step_climb_1_air_speed_m_s,
            p.step_climb_1_rate_m_s,
            temperature_deviation_k,
        ));
        schedule.push(cruise(
            "cruise_step_2",
            None,
            0.0,
            p.cruise_2_air_speed_m_s,
            temperature_deviation_k,
        ));
    }
    if active_cruise[2] {
        schedule.push(climb(
            "step_climb_2",
            None,
            cruise_altitude,
            p.step_climb_2_air_speed_m_s,
            p.step_climb_2_rate_m_s,
            temperature_deviation_k,
        ));
        schedule.push(cruise(
            "cruise_step_3",
            None,
            0.0,
            p.cruise_3_air_speed_m_s,
            temperature_deviation_k,
        ));
    }

    let descent_steps = [
        (
            p.descent_1_altitude_ft,
            p.descent_1_air_speed_m_s,
            p.descent_1_rate_m_s,
        ),
        (
            p.descent_2_altitude_ft,
            p.descent_2_air_speed_m_s,
            p.descent_2_rate_m_s,
        ),
        (
            p.descent_3_altitude_ft,
            p.descent_3_air_speed_m_s,
            p.descent_3_rate_m_s,
        ),
        (
            p.descent_4_altitude_ft,
            p.descent_4_air_speed_m_s,
            p.descent_4_rate_m_s,
        ),
    ];
    let arrival_ft = request.arrival_elevation_m / METRES_PER_FOOT;
    for (index, (altitude_ft, speed, rate)) in descent_steps.into_iter().enumerate() {
        if altitude_ft > arrival_ft {
            schedule.push(descent(
                &format!("descent_{}", index + 1),
                None,
                altitude_ft * METRES_PER_FOOT,
                speed,
                rate,
                temperature_deviation_k,
            ));
        }
    }
    schedule.push(descent(
        "final_landing",
        None,
        request.arrival_elevation_m,
        p.landing_air_speed_m_s,
        p.landing_descent_rate_m_s,
        temperature_deviation_k,
    ));

    // Preserve the full configured altitude profile when it fits. For a short
    // route, reduce its altitude excursion continuously while keeping both
    // airport elevations fixed. This is a profile adaptation, not a minimum
    // route-length rule: climb/descent footprint depends on the selected
    // altitudes, rates and speeds.
    schedule = fit_altitude_profile(
        &schedule,
        request.departure_elevation_m,
        request.arrival_elevation_m,
        request.route_distance_m,
    )?;
    let non_cruise_distance_m =
        profile_horizontal_distance_of_schedule(&schedule, request.departure_elevation_m);
    if !non_cruise_distance_m.is_finite() {
        return Err(
            "mission profile produces a non-finite climb/descent horizontal distance".to_owned(),
        );
    }
    if request.route_distance_m + DISTANCE_TOLERANCE_M < non_cruise_distance_m {
        return Err(format!(
            "mission route distance {:.3} m cannot accommodate the {:.3} m horizontal footprint required to connect the airport elevations",
            request.route_distance_m, non_cruise_distance_m
        ));
    }
    let cruise_remainder_m = (request.route_distance_m - non_cruise_distance_m).max(0.0);
    let cruise_shares = [
        ("cruise_step_1", p.cruise_1_distance_fraction),
        ("cruise_step_2", p.cruise_2_distance_fraction),
        ("cruise_step_3", p.cruise_3_distance_fraction),
    ];
    let fraction_total: f64 = cruise_shares
        .iter()
        .filter(|(tag, _)| schedule.iter().any(|segment| segment.tag == *tag))
        .map(|(_, fraction)| *fraction)
        .sum();
    if !fraction_total.is_finite() {
        return Err("mission cruise distance fractions sum to a non-finite value".to_owned());
    }
    if cruise_remainder_m > 1.0e-9 && fraction_total <= 0.0 {
        return Err(
            "mission cruise distance fractions must include an active leg when route distance remains after profile legs"
                .to_owned(),
        );
    }
    if fraction_total > 0.0 {
        for (tag, fraction) in cruise_shares {
            if let Some(segment) = schedule.iter_mut().find(|segment| segment.tag == tag) {
                if let SegmentKind::Cruise { distance_m, .. } = &mut segment.kind {
                    *distance_m = cruise_remainder_m * fraction / fraction_total;
                }
            }
        }
    }

    let flown_distance_m = schedule_horizontal_distance(&schedule, request.departure_elevation_m);
    if !flown_distance_m.is_finite()
        || (flown_distance_m - request.route_distance_m).abs() > DISTANCE_TOLERANCE_M
    {
        return Err(format!(
            "mission schedule does not close route distance: requested {:.6} m, scheduled {:.6} m",
            request.route_distance_m, flown_distance_m
        ));
    }

    Ok(schedule)
}

/// Validate the aircraft-independent kinematic contract before any
/// pseudospectral segment is constructed. These are profile inputs, not
/// hidden defaults: every aircraft supplies its own speeds, rates and cruise
/// shares, while the mission solver remains agnostic to aircraft type.
fn validate_profile_inputs(request: &MissionRequest) -> Result<(), String> {
    for (name, value) in [
        ("cruise altitude", request.cruise_altitude_m),
        ("departure elevation", request.departure_elevation_m),
        ("arrival elevation", request.arrival_elevation_m),
        ("departure ISA deviation", request.departure_isa_deviation_c),
    ] {
        if !value.is_finite() {
            return Err(format!("mission {name} must be finite, got {value}"));
        }
    }
    if request.cruise_altitude_m <= 0.0 {
        return Err("mission cruise altitude must be positive".to_owned());
    }

    let p = &request.profile;
    if !p.takeoff_altitude_gain_m.is_finite() || p.takeoff_altitude_gain_m < 0.0 {
        return Err("mission takeoff altitude gain must be finite and non-negative".to_owned());
    }
    for (name, speed, rate) in [
        ("takeoff", p.takeoff_air_speed_m_s, p.takeoff_climb_rate_m_s),
        (
            "initial climb",
            p.initial_climb_air_speed_m_s,
            p.initial_climb_rate_m_s,
        ),
        (
            "step climb 1",
            p.step_climb_1_air_speed_m_s,
            p.step_climb_1_rate_m_s,
        ),
        (
            "step climb 2",
            p.step_climb_2_air_speed_m_s,
            p.step_climb_2_rate_m_s,
        ),
        ("descent 1", p.descent_1_air_speed_m_s, p.descent_1_rate_m_s),
        ("descent 2", p.descent_2_air_speed_m_s, p.descent_2_rate_m_s),
        ("descent 3", p.descent_3_air_speed_m_s, p.descent_3_rate_m_s),
        ("descent 4", p.descent_4_air_speed_m_s, p.descent_4_rate_m_s),
        (
            "final landing",
            p.landing_air_speed_m_s,
            p.landing_descent_rate_m_s,
        ),
    ] {
        validate_speed_rate(name, speed, rate)?;
    }
    for (name, speed) in [
        ("cruise 1", p.cruise_1_air_speed_m_s),
        ("cruise 2", p.cruise_2_air_speed_m_s),
        ("cruise 3", p.cruise_3_air_speed_m_s),
    ] {
        if !speed.is_finite() || speed <= 0.0 {
            return Err(format!(
                "mission {name} airspeed must be finite and positive, got {speed}"
            ));
        }
    }
    for (name, fraction) in [
        (
            "initial climb altitude fraction",
            p.initial_climb_altitude_fraction,
        ),
        (
            "step climb 1 altitude fraction",
            p.step_climb_1_altitude_fraction,
        ),
    ] {
        if !fraction.is_finite() || fraction < 0.0 {
            return Err(format!(
                "mission {name} must be finite and non-negative, got {fraction}"
            ));
        }
    }
    for (name, altitude_ft) in [
        ("descent 1 altitude", p.descent_1_altitude_ft),
        ("descent 2 altitude", p.descent_2_altitude_ft),
        ("descent 3 altitude", p.descent_3_altitude_ft),
        ("descent 4 altitude", p.descent_4_altitude_ft),
    ] {
        if !altitude_ft.is_finite() || altitude_ft < 0.0 {
            return Err(format!(
                "mission {name} must be finite and non-negative, got {altitude_ft}"
            ));
        }
    }
    Ok(())
}

fn validate_speed_rate(name: &str, speed_m_s: f64, rate_m_s: f64) -> Result<(), String> {
    if !speed_m_s.is_finite() || speed_m_s <= 0.0 {
        return Err(format!(
            "mission {name} airspeed must be finite and positive, got {speed_m_s}"
        ));
    }
    if !rate_m_s.is_finite() || rate_m_s <= 0.0 {
        return Err(format!(
            "mission {name} vertical rate must be finite and positive, got {rate_m_s}"
        ));
    }
    if rate_m_s >= speed_m_s {
        return Err(format!(
            "mission {name} vertical rate {rate_m_s} m/s must be below airspeed {speed_m_s} m/s"
        ));
    }
    Ok(())
}

fn climb(
    tag: &str,
    start: Option<f64>,
    end: f64,
    speed: f64,
    rate: f64,
    temperature_deviation_k: f64,
) -> SegmentSpec {
    SegmentSpec {
        tag: tag.to_owned(),
        kind: SegmentKind::Climb {
            altitude_start_m: start,
            altitude_end_m: end,
            climb_rate_m_s: rate,
        },
        air_speed_m_s: speed,
        true_course_rad: 0.0,
        temperature_deviation_k,
        number_control_points: CONTROL_POINTS,
    }
}

fn cruise(
    tag: &str,
    altitude: Option<f64>,
    distance: f64,
    speed: f64,
    temperature_deviation_k: f64,
) -> SegmentSpec {
    SegmentSpec {
        tag: tag.to_owned(),
        kind: SegmentKind::Cruise {
            altitude_m: altitude,
            distance_m: distance,
        },
        air_speed_m_s: speed,
        true_course_rad: 0.0,
        temperature_deviation_k,
        number_control_points: CONTROL_POINTS,
    }
}

fn descent(
    tag: &str,
    start: Option<f64>,
    end: f64,
    speed: f64,
    rate: f64,
    temperature_deviation_k: f64,
) -> SegmentSpec {
    SegmentSpec {
        tag: tag.to_owned(),
        kind: SegmentKind::Descent {
            altitude_start_m: start,
            altitude_end_m: end,
            descent_rate_m_s: rate,
        },
        air_speed_m_s: speed,
        true_course_rad: 0.0,
        temperature_deviation_k,
        number_control_points: CONTROL_POINTS,
    }
}
