// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Altitude fitting and horizontal distance closure for mission schedules.

use super::junction::{family, join_speeds, merge_pieces};
use super::speed_change::{is_speed_change, SpeedMemory};
use super::{ALTITUDE_TOLERANCE_M, DISTANCE_TOLERANCE_M, PROFILE_SCALE_ITERATIONS};
use alas_atmo::us1976_try_compute_values;
use alas_config::mission::{SpeedCaps, SpeedReference};
use alas_mission::segments::{SegmentKind, SegmentSpec};
use alas_mission::{MissionRequest, Numerics};

/// Refit the current altitude profile and redistribute the remaining route
/// distance over its active cruise legs after a guidance revision.
///
/// Guidance changes a vertical rate, so the horizontal footprint of the
/// climb/descent legs changes as well. Keeping the old cruise distances would
/// silently change the requested route. This function makes that dependency
/// explicit and refuses a revised profile that cannot fit between the named
/// airports.
pub(crate) fn close_schedule_distance(
    schedule: &mut Vec<SegmentSpec>,
    request: &MissionRequest,
    memory: &SpeedMemory,
) -> Result<(), String> {
    let refitted = fit_altitude_profile(
        schedule,
        request.departure_elevation_m,
        request.arrival_elevation_m,
        request.route_distance_m,
        memory,
    )?;
    *schedule = refitted;

    let non_cruise_distance_m = footprint_of_schedule(schedule, request.departure_elevation_m);
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
            (matches!(segment.kind, SegmentKind::Cruise { .. }) && !is_speed_change(segment))
                .then_some(index)
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
/// the route and leave the memory's minimum cruise share of it for cruise.
/// Intermediate climb altitudes scale from the departure elevation;
/// descent altitudes scale from the arrival elevation. Thus a scale of zero is
/// the irreducible direct elevation transition and a scale of one is the
/// configured profile.
pub(crate) fn fit_altitude_profile(
    nominal: &[SegmentSpec],
    departure_elevation_m: f64,
    arrival_elevation_m: f64,
    route_distance_m: f64,
    memory: &SpeedMemory,
) -> Result<Vec<SegmentSpec>, String> {
    let limit_m = route_distance_m * (1.0 - memory.min_cruise_fraction());
    let full_profile = scaled_altitude_profile(
        nominal,
        departure_elevation_m,
        arrival_elevation_m,
        1.0,
        memory,
    );
    let full_distance_m = footprint_of_schedule(&full_profile, departure_elevation_m);
    if full_distance_m.is_finite() && full_distance_m <= limit_m {
        return Ok(full_profile);
    }

    let minimum_profile = scaled_altitude_profile(
        nominal,
        departure_elevation_m,
        arrival_elevation_m,
        0.0,
        memory,
    );
    let minimum_distance_m = footprint_of_schedule(&minimum_profile, departure_elevation_m);
    if !minimum_distance_m.is_finite() {
        return Err(
            "mission profile produces a non-finite climb/descent horizontal distance".to_owned(),
        );
    }
    if minimum_distance_m > limit_m + DISTANCE_TOLERANCE_M {
        // A route too short for the level speed changes between the legs does
        // not fly them; only the climbs and descents themselves have to fit.
        let mut stripped = minimum_profile.clone();
        stripped.retain(|spec| !is_speed_change(spec));
        let stripped_distance_m = footprint_of_schedule(&stripped, departure_elevation_m);
        if stripped_distance_m.is_finite() && stripped_distance_m <= limit_m + DISTANCE_TOLERANCE_M
        {
            return Ok(stripped);
        }
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
            memory,
        );
        let candidate_distance_m = footprint_of_schedule(&candidate, departure_elevation_m);
        if candidate_distance_m.is_finite() && candidate_distance_m <= limit_m {
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
    memory: &SpeedMemory,
) -> Vec<SegmentSpec> {
    let merged = merge_pieces(nominal.to_vec());
    let mut profile = Vec::with_capacity(nominal.len());
    let mut current_altitude_m = departure_elevation_m;

    for segment in &merged {
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
                family(&segment.tag).starts_with("final_landing"),
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
        // The speed-restriction altitude (14 CFR 91.117(a)) is a fixed level, not a fraction of
        // the profile: a fit that shortens the profile does not move the climb that meets it, and
        // no descent flown faster than the restriction speed may end below it. Scaled below
        // 10,000 ft, such a descent would fly above 250 kt where that is not allowed, along a band
        // whose vertical rate was not set for it (A350-900 diversion: 266 KCAS down to 6,000 ft,
        // below flight idle), and the rung that does meet 10,000 ft would then be dropped.
        let restriction_m = alas_config::mission::SPEED_RESTRICTION_ALTITUDE_M;
        let scaled_end_m = anchor_m + scale * (nominal_end_m - anchor_m);
        let altitude_end_m = if is_final {
            arrival_elevation_m
        } else if is_climb && (nominal_end_m - restriction_m).abs() < 1.0 {
            nominal_end_m
        } else if !is_climb && faster_than_restriction(segment) {
            scaled_end_m.max(nominal_end_m.min(restriction_m))
        } else {
            scaled_end_m
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
    join_speeds(&mut profile, departure_elevation_m, memory);
    profile
}

/// Whether the descent `segment` flies the 10,000 ft speed-restriction altitude above the
/// restriction's 250 kt calibrated airspeed (14 CFR 91.117(a)), on its own day.
pub(super) fn faster_than_restriction(segment: &SegmentSpec) -> bool {
    use alas_config::mission::{SPEED_RESTRICTION_ALTITUDE_M, SPEED_RESTRICTION_CAS_M_S};
    let cas_m_s = match segment.air_speed_reference {
        SpeedReference::CalibratedAirspeed => segment.air_speed_m_s,
        SpeedReference::TrueAirspeed => {
            let Ok(ambient) = us1976_try_compute_values(
                SPEED_RESTRICTION_ALTITUDE_M,
                segment.temperature_deviation_k,
            ) else {
                return false;
            };
            alas_atmo::airspeed::calibrated_from_true(
                segment.air_speed_m_s,
                ambient.pressure_pa,
                ambient.temperature_k,
            )
            .unwrap_or(f64::INFINITY)
        }
    };
    cas_m_s > SPEED_RESTRICTION_CAS_M_S + super::speed_change::SPEED_IDENTITY_M_S
}

/// Horizontal distance covered by the profile legs in `schedule`.
///
/// Climb/descent specifications defer their starting altitude to the prior
/// segment, so the current altitude is carried while walking the schedule.
/// Cruise distances are already horizontal and are included directly.
pub(crate) fn schedule_horizontal_distance(
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
                distance_m +=
                    profile_horizontal_distance(spec, start, altitude_end_m, climb_rate_m_s);
                current_altitude_m = altitude_end_m;
            }
            SegmentKind::Descent {
                altitude_start_m,
                altitude_end_m,
                descent_rate_m_s,
            } => {
                let start = altitude_start_m.unwrap_or(current_altitude_m);
                distance_m +=
                    profile_horizontal_distance(spec, start, altitude_end_m, descent_rate_m_s);
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
pub(super) fn profile_horizontal_distance_of_schedule(
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
                distance_m +=
                    profile_horizontal_distance(spec, start, altitude_end_m, climb_rate_m_s);
                current_altitude_m = altitude_end_m;
            }
            SegmentKind::Descent {
                altitude_start_m,
                altitude_end_m,
                descent_rate_m_s,
            } => {
                let start = altitude_start_m.unwrap_or(current_altitude_m);
                distance_m +=
                    profile_horizontal_distance(spec, start, altitude_end_m, descent_rate_m_s);
                current_altitude_m = altitude_end_m;
            }
            SegmentKind::Cruise { .. } => {}
        }
    }
    distance_m
}

/// Horizontal distance of everything that is not a cruise share: the climb and
/// descent legs and the level speed-change legs between them.
pub(crate) fn footprint_of_schedule(schedule: &[SegmentSpec], initial_altitude_m: f64) -> f64 {
    let changes: f64 = schedule
        .iter()
        .filter(|spec| is_speed_change(spec))
        .map(|spec| match spec.kind {
            SegmentKind::Cruise { distance_m, .. } => distance_m,
            _ => 0.0,
        })
        .sum();
    profile_horizontal_distance_of_schedule(schedule, initial_altitude_m) + changes
}

/// Still-air horizontal distance of one climb or descent leg, m.
///
/// In [`SpeedReference::TrueAirspeed`] mode the true airspeed is constant and
/// this is the closed form `|dh| / Vz * sqrt(V^2 - Vz^2)`.
///
/// In [`SpeedReference::CalibratedAirspeed`] mode the true airspeed varies
/// along the ramp, and the distance is *the same quadrature the native segment
/// flies*: the true airspeed resolved from the configured CAS at each of the
/// segment's own Chebyshev control-point altitudes (against the segment's ISA
/// deviation, exactly as `alas_mission::segments::Segment` lays the ramp down),
/// its horizontal component at each node, contracted with the segment's
/// integration operator over a time grid that is linear in the node coordinate
/// because the vertical rate is constant. Reusing the segment's discretization
/// rather than a finer or analytic one is what makes the closed route distance
/// equal the distance the pseudospectral mission then actually flies, instead of
/// agreeing with it only to within quadrature error.
///
/// Non-finite when the calibrated airspeed cannot be resolved (supersonic or
/// invalid state) or the resolved true airspeed does not exceed the vertical
/// rate; callers already reject a non-finite footprint, and the segment setup
/// reports the same condition as a typed error.
pub(super) fn profile_horizontal_distance(
    spec: &SegmentSpec,
    altitude_start_m: f64,
    altitude_end_m: f64,
    vertical_rate_m_s: f64,
) -> f64 {
    let rate_m_s = vertical_rate_m_s.abs();
    let altitude_change_m = (altitude_end_m - altitude_start_m).abs();
    let capped = spec.speed_caps != SpeedCaps::default();
    match spec.air_speed_reference {
        SpeedReference::TrueAirspeed
            if !capped && spec.entry_speed_m_s <= 0.0 && spec.exit_speed_m_s <= 0.0 =>
        {
            let horizontal_speed_m_s =
                (spec.air_speed_m_s * spec.air_speed_m_s - rate_m_s * rate_m_s).sqrt();
            altitude_change_m / rate_m_s * horizontal_speed_m_s
        }
        SpeedReference::TrueAirspeed | SpeedReference::CalibratedAirspeed => {
            let mut numerics = Numerics {
                number_control_points: i64::try_from(spec.number_control_points)
                    .unwrap_or(i64::MAX),
                ..Numerics::default()
            };
            if numerics.initialize_differentials_dimensionless().is_err() {
                return f64::NAN;
            }
            let Some(last_row) = numerics.dimensionless.integrate.last() else {
                return f64::NAN;
            };
            let span_s = altitude_change_m / rate_m_s;
            let mut distance_m = 0.0;
            // The speed the leg flies at `altitude_m` on its own schedule.
            let nominal = |altitude_m: f64| -> Option<f64> {
                let atmosphere =
                    us1976_try_compute_values(altitude_m, spec.temperature_deviation_k).ok()?;
                spec.speed_caps
                    .true_airspeed_m_s(
                        spec.air_speed_reference,
                        spec.air_speed_m_s,
                        atmosphere.pressure_pa,
                        atmosphere.temperature_k,
                    )
                    .ok()
            };
            let (Some(nominal_start_m_s), Some(nominal_end_m_s)) =
                (nominal(altitude_start_m), nominal(altitude_end_m))
            else {
                return f64::NAN;
            };
            let ends = alas_mission::segments::PathEnds {
                entry_m_s: spec.entry_speed_m_s,
                exit_m_s: spec.exit_speed_m_s,
                start_altitude_m: altitude_start_m,
                end_altitude_m: altitude_end_m,
                nominal_start_m_s,
                nominal_end_m_s,
            };
            for (&weight, &node) in last_row.iter().zip(&numerics.dimensionless.control_points) {
                let altitude_m = altitude_start_m + node * (altitude_end_m - altitude_start_m);
                let Some(nominal_m_s) = nominal(altitude_m) else {
                    return f64::NAN;
                };
                let true_air_speed_m_s =
                    alas_mission::segments::ramp_speed(&ends, altitude_m, nominal_m_s);
                let horizontal_sq = true_air_speed_m_s * true_air_speed_m_s - rate_m_s * rate_m_s;
                if !horizontal_sq.is_finite() || horizontal_sq <= 0.0 {
                    return f64::NAN;
                }
                distance_m += weight * span_s * horizontal_sq.sqrt();
            }
            distance_m
        }
    }
}
