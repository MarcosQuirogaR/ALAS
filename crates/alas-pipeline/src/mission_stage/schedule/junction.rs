// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Joining the speeds of consecutive legs.
//!
//! Two consecutive legs rarely share a speed at their junction. The change is
//! flown (see [`super::speed_change`]) in the way a pilot flies it:
//!
//! * **Along the climb or descent next to the junction**, by the BADA 3.8
//!   total-energy model with its energy share factor (Sec. 3.2, printed
//!   pp. 14-16): accelerating in a climb or decelerating in a descent shares
//!   the excess power 0.3 : 0.7 between height and speed, the other two
//!   combinations 1.7. The change occupies only the altitude span in which
//!   the energy relation takes the speed from one value to the other, so the
//!   climb or descent is cut there into a *piece* that flies the change (at
//!   whatever rate the engine allows for it) and a piece that flies the leg's
//!   own schedule at the rate it always had.
//! * **In level flight** where no neighbouring climb or descent has the
//!   altitude to hold the change: a level leg integrated from the aircraft's
//!   own thrust and drag.
//!
//! Pieces carry the tag of their leg followed by [`PIECE_MARK`] and are merged
//! back into it whenever the speeds are joined again, so the altitude fit and
//! the engine-rate search work on whole legs.

use crate::mission_stage::flight::descent_pieces::PIECE_SUFFIX as DESCENT_PIECE_SUFFIX;
use crate::mission_stage::guidance::LOWER_DESCENT_TAG_SUFFIX;
use alas_config::mission::{SpeedCaps, SpeedReference};
use alas_mission::segments::{along_path_speed, toward_exit_speed, SegmentKind, SegmentSpec};

use super::speed_change::{
    certified_limit_m_s, fallback_profile, is_speed_change, leg_speed_m_s, retarget, SpeedMemory,
    SPEED_CHANGE_PREFIX, SPEED_IDENTITY_M_S,
};

/// Marks the tag of a piece of a leg that was cut at the end of a path change.
pub(crate) const PIECE_MARK: &str = "_path_";
/// Suffix of the piece that flies a leg's entry change.
const ENTRY_SUFFIX: &str = "_path_entry";
/// Suffix of the piece that flies a leg's exit change.
const EXIT_SUFFIX: &str = "_path_exit";

/// Altitude samples used to find where a path change meets the schedule.
const SCAN_POINTS: usize = 64;
/// Bisection steps refining that altitude.
const BISECTIONS: usize = 40;
/// A piece shorter than this altitude span, m, is not cut off.
const MINIMUM_PIECE_M: f64 = 10.0;
/// The altitude of the 250 kt speed restriction, m: 10,000 ft (14 CFR 91.117(a), the
/// rule most states apply). A slowing to the restriction speed into a descent that
/// crosses it is finished level at this altitude, not along the descent below it.
const SPEED_RESTRICTION_ALTITUDE_M: f64 = 10_000.0 * 0.3048;
pub(crate) use alas_config::mission::MINIMUM_PATH_CHANGE_RATE_M_S;

/// The tag of the whole leg a piece belongs to.
pub(crate) fn family(tag: &str) -> &str {
    tag.split(PIECE_MARK).next().unwrap_or(tag)
}

fn is_ramp(spec: &SegmentSpec) -> bool {
    matches!(
        spec.kind,
        SegmentKind::Climb { .. } | SegmentKind::Descent { .. }
    )
}

fn leg_end_altitude_m(spec: &SegmentSpec, start_altitude_m: f64) -> f64 {
    match spec.kind {
        SegmentKind::Climb { altitude_end_m, .. } | SegmentKind::Descent { altitude_end_m, .. } => {
            altitude_end_m
        }
        SegmentKind::Cruise { .. } => start_altitude_m,
    }
}

fn set_span(spec: &mut SegmentSpec, start_m: f64, end_m: f64) {
    match &mut spec.kind {
        SegmentKind::Climb {
            altitude_start_m,
            altitude_end_m,
            ..
        }
        | SegmentKind::Descent {
            altitude_start_m,
            altitude_end_m,
            ..
        } => {
            *altitude_start_m = Some(start_m);
            *altitude_end_m = end_m;
        }
        SegmentKind::Cruise { .. } => {}
    }
}

