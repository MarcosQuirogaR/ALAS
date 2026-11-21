// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Flying one schedule with bounded guidance revisions.
//!
//! A segment that does not converge, or that needs more than full throttle,
//! is not a flown segment. Rather than propagating it, the flight replans
//! the failed leg from the propulsion envelope: a shallower climb, a slower
//! cruise, a gentler descent: closes the route distance again, and flies
//! once more, up to a bounded number of revisions. The same loop serves the
//! dispatch closure, which flies the route at several takeoff masses, and
//! the final flight the pipeline reports.
//!
//! A climb or descent rate the engine refuses is not simply cut: it is
//! bracketed between the largest rate flown and the smallest refused (see
//! [`super::rate_search`]) so a segment ends at the engine's own boundary,
//! the maximum-thrust climb or the flight-idle descent, rather than at an
//! arbitrary fraction of the request.

use std::collections::HashMap;

use alas_mission::segments::{SegmentKind, SegmentSpec};
use alas_mission::{MassChange, Mission, MissionRequest, MissionResult};

use super::dispatch::field_climb;
use super::guidance::{end_refused_climb, CLIMB_END_RESIDUAL_RATE_M_S, SPEED_REDUCTION_FACTOR};
use super::rate_pass::{find_boundary_rates, set_vertical_rate_m_s, vertical_rate_m_s};
use super::rate_search::RateSearch;
use super::rated_climb;
use super::schedule::close_schedule_distance;
use super::schedule::junction::{self, MINIMUM_PATH_CHANGE_RATE_M_S};
use super::schedule::speed_change::{self, SpeedMemory};
use super::{adapt_failed_cruise, adapt_failed_descent, MissionAnalyses};

pub(super) mod descent_pieces;

/// Upper bound on guidance revisions before the last result is returned as
/// it stands, converged or not. Bracketing a rate costs a few evaluations
/// per climb or descent segment, so the bound is larger than a fixed-cut
/// policy needed.
const MAX_GUIDANCE_REVISIONS: usize = 48;

/// Upper bound on rate-finding passes (each re-plans at least one junction).
const MAX_PASSES: usize = 10;

/// Lengthenings of one speed-change leg before the cruise it leads to is slowed instead.
const MAX_LENGTHENINGS: u32 = 4;

/// A closure of the schedule after a revision.
pub(super) type Reclose<'a> =
    dyn FnMut(&mut Vec<SegmentSpec>, &SpeedMemory) -> Result<(), String> + 'a;

/// What a flight needs beyond the schedule: how the route is closed again
/// after a revision, the mass change before each leg, and whether the
/// engine-boundary rate passes run first.
pub(super) struct Hooks<'a> {
    /// Re-close the schedule after a guidance revision.
    pub(super) reclose: &'a mut Reclose<'a>,
    /// The mass change before each leg of the current schedule.
    pub(super) changes: &'a dyn Fn(&[SegmentSpec]) -> Vec<MassChange>,
    /// Whether to run the engine-boundary rate passes before the flight.
    pub(super) rate_passes: bool,
    /// Whether a leg that asks for less force than idle delivers is revised.
    pub(super) adapt_idle_limited: bool,
    /// Every path change the flight flew level because no constant descent
    /// rate flies it, in words: a reported finding, not tracing only.
    pub(super) flown_level: &'a mut Vec<String>,
}

