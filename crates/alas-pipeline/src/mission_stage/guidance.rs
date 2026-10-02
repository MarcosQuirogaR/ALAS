// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Small, bounded profile adaptations for an aircraft that cannot fly the
//! initially requested vertical-rate schedule.
//!
//! A fixed climb rate is a guidance target, not an aircraft capability. The
//! mission root solve is therefore allowed to reject that target at the
//! propulsion envelope, after which this module proposes a less demanding
//! target and the complete mission is solved again from the initial state.
//! No failed state is propagated into a revised mission.

use alas_mission::segments::{Segment, SegmentKind, SegmentSpec};

/// Smallest positive vertical rate for which a climb or descent remains a
/// meaningful flight segment. A zero-rate segment is not a climb and would
/// make the altitude-time differential singular.
const MIN_VERTICAL_RATE_M_S: f64 = 0.5;

/// Fractional change applied on one guidance revision. Repeated bounded
/// revisions converge quickly without pretending the first residual gives a
/// reliable aircraft-independent climb-rate derivative.
const RATE_REDUCTION_FACTOR: f64 = 0.5;
const RATE_INCREASE_FACTOR: f64 = 1.5;
const SPEED_REDUCTION_FACTOR: f64 = 0.96;

/// A recorded profile change for diagnostics and tracing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct GuidanceChange {
    /// Previous vertical rate, m/s.
    pub old_rate_m_s: f64,
    /// Revised vertical rate, m/s.
    pub new_rate_m_s: f64,
    /// Previous non-final climb endpoint, m.
    pub old_end_altitude_m: f64,
    /// Revised non-final climb endpoint, m.
    pub new_end_altitude_m: f64,
    /// Previous segment true airspeed, m/s.
    pub old_speed_m_s: f64,
    /// Revised segment true airspeed, m/s.
    pub new_speed_m_s: f64,
}

/// A revised cruise true airspeed, m/s.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct CruiseSpeedChange {
    /// Previous cruise true airspeed, m/s.
    pub old_speed_m_s: f64,
    /// Revised cruise true airspeed, m/s.
    pub new_speed_m_s: f64,
}

/// Suffix that names the upper piece of a climb split at its saturation
/// altitude. The first piece keeps the original tag, which schedule code looks
/// up by name (`initial_climb`); repeated splits of an upper piece append the
/// suffix again, so every tag in a schedule stays unique.
const UPPER_CLIMB_TAG_SUFFIX: &str = "_upper";

/// Shortest climb piece worth flying, s at the piece's own rate. A piece that
/// lasts less than one second is not a meaningful segment and would make the
/// altitude-time differential nearly singular, the same reasoning as
/// [`MIN_VERTICAL_RATE_M_S`].
const MIN_CLIMB_PIECE_DURATION_S: f64 = 1.0;

/// Altitude, m, of the last unsaturated control point before the failed climb
/// first asks for full throttle, or `None` when the saturation is not an
/// interior feature of the segment (no saturated point, saturation from the
/// first point, or a piece on either side shorter than
/// [`MIN_CLIMB_PIECE_DURATION_S`]).
///
/// Throttle is the segment's own record of where the requested rate stopped
/// being flyable, so the rate that was flyable below that altitude is kept.
fn first_saturated_altitude_m(
    failed_segment: &Segment,
    start_m: f64,
    end_m: f64,
    climb_rate_m_s: f64,
) -> Option<f64> {
    let conditions = &failed_segment.conditions;
    let saturated = conditions
        .throttle
        .iter()
        .position(|&throttle| !throttle.is_finite() || throttle >= 1.0)?;
    let altitude_m = *conditions.altitude_m.get(saturated.checked_sub(1)?)?;
    let minimum_piece_m = climb_rate_m_s * MIN_CLIMB_PIECE_DURATION_S;
    (altitude_m.is_finite()
        && altitude_m - start_m >= minimum_piece_m
        && end_m - altitude_m >= minimum_piece_m)
        .then_some(altitude_m)
}

