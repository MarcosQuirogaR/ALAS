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

use super::dispatch::field_climb;

/// Smallest positive vertical rate for which a descent remains a meaningful
/// flight segment. A zero-rate segment is not a descent and would make the
/// altitude-time differential singular. A climb ends at
/// [`CLIMB_END_RESIDUAL_RATE_M_S`] instead.
const MIN_VERTICAL_RATE_M_S: f64 = 0.5;

/// Residual rate of climb, m/s, below which a climb ends at the altitude it
/// reached: 100 ft/min, the rate that defines the service ceiling, "the highest
/// altitude at which the aircraft can climb with a rate of 100 ft/min" (Sadraey,
/// Aircraft Design: A Systems Engineering Approach, 2013, Sec. 4.3.6, p 140).
/// The same rate below which a path change is flown level, so one rate decides
/// where any climb stops.
pub(super) const CLIMB_END_RESIDUAL_RATE_M_S: f64 =
    alas_config::mission::MINIMUM_PATH_CHANGE_RATE_M_S;

/// Steepest descent the revision may reach, as a share of the true airspeed.
/// The bisection of a descent that lacks thrust runs between the refused rate
/// and this cap (see [`adapt_failed_descent`]).
const MAXIMUM_DESCENT_SHARE: f64 = 0.8;
pub(super) const SPEED_REDUCTION_FACTOR: f64 = 0.96;

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
pub(super) fn first_saturated_altitude_m(
    failed_segment: &Segment,
    start_m: f64,
    end_m: f64,
    climb_rate_m_s: f64,
) -> Option<f64> {
    let conditions = &failed_segment.conditions;
    // The first control point of a climb that must raise the path angle is
    // saturated whatever the rate (the transient of the entry state), so the
    // saturation that bounds the rate is the first one after the first free point.
    let is_saturated = |throttle: &f64| !throttle.is_finite() || *throttle >= 1.0;
    let first_free = conditions
        .throttle
        .iter()
        .position(|throttle| !is_saturated(throttle))?;
    let saturated = first_free
        + conditions.throttle[first_free..]
            .iter()
            .position(is_saturated)?;
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
    reduced_rate_m_s: f64,
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
    if climb_rate_m_s > CLIMB_END_RESIDUAL_RATE_M_S {
        let start_m =
            altitude_start_m.or_else(|| failed_segment.conditions.altitude_m.first().copied());
        if let Some(saturation_altitude_m) = start_m.and_then(|start_m| {
            first_saturated_altitude_m(failed_segment, start_m, altitude_end_m, climb_rate_m_s)
        }) {
            let reduced_rate_m_s = reduced_rate_m_s.max(CLIMB_END_RESIDUAL_RATE_M_S);
            let mut upper = schedule[index].clone();
            upper.entry_speed_m_s = 0.0;
            let base_tag = format!("{}{UPPER_CLIMB_TAG_SUFFIX}", upper.tag);
            upper.tag = base_tag.clone();
            let mut copy = 1;
            while schedule.iter().any(|spec| spec.tag == upper.tag) {
                copy += 1;
                upper.tag = format!("{base_tag}_{copy}");
            }
            upper.kind = SegmentKind::Climb {
                altitude_start_m: Some(saturation_altitude_m),
                altitude_end_m,
                climb_rate_m_s: reduced_rate_m_s,
            };
            // Only the upper piece still ends at the cruise speed.
            schedule[index].entry_speed_m_s = 0.0;
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
    let (new_rate, new_end_altitude_m) = if climb_rate_m_s > CLIMB_END_RESIDUAL_RATE_M_S {
        (
            reduced_rate_m_s.max(CLIMB_END_RESIDUAL_RATE_M_S),
            altitude_end_m,
        )
    } else {
        let start_altitude_m =
            altitude_start_m.or_else(|| failed_segment.conditions.altitude_m.first().copied())?;
        if !start_altitude_m.is_finite() {
            return None;
        }
        // Once the residual climb rate is still infeasible the climb has
        // reached its ceiling: a shorter copy of the same climb does not
        // change its local force balance, so merge the phase away and let the
        // next schedule item inherit the achieved altitude (a flight's first leg stays).
        field_climb::remove_leg(schedule, index)?;
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

/// End a climb the aircraft cannot fly at [`CLIMB_END_RESIDUAL_RATE_M_S`] at
/// the altitude it actually reached, and return that altitude, m: where the
/// refused solve, flown at a rate no lower than the residual, last held the
/// throttle below its limit (the leg is cut there), else the leg's start (the
/// leg is removed, the next item inherits the altitude; a flight's first: `None`).
pub(super) fn end_refused_climb(
    schedule: &mut Vec<SegmentSpec>,
    index: usize,
    failed_segment: &Segment,
) -> Option<f64> {
    let SegmentKind::Climb {
        altitude_start_m,
        altitude_end_m,
        climb_rate_m_s,
    } = schedule.get(index)?.kind
    else {
        return None;
    };
    let start_m = altitude_start_m
        .or_else(|| failed_segment.conditions.altitude_m.first().copied())
        .filter(|altitude_m| altitude_m.is_finite())?;
    let reached_m = (climb_rate_m_s >= CLIMB_END_RESIDUAL_RATE_M_S)
        .then(|| {
            first_saturated_altitude_m(failed_segment, start_m, altitude_end_m, climb_rate_m_s)
        })
        .flatten();
    match reached_m {
        Some(reached_m) => {
            schedule[index].kind = SegmentKind::Climb {
                altitude_start_m: Some(start_m),
                altitude_end_m: reached_m,
                climb_rate_m_s,
            };
            Some(reached_m)
        }
        None => field_climb::remove_leg(schedule, index).map(|()| start_m),
    }
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
///
/// A descent can be refused at both ends: too shallow for the thrust and too
/// steep for flight idle (the LJ60 `final_landing_path_entry`, slowing from
/// 150 to 84 m/s in the landing configuration, lacked thrust at 3 m/s and was
/// below idle at 35 m/s, the midpoint towards the cap). The flyable rates then
/// lie between the two refusals, so the steepening runs towards
/// `idle_refused_m_s`, the smallest rate refused at idle, when it is known,
/// rather than towards the cap.
pub(super) fn adapt_failed_descent(
    schedule: &mut [SegmentSpec],
    index: usize,
    required_below_idle: bool,
    idle_reduced_rate_m_s: f64,
    idle_refused_m_s: Option<f64>,
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
    let cap = (spec.air_speed_m_s * MAXIMUM_DESCENT_SHARE).max(MIN_VERTICAL_RATE_M_S);
    let idle_bound =
        idle_refused_m_s.filter(|refused| *refused > descent_rate_m_s && *refused < cap);
    let maximum_rate = idle_bound.unwrap_or(cap);
    let new_rate = if required_below_idle {
        idle_reduced_rate_m_s.max(MIN_VERTICAL_RATE_M_S)
    } else if idle_bound.is_some_and(|refused| {
        refused - descent_rate_m_s <= super::rate_search::WINDOW_CLOSED_TOLERANCE_M_S
    }) {
        // Against an idle refusal the bracket is closed at a physical
        // resolution: no constant rate flies the leg (refused for thrust
        // below, for idle above), and there is no revision to make.
        return None;
    } else if idle_bound.is_none()
        && maximum_rate - descent_rate_m_s <= super::rate_search::RELATIVE_TOLERANCE * maximum_rate
    {
        // Against the cap, the cap is the answer.
        maximum_rate
    } else {
        // Bisection of [refused rate, cap or idle refusal]: the refused rate is the lower bracket
        // and each revision that is still refused moves it to the midpoint, as
        // `RateSearch` does for climbs (rate_search.rs). The earlier fixed x1.5
        // step jumped over the interval between the refused rate and the cap, so
        // the accepted rate depended on where the first refusal happened to fall.
        // Not validated against the descent_1 branch of AVE; whether this removes the flip
        // is not established.
        0.5 * (descent_rate_m_s + maximum_rate)
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

/// Suffix of the lower piece of a descent cut where it first reaches flight idle.
pub(crate) const LOWER_DESCENT_TAG_SUFFIX: &str = "_lower";

/// Which piece of a cut descent keeps the rate the descent had.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IdleSplit {
    /// The upper piece keeps its rate; the lower piece, where the profile asks
    /// for less than idle thrust, flies the reduced rate.
    UpperKeeps,
    /// The lower piece keeps the rate; the upper piece, idle-limited from the
    /// start of the rung (the thin air at the top of a descent gives the aircraft
    /// less drag to lose), flies the reduced rate.
    LowerKeeps,
}

/// Cut a descent that is idle-limited over only part of its altitude span, so
/// that it is steep where the engine allows and only as shallow as idle requires
/// where it does not: the mirror of the climb staircase.
///
/// Where the flight-idle floor first binds part way down, the first piece keeps
/// its rate and tag and the lower piece gets `reduced_rate_m_s`
/// ([`IdleSplit::UpperKeeps`]). Where it binds from the start of the rung and
/// stops binding lower down, the upper piece (keeping the tag) gets the reduced
/// rate and the lower piece the rate the descent had ([`IdleSplit::LowerKeeps`]):
/// without that cut the whole rung is flown at the shallow rate the top of it
/// needs (A340-300 `descent_1`: 10.9 m/s down to 31,000 ft against the 12.6 m/s a
/// 3 degree path asks and the lower part can fly, 8 % more descent distance and
/// 50 % more fuel than the priced descent, which takes each step at its own limit).
/// `None` when the descent was not cut.
pub(super) fn split_idle_limited_descent(
    schedule: &mut Vec<SegmentSpec>,
    index: usize,
    failed_segment: &Segment,
    reduced_rate_m_s: f64,
) -> Option<IdleSplit> {
    let spec = schedule.get(index)?;
    let SegmentKind::Descent {
        altitude_start_m,
        altitude_end_m,
        descent_rate_m_s,
    } = spec.kind
    else {
        return None;
    };
    let conditions = &failed_segment.conditions;
    // A point binds where its request lies below its own flight-idle floor, whatever the
    // floor's sign: a positive floor (the engine delivers more than the profile asks) or a
    // negative one (the engine's net drag at idle is not drag enough). `conditions.throttle`
    // holds the request (clamped at the lowest command evaluated, far below any floor).
    let binds = |i: usize| {
        conditions
            .available_throttle_floor
            .get(i)
            .zip(conditions.throttle.get(i))
            .is_some_and(|(floor, request)| {
                floor.is_finite()
                    && request.is_finite()
                    && *request < *floor - alas_mission::ENVELOPE_TOLERANCE
            })
    };
    let first = (0..conditions.altitude_m.len()).find(|&i| binds(i))?;
    let (split_m, kind) = if first > 0 {
        (
            conditions.altitude_m.get(first - 1).copied()?,
            IdleSplit::UpperKeeps,
        )
    } else {
        // Bound from the start: the cut is where the floor stops binding.
        let released = (1..conditions.altitude_m.len()).find(|&i| !binds(i))?;
        (
            conditions.altitude_m.get(released).copied()?,
            IdleSplit::LowerKeeps,
        )
    };
    let start_m = altitude_start_m.or_else(|| conditions.altitude_m.first().copied())?;
    let minimum_piece_m = descent_rate_m_s * MIN_CLIMB_PIECE_DURATION_S;
    if !(split_m.is_finite()
        && start_m - split_m >= minimum_piece_m
        && split_m - altitude_end_m >= minimum_piece_m)
    {
        return None;
    }
    let reduced = reduced_rate_m_s.max(MIN_VERTICAL_RATE_M_S);
    let (upper_rate, lower_rate) = match kind {
        IdleSplit::UpperKeeps => (descent_rate_m_s, reduced),
        IdleSplit::LowerKeeps => (reduced, descent_rate_m_s),
    };
    let mut lower = spec.clone();
    lower.entry_speed_m_s = 0.0;
    let base_tag = format!("{}{LOWER_DESCENT_TAG_SUFFIX}", spec.tag);
    lower.tag = base_tag.clone();
    let mut copy = 1;
    while schedule.iter().any(|other| other.tag == lower.tag) {
        copy += 1;
        lower.tag = format!("{base_tag}_{copy}");
    }
    lower.kind = SegmentKind::Descent {
        altitude_start_m: Some(split_m),
        altitude_end_m,
        descent_rate_m_s: lower_rate,
    };
    schedule[index].exit_speed_m_s = 0.0;
    schedule[index].kind = SegmentKind::Descent {
        altitude_start_m: Some(start_m),
        altitude_end_m: split_m,
        descent_rate_m_s: upper_rate,
    };
    schedule.insert(index + 1, lower);
    Some(kind)
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
// A failed unwrap here is a failed test assertion.
#[allow(clippy::unwrap_used)]
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
            speed_caps: alas_config::mission::SpeedCaps::default(),
            true_course_rad: 0.0,
            temperature_deviation_k: 0.0,
            number_control_points: 16,
            takeoff_rated: false,
            rating_limited: false,
            configuration_delta_cd: 0.0,
            load_factor: 1.0,
            entry_speed_m_s: 0.0,
            exit_speed_m_s: 0.0,
            speed_profile: Vec::new(),
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

    /// The earlier fixed policy: retry the failed climb at half its rate.
    fn halve_and_adapt(
        schedule: &mut Vec<SegmentSpec>,
        index: usize,
        failed: &Segment,
    ) -> Option<GuidanceChange> {
        let half = 0.5 * rate_and_bounds(&schedule[index]).2;
        adapt_failed_climb(schedule, index, failed, half)
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
            halve_and_adapt(&mut schedule, 1, &failed).unwrap_or_else(|| panic!("a revision"));

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
        halve_and_adapt(&mut schedule, 0, &failed).unwrap_or_else(|| panic!("first split"));
        // The upper piece now also saturates part way up.
        let failed = failed_segment(&schedule[1], Some(8));
        halve_and_adapt(&mut schedule, 1, &failed).unwrap_or_else(|| panic!("second split"));

        assert_eq!(schedule.len(), 3);
        assert_eq!(schedule[2].tag, "initial_climb_upper_upper");
        assert_eq!(rate_and_bounds(&schedule[0]).2, 5.0);
        assert_eq!(rate_and_bounds(&schedule[1]).2, 2.5);
        assert_eq!(rate_and_bounds(&schedule[2]).2, 1.25);
        assert!(schedule
            .iter()
            .all(|spec| rate_and_bounds(spec).2 >= CLIMB_END_RESIDUAL_RATE_M_S));
    }

    #[test]
    fn a_climb_saturated_from_its_first_point_still_halves_the_whole_segment() {
        let original = climb_spec("initial_climb", Some(1_000.0), 4_000.0, 5.0);
        let failed = failed_segment(&original, Some(0));
        let mut schedule = vec![original];

        halve_and_adapt(&mut schedule, 0, &failed).unwrap_or_else(|| panic!("a revision"));

        assert_eq!(schedule.len(), 1);
        assert_eq!(rate_and_bounds(&schedule[0]), (1_000.0, 4_000.0, 2.5));
    }

    /// The first control point of a climb that must raise its path angle is saturated at any
    /// rate; the saturation that bounds the rate is the first one after the first free point
    /// (A380-800 `step_climb_2`: the whole 8,000 ft rung was flown at the rate of its top,
    /// 363 ft/min, with throttle 0.82 at the bottom).
    #[test]
    fn a_leading_saturated_point_does_not_hide_the_saturation_part_way_up() {
        let original = climb_spec("initial_climb", Some(1_000.0), 4_000.0, 5.0);
        let mut failed = failed_segment(&original, Some(10));
        failed.conditions.throttle[0] = 1.0;
        let saturation_m = failed.conditions.altitude_m[9];
        let mut schedule = vec![original];

        halve_and_adapt(&mut schedule, 0, &failed).unwrap_or_else(|| panic!("a revision"));

        assert_eq!(schedule.len(), 2);
        assert_eq!(rate_and_bounds(&schedule[0]), (1_000.0, saturation_m, 5.0));
        assert_eq!(rate_and_bounds(&schedule[1]), (saturation_m, 4_000.0, 2.5));
    }

    /// A descent whose request lies below a *negative* flight-idle floor (the engine's net drag
    /// at idle is not drag enough for the path asked) is split where the floor stops or starts
    /// binding, like one below a positive floor, and a rung whose floor is negative but whose
    /// request is above it is not split.
    #[test]
    fn a_descent_below_a_negative_idle_floor_is_split_where_it_binds() {
        let mut spec = climb_spec("descent_2", Some(0.0), 3_000.0, 9.0);
        spec.kind = SegmentKind::Descent {
            altitude_start_m: Some(9_000.0),
            altitude_end_m: 6_000.0,
            descent_rate_m_s: 9.0,
        };
        let failed = |binding: &dyn Fn(usize) -> bool| {
            let mut segment =
                Segment::new(spec.clone(), None).unwrap_or_else(|e| panic!("segment: {e}"));
            let points = segment.conditions.altitude_m.len();
            segment.conditions.altitude_m = (0..points)
                .map(|p| 9_000.0 - 3_000.0 * p as f64 / (points - 1) as f64)
                .collect();
            segment.conditions.available_throttle_floor = vec![-0.02; points];
            segment.conditions.throttle = (0..points)
                .map(|p| if binding(p) { -0.08 } else { 0.1 })
                .collect();
            segment
        };
        // Free at the top, below the floor from point 10 down: the upper piece keeps the rate.
        let mut schedule = vec![spec.clone()];
        let segment = failed(&|p| p >= 10);
        let split_m = segment.conditions.altitude_m[9];
        let kind = split_idle_limited_descent(&mut schedule, 0, &segment, 5.0)
            .unwrap_or_else(|| panic!("a split"));
        assert_eq!(kind, IdleSplit::UpperKeeps);
        assert_eq!(schedule.len(), 2);
        let SegmentKind::Descent {
            altitude_end_m,
            descent_rate_m_s,
            ..
        } = schedule[0].kind
        else {
            panic!("descent");
        };
        assert_eq!((altitude_end_m, descent_rate_m_s), (split_m, 9.0));
        // Below the floor at the top only: the lower piece keeps the rate.
        let mut schedule = vec![spec.clone()];
        let segment = failed(&|p| p < 6);
        let kind = split_idle_limited_descent(&mut schedule, 0, &segment, 5.0)
            .unwrap_or_else(|| panic!("a split"));
        assert_eq!(kind, IdleSplit::LowerKeeps);
        // A negative floor that no request is below does not split.
        let mut schedule = vec![spec.clone()];
        let segment = failed(&|_| false);
        assert!(split_idle_limited_descent(&mut schedule, 0, &segment, 5.0).is_none());
    }

    /// A descent refused for thrust is steepened towards the smallest rate refused at idle,
    /// not towards the cap, and a bracket closed between the two refusals is not revised.
    #[test]
    fn a_descent_lacking_thrust_is_steepened_towards_its_idle_refusal() {
        let descent = |rate: f64| {
            let mut spec = climb_spec("final_landing_path_entry", Some(914.0), 576.0, rate);
            spec.kind = SegmentKind::Descent {
                altitude_start_m: Some(914.0),
                altitude_end_m: 576.0,
                descent_rate_m_s: rate,
            };
            spec
        };
        let rate_of = |spec: &SegmentSpec| match spec.kind {
            SegmentKind::Descent {
                descent_rate_m_s, ..
            } => descent_rate_m_s,
            _ => panic!("descent"),
        };
        // No idle refusal known: the bisection runs to the cap, 0.8 x 120 m/s.
        let mut schedule = vec![descent(3.0)];
        adapt_failed_descent(&mut schedule, 0, false, 3.0, None)
            .unwrap_or_else(|| panic!("a revision"));
        assert!((rate_of(&schedule[0]) - 0.5 * (3.0 + 96.0)).abs() < 1.0e-12);
        // Refused at idle at 35 m/s: the bisection runs to 35 m/s.
        let mut schedule = vec![descent(3.0)];
        adapt_failed_descent(&mut schedule, 0, false, 3.0, Some(35.0))
            .unwrap_or_else(|| panic!("a revision"));
        assert!((rate_of(&schedule[0]) - 19.0).abs() < 1.0e-12);
        // An idle refusal at or below the refused rate does not bound it.
        let mut schedule = vec![descent(3.0)];
        adapt_failed_descent(&mut schedule, 0, false, 3.0, Some(2.0))
            .unwrap_or_else(|| panic!("a revision"));
        assert!((rate_of(&schedule[0]) - 49.5).abs() < 1.0e-12);
        // Refused for thrust at 34 m/s and at idle at 35 m/s: a 1 m/s window (3 %)
        // may still hold a constant rate, so the bisection goes on.
        let mut schedule = vec![descent(34.0)];
        adapt_failed_descent(&mut schedule, 0, false, 34.0, Some(35.0))
            .unwrap_or_else(|| panic!("a revision"));
        assert!((rate_of(&schedule[0]) - 34.5).abs() < 1.0e-12);
        // Refused for thrust at 34.995 m/s and at idle at 35 m/s: within the physical
        // resolution no constant rate flies it.
        let mut schedule = vec![descent(34.995)];
        assert!(adapt_failed_descent(&mut schedule, 0, false, 34.995, Some(35.0)).is_none());
        assert_eq!(rate_of(&schedule[0]), 34.995);
    }

    #[test]
    fn a_climb_without_a_saturated_point_halves_the_whole_segment() {
        let original = climb_spec("initial_climb", Some(1_000.0), 4_000.0, 5.0);
        let failed = failed_segment(&original, None);
        let mut schedule = vec![original];

        halve_and_adapt(&mut schedule, 0, &failed).unwrap_or_else(|| panic!("a revision"));

        assert_eq!(schedule.len(), 1);
        assert_eq!(rate_and_bounds(&schedule[0]).2, 2.5);
    }

    #[test]
    fn the_upper_piece_rate_is_floored_at_the_residual_climb() {
        let original = climb_spec("initial_climb", Some(0.0), 3_000.0, 0.8);
        let failed = failed_segment(&original, Some(8));
        let mut schedule = vec![original];

        halve_and_adapt(&mut schedule, 0, &failed).unwrap_or_else(|| panic!("a revision"));

        assert_eq!(schedule.len(), 2);
        assert_eq!(rate_and_bounds(&schedule[1]).2, CLIMB_END_RESIDUAL_RATE_M_S);
        assert_eq!(rate_and_bounds(&schedule[0]).2, 0.8);
    }

    /// 100 ft/min, the residual climb of the service ceiling.
    #[test]
    fn the_residual_climb_is_100_ft_per_minute() {
        assert!((CLIMB_END_RESIDUAL_RATE_M_S - 100.0 * 0.3048 / 60.0).abs() < 1.0e-12);
    }

    #[test]
    fn a_refused_climb_slower_than_the_residual_ends_at_its_start() {
        let original = climb_spec("step_climb_2", Some(10_000.0), 11_000.0, 0.4);
        let failed = failed_segment(&original, Some(8));
        let mut schedule = vec![
            climb_spec("before", Some(9_000.0), 10_000.0, 5.0),
            original,
            climb_spec("after", None, 12_000.0, 5.0),
        ];

        let reached_m = end_refused_climb(&mut schedule, 1, &failed).unwrap();

        assert_eq!(reached_m, 10_000.0);
        assert_eq!(schedule.len(), 2);
        assert_eq!(schedule[1].tag, "after");
    }

    /// The same refused climb as the first leg of a flight (the first of the schedule, or a
    /// plan stage's take-off) is not removed: the leg after it takes its start from it. There is
    /// no revision, and the flight stops on it.
    #[test]
    fn a_refused_first_leg_is_kept_and_not_ended() {
        let first = climb_spec("takeoff@00", Some(100.0), 600.0, 0.4);
        let failed = failed_segment(&first, Some(8));
        let mut schedule = vec![first.clone(), climb_spec("after", None, 3_000.0, 5.0)];
        assert_eq!(end_refused_climb(&mut schedule, 0, &failed), None);
        assert_eq!(
            schedule,
            vec![first.clone(), climb_spec("after", None, 3_000.0, 5.0)]
        );

        let mut takeoff = first.clone();
        takeoff.tag = "takeoff@03".to_owned();
        let failed = failed_segment(&takeoff, Some(8));
        let mut schedule = vec![
            climb_spec("landing@02", Some(600.0), 700.0, 5.0),
            takeoff,
            climb_spec("after", None, 3_000.0, 5.0),
        ];
        assert_eq!(end_refused_climb(&mut schedule, 1, &failed), None);
        assert_eq!(schedule.len(), 3);
    }

    /// A climb already at the residual rate that still fails is merged away, unless it is the
    /// first leg of a flight: that one is kept and there is no revision.
    #[test]
    fn a_failed_first_leg_at_the_residual_rate_is_not_merged_away() {
        let first = climb_spec("takeoff@00", Some(100.0), 600.0, 0.4);
        let failed = failed_segment(&first, Some(8));
        let mut schedule = vec![first.clone(), climb_spec("after", None, 3_000.0, 5.0)];
        assert!(adapt_failed_climb(&mut schedule, 0, &failed, 0.2).is_none());
        assert_eq!(schedule.len(), 2);
        assert_eq!(schedule[0], first);

        let later = climb_spec("climb@01", Some(600.0), 3_000.0, 0.4);
        let failed = failed_segment(&later, Some(8));
        let mut schedule = vec![first, later, climb_spec("after", None, 6_000.0, 5.0)];
        let change = adapt_failed_climb(&mut schedule, 1, &failed, 0.2).unwrap();
        assert_eq!(change.new_end_altitude_m, 600.0);
        assert_eq!(schedule.len(), 2);
        assert_eq!(schedule[1].tag, "after");
    }

    #[test]
    fn a_refused_climb_ends_where_its_throttle_first_saturated() {
        let original = climb_spec("step_climb_2", Some(10_000.0), 11_000.0, 2.0);
        let failed = failed_segment(&original, Some(10));
        let saturation_m = failed.conditions.altitude_m[9];
        let mut schedule = vec![original];

        let reached_m = end_refused_climb(&mut schedule, 0, &failed).unwrap();

        assert_eq!(reached_m, saturation_m);
        assert_eq!(schedule.len(), 1);
        assert_eq!(rate_and_bounds(&schedule[0]), (10_000.0, saturation_m, 2.0));
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