/// Fly `schedule` against `analyses`, replanning failed legs from the
/// available propulsion envelope until the mission completes or the
/// revision budget is spent.
///
/// The returned result is the last flight attempted; a caller reads its
/// `completed_summary` to learn whether the route was actually closed.
pub(super) fn fly_with_guidance(
    schedule: Vec<SegmentSpec>,
    request: &MissionRequest,
    analyses: &MissionAnalyses,
    memory: SpeedMemory,
    adapt_idle_limited: bool,
    flown_level: &mut Vec<String>,
) -> Result<MissionResult, String> {
    // The closure fits the cruise to the footprint of the climbs and descents it computes, which
    // the flight then does not fly exactly (the A400M descents cover 145 km against the 107 km
    // the footprint gave). The cruise is therefore refitted against the ground distance the
    // flight actually covered, so the native flight flies the route distance the dispatch prices:
    // the closing target moves by the flown excess until the flight covers the route.
    let mut target_m = request.route_distance_m;
    for pass in 1..=MAX_ROUTE_CLOSING_PASSES {
        let mut attempt = request.clone();
        attempt.route_distance_m = target_m;
        let mut reclose = |schedule: &mut Vec<SegmentSpec>, memory: &SpeedMemory| {
            close_schedule_distance(schedule, &attempt, memory)
        };
        // The descents are cut into pieces before the flight (the standard mission only: a
        // mission plan keeps its legs).
        let mut cut = schedule.clone();
        descent_pieces::subdivide_descents(&mut cut, request.departure_elevation_m);
        // Only the last pass's flight is returned, so only its notes are kept.
        flown_level.clear();
        let hooks = Hooks {
            reclose: &mut reclose,
            changes: &|_| Vec::new(),
            rate_passes: true,
            adapt_idle_limited,
            flown_level: &mut *flown_level,
        };
        let result = fly_guided(&mut cut, analyses, memory.clone(), hooks)?;
        let Some(flown_m) = result.completed_summary().map(|s| s.distance_flown_m) else {
            // A flight that did not complete is returned as it is: its refusal is the answer.
            return Ok(result);
        };
        let excess_m = flown_m - request.route_distance_m;
        if excess_m.abs() <= ROUTE_DISTANCE_TOLERANCE_M {
            return Ok(result);
        }
        if pass == MAX_ROUTE_CLOSING_PASSES {
            return Err(format!(
                "the native flight covers {flown_m:.1} m against the {:.1} m route after {pass} \
                 closing passes (tolerance {ROUTE_DISTANCE_TOLERANCE_M} m)",
                request.route_distance_m
            ));
        }
        target_m -= excess_m;
    }
    Err("the route closing loop has no passes".to_owned())
}

/// Closing passes of the native flight's route distance against its flown ground distance.
const MAX_ROUTE_CLOSING_PASSES: usize = 4;

/// Tolerance between the flown ground distance and the route distance the dispatch prices, m: a
/// threshold chosen up front (1 m, the scale of the flown legs' integration), not a published value.
/// A residual above it after [`MAX_ROUTE_CLOSING_PASSES`] is an error, not accepted.
const ROUTE_DISTANCE_TOLERANCE_M: f64 = 1.0;

/// What refused a climb, by the solve's own flags, for a flown-level note: the throttle limit
/// (with the largest throttle the root finder asked for, which separates a marginal shortfall
/// from a diverging solve), an unconverged solve with no limit reached, or the idle floor.
pub(super) fn climb_refusal(solution: &alas_mission::SegmentSolution) -> String {
    if solution.throttle_limited {
        format!(
            "climb refused for thrust (throttle asked {:.3})",
            solution.peak_throttle
        )
    } else if !solution.converged {
        format!(
            "climb solve did not converge (MINPACK {:?}, no throttle limit reached)",
            solution.status
        )
    } else {
        "climb refused below flight idle".to_owned()
    }
}

/// Record that the climb `tag` ended at `reached_m`, refused as `refused` at `rate_m_s` with
/// `target_m_s` the next rate to try: the aircraft cannot hold the residual climb of its service
/// ceiling above that altitude, and flies level there.
pub(super) fn note_ended_climb(
    flown_level: &mut Vec<String>,
    tag: &str,
    refused: &str,
    (rate_m_s, target_m_s): (f64, f64),
    reached_m: f64,
) {
    note_flown_level(
        flown_level,
        tag,
        &format!(
            "{refused} at {rate_m_s:.3} m/s; the next rate to try, {target_m_s:.3} m/s, is below \
             the {CLIMB_END_RESIDUAL_RATE_M_S:.3} m/s residual climb of the service ceiling: the \
             climb ends at {reached_m:.0} m"
        ),
    );
}