/// Start altitude of a climb or descent, when it states one.
fn start_altitude_of(spec: &SegmentSpec) -> Option<f64> {
    match spec.kind {
        SegmentKind::Climb {
            altitude_start_m, ..
        }
        | SegmentKind::Descent {
            altitude_start_m, ..
        } => altitude_start_m,
        SegmentKind::Cruise { .. } => None,
    }
}

/// Put the pieces cut by an earlier joining back into their legs: an exit
/// piece into the leg before it, an entry piece into the leg after it.
pub(crate) fn merge_pieces(legs: Vec<SegmentSpec>) -> Vec<SegmentSpec> {
    let mut merged: Vec<SegmentSpec> = Vec::with_capacity(legs.len());
    let mut entry_piece: Option<SegmentSpec> = None;
    for mut leg in legs {
        if let Some(piece) = entry_piece.take() {
            match start_altitude_of(&piece) {
                Some(start) if leg.tag == family(&piece.tag) && is_ramp(&leg) => {
                    let end = leg_end_altitude_m(&leg, start);
                    set_span(&mut leg, start, end);
                }
                _ => merged.push(piece),
            }
        }
        if leg.tag.ends_with(ENTRY_SUFFIX) {
            entry_piece = Some(leg);
            continue;
        }
        if leg.tag.ends_with(EXIT_SUFFIX) {
            if let Some(previous) = merged.last_mut() {
                let whole = family(&leg.tag);
                // The exit was cut from the whole leg before the descent was
                // subdivided (`descent_pieces`) or cut where it first reaches flight
                // idle (`guidance`, the lower piece follows the leg it was cut
                // from): it belongs to the last piece of that leg, else it would
                // stay an orphan leg that no level preference ever reaches, and
                // the rate pass would stop at it on every call.
                let same_leg = previous.tag == whole
                    || previous.tag.strip_prefix(whole).is_some_and(|tail| {
                        tail.starts_with("_upper")
                            || tail.starts_with(LOWER_DESCENT_TAG_SUFFIX)
                            || tail.starts_with(DESCENT_PIECE_SUFFIX)
                    });
                if same_leg && is_ramp(previous) {
                    let end = leg_end_altitude_m(&leg, 0.0);
                    let start = start_altitude_of(previous).unwrap_or(end);
                    set_span(previous, start, end);
                    continue;
                }
            }
        }
        merged.push(leg);
    }
    merged.extend(entry_piece);
    merged
}

/// Altitude at which a change of the leg's speed from `from_m_s` at its start
/// (`entry`) or to `to_m_s` at its end (exit) is complete, when the leg spans
/// enough altitude for it.
fn change_boundary_m(
    leg: &SegmentSpec,
    start_m: f64,
    end_m: f64,
    entry_from_m_s: Option<f64>,
    exit_to_m_s: Option<f64>,
) -> Option<f64> {
    let nominal_start = leg_speed_m_s(leg, start_m)?;
    let nominal_end = leg_speed_m_s(leg, end_m)?;
    let met = |altitude_m: f64| -> Option<bool> {
        let nominal = leg_speed_m_s(leg, altitude_m)?;
        let along = match (entry_from_m_s, exit_to_m_s) {
            (Some(from), _) => along_path_speed(from, nominal_start, start_m, altitude_m, nominal),
            (_, Some(to)) => toward_exit_speed(to, nominal_end, end_m, altitude_m, nominal),
            _ => return None,
        };
        Some((along - nominal).abs() <= SPEED_IDENTITY_M_S)
    };
    // The altitude end where the change begins and the one it is complete at.
    let (origin, far) = if entry_from_m_s.is_some() {
        (start_m, end_m)
    } else {
        (end_m, start_m)
    };
    if !met(far)? {
        return None;
    }
    let (mut unmet, mut met_at) = (origin, far);
    for step in 1..=SCAN_POINTS {
        let altitude_m = origin + (far - origin) * step as f64 / SCAN_POINTS as f64;
        if met(altitude_m)? {
            met_at = altitude_m;
            break;
        }
        unmet = altitude_m;
    }
    for _ in 0..BISECTIONS {
        let middle = 0.5 * (unmet + met_at);
        if met(middle)? {
            met_at = middle;
        } else {
            unmet = middle;
        }
    }
    Some(met_at)
}

