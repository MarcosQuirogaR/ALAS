// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The level speed change of a junction, integrated from the mission's own
//! thrust and drag.
//!
//! In level flight the whole excess force goes into speed (BADA 3.8 total-energy
//! model, eq. 3.2-1 with `dh/dt = 0`): `m dV/dt = T - D`. The aircraft
//! accelerates at maximum climb thrust and decelerates at flight idle. The
//! drag is the thrust of a steady level flight at the same speed, altitude and
//! mass (one small cruise solve per sample speed); the available thrust is the
//! deck's, at full throttle and at idle. A level speed change, accelerating or decelerating, is limited
//! to BADA's `a_l,max(civ)` (2 ft/s^2, `alas_config::mission::LEVEL_ACCELERATION_LIMIT_M_S2`;
//! BADA 3.8 User Manual Sec. 5.2, printed p. 33, eq. 5.2-1 bounds the TAS increment in either
//! sense). The take-off
//! acceleration is not a level change and does not use the cap: it is flown as an accelerating
//! climb with the BADA energy share (`takeoff.rs`). Integrating `dt = dV / a(V)` (trapezoid rule
//! on `1/a`) and `ds = V dt` over samples of the speed gives the duration, the distance and
//! the speed against time of the change: a decaying acceleration, as the speed
//! nears the equilibrium one.
//!
//! The change runs to the target speed whenever the aircraft can hold a
//! positive excess force up to it. When the excess force vanishes before the
//! target (the equilibrium speed lies below it) the approach is asymptotic and
//! never completes, so completion is a convention: the usual 2 % settling band
//! of an asymptotic approach, 98 % of the way to the equilibrium speed.

use alas_atmo::us1976_compute_values;
use alas_mission::segments::{SegmentKind, SegmentSpec};
use alas_mission::{converge_root, Initials, MissionAnalyses, Segment};

use super::dispatch::field_climb;
use super::schedule::speed_change::{
    leg_speed_m_s, LevelProfile, SpeedMemory, LEVEL_ACCELERATION_LIMIT_M_S2, SETTLED_SHARE,
    SPEED_IDENTITY_M_S,
};

/// Speed samples between the start and the end of the change.
const SAMPLES: usize = 48;

/// Bisection steps locating the equilibrium speed between two samples.
const EQUILIBRIUM_BISECTIONS: usize = 6;

/// Excess force per unit mass below which the aircraft is taken not to change
/// speed at all, m/s^2 (numerical zero).
const NUMERICAL_ZERO_M_S2: f64 = 1.0e-6;

/// What the level-flight solve at one speed found.
enum LevelForces {
    /// Thrust of a steady level flight, full-throttle thrust and idle thrust, N.
    Held(f64, f64, f64),
    /// The aircraft cannot hold this speed level: it needs more than full thrust
    /// (`above_full`) or less than idle. `full` and `idle` are the deck's thrusts, N.
    CannotHold {
        above_full: bool,
        full: f64,
        idle: f64,
    },
    /// The solve itself failed.
    Failed,
}

/// Forces of a level flight at `speed_m_s`.
fn level_forces(
    analyses: &MissionAnalyses,
    template: &SegmentSpec,
    initials: Initials,
    altitude_m: f64,
    speed_m_s: f64,
) -> LevelForces {
    let mut spec = template.clone();
    spec.kind = SegmentKind::Cruise {
        altitude_m: Some(altitude_m),
        distance_m: 2_000.0,
    };
    spec.air_speed_m_s = speed_m_s;
    spec.speed_profile.clear();
    spec.entry_speed_m_s = 0.0;
    spec.exit_speed_m_s = 0.0;
    spec.number_control_points = 6;
    let Ok(mut segment) = Segment::new(spec.clone(), Some(initials)) else {
        return LevelForces::Failed;
    };
    let Ok(solution) = converge_root(&mut segment, analyses) else {
        return LevelForces::Failed;
    };
    let atmosphere = us1976_compute_values(altitude_m, spec.temperature_deviation_k);
    let mach = speed_m_s / atmosphere.speed_of_sound_m_s;
    let middle = segment.conditions.len() / 2;
    let Some(gravity) = segment.conditions.gravity_m_s2.get(middle).copied() else {
        return LevelForces::Failed;
    };
    let at = |throttle: f64| {
        analyses
            .thrust(&atmosphere, altitude_m, speed_m_s, mach, gravity, throttle)
            .thrust_n
    };
    let full_n = at(1.0);
    let idle_n = at(0.0);
    if !(full_n.is_finite() && idle_n.is_finite()) {
        return LevelForces::Failed;
    }
    if solution.throttle_limited || solution.idle_floor_limited {
        return LevelForces::CannotHold {
            above_full: solution.throttle_limited,
            full: full_n,
            idle: idle_n,
        };
    }
    let required_n = segment
        .conditions
        .thrust
        .get(middle)
        .map(|thrust| thrust.thrust_n)
        .filter(|thrust| thrust.is_finite());
    match required_n {
        Some(required) if solution.converged => LevelForces::Held(required, full_n, idle_n),
        _ => LevelForces::Failed,
    }
}