/// Record that the path change of the leg `tag` is flown level, and why. A later record for
/// the same leg replaces an earlier one, so a leg the guidance revisits is said once.
pub(super) fn note_flown_level(flown_level: &mut Vec<String>, tag: &str, why: &str) {
    let prefix = format!("{tag}: ");
    flown_level.retain(|note| !note.starts_with(&prefix));
    flown_level.push(format!("{prefix}{why}; flown level"));
}

/// [`fly_with_guidance`] on a schedule the caller keeps (it is left as the
/// last flight revised it), with the closure and the mass changes of `hooks`.
pub(super) fn fly_guided(
    schedule: &mut Vec<SegmentSpec>,
    analyses: &MissionAnalyses,
    mut memory: SpeedMemory,
    hooks: Hooks<'_>,
) -> Result<MissionResult, String> {
    let Hooks {
        reclose,
        changes,
        rate_passes,
        adapt_idle_limited,
        flown_level,
    } = hooks;
    // Set every climb and descent to its engine boundary leg by leg, then
    // re-close the route distance around the revised footprint.
    let mut searches: HashMap<String, RateSearch> = HashMap::new();
    let mut lengthened: HashMap<String, u32> = HashMap::new();
    // Per descent, the largest rate the full flight refused for lack of thrust, m/s.
    let mut thrust_refused: HashMap<String, f64> = HashMap::new();
    // Closing the route refits the altitudes (the level changes are part of the
    // footprint), which moves the states the pass solved from: a second pass
    // from the closed schedule re-brackets and re-integrates at them.
    // A path change the engine cannot fly at a meaningful rate is re-planned as a
    // level change, and the passes repeat for the schedule that results.
    for pass in 0..MAX_PASSES {
        if !rate_passes {
            break;
        }
        let replanned = find_boundary_rates(
            schedule,
            analyses,
            &mut searches,
            &mut memory,
            changes,
            flown_level,
        )?;
        reclose(schedule, &memory)?;
        if pass >= 1 && !replanned {
            break;
        }
    }
    for revision in 0..=MAX_GUIDANCE_REVISIONS {
        let result = Mission {
            schedule: schedule.clone(),
        }
        .evaluate_with_changes(analyses, &changes(schedule))
        .map_err(|error| format!("native mission failed: {error}"))?;
        // The first leg of a flight that cannot climb from its start at the rating: the flight
        // stops on it, and the run reports it (`dispatch::field_climb`).
        if let Some(index) = field_climb::refused_first_climb(&result) {
            return Ok(field_climb::stop_on(result, index));
        }

        // Every segment flown without hitting an engine limit proves its
        // current rate flyable.
        for (spec, solution) in schedule.iter().zip(&result.solutions) {
            // A climb at the maximum-climb rating has no rate to search for.
            if spec.rating_limited {
                continue;
            }
            let flown =
                solution.converged && !solution.throttle_limited && !solution.idle_floor_limited;
            if let (true, Some(rate)) = (flown, vertical_rate_m_s(spec)) {
                memory.remember_rate(&spec.tag, rate);
                searches
                    .entry(spec.tag.clone())
                    .or_insert_with(|| {
                        RateSearch::for_climb(matches!(spec.kind, SegmentKind::Climb { .. }))
                    })
                    .record_flown(rate);
            }
        }

        if result.completed_summary().is_some() {
            // Narrow any bracket still wider than the tolerance, earliest
            // segment first (a later one starts from its state).
            let refinement = schedule.iter().enumerate().find_map(|(index, spec)| {
                let rate = searches.get(&spec.tag)?.target()?;
                let current = vertical_rate_m_s(spec)?;
                ((rate - current).abs() > 1.0e-9).then_some((index, rate))
            });
            match refinement {
                Some((index, rate)) if revision < MAX_GUIDANCE_REVISIONS => {
                    tracing::info!(
                        segment = %schedule[index].tag,
                        revision,
                        rate_m_s = rate,
                        "refining a climb or descent rate towards the engine boundary"
                    );
                    set_vertical_rate_m_s(&mut schedule[index], rate);
                    reclose(schedule, &memory)?;
                    continue;
                }
                _ => return Ok(result),
            }
        }
        if revision == MAX_GUIDANCE_REVISIONS {
            return Ok(result);
        }

        let Some(index) = result.segments.len().checked_sub(1) else {
            return Ok(result);
        };
        let segment_tag = schedule[index].tag.clone();
        let solution = &result.solutions[index];
        // A plan, whose masses change between legs, also revises a leg that asks
        // for less force than idle delivers: a speed change is lengthened and a
        // descent made shallower, as the sequential pass does for the mass it saw.
        let idle_limited = adapt_idle_limited && solution.idle_floor_limited;
        let adapted = if !solution.converged || solution.throttle_limited || idle_limited {
            let kind = schedule[index].kind;
            let rated = schedule[index].rating_limited;
            match kind {
                // A climb at the maximum-climb rating that fails on the full flight has met its
                // ceiling there; the first leg of a flight is kept, and the flight stops on it.
                SegmentKind::Climb { .. } if rated => {
                    result.segments.get(index).and_then(|failed| {
                        let outcome = rated_climb::settle(schedule, index, failed, false);
                        rated_climb::note_ceiling(flown_level, &segment_tag, outcome)
                    })
                }
                // A commanded rate the rating refuses: the refused part is flown at
                // the rating. So is a commanded climb the solve could not converge: at the
                // rating its rate is the output of the force balance, not an input to it.
                SegmentKind::Climb { .. }
                    if (solution.throttle_limited || !solution.converged)
                        && !schedule[index].takeoff_rated
                        && result.segments.get(index).is_some_and(|failed| {
                            rated_climb::rate_refused_climb(schedule, index, failed)
                        }) =>
                {
                    searches.remove(&segment_tag);
                    Some(())
                }
                SegmentKind::Climb { climb_rate_m_s, .. } => {
                    // A rate the engine refuses on the full flight (the state at
                    // the leg moved since the sequential pass): record it, cut
                    // the leg and run the pass again from the start, which
                    // re-brackets every leg from the new states.
                    let search = searches
                        .entry(segment_tag.clone())
                        .or_insert_with(|| RateSearch::for_climb(true));
                    search.record_refused(climb_rate_m_s);
                    let target = search.target().unwrap_or(climb_rate_m_s);
                    // Below the residual climb of the service ceiling (the path-change rate) a
                    // path change is flown level, and any other climb ends where it reached.
                    let path_change = junction::path_change_key(&schedule[index]);
                    let level_key = path_change
                        .clone()
                        .filter(|_| target < CLIMB_END_RESIDUAL_RATE_M_S);
                    if let Some(key) = level_key {
                        let refused = climb_refusal(solution);
                        note_flown_level(
                            flown_level,
                            &segment_tag,
                            &format!(
                                "{refused} at {climb_rate_m_s:.3} m/s; the next \
                                 rate to try, {target:.3} m/s, is below the \
                                 {MINIMUM_PATH_CHANGE_RATE_M_S:.3} m/s of a path change"
                            ),
                        );
                        memory.prefer_level(&key);
                        Some(())
                    } else if target < climb_rate_m_s && target >= CLIMB_END_RESIDUAL_RATE_M_S {
                        set_vertical_rate_m_s(&mut schedule[index], target);
                        find_boundary_rates(
                            schedule,
                            analyses,
                            &mut searches,
                            &mut memory,
                            changes,
                            flown_level,
                        )?;
                        tracing::info!(
                            segment = %segment_tag,
                            revision,
                            old_rate_m_s = climb_rate_m_s,
                            new_rate_m_s = target,
                            "re-bracketed the climb rates after a refusal on the full flight"
                        );
                        Some(())
                    } else if path_change.is_none() && target < CLIMB_END_RESIDUAL_RATE_M_S {
                        // Below the residual climb of the service ceiling the climb ends at the
                        // altitude it reached; the flight goes on from there, not from a level
                        // fitted under the priced route.
                        let refused = climb_refusal(solution);
                        result
                            .segments
                            .get(index)
                            .and_then(|failed| end_refused_climb(schedule, index, failed))
                            .map(|reached_m| {
                                note_ended_climb(
                                    flown_level,
                                    &segment_tag,
                                    &refused,
                                    (climb_rate_m_s, target),
                                    reached_m,
                                );
                                searches.remove(&segment_tag);
                                tracing::info!(
                                    segment = %segment_tag,
                                    revision,
                                    reached_m,
                                    "ended a climb below the residual rate at the altitude reached"
                                );
                            })
                    } else {
                        None
                    }
                }
                // A speed change into a climb at the rating that lengthening has not made
                // flyable: the climb ends here, unless a flight starts on it
                // (`dispatch::field_climb`; a reserve diversion from MTOW above the default
                // aircraft's level had stopped with the Mach rung's entry needing 1.44 times the
                // rating).
                SegmentKind::Cruise { .. }
                    if speed_change::is_speed_change(&schedule[index])
                        && lengthened.get(&segment_tag).copied().unwrap_or(0)
                            >= MAX_LENGTHENINGS
                        && schedule.get(index + 1).is_some_and(|next| {
                            next.rating_limited && matches!(next.kind, SegmentKind::Climb { .. })
                        }) =>
                {
                    let change = result.segments.get(index);
                    let throttle = solution.peak_throttle;
                    field_climb::end_climb_after_speed_change(
                        schedule,
                        index,
                        change,
                        throttle,
                        flown_level,
                        revision,
                    )
                }
                // A speed change that lengthening has not made flyable is not an acceleration
                // problem: the speed it ends at is not sustainable at the level. The cruise leg
                // after it is held to a lower speed.
                SegmentKind::Cruise { .. }
                    if speed_change::is_speed_change(&schedule[index])
                        && lengthened.get(&segment_tag).copied().unwrap_or(0)
                            >= MAX_LENGTHENINGS
                        && schedule.get(index + 1).is_some_and(|next| {
                            matches!(next.kind, SegmentKind::Cruise { .. })
                        }) =>
                {
                    let next = &schedule[index + 1];
                    let capped_m_s = (next.air_speed_m_s * SPEED_REDUCTION_FACTOR).max(30.0);
                    memory.cap_cruise_speed(&next.tag.clone(), capped_m_s);
                    tracing::info!(
                        segment = %segment_tag,
                        revision,
                        capped_m_s,
                        "held the cruise after an unflyable speed change to a lower speed"
                    );
                    Some(())
                }
                SegmentKind::Cruise { .. }
                    if speed_change::lengthen(schedule, index, &mut memory) =>
                {
                    *lengthened.entry(segment_tag.clone()).or_default() += 1;
                    tracing::info!(
                        segment = %segment_tag,
                        revision,
                        "lengthened a speed-change leg the aircraft could not fly"
                    );
                    Some(())
                }
                SegmentKind::Cruise { .. } => adapt_failed_cruise(schedule, index).map(|change| {
                    tracing::info!(
                        segment = %segment_tag,
                        revision,
                        old_speed_m_s = change.old_speed_m_s,
                        new_speed_m_s = change.new_speed_m_s,
                        "replanned unconverged cruise below the propulsion envelope"
                    );
                }),
                SegmentKind::Descent {
                    descent_rate_m_s, ..
                } => {
                    // The refusal's own cause chooses the direction: a rung
                    // the engine cannot hold *down* to needs to be flown more
                    // shallowly (towards the flight-idle path), and every
                    // other failure needs it steeper.
                    let below_idle = solution.idle_floor_limited;
                    // The largest rate refused for thrust bounds the flyable rates from
                    // below, the smallest refused at idle from above: each direction
                    // bisects towards the other's refusal once both are known.
                    let thrust_floor = thrust_refused.get(&segment_tag).copied();
                    let reduced = if below_idle {
                        let search = searches
                            .entry(segment_tag.clone())
                            .or_insert_with(|| RateSearch::for_climb(false));
                        search.record_refused(descent_rate_m_s);
                        let target = search.target().unwrap_or(descent_rate_m_s);
                        match thrust_floor.filter(|floor| *floor < descent_rate_m_s) {
                            Some(floor) if target <= floor => 0.5 * (floor + descent_rate_m_s),
                            _ => target,
                        }
                    } else {
                        let floor =
                            thrust_floor.map_or(descent_rate_m_s, |f| f.max(descent_rate_m_s));
                        thrust_refused.insert(segment_tag.clone(), floor);
                        descent_rate_m_s
                    };
                    let idle_refused = searches.get(&segment_tag).and_then(RateSearch::refused_m_s);
                    // Refused for thrust within a physical resolution below a rate refused
                    // at idle (or above it, a bracket that has crossed): no constant rate
                    // flies the leg. The bracket: thrust-refused rate, idle-refused rate, m/s.
                    let tolerance = super::rate_search::WINDOW_CLOSED_TOLERANCE_M_S;
                    let bracket = if below_idle {
                        thrust_floor.map(|floor| (floor, descent_rate_m_s))
                    } else {
                        idle_refused.map(|refused| (descent_rate_m_s, refused))
                    };
                    let window_closed =
                        bracket.is_some_and(|(thrust, idle)| idle - thrust <= tolerance);
                    // A path change the engine can fly only more slowly than a meaningful
                    // descent (plans only), or at no constant rate at all, is flown level.
                    let level_key = junction::path_change_key(&schedule[index]).filter(|_| {
                        window_closed
                            || (adapt_idle_limited
                                && below_idle
                                && reduced < MINIMUM_PATH_CHANGE_RATE_M_S)
                    });
                    if level_key.is_some() {
                        tracing::info!(
                            segment = %segment_tag,
                            revision,
                            descent_rate_m_s,
                            window_closed,
                            "flew a path change level"
                        );
                        // "No constant rate" only where the bracket closed: refused for thrust
                        // and below flight idle within the physical resolution of each other.
                        let why = match bracket.filter(|_| window_closed) {
                            Some((thrust, idle)) => format!(
                                "no constant descent rate flies it (refused for thrust at \
                                 {thrust:.3} m/s, below flight idle at {idle:.3} m/s)"
                            ),
                            None => format!(
                                "descent refused below flight idle at {descent_rate_m_s:.3} \
                                 m/s; the next rate to try, {reduced:.3} m/s, is below the \
                                 {MINIMUM_PATH_CHANGE_RATE_M_S:.3} m/s of a path change"
                            ),
                        };
                        note_flown_level(flown_level, &segment_tag, &why);
                    }
                    if let Some(key) = level_key {
                        memory.prefer_level(&key);
                        find_boundary_rates(
                            schedule,
                            analyses,
                            &mut searches,
                            &mut memory,
                            changes,
                            flown_level,
                        )?;
                        reclose(schedule, &memory)?;
                        continue;
                    }
                    let change = if window_closed {
                        None
                    } else {
                        adapt_failed_descent(schedule, index, below_idle, reduced, idle_refused)
                    };
                    if below_idle && change.is_some() {
                        find_boundary_rates(
                            schedule,
                            analyses,
                            &mut searches,
                            &mut memory,
                            changes,
                            flown_level,
                        )?;
                    }
                    change.map(|change| {
                        tracing::info!(
                            segment = %segment_tag,
                            revision,
                            old_rate_m_s = change.old_rate_m_s,
                            new_rate_m_s = change.new_rate_m_s,
                            below_idle,
                            "replanned unconverged descent from the available propulsion envelope"
                        );
                    })
                }
            }
        } else {
            None
        };
        if adapted.is_none() {
            tracing::warn!(
                segment = %segment_tag,
                revision,
                status = ?solution.status,
                converged = solution.converged,
                throttle_limited = solution.throttle_limited,
                "mission guidance could not find a bounded profile revision"
            );
            return Ok(result);
        }
        // The revised rate of a piece survives the re-joining that follows.
        for spec in schedule.iter() {
            if let Some(rate) = vertical_rate_m_s(spec) {
                memory.remember_rate(&spec.tag, rate);
            }
            if spec.rating_limited {
                memory.remember_rated(&spec.tag);
            }
        }
        reclose(schedule, &memory)?;
    }
    unreachable!("bounded guidance revision loop always returns")
}