/// Give a piece the rate it was last flown at, when one is remembered, and the maximum-climb
/// rating it was flown at, when it was.
fn with_remembered_rate(mut piece: SegmentSpec, memory: &SpeedMemory) -> SegmentSpec {
    if memory.is_rated(&piece.tag) && matches!(piece.kind, SegmentKind::Climb { .. }) {
        piece.rating_limited = true;
    }
    if let Some(rate) = memory.rate_of(&piece.tag) {
        match &mut piece.kind {
            SegmentKind::Climb { climb_rate_m_s, .. } => *climb_rate_m_s = rate,
            SegmentKind::Descent {
                descent_rate_m_s, ..
            } => *descent_rate_m_s = rate,
            SegmentKind::Cruise { .. } => {}
        }
    }
    piece
}

/// The legs that fly `leg` when it must start at `from_m_s`: the leg alone, or
/// the piece that flies the entry change followed by the leg.
fn with_entry_change(
    mut leg: SegmentSpec,
    from_m_s: f64,
    start_m: f64,
    boundary_m: f64,
    memory: &SpeedMemory,
) -> Vec<SegmentSpec> {
    let end_m = leg_end_altitude_m(&leg, start_m);
    if (end_m - boundary_m).abs() < MINIMUM_PIECE_M {
        leg.entry_speed_m_s = from_m_s;
        return vec![leg];
    }
    let mut change = leg.clone();
    change.entry_speed_m_s = from_m_s;
    change.tag = format!("{}{ENTRY_SUFFIX}", leg.tag);
    set_span(&mut change, start_m, boundary_m);
    set_span(&mut leg, boundary_m, end_m);
    vec![with_remembered_rate(change, memory), leg]
}

/// Cut the previous leg (the last of `out`, spanning `start_m` to `end_m`) at
/// the start of the exit change and let the last piece end at `to_m_s`.
fn with_exit_change(
    out: &mut Vec<SegmentSpec>,
    to_m_s: f64,
    start_m: f64,
    end_m: f64,
    boundary_m: f64,
    memory: &SpeedMemory,
) {
    let Some(last) = out.last_mut() else {
        return;
    };
    if (boundary_m - start_m).abs() < MINIMUM_PIECE_M {
        last.exit_speed_m_s = to_m_s;
        return;
    }
    let mut piece = last.clone();
    piece.tag = format!("{}{EXIT_SUFFIX}", last.tag);
    piece.exit_speed_m_s = to_m_s;
    piece.entry_speed_m_s = 0.0;
    set_span(last, start_m, boundary_m);
    set_span(&mut piece, boundary_m, end_m);
    out.push(with_remembered_rate(piece, memory));
}

