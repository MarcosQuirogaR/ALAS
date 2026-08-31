// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mission profile construction and route-distance closure.

use alas_mission::segments::{SegmentKind, SegmentSpec};
use alas_mission::MissionRequest;

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

/// Refit the current altitude profile and redistribute the remaining route
/// distance over its active cruise legs after a guidance revision.
///
/// Guidance changes a vertical rate, so the horizontal footprint of the
/// climb/descent legs changes as well. Keeping the old cruise distances would
/// silently change the requested route. This function makes that dependency
/// explicit and refuses a revised profile that cannot fit between the named
/// airports.
pub(super) fn close_schedule_distance(
    schedule: &mut Vec<SegmentSpec>,
    request: &MissionRequest,
) -> Result<(), String> {
    let refitted = fit_altitude_profile(
        schedule,
        request.departure_elevation_m,
        request.arrival_elevation_m,
        request.route_distance_m,
    )?;
    *schedule = refitted;

    let non_cruise_distance_m =
        profile_horizontal_distance_of_schedule(schedule, request.departure_elevation_m);
    if !non_cruise_distance_m.is_finite() {
        return Err(
            "revised mission profile produces a non-finite climb/descent horizontal distance"
                .to_owned(),
        );
    }
    if non_cruise_distance_m > request.route_distance_m + DISTANCE_TOLERANCE_M {
        return Err(format!(
            "revised mission profile requires {:.3} m but the route is only {:.3} m",
            non_cruise_distance_m, request.route_distance_m
        ));
    }

    let cruise_indices: Vec<usize> = schedule
        .iter()
        .enumerate()
        .filter_map(|(index, segment)| {
            matches!(segment.kind, SegmentKind::Cruise { .. }).then_some(index)
        })
        .collect();
    let cruise_distance_total_m: f64 = cruise_indices
        .iter()
        .filter_map(|&index| match schedule[index].kind {
            SegmentKind::Cruise { distance_m, .. } => Some(distance_m),
            _ => None,
        })
        .sum();
    let cruise_remainder_m = (request.route_distance_m - non_cruise_distance_m).max(0.0);
    let configured_weight = |tag: &str| match tag {
        "cruise_step_1" => request.profile.cruise_1_distance_fraction,
        "cruise_step_2" => request.profile.cruise_2_distance_fraction,
        "cruise_step_3" => request.profile.cruise_3_distance_fraction,
        _ => 0.0,
    };
    let weights: Vec<f64> = cruise_indices
        .iter()
        .map(|&index| match schedule[index].kind {
            SegmentKind::Cruise { distance_m, .. }
                if cruise_distance_total_m.is_finite() && cruise_distance_total_m > 0.0 =>
            {
                distance_m.max(0.0)
            }
            _ => configured_weight(&schedule[index].tag).max(0.0),
        })
        .collect();
    let weight_total_m: f64 = weights.iter().sum();
    if cruise_remainder_m > DISTANCE_TOLERANCE_M
        && (!weight_total_m.is_finite() || weight_total_m <= 0.0)
    {
        return Err(
            "revised mission profile has route distance left but no active cruise leg".to_owned(),
        );
    }
    if weight_total_m > 0.0 {
        for (index, weight) in cruise_indices.into_iter().zip(weights) {
            if let SegmentKind::Cruise { altitude_m, .. } = schedule[index].kind {
                schedule[index].kind = SegmentKind::Cruise {
                    altitude_m,
                    distance_m: cruise_remainder_m * weight / weight_total_m,
                };
            }
        }
    }

    let closed_distance_m = schedule_horizontal_distance(schedule, request.departure_elevation_m);
    if !closed_distance_m.is_finite()
        || (closed_distance_m - request.route_distance_m).abs() > DISTANCE_TOLERANCE_M
    {
        return Err(format!(
            "revised mission schedule does not close route distance: requested {:.6} m, scheduled {:.6} m",
            request.route_distance_m, closed_distance_m
        ));
    }
    Ok(())
}