/// Reduce an unconverged climb's requested rate while retaining its altitude
/// target. The schedule closure may subsequently reduce that target when the
/// slower climb no longer fits the available route distance.
///
/// A climb that saturates the throttle only part way up is split at the last
/// altitude it could still fly: the lower piece keeps the original rate and
/// tag, and only the upper piece gets the reduced rate. Halving the whole
/// segment instead penalised the unsaturated lower part and, compounding over
/// revisions, drove a climb that was only marginal near the top down to the
/// minimum rate over its entire height.
pub(super) fn adapt_failed_climb(
    schedule: &mut Vec<SegmentSpec>,
    index: usize,
    failed_segment: &Segment,
) -> Option<GuidanceChange> {
    let (altitude_start_m, altitude_end_m, climb_rate_m_s, old_speed_m_s) = {
        let spec = schedule.get(index)?;
        let SegmentKind::Climb {
            altitude_start_m,
            altitude_end_m,
            climb_rate_m_s,
        } = spec.kind
        else {
            return None;
        };
        (
            altitude_start_m,
            altitude_end_m,
            climb_rate_m_s,
            spec.air_speed_m_s,
        )
    };
    if !climb_rate_m_s.is_finite() || !altitude_end_m.is_finite() {
        return None;
    }
    if !old_speed_m_s.is_finite() || old_speed_m_s <= MIN_VERTICAL_RATE_M_S {
        return None;
    }
    if climb_rate_m_s > MIN_VERTICAL_RATE_M_S {
        let start_m =
            altitude_start_m.or_else(|| failed_segment.conditions.altitude_m.first().copied());
        if let Some(saturation_altitude_m) = start_m.and_then(|start_m| {
            first_saturated_altitude_m(failed_segment, start_m, altitude_end_m, climb_rate_m_s)
        }) {
            let reduced_rate_m_s =
                (climb_rate_m_s * RATE_REDUCTION_FACTOR).max(MIN_VERTICAL_RATE_M_S);
            let mut upper = schedule[index].clone();
            upper.tag = format!("{}{UPPER_CLIMB_TAG_SUFFIX}", upper.tag);
            upper.kind = SegmentKind::Climb {
                altitude_start_m: Some(saturation_altitude_m),
                altitude_end_m,
                climb_rate_m_s: reduced_rate_m_s,
            };
            schedule[index].kind = SegmentKind::Climb {
                altitude_start_m: start_m,
                altitude_end_m: saturation_altitude_m,
                climb_rate_m_s,
            };
            schedule.insert(index + 1, upper);
            return Some(GuidanceChange {
                old_rate_m_s: climb_rate_m_s,
                new_rate_m_s: reduced_rate_m_s,
                old_end_altitude_m: altitude_end_m,
                new_end_altitude_m: altitude_end_m,
                old_speed_m_s,
                new_speed_m_s: old_speed_m_s,
            });
        }
    }
    let (new_rate, new_end_altitude_m) = if climb_rate_m_s > MIN_VERTICAL_RATE_M_S {
        (
            (climb_rate_m_s * RATE_REDUCTION_FACTOR).max(MIN_VERTICAL_RATE_M_S),
            altitude_end_m,
        )
    } else {
        let start_altitude_m =
            altitude_start_m.or_else(|| failed_segment.conditions.altitude_m.first().copied())?;
        if !start_altitude_m.is_finite() {
            return None;
        }
        // Once the minimum represented climb rate is still infeasible, a
        // shorter copy of the same climb does not change its local force
        // balance. Keeping it would only drive the solver toward a
        // zero-duration singular segment, so merge the phase away and let the
        // next schedule item inherit the achieved altitude.
        schedule.remove(index);
        return Some(GuidanceChange {
            old_rate_m_s: climb_rate_m_s,
            new_rate_m_s: climb_rate_m_s,
            old_end_altitude_m: altitude_end_m,
            new_end_altitude_m: start_altitude_m,
            old_speed_m_s,
            new_speed_m_s: old_speed_m_s,
        });
    };
    let new_speed_m_s = old_speed_m_s;
    if new_rate >= climb_rate_m_s
        && new_end_altitude_m >= altitude_end_m
        && new_speed_m_s >= old_speed_m_s
    {
        return None;
    }
    schedule[index].kind = SegmentKind::Climb {
        altitude_start_m: altitude_start_m
            .or_else(|| failed_segment.conditions.altitude_m.first().copied()),
        altitude_end_m: new_end_altitude_m,
        climb_rate_m_s: new_rate,
    };
    schedule[index].air_speed_m_s = new_speed_m_s;
    Some(GuidanceChange {
        old_rate_m_s: climb_rate_m_s,
        new_rate_m_s: new_rate,
        old_end_altitude_m: altitude_end_m,
        new_end_altitude_m,
        old_speed_m_s,
        new_speed_m_s,
    })
}