/// Make the speeds of consecutive legs consistent.
///
/// * A level leg is flown no faster than the certified speed limits (VMO, MMO)
///   at its altitude: the declared cruise true airspeed of a low step cruise
///   (A320 `cruise_step_1` at 11,800 ft) is the one for the cruise level and
///   would exceed VMO there. Nothing else limits a cruise speed here.
/// * Wherever two consecutive legs end and start at speeds that differ the
///   change is flown, along the climb or descent next to the junction when it
///   has the altitude for it (see the module documentation), otherwise as a
///   level leg, with the level change remembered in `memory` when there is
///   one. Existing speed-change legs are removed and rebuilt, pieces of earlier
///   joinings merged back.
/// * The explicit configuration drag of a speed-change leg is the mean of the
///   legs around it (flaps retracting or extending during the change).
///
/// Altitudes are read from the schedule as it stands, so call this after any
/// change of the altitude profile.
pub(crate) fn join_speeds(
    schedule: &mut Vec<SegmentSpec>,
    initial_altitude_m: f64,
    memory: &SpeedMemory,
) {
    let legs = merge_pieces(
        schedule
            .drain(..)
            .filter(|spec| !is_speed_change(spec))
            .collect(),
    );

    let mut out: Vec<SegmentSpec> = Vec::with_capacity(legs.len() + 8);
    // Start altitude of the last leg in `out`, and the speed it ends at.
    let mut previous_start_m = initial_altitude_m;
    let mut altitude_m = initial_altitude_m;
    let mut previous_end_speed_m_s: Option<f64> = None;
    for mut leg in legs {
        leg.entry_speed_m_s = 0.0;
        leg.exit_speed_m_s = 0.0;
        let start_altitude_m = match leg.kind {
            SegmentKind::Climb {
                altitude_start_m, ..
            }
            | SegmentKind::Descent {
                altitude_start_m, ..
            } => altitude_start_m.unwrap_or(altitude_m),
            SegmentKind::Cruise {
                altitude_m: level, ..
            } => level.unwrap_or(altitude_m),
        };
        if let (false, Some(speed)) = (
            is_speed_change(&leg),
            memory.cruise_speed(&leg.tag, start_altitude_m),
        ) {
            leg.air_speed_m_s = speed;
        }
        if let (false, Some(cap)) = (is_speed_change(&leg), memory.cruise_cap(&leg.tag)) {
            leg.air_speed_m_s = leg.air_speed_m_s.min(cap);
        }
        if let SegmentKind::Cruise { .. } = leg.kind {
            // The level leg's own certified limits; a leg built without them
            // takes the leg before it.
            let caps = if leg.speed_caps == SpeedCaps::default() {
                out.last().map(|previous| previous.speed_caps)
            } else {
                Some(leg.speed_caps)
            };
            if let Some(limit) = caps.and_then(|caps| {
                certified_limit_m_s(&caps, leg.temperature_deviation_k, start_altitude_m)
            }) {
                leg.air_speed_m_s = leg.air_speed_m_s.min(limit);
            }
        }
        let end_altitude_m = leg_end_altitude_m(&leg, start_altitude_m);
        let start_speed = leg_speed_m_s(&leg, start_altitude_m);
        let mut pieces: Vec<SegmentSpec> = Vec::new();
        let mut joined = false;
        if let (Some(from), Some(to), Some(_)) = (previous_end_speed_m_s, start_speed, out.last()) {
            if (to - from).abs() > SPEED_IDENTITY_M_S {
                joined = true;
                // The acceleration to a cruise speed is finished in the climb before the
                // junction (the exit of that leg); any other ramp takes its change in front.
                if !is_ramp(&leg)
                    && out.last().is_some_and(|previous| {
                        is_ramp(previous)
                            && !memory.level_preferred(&format!("{}{EXIT_SUFFIX}", previous.tag))
                    })
                {
                    let previous = out.last().cloned();
                    if let Some(previous) = previous.filter(|p| p.entry_speed_m_s <= 0.0) {
                        if let Some(boundary) = change_boundary_m(
                            &previous,
                            previous_start_m,
                            altitude_m,
                            None,
                            Some(to),
                        ) {
                            // The previous leg is the last one pushed: cut it.
                            with_exit_change(
                                &mut out,
                                to,
                                previous_start_m,
                                altitude_m,
                                boundary,
                                memory,
                            );
                            pieces = vec![leg.clone()];
                        }
                    }
                }
                let at_restriction = to < from
                    && matches!(leg.kind, SegmentKind::Descent { .. })
                    && (start_altitude_m - SPEED_RESTRICTION_ALTITUDE_M).abs() < 1.0;
                if pieces.is_empty()
                    && is_ramp(&leg)
                    && !at_restriction
                    && !memory.level_preferred(&format!("{}{ENTRY_SUFFIX}", leg.tag))
                {
                    if let Some(boundary) =
                        change_boundary_m(&leg, start_altitude_m, end_altitude_m, Some(from), None)
                    {
                        pieces = with_entry_change(
                            leg.clone(),
                            from,
                            start_altitude_m,
                            boundary,
                            memory,
                        );
                    }
                }
                if pieces.is_empty() {
                    pieces = level_change(&mut out, leg.clone(), from, to, memory);
                }
            }
        }
        if !joined {
            pieces = vec![leg];
        }
        if let Some(last) = pieces.last() {
            let end_m = end_altitude_m;
            previous_end_speed_m_s = if last.exit_speed_m_s > 0.0 {
                Some(last.exit_speed_m_s)
            } else {
                leg_speed_m_s(last, end_m)
            };
        }
        // Start altitude of the last piece.
        previous_start_m = match pieces.last().map(|piece| piece.kind) {
            Some(SegmentKind::Climb {
                altitude_start_m: Some(start),
                ..
            })
            | Some(SegmentKind::Descent {
                altitude_start_m: Some(start),
                ..
            }) => start,
            _ => start_altitude_m,
        };
        altitude_m = end_altitude_m;
        out.extend(pieces);
    }
    *schedule = out;
}