/// Return the highest-altitude version of `nominal` whose non-cruise legs fit
/// the route. Intermediate climb altitudes scale from the departure elevation;
/// descent altitudes scale from the arrival elevation. Thus a scale of zero is
/// the irreducible direct elevation transition and a scale of one is the
/// configured profile.
pub(super) fn fit_altitude_profile(
    nominal: &[SegmentSpec],
    departure_elevation_m: f64,
    arrival_elevation_m: f64,
    route_distance_m: f64,
) -> Result<Vec<SegmentSpec>, String> {
    let full_profile =
        scaled_altitude_profile(nominal, departure_elevation_m, arrival_elevation_m, 1.0);
    let full_distance_m =
        profile_horizontal_distance_of_schedule(&full_profile, departure_elevation_m);
    if full_distance_m.is_finite() && full_distance_m <= route_distance_m {
        return Ok(full_profile);
    }

    let minimum_profile =
        scaled_altitude_profile(nominal, departure_elevation_m, arrival_elevation_m, 0.0);
    let minimum_distance_m =
        profile_horizontal_distance_of_schedule(&minimum_profile, departure_elevation_m);
    if !minimum_distance_m.is_finite() {
        return Err(
            "mission profile produces a non-finite climb/descent horizontal distance".to_owned(),
        );
    }
    if minimum_distance_m > route_distance_m + DISTANCE_TOLERANCE_M {
        return Ok(minimum_profile);
    }

    let mut lower_scale = 0.0;
    let mut upper_scale = 1.0;
    let mut best_profile = minimum_profile;
    for _ in 0..PROFILE_SCALE_ITERATIONS {
        let candidate_scale = 0.5 * (lower_scale + upper_scale);
        let candidate = scaled_altitude_profile(
            nominal,
            departure_elevation_m,
            arrival_elevation_m,
            candidate_scale,
        );
        let candidate_distance_m =
            profile_horizontal_distance_of_schedule(&candidate, departure_elevation_m);
        if candidate_distance_m.is_finite() && candidate_distance_m <= route_distance_m {
            lower_scale = candidate_scale;
            best_profile = candidate;
        } else {
            upper_scale = candidate_scale;
        }
    }
    Ok(best_profile)
}

fn scaled_altitude_profile(
    nominal: &[SegmentSpec],
    departure_elevation_m: f64,
    arrival_elevation_m: f64,
    scale: f64,
) -> Vec<SegmentSpec> {
    let mut profile = Vec::with_capacity(nominal.len());
    let mut current_altitude_m = departure_elevation_m;

    for segment in nominal {
        let (nominal_end_m, vertical_rate_m_s, is_final, is_climb) = match segment.kind {
            SegmentKind::Climb {
                altitude_end_m,
                climb_rate_m_s,
                ..
            } => (altitude_end_m, climb_rate_m_s.abs(), false, true),
            SegmentKind::Descent {
                altitude_end_m,
                descent_rate_m_s,
                ..
            } => (
                altitude_end_m,
                descent_rate_m_s.abs(),
                segment.tag == "final_landing",
                false,
            ),
            SegmentKind::Cruise {
                distance_m,
                altitude_m,
            } => {
                let mut adapted = segment.clone();
                adapted.kind = SegmentKind::Cruise {
                    altitude_m: altitude_m
                        .or_else(|| profile.is_empty().then_some(current_altitude_m)),
                    distance_m,
                };
                profile.push(adapted);
                continue;
            }
        };
        let anchor_m = if matches!(segment.kind, SegmentKind::Climb { .. }) {
            departure_elevation_m
        } else {
            arrival_elevation_m
        };
        let altitude_end_m = if is_final {
            arrival_elevation_m
        } else {
            anchor_m + scale * (nominal_end_m - anchor_m)
        };
        let altitude_change_m = altitude_end_m - current_altitude_m;
        if altitude_change_m.abs() <= ALTITUDE_TOLERANCE_M
            || (is_climb && altitude_change_m < 0.0)
            || (!is_climb && altitude_change_m > 0.0)
        {
            continue;
        }

        let mut adapted = segment.clone();
        adapted.kind = if is_climb {
            SegmentKind::Climb {
                altitude_start_m: Some(current_altitude_m),
                altitude_end_m,
                climb_rate_m_s: vertical_rate_m_s,
            }
        } else {
            SegmentKind::Descent {
                altitude_start_m: Some(current_altitude_m),
                altitude_end_m,
                descent_rate_m_s: vertical_rate_m_s,
            }
        };
        profile.push(adapted);
        current_altitude_m = altitude_end_m;
    }
    profile
}