/// Revise a descent rate the aircraft could not fly, in the direction the
/// solver's own refusal indicates. The rate is kept below the true airspeed so
/// the horizontal component remains real, and above
/// [`MIN_VERTICAL_RATE_M_S`] so the segment remains a descent.
///
/// # Why the direction has to be chosen rather than fixed
///
/// A descent can fail at either end of the propulsion envelope and the two
/// need opposite revisions:
///
/// - **Not enough thrust.** At the commanded rate and speed the aircraft
///   cannot hold its energy; the descent must become *steeper*, trading
///   altitude for the energy the engine will not supply.
/// - **Too much thrust at flight idle** (`required_below_idle`). The engine's
///   own floor already delivers more force than the commanded profile can
///   absorb, so the solver asks for a command below it. Steepening makes that
///   strictly worse: the forward weight component grows and the aircraft needs
///   even less thrust. The descent must become *shallower*, so that the
///   thrust the profile requires rises back to something the engine can hold.
///
/// This function increased the rate unconditionally until the solver could
/// distinguish the two. It could not before: below the deck's flight-idle
/// floor every command produced the identical force, so a sub-idle descent
/// surfaced only as a stalled root find, and every revision moved it further
/// from a schedule it could fly until the increase saturated against
/// `maximum_rate` and the loop gave up.
///
/// Both branches reuse the existing bounded factors. No new rate, speed or
/// schedule is introduced here, and the convergence gate is untouched: a
/// revised profile still has to be flown and still has to converge.
pub(super) fn adapt_failed_descent(
    schedule: &mut [SegmentSpec],
    index: usize,
    required_below_idle: bool,
) -> Option<GuidanceChange> {
    let spec = schedule.get_mut(index)?;
    let SegmentKind::Descent {
        altitude_start_m,
        altitude_end_m,
        descent_rate_m_s,
    } = spec.kind
    else {
        return None;
    };
    if !descent_rate_m_s.is_finite()
        || descent_rate_m_s <= 0.0
        || !spec.air_speed_m_s.is_finite()
        || spec.air_speed_m_s <= MIN_VERTICAL_RATE_M_S
    {
        return None;
    }
    let maximum_rate = (spec.air_speed_m_s * 0.8).max(MIN_VERTICAL_RATE_M_S);
    let new_rate = if required_below_idle {
        (descent_rate_m_s * RATE_REDUCTION_FACTOR).max(MIN_VERTICAL_RATE_M_S)
    } else {
        (descent_rate_m_s * RATE_INCREASE_FACTOR).min(maximum_rate)
    };
    if (new_rate - descent_rate_m_s).abs() <= f64::EPSILON {
        return None;
    }
    spec.kind = SegmentKind::Descent {
        altitude_start_m,
        altitude_end_m,
        descent_rate_m_s: new_rate,
    };
    Some(GuidanceChange {
        old_rate_m_s: descent_rate_m_s,
        new_rate_m_s: new_rate,
        old_end_altitude_m: altitude_end_m,
        new_end_altitude_m: altitude_end_m,
        old_speed_m_s: spec.air_speed_m_s,
        new_speed_m_s: spec.air_speed_m_s,
    })
}