/// Acceleration (positive: the speed changes the way asked), m/s^2, at
/// `speed_m_s`; `None` when the solve failed.
fn acceleration(
    analyses: &MissionAnalyses,
    template: &SegmentSpec,
    initials: Initials,
    altitude_m: f64,
    speed_m_s: f64,
    accelerating: bool,
) -> Option<f64> {
    let excess_n = match level_forces(analyses, template, initials, altitude_m, speed_m_s) {
        LevelForces::Held(required, full, idle) => {
            if accelerating {
                full - required
            } else {
                required - idle
            }
        }
        // Needs more than full thrust to hold the speed: no excess when accelerating,
        // at least the full-to-idle gap when decelerating; the mirror for a speed that
        // needs less than idle.
        LevelForces::CannotHold {
            above_full,
            full,
            idle,
        } => {
            if accelerating == above_full {
                0.0
            } else {
                full - idle
            }
        }
        LevelForces::Failed => {
            tracing::warn!(
                speed_m_s,
                altitude_m,
                "a level-flight solve failed while integrating a speed change"
            );
            return None;
        }
    };
    let available = excess_n / initials.total_mass_kg;
    // BADA bounds the longitudinal acceleration in either sense (User Manual 3.8, printed p. 33,
    // eq. 5.2-1, `a_l,max(civ)` 2 ft/s^2): the change takes at least dV / a_max, whether it is an
    // acceleration or a deceleration, as the priced restriction legs do.
    Some(available.min(LEVEL_ACCELERATION_LIMIT_M_S2))
}

/// Integrate the level change from `from_m_s` to `to_m_s` at `altitude_m` for
/// an aircraft of `mass_kg` (`template` supplies the leg's atmosphere day and
/// configuration drag). `None` when the aircraft cannot change speed in level
/// flight at all.
pub(super) fn integrate_level_change(
    analyses: &MissionAnalyses,
    template: &SegmentSpec,
    initials: Initials,
    altitude_m: f64,
    from_m_s: f64,
    to_m_s: f64,
) -> Option<LevelProfile> {
    let accelerating = to_m_s > from_m_s;
    let span = to_m_s - from_m_s;
    let a_at = |speed: f64| {
        acceleration(
            analyses,
            template,
            initials,
            altitude_m,
            speed,
            accelerating,
        )
    };
    let mut speeds: Vec<f64> = Vec::with_capacity(SAMPLES + 2);
    let mut accelerations: Vec<f64> = Vec::with_capacity(SAMPLES + 2);
    let mut equilibrium_between: Option<(f64, f64)> = None;
    for i in 0..=SAMPLES {
        let speed = from_m_s + span * i as f64 / SAMPLES as f64;
        let a = a_at(speed)?;
        if a <= NUMERICAL_ZERO_M_S2 {
            equilibrium_between = speeds.last().map(|last| (*last, speed));
            break;
        }
        speeds.push(speed);
        accelerations.push(a);
    }
    if let Some((mut inside, mut outside)) = equilibrium_between {
        // The excess force vanishes between two samples: locate the equilibrium
        // speed and stop at the settling band of the approach to it.
        for _ in 0..EQUILIBRIUM_BISECTIONS {
            let middle = 0.5 * (inside + outside);
            if a_at(middle)? <= NUMERICAL_ZERO_M_S2 {
                outside = middle;
            } else {
                inside = middle;
            }
        }
        let end = from_m_s + SETTLED_SHARE * (inside - from_m_s);
        while speeds
            .last()
            .is_some_and(|last| (last - from_m_s).abs() >= (end - from_m_s).abs())
        {
            speeds.pop();
            accelerations.pop();
        }
        if (end - from_m_s).abs() > SPEED_IDENTITY_M_S {
            accelerations.push(a_at(end)?.max(NUMERICAL_ZERO_M_S2));
            speeds.push(end);
        }
    }
    let usable = speeds.len();
    if usable < 2 {
        return None;
    }
    let mut time = vec![0.0];
    let mut distance = 0.0;
    for i in 0..usable - 1 {
        let dv = (speeds[i + 1] - speeds[i]).abs();
        // Trapezoid rule on 1/a.
        let dt = dv * 0.5 * (1.0 / accelerations[i] + 1.0 / accelerations[i + 1]);
        distance += 0.5 * (speeds[i] + speeds[i + 1]) * dt;
        time.push(time[i] + dt);
    }
    let total = *time.last()?;
    let table: Vec<[f64; 2]> = time
        .iter()
        .zip(&speeds)
        .map(|(t, v)| [t / total, *v])
        .collect();
    Some(LevelProfile {
        from_m_s,
        target_m_s: to_m_s,
        reached_m_s: speeds[usable - 1],
        distance_m: distance,
        duration_s: total,
        table,
    })
}