/// Horizontal distance covered by the profile legs in `schedule`.
///
/// Climb/descent specifications defer their starting altitude to the prior
/// segment, so the current altitude is carried while walking the schedule.
/// Cruise distances are already horizontal and are included directly.
pub(super) fn schedule_horizontal_distance(
    schedule: &[SegmentSpec],
    initial_altitude_m: f64,
) -> f64 {
    let mut current_altitude_m = initial_altitude_m;
    let mut distance_m = 0.0;
    for spec in schedule {
        match spec.kind {
            SegmentKind::Climb {
                altitude_start_m,
                altitude_end_m,
                climb_rate_m_s,
            } => {
                let start = altitude_start_m.unwrap_or(current_altitude_m);
                distance_m += profile_horizontal_distance(
                    start,
                    altitude_end_m,
                    spec.air_speed_m_s,
                    climb_rate_m_s,
                );
                current_altitude_m = altitude_end_m;
            }
            SegmentKind::Descent {
                altitude_start_m,
                altitude_end_m,
                descent_rate_m_s,
            } => {
                let start = altitude_start_m.unwrap_or(current_altitude_m);
                distance_m += profile_horizontal_distance(
                    start,
                    altitude_end_m,
                    spec.air_speed_m_s,
                    descent_rate_m_s,
                );
                current_altitude_m = altitude_end_m;
            }
            SegmentKind::Cruise {
                distance_m: cruise, ..
            } => {
                distance_m += cruise;
            }
        }
    }
    distance_m
}

/// Horizontal distance covered only by climb and descent legs.
fn profile_horizontal_distance_of_schedule(
    schedule: &[SegmentSpec],
    initial_altitude_m: f64,
) -> f64 {
    let mut current_altitude_m = initial_altitude_m;
    let mut distance_m = 0.0;
    for spec in schedule {
        match spec.kind {
            SegmentKind::Climb {
                altitude_start_m,
                altitude_end_m,
                climb_rate_m_s,
            } => {
                let start = altitude_start_m.unwrap_or(current_altitude_m);
                distance_m += profile_horizontal_distance(
                    start,
                    altitude_end_m,
                    spec.air_speed_m_s,
                    climb_rate_m_s,
                );
                current_altitude_m = altitude_end_m;
            }
            SegmentKind::Descent {
                altitude_start_m,
                altitude_end_m,
                descent_rate_m_s,
            } => {
                let start = altitude_start_m.unwrap_or(current_altitude_m);
                distance_m += profile_horizontal_distance(
                    start,
                    altitude_end_m,
                    spec.air_speed_m_s,
                    descent_rate_m_s,
                );
                current_altitude_m = altitude_end_m;
            }
            SegmentKind::Cruise { .. } => {}
        }
    }
    distance_m
}

fn profile_horizontal_distance(
    altitude_start_m: f64,
    altitude_end_m: f64,
    air_speed_m_s: f64,
    vertical_rate_m_s: f64,
) -> f64 {
    let horizontal_speed_m_s =
        (air_speed_m_s * air_speed_m_s - vertical_rate_m_s * vertical_rate_m_s).sqrt();
    (altitude_end_m - altitude_start_m).abs() / vertical_rate_m_s.abs() * horizontal_speed_m_s
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
