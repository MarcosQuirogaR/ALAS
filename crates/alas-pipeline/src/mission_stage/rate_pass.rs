// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One sequential pass that sets every climb and descent rate to the engine's
//! own boundary before the route is closed and the whole mission is flown.
//!
//! Each segment is solved alone from the state its predecessor ends in. A
//! refused climb or descent is re-solved at a bracketed rate (see
//! [`super::rate_search`]) until the largest flyable rate is known to within
//! the bracket tolerance, then the pass moves on. Trying a rate costs one
//! segment solve instead of a whole mission, so the bracketing is cheap. The
//! cruise legs are flown at the distance the schedule currently gives them;
//! their length moves the aircraft mass by a fraction of a percent, far less
//! than the bracket tolerance, and the full flight afterwards re-checks every
//! leg and revises any that the final masses push past an engine limit.

use std::collections::HashMap;

use alas_mission::segments::{SegmentKind, SegmentSpec};
use alas_mission::{converge_root, Initials, MassChange, MissionAnalyses, Segment};

use super::guidance::{self, adapt_failed_climb};
use super::level_change;
use super::rate_search::{RateSearch, CLIMB_CUT_FACTOR};
use super::rated_climb::{self, Rated};
use super::schedule::junction::{self, MINIMUM_PATH_CHANGE_RATE_M_S};
use super::schedule::speed_change::{self, SpeedMemory};

/// Upper bound on single-segment solves in the pass.
const MAX_SEGMENT_SOLVES: usize = 600;

/// The vertical rate of a climb or descent leg, m/s.
pub(super) fn vertical_rate_m_s(spec: &SegmentSpec) -> Option<f64> {
    match spec.kind {
        SegmentKind::Climb { climb_rate_m_s, .. } => Some(climb_rate_m_s),
        SegmentKind::Descent {
            descent_rate_m_s, ..
        } => Some(descent_rate_m_s),
        SegmentKind::Cruise { .. } => None,
    }
}

/// Set the vertical rate of a climb or descent leg.
pub(super) fn set_vertical_rate_m_s(spec: &mut SegmentSpec, rate_m_s: f64) {
    match &mut spec.kind {
        SegmentKind::Climb { climb_rate_m_s, .. } => *climb_rate_m_s = rate_m_s,
        SegmentKind::Descent {
            descent_rate_m_s, ..
        } => *descent_rate_m_s = rate_m_s,
        SegmentKind::Cruise { .. } => {}
    }
}

