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
        schedule_horizontal_distance(&schedule, request.departure_elevation_m);
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

/// Return the highest-altitude version of `nominal` whose non-cruise legs fit
/// the route. Intermediate climb altitudes scale from the departure elevation;
/// descent altitudes scale from the arrival elevation. Thus a scale of zero is
/// the irreducible direct elevation transition and a scale of one is the
/// configured profile.
fn fit_altitude_profile(
    nominal: &[SegmentSpec],
    departure_elevation_m: f64,
    arrival_elevation_m: f64,
    route_distance_m: f64,
) -> Result<Vec<SegmentSpec>, String> {
    let full_profile =
        scaled_altitude_profile(nominal, departure_elevation_m, arrival_elevation_m, 1.0);
    let full_distance_m = schedule_horizontal_distance(&full_profile, departure_elevation_m);
    if full_distance_m.is_finite() && full_distance_m <= route_distance_m + DISTANCE_TOLERANCE_M {
        return Ok(full_profile);
    }

    let minimum_profile =
        scaled_altitude_profile(nominal, departure_elevation_m, arrival_elevation_m, 0.0);
    let minimum_distance_m = schedule_horizontal_distance(&minimum_profile, departure_elevation_m);
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
        let candidate_distance_m = schedule_horizontal_distance(&candidate, departure_elevation_m);
        if candidate_distance_m.is_finite()
            && candidate_distance_m <= route_distance_m + DISTANCE_TOLERANCE_M
        {
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
        let (nominal_end_m, vertical_rate_m_s, is_final) = match segment.kind {
            SegmentKind::Climb {
                altitude_end_m,
                climb_rate_m_s,
                ..
            } => (altitude_end_m, climb_rate_m_s.abs(), false),
            SegmentKind::Descent {
                altitude_end_m,
                descent_rate_m_s,
                ..
            } => (
                altitude_end_m,
                descent_rate_m_s.abs(),
                segment.tag == "final_landing",
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
        if altitude_change_m.abs() <= ALTITUDE_TOLERANCE_M {
            continue;
        }

        let mut adapted = segment.clone();
        adapted.kind = if altitude_change_m > 0.0 {
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