/// The legs that fly a junction as a level change: the level leg (when the
/// aircraft can change speed at all) followed by `leg` adjusted to the speed
/// the change reaches.
fn level_change(
    out: &mut [SegmentSpec],
    mut leg: SegmentSpec,
    from: f64,
    to: f64,
    memory: &SpeedMemory,
) -> Vec<SegmentSpec> {
    let Some(previous) = out.last() else {
        return vec![leg];
    };
    let tag = format!("{SPEED_CHANGE_PREFIX}{}", leg.tag);
    let mut profile = memory
        .valid_for(&tag, from, to)
        .cloned()
        .or_else(|| memory.rescaled_for(&tag, from, to))
        .unwrap_or_else(|| fallback_profile(from, to));
    // A cruise leg is flown at the speed its change reaches (the change may stop
    // short of the cruise speed); a climb or descent follows the speed its own
    // schedule gives at the altitude the junction has now.
    if memory.follows_nominal() && !matches!(leg.kind, SegmentKind::Cruise { .. }) {
        retarget(&mut profile, to);
    }
    // The speed the leg after the junction starts at: what the level change
    // reaches (the speed it has when the aircraft cannot change speed in level
    // flight at all).
    match leg.kind {
        SegmentKind::Cruise { .. } => leg.air_speed_m_s = profile.reached_m_s,
        _ if (to - profile.reached_m_s).abs() > SPEED_IDENTITY_M_S => {
            leg.entry_speed_m_s = profile.reached_m_s;
        }
        _ => {}
    }
    let mut pieces = Vec::with_capacity(2);
    if (profile.reached_m_s - from).abs() >= SPEED_IDENTITY_M_S {
        let mut change = previous.clone();
        change.kind = SegmentKind::Cruise {
            altitude_m: None,
            distance_m: profile.distance_m,
        };
        change.tag = tag;
        change.air_speed_m_s = profile.from_m_s;
        change.speed_profile = profile.table.clone();
        change.entry_speed_m_s = 0.0;
        change.exit_speed_m_s = 0.0;
        change.air_speed_reference = SpeedReference::TrueAirspeed;
        change.speed_caps = SpeedCaps::default();
        change.configuration_delta_cd =
            0.5 * (previous.configuration_delta_cd + leg.configuration_delta_cd);
        pieces.push(change);
    }
    pieces.push(leg);
    pieces
}

/// The key under which a leg or piece that carries a path change is
/// remembered as flown level (`SpeedMemory::prefer_level`), `None` for a leg
/// that carries none.
pub(crate) fn path_change_key(spec: &SegmentSpec) -> Option<String> {
    if spec.tag.ends_with(ENTRY_SUFFIX) || spec.tag.ends_with(EXIT_SUFFIX) {
        Some(spec.tag.clone())
    } else if spec.entry_speed_m_s > 0.0 {
        Some(format!("{}{ENTRY_SUFFIX}", spec.tag))
    } else if spec.exit_speed_m_s > 0.0 {
        Some(format!("{}{EXIT_SUFFIX}", spec.tag))
    } else {
        None
    }
}