/// Reduce a throttle-limited cruise speed by a small, bounded amount. This
/// preserves the route distance and altitude while giving an aircraft with a
/// nearly saturated thrust requirement a feasible operating point.
pub(super) fn adapt_failed_cruise(
    schedule: &mut [SegmentSpec],
    index: usize,
) -> Option<CruiseSpeedChange> {
    let spec = schedule.get_mut(index)?;
    if !matches!(spec.kind, SegmentKind::Cruise { .. })
        || !spec.air_speed_m_s.is_finite()
        || spec.air_speed_m_s <= MIN_VERTICAL_RATE_M_S
    {
        return None;
    }
    let old_speed_m_s = spec.air_speed_m_s;
    let new_speed_m_s = (old_speed_m_s * SPEED_REDUCTION_FACTOR).max(30.0);
    if new_speed_m_s >= old_speed_m_s {
        return None;
    }
    spec.air_speed_m_s = new_speed_m_s;
    Some(CruiseSpeedChange {
        old_speed_m_s,
        new_speed_m_s,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::mission::SpeedReference;

    fn climb_spec(tag: &str, start: Option<f64>, end: f64, rate: f64) -> SegmentSpec {
        SegmentSpec {
            tag: tag.to_owned(),
            kind: SegmentKind::Climb {
                altitude_start_m: start,
                altitude_end_m: end,
                climb_rate_m_s: rate,
            },
            air_speed_m_s: 120.0,
            air_speed_reference: SpeedReference::TrueAirspeed,
            true_course_rad: 0.0,
            temperature_deviation_k: 0.0,
            number_control_points: 16,
        }
    }

    /// A failed climb over `[start, end]` whose throttle first reaches 1 at
    /// `saturated_from` (a control-point index).
    fn failed_segment(spec: &SegmentSpec, saturated_from: Option<usize>) -> Segment {
        let mut segment =
            Segment::new(spec.clone(), None).unwrap_or_else(|error| panic!("segment: {error}"));
        let SegmentKind::Climb {
            altitude_start_m: Some(start),
            altitude_end_m: end,
            ..
        } = spec.kind
        else {
            panic!("climb spec with a start altitude");
        };
        let points = segment.conditions.altitude_m.len();
        segment.conditions.altitude_m = (0..points)
            .map(|point| start + (end - start) * point as f64 / (points - 1) as f64)
            .collect();
        segment.conditions.throttle = (0..points)
            .map(|point| match saturated_from {
                Some(first) if point >= first => 1.2,
                _ => 0.8,
            })
            .collect();
        segment
    }

    fn rate_and_bounds(spec: &SegmentSpec) -> (f64, f64, f64) {
        let SegmentKind::Climb {
            altitude_start_m,
            altitude_end_m,
            climb_rate_m_s,
        } = spec.kind
        else {
            panic!("expected a climb");
        };
        (
            altitude_start_m.unwrap_or_else(|| panic!("explicit start")),
            altitude_end_m,
            climb_rate_m_s,
        )
    }

    #[test]
    fn a_climb_saturating_part_way_up_is_split_at_its_last_flyable_altitude() {
        let original = climb_spec("initial_climb", Some(1_000.0), 4_000.0, 5.0);
        let failed = failed_segment(&original, Some(10));
        let saturation_m = failed.conditions.altitude_m[9];
        let mut schedule = vec![
            climb_spec("takeoff", Some(0.0), 1_000.0, 8.0),
            original,
            climb_spec("cruise_marker", Some(4_000.0), 5_000.0, 2.0),
        ];

        let change =
            adapt_failed_climb(&mut schedule, 1, &failed).unwrap_or_else(|| panic!("a revision"));

        assert_eq!(schedule.len(), 4);
        assert_eq!(
            schedule[1].tag, "initial_climb",
            "first piece keeps its tag"
        );
        assert_eq!(schedule[2].tag, "initial_climb_upper");
        let (lower_start, lower_end, lower_rate) = rate_and_bounds(&schedule[1]);
        let (upper_start, upper_end, upper_rate) = rate_and_bounds(&schedule[2]);
        assert_eq!(lower_start, 1_000.0);
        assert_eq!(lower_end, saturation_m);
        assert_eq!(lower_rate, 5.0, "the flyable part keeps the requested rate");
        assert_eq!(upper_start, saturation_m);
        assert_eq!(upper_end, 4_000.0);
        assert_eq!(upper_rate, 2.5);
        assert_eq!(change.new_rate_m_s, 2.5);
        let mut tags: Vec<&str> = schedule.iter().map(|spec| spec.tag.as_str()).collect();
        tags.sort_unstable();
        tags.dedup();
        assert_eq!(tags.len(), schedule.len(), "tags stay unique");
    }

    #[test]
    fn repeated_failures_of_the_upper_piece_never_touch_the_lower_rate() {
        let mut schedule = vec![climb_spec("initial_climb", Some(0.0), 6_000.0, 5.0)];
        let failed = failed_segment(&schedule[0], Some(8));
        adapt_failed_climb(&mut schedule, 0, &failed).unwrap_or_else(|| panic!("first split"));
        // The upper piece now also saturates part way up.
        let failed = failed_segment(&schedule[1], Some(8));
        adapt_failed_climb(&mut schedule, 1, &failed).unwrap_or_else(|| panic!("second split"));

        assert_eq!(schedule.len(), 3);
        assert_eq!(schedule[2].tag, "initial_climb_upper_upper");
        assert_eq!(rate_and_bounds(&schedule[0]).2, 5.0);
        assert_eq!(rate_and_bounds(&schedule[1]).2, 2.5);
        assert_eq!(rate_and_bounds(&schedule[2]).2, 1.25);
        assert!(schedule
            .iter()
            .all(|spec| rate_and_bounds(spec).2 >= MIN_VERTICAL_RATE_M_S));
    }

    #[test]
    fn a_climb_saturated_from_its_first_point_still_halves_the_whole_segment() {
        let original = climb_spec("initial_climb", Some(1_000.0), 4_000.0, 5.0);
        let failed = failed_segment(&original, Some(0));
        let mut schedule = vec![original];

        adapt_failed_climb(&mut schedule, 0, &failed).unwrap_or_else(|| panic!("a revision"));

        assert_eq!(schedule.len(), 1);
        assert_eq!(rate_and_bounds(&schedule[0]), (1_000.0, 4_000.0, 2.5));
    }

    #[test]
    fn a_climb_without_a_saturated_point_halves_the_whole_segment() {
        let original = climb_spec("initial_climb", Some(1_000.0), 4_000.0, 5.0);
        let failed = failed_segment(&original, None);
        let mut schedule = vec![original];

        adapt_failed_climb(&mut schedule, 0, &failed).unwrap_or_else(|| panic!("a revision"));

        assert_eq!(schedule.len(), 1);
        assert_eq!(rate_and_bounds(&schedule[0]).2, 2.5);
    }

    #[test]
    fn the_upper_piece_rate_is_floored_at_the_minimum_vertical_rate() {
        let original = climb_spec("initial_climb", Some(0.0), 3_000.0, 0.8);
        let failed = failed_segment(&original, Some(8));
        let mut schedule = vec![original];

        adapt_failed_climb(&mut schedule, 0, &failed).unwrap_or_else(|| panic!("a revision"));

        assert_eq!(schedule.len(), 2);
        assert_eq!(rate_and_bounds(&schedule[1]).2, MIN_VERTICAL_RATE_M_S);
        assert_eq!(rate_and_bounds(&schedule[0]).2, 0.8);
    }
}

#[cfg(test)]
mod preset_regression {
    use crate::full_analysis::FullAnalysis;

    /// A whole-segment halving takes the ATR72-600 initial climb from 5.08 to
    /// 0.635 m/s and about 94 minutes (5,590 s for the climb alone). Only the top
    /// of the climb saturates the throttle, so splitting at that altitude keeps
    /// the full rate where the engine can fly it. The bound is a regression
    /// guard well below the whole-segment value, not a claim about the
    /// aircraft's real climb.
    #[test]
    fn the_atr72_initial_climb_is_no_longer_halved_as_one_block() {
        let config =
            alas_config::AlasConfig::from_value(&serde_json::json!({"preset": "ATR72-600"}))
                .unwrap_or_else(|error| panic!("ATR preset: {error}"));
        let design = alas_config::presets::get("ATR72-600")
            .unwrap_or_else(|error| panic!("ATR preset: {error}"))
            .design_vector;
        let report = FullAnalysis::new(config.clone())
            .run(&design, true)
            .unwrap_or_else(|error| panic!("ATR report: {error}"));
        let origin = alas_config::airports::get(&config.departure_airport)
            .unwrap_or_else(|error| panic!("origin: {error}"));
        let destination = alas_config::airports::get(&config.arrival_airport)
            .unwrap_or_else(|error| panic!("destination: {error}"));
        let route = alas_route::route::Route::great_circle(
            origin,
            destination,
            config.mission.great_circle_points as usize,
        );
        let (result, _) = super::super::evaluate(
            &config,
            &report,
            origin,
            destination,
            route.total_distance_m(),
        )
        .unwrap_or_else(|error| panic!("ATR mission: {error}"));
        let result = result.unwrap_or_else(|| panic!("ATR native flight"));

        let climb_time_s: f64 = result
            .segments
            .iter()
            .filter(|segment| segment.spec.tag.starts_with("initial_climb"))
            .map(|segment| {
                let time = &segment.conditions.time_s;
                time[time.len() - 1] - time[0]
            })
            .sum();
        assert!(
            climb_time_s < 75.0 * 60.0,
            "ATR72 initial climb took {:.1} min; the whole-segment halving gave about 94",
            climb_time_s / 60.0
        );
    }
}