/// Bring every climb and descent rate of `schedule` to the engine boundary.
///
/// Records what was learned about each leg in `searches` so the full flight that follows
/// starts from the same brackets. The pass stops quietly at the first leg it
/// cannot adapt (a cruise that is throttle-limited, a descent that lacks
/// thrust): the full flight's own guidance handles those. It stops too on the first leg of a
/// flight that cannot climb from its start, which the full flight stops on
/// (`dispatch::field_climb`). Returns whether a path change was re-planned as a level change.
///
/// # Errors
///
/// A leg the pass cannot set up as a segment (a leg left without a start altitude, a ramp
/// faster than its airspeed): the full flight could not fly it either, and the pass says which.
pub(super) fn find_boundary_rates(
    schedule: &mut Vec<SegmentSpec>,
    analyses: &MissionAnalyses,
    searches: &mut HashMap<String, RateSearch>,
    memory: &mut SpeedMemory,
    changes: &dyn Fn(&[SegmentSpec]) -> Vec<MassChange>,
    flown_level: &mut Vec<String>,
) -> Result<bool, String> {
    let mut initials: Option<Initials> = None;
    let mut refreshed: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut index = 0;
    // The mass change before a leg (a drop, fuel received) is applied once, when
    // the pass reaches the leg.
    let mut changed_at = usize::MAX;
    for _ in 0..MAX_SEGMENT_SOLVES {
        let Some(spec) = schedule.get(index).cloned() else {
            break;
        };
        if changed_at != index {
            changed_at = index;
            if let (Some(state), Some(change)) = (initials.as_mut(), changes(schedule).get(index)) {
                state.total_mass_kg += change.dry_kg + change.fuel_kg;
            }
        }
        if speed_change::is_speed_change(&spec) && refreshed.insert(spec.tag.clone()) {
            if let Some(state) = initials {
                if !level_change::refresh_level_leg(schedule, index, state, analyses, memory) {
                    continue;
                }
            }
        }
        let Some(spec) = schedule.get(index).cloned() else {
            break;
        };
        let mut segment = set_up(&spec, initials)?;
        let Ok(solution) = converge_root(&mut segment, analyses) else {
            break;
        };
        segment.finalize();
        if segment.is_rating_limited() {
            // Flown at the maximum-climb rating: its rate is an output, not searched for.
            let outcome = rated_climb::settle(schedule, index, &segment, solution.converged);
            searches.remove(&spec.tag);
            if outcome == Rated::Flown {
                initials = Some(segment.initials_for_next());
                index += 1;
            } else if rated_climb::note_ceiling(flown_level, &spec.tag, outcome).is_none() {
                // The first leg of a flight that cannot climb from its start: kept, and the
                // flight stops on it.
                break;
            }
            continue;
        }
        let flown =
            solution.converged && !solution.throttle_limited && !solution.idle_floor_limited;
        // A commanded climb rate the rating cannot deliver: the part of the climb it
        // refuses is flown at the rating, its rate from the specific excess power; so is a
        // commanded climb the solve could not converge.
        if !flown
            && (solution.throttle_limited || !solution.converged)
            && !spec.takeoff_rated
            && rated_climb::rate_refused_climb(schedule, index, &segment)
        {
            // A path-change piece is cut again when the route re-closes: it stays rated.
            for rated in schedule.iter().filter(|spec| spec.rating_limited) {
                memory.remember_rated(&rated.tag);
            }
            searches.remove(&spec.tag);
            continue;
        }
        let Some(rate) = vertical_rate_m_s(&spec) else {
            if !flown {
                if speed_change::lengthen(schedule, index, memory) {
                    continue;
                }
                break;
            }
            initials = Some(segment.initials_for_next());
            index += 1;
            continue;
        };
        let search = searches.entry(spec.tag.clone()).or_insert_with(|| {
            RateSearch::for_climb(matches!(spec.kind, SegmentKind::Climb { .. }))
        });
        if flown {
            search.record_flown(rate);
            memory.remember_rate(&spec.tag, rate);
        } else if matches!(spec.kind, SegmentKind::Climb { .. })
            || (matches!(spec.kind, SegmentKind::Descent { .. }) && solution.idle_floor_limited)
        {
            search.record_refused(rate);
        } else {
            break;
        }
        let Some(target) = search.target() else {
            break;
        };
        let is_climb = matches!(spec.kind, SegmentKind::Climb { .. });
        if !flown && target < MINIMUM_PATH_CHANGE_RATE_M_S {
            if let Some(key) = junction::path_change_key(&spec) {
                // The engine cannot fly this change as a climb or descent at a
                // meaningful rate: it is flown level, and said.
                let refused = if is_climb {
                    super::flight::climb_refusal(&solution)
                } else {
                    "descent refused below flight idle".to_owned()
                };
                super::flight::note_flown_level(
                    flown_level,
                    &spec.tag,
                    &format!(
                        "{refused} at {rate:.3} m/s; the next rate to try, {target:.3} m/s, is \
                         below the {MINIMUM_PATH_CHANGE_RATE_M_S:.3} m/s of a path change"
                    ),
                );
                memory.prefer_level(&key);
                return Ok(true);
            }
        }
        // Below the residual climb of the service ceiling a climb ends at the altitude it reached,
        // and the pass goes on from there. A path change is not ended: its speed change rides
        // the climb down to the path-change rate and is then flown level (above).
        if !flown
            && is_climb
            && target < guidance::CLIMB_END_RESIDUAL_RATE_M_S
            && junction::path_change_key(&spec).is_none()
        {
            let Some(reached_m) = guidance::end_refused_climb(schedule, index, &segment) else {
                break;
            };
            super::flight::note_ended_climb(
                flown_level,
                &spec.tag,
                &super::flight::climb_refusal(&solution),
                (rate, target),
                reached_m,
            );
            searches.remove(&spec.tag);
            continue;
        }
        if flown && (target - rate).abs() <= 1.0e-9 {
            if let (Some(state), Some(key)) = (initials, junction::path_change_key(&spec)) {
                if spec.exit_speed_m_s > 0.0
                    && level_change::level_change_is_quicker(
                        analyses, schedule, index, &segment, state,
                    )
                {
                    memory.prefer_level(&key);
                    return Ok(true);
                }
            }
            initials = Some(segment.initials_for_next());
            index += 1;
            continue;
        }
        let descent_split = matches!(spec.kind, SegmentKind::Descent { .. })
            && !flown
            && solution.idle_floor_limited
            && !search.has_flyable()
            && spec.entry_speed_m_s <= 0.0
            && spec.exit_speed_m_s <= 0.0;
        if descent_split {
            if let Some(kind) =
                guidance::split_idle_limited_descent(schedule, index, &segment, target)
            {
                // The piece that keeps the refused rate is known to fly it (it is
                // where the floor did not bind); the other starts a bracket of its
                // own below the refused rate.
                let (keeps, reduced) = match kind {
                    guidance::IdleSplit::UpperKeeps => {
                        (spec.tag.clone(), schedule[index + 1].tag.clone())
                    }
                    guidance::IdleSplit::LowerKeeps => {
                        (schedule[index + 1].tag.clone(), spec.tag.clone())
                    }
                };
                searches
                    .entry(keeps)
                    .or_insert_with(|| RateSearch::for_climb(false))
                    .record_flown(rate);
                searches.insert(reduced, RateSearch::refused_at(rate, CLIMB_CUT_FACTOR));
                continue;
            }
        }
        let split = matches!(spec.kind, SegmentKind::Climb { .. }) && !flown && !search.has_flyable()
            // A climb that blends a boundary speed in is cut as a whole: its
            // saturation at the start is the acceleration, not the climb.
            && spec.entry_speed_m_s <= 0.0
            && spec.exit_speed_m_s <= 0.0;
        if split {
            let length_before = schedule.len();
            let reduced = target;
            if adapt_failed_climb(schedule, index, &segment, reduced).is_none() {
                break;
            }
            if schedule.len() > length_before {
                // A split: the lower piece keeps its flyable rate, the upper
                // piece starts a bracket of its own.
                searches
                    .entry(spec.tag.clone())
                    .or_insert_with(|| RateSearch::for_climb(true))
                    .record_flown(rate);
                let upper = schedule[index + 1].tag.clone();
                searches.insert(upper, RateSearch::refused_at(rate, CLIMB_CUT_FACTOR));
            } else if schedule.len() < length_before {
                // The segment was merged away at the minimum rate.
                continue;
            }
        } else {
            set_vertical_rate_m_s(&mut schedule[index], target);
        }
    }
    Ok(false)
}

/// The leg `spec` set up as a segment from the state `initials` the leg before it ends in.
///
/// # Errors
///
/// The setup error, naming the leg: reported, not a quiet end of the pass.
fn set_up(spec: &SegmentSpec, initials: Option<Initials>) -> Result<Segment, String> {
    Segment::new(spec.clone(), initials).map_err(|error| {
        format!(
            "the engine-boundary rate pass cannot set up segment {}: {error}",
            spec.tag
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission_stage::dispatch::field_climb::test_support::climb;

    /// A climb left without a start altitude and with no leg before it cannot be set up: the
    /// pass reports it by name instead of stopping quietly.
    #[test]
    fn a_leg_the_pass_cannot_set_up_is_reported() {
        let orphan = climb("initial_climb@01", None, 3_000.0, false);
        let error = set_up(&orphan, None)
            .err()
            .unwrap_or_else(|| panic!("a leg with no start altitude cannot be set up"));
        assert!(
            error.starts_with(
                "the engine-boundary rate pass cannot set up segment initial_climb@01"
            ),
            "{error}"
        );
        assert!(error.contains("predecessor it does not have"), "{error}");
        let started = climb("takeoff@00", Some(0.0), 300.0, false);
        assert!(set_up(&started, None).is_ok());
    }
}
