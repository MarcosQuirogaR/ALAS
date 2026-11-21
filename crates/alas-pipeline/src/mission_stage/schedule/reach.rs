// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the priced route says the aircraft can reach, applied to the planned ladder.

use alas_mission::segments::{SegmentKind, SegmentSpec};

/// What the priced route shows the aircraft can fly at the dispatched mass: the highest cruise level
/// it holds and the speed it commands at each level it cruises at. The native flight is held to it,
/// so a configured level or speed above it is lowered in the planner, not found infeasible in flight.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::mission_stage) struct Reach {
    /// Highest cruise level, m.
    pub top_m: f64,
    /// `(level m, commanded true airspeed m/s)` of the priced cruise runs.
    pub runs: Vec<(f64, f64)>,
}

impl Reach {
    /// The commanded speed of the priced run nearest in level to `level_m`.
    pub(super) fn speed_near(&self, level_m: f64) -> Option<f64> {
        self.runs
            .iter()
            .min_by(|a, b| (a.0 - level_m).abs().total_cmp(&(b.0 - level_m).abs()))
            .map(|run| run.1)
    }
}

/// The level of a cruise leg of `schedule`: its own, or the end of the leg before it.
pub(super) fn cruise_level(spec: &SegmentSpec, schedule: &[SegmentSpec]) -> Option<f64> {
    let SegmentKind::Cruise { altitude_m, .. } = spec.kind else {
        return None;
    };
    altitude_m.or_else(|| {
        let at = schedule
            .iter()
            .position(|other| std::ptr::eq(other, spec))?;
        schedule[..at]
            .iter()
            .rev()
            .find_map(|other| match other.kind {
                SegmentKind::Climb { altitude_end_m, .. }
                | SegmentKind::Descent { altitude_end_m, .. } => Some(altitude_end_m),
                SegmentKind::Cruise { altitude_m, .. } => altitude_m,
            })
    })
}

/// A climb rung that gains less than this, m, is not flown as a piece of its own: the rung before
/// it carries on (the 30 m altitude tolerance within which `Builder::reach` treats a route
/// altitude as already reached).
const MINIMUM_RUNG_GAIN_M: f64 = 30.0;

/// The ends of the three climb rungs, m: the profile's own fractions of the top
/// `cruise_altitude_m`, the last rung ending at the top.
///
/// The fractions are the altitudes at which the climb schedule changes (250 kt below 10,000 ft, the
/// type's CAS, then its Mach), the way the priced route reads them
/// (`alas-opt` `climb_bands`). They are climbed through without a cruise leg unless they are
/// themselves cruise levels (see [`flown_cruise_legs`]). The first rung ends at least a take-off
/// band plus a minimum rung above the field.
pub(super) fn levels(request: &alas_mission::MissionRequest, cruise_altitude_m: f64) -> (f64, f64) {
    let p = &request.profile;
    let floor_m = request.departure_elevation_m + p.takeoff_altitude_gain_m + MINIMUM_RUNG_GAIN_M;
    let first = (cruise_altitude_m * p.initial_climb_altitude_fraction)
        .max(floor_m)
        .min(cruise_altitude_m);
    let second = (cruise_altitude_m * p.step_climb_1_altitude_fraction)
        .max(first)
        .min(cruise_altitude_m);
    (first, second)
}

/// The climb rungs the schedule flies and where each cruise leg sits among them.
///
/// The climb goes on through every rung up to the top: a rung boundary is a change of the climb
/// schedule, not a level to cruise at. The last active cruise leg is flown at the top, after the
/// highest rung; an earlier active leg is a step cruise at the end of its own rung, kept only
/// where the route lasts long enough to justify the step (see [`unjustified_step_legs`]). A rung
/// whose height is below [`MINIMUM_RUNG_GAIN_M`] is not flown.
pub(super) struct Flown {
    /// End of `initial_climb`, `step_climb_1`, `step_climb_2` (the highest flown rung ends at the
    /// top), m; `None` for a rung that is not flown.
    pub rung_ends_m: [Option<f64>; 3],
    /// For `cruise_step_1..3`: the rung it follows, `None` for a leg that is not active.
    pub cruise_after: [Option<usize>; 3],
}

pub(super) fn flown_cruise_legs(
    active: [bool; 3],
    first_m: f64,
    second_m: f64,
    top_m: f64,
) -> Flown {
    let levels = [first_m, second_m, top_m];
    let mut rung_ends_m = [None; 3];
    let mut previous_m = f64::NEG_INFINITY;
    for index in 0..3 {
        let end_m = if index == 2 { top_m } else { levels[index] };
        if index == 0 || end_m - previous_m >= MINIMUM_RUNG_GAIN_M {
            rung_ends_m[index] = Some(end_m);
            previous_m = end_m;
        }
    }
    let highest = rung_ends_m.iter().rposition(Option::is_some).unwrap_or(0);
    rung_ends_m[highest] = Some(top_m);
    let last = active.iter().rposition(|&on| on);
    let mut cruise_after = [None; 3];
    for index in 0..3 {
        if active[index] {
            cruise_after[index] = Some(if Some(index) == last { highest } else { index });
        }
    }
    Flown {
        rung_ends_m,
        cruise_after,
    }
}

/// A cruise leg of the ladder, bottom to top: tag, level m, share of the cruise distance, speed m/s.
pub(super) type LadderLeg = (String, f64, f64, f64);

/// The tags of the lower cruise legs a ladder cannot justify, lowest first, so that the aircraft
/// climbs through their levels to the top instead of cruising at them.
///
/// A step climb to a higher level pays when the weight burnt on the leg before it lets the
/// aircraft drift up that far (`alas_mission::minimum_cruise_time_for_step_s`, the first-order
/// Breguet altitude drift): the first leg lasts at least half of the time one step needs, each later
/// leg the whole of it, as `cruise_legs_have_enough_time_to_justify_the_ladder` judges a proposed
/// ladder. The cruise distance `remainder_m` splits among the legs by their shares. A short sector
/// therefore flies one level, the top.
pub(super) fn unjustified_step_legs(legs: &[LadderLeg], remainder_m: f64) -> Vec<String> {
    let mut dropped = Vec::new();
    let mut kept: Vec<&LadderLeg> = legs.iter().collect();
    while kept.len() > 1 {
        let total_share: f64 = kept.iter().map(|leg| leg.2).sum();
        let duration_s = |leg: &LadderLeg| remainder_m * leg.2 / total_share / leg.3;
        let justified = total_share > 0.0
            && kept.windows(2).enumerate().all(|(index, pair)| {
                let step_m = (pair[1].1 - pair[0].1).max(0.0);
                let need_s = alas_mission::minimum_cruise_time_for_step_s(step_m);
                if index == 0 {
                    duration_s(pair[0]) >= 0.5 * need_s && duration_s(pair[1]) >= need_s
                } else {
                    duration_s(pair[1]) >= need_s
                }
            });
        if justified {
            break;
        }
        dropped.push(kept.remove(0).0.clone());
    }
    dropped
}