/// Re-integrate the level change at `index` of `schedule` from the state the
/// previous leg ends in (`initials`), update the leg and the start speed of
/// the leg after it, and remember the result. A change the aircraft cannot
/// make in level flight is removed (`false`): the leg after the junction then
/// starts at the speed the previous leg ended at. One a flight starts on is
/// kept (`dispatch::field_climb`), so `true` says a leg is still at `index`.
pub(super) fn refresh_level_leg(
    schedule: &mut Vec<SegmentSpec>,
    index: usize,
    initials: Initials,
    analyses: &MissionAnalyses,
    memory: &mut SpeedMemory,
) -> bool {
    let leg = schedule[index].clone();
    let Some(next) = schedule.get(index + 1).cloned() else {
        return true;
    };
    let (Some(first), Some(last)) = (leg.speed_profile.first(), leg.speed_profile.last()) else {
        return true;
    };
    let from = first[1];
    let altitude_m = -initials.position_vector_m[2];
    let remembered = memory.requested_target(&next.tag).unwrap_or(last[1]);
    // A plan holds the change to what the leg after it asks for at this
    // altitude now, not at the altitude the change was first remembered for.
    let target = if memory.follows_nominal() && !matches!(next.kind, SegmentKind::Cruise { .. }) {
        leg_speed_m_s(&next, altitude_m).unwrap_or(remembered)
    } else {
        remembered
    };
    let profile = integrate_level_change(analyses, &leg, initials, altitude_m, from, target)
        .filter(|profile| (profile.reached_m_s - from).abs() > SPEED_IDENTITY_M_S)
        .unwrap_or(LevelProfile {
            from_m_s: from,
            target_m_s: target,
            reached_m_s: from,
            distance_m: 0.0,
            duration_s: 0.0,
            table: Vec::new(),
        });
    let reached = profile.reached_m_s;
    match schedule[index + 1].kind {
        SegmentKind::Cruise { .. } => schedule[index + 1].air_speed_m_s = reached,
        _ if (target - reached).abs() > SPEED_IDENTITY_M_S => {
            schedule[index + 1].entry_speed_m_s = reached;
        }
        _ => schedule[index + 1].entry_speed_m_s = 0.0,
    }
    let kept = !profile.table.is_empty();
    if kept {
        schedule[index].kind = SegmentKind::Cruise {
            altitude_m: None,
            distance_m: profile.distance_m,
        };
        schedule[index].speed_profile = profile.table.clone();
        schedule[index].air_speed_m_s = from;
    }
    memory.insert(&next.tag, profile);
    kept || field_climb::remove_leg(schedule, index).is_none()
}

/// Whether the level change would take less time than the exit piece `index`
/// that flies the speed change at the end of a climb or descent along its path
/// (at the rate the engine allows for it): the piece's duration against the time
/// to cover the same altitude at the rate of the leg before it and then change
/// the speed in level flight. Both ways reach the same altitude and speed, so
/// the faster one is the procedure flown; where the excess power is thin at the
/// end of the climb the level change wins. `false` when the aircraft cannot
/// change speed in level flight at all.
pub(super) fn level_change_is_quicker(
    analyses: &MissionAnalyses,
    schedule: &[SegmentSpec],
    index: usize,
    piece: &Segment,
    initials: Initials,
) -> bool {
    let spec = &schedule[index];
    let Some(previous_rate_m_s) = index
        .checked_sub(1)
        .and_then(|before| super::rate_pass::vertical_rate_m_s(&schedule[before]))
    else {
        return false;
    };
    let times = &piece.conditions.time_s;
    let altitudes = &piece.conditions.altitude_m;
    let (Some(&t0), Some(&t1), Some(&h0), Some(&h1)) = (
        times.first(),
        times.last(),
        altitudes.first(),
        altitudes.last(),
    ) else {
        return false;
    };
    let Some(from_m_s) = super::schedule::speed_change::leg_speed_m_s(spec, h1) else {
        return false;
    };
    let Some(level) =
        integrate_level_change(analyses, spec, initials, h1, from_m_s, spec.exit_speed_m_s)
    else {
        return false;
    };
    let alternative_s =
        (h1 - h0).abs() / previous_rate_m_s.max(f64::MIN_POSITIVE) + level.duration_s;
    t1 - t0 > alternative_s
}
