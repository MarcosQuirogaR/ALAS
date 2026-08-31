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

/// Reduce an unconverged climb's requested rate while retaining its altitude
/// target. The schedule closure may subsequently reduce that target when the
/// slower climb no longer fits the available route distance.
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

/// Increase a descent rate when the minimum available thrust leaves too much
/// excess energy for the requested descent. The rate is kept below the true
/// airspeed so the horizontal component remains real.
pub(super) fn adapt_failed_descent(
    schedule: &mut [SegmentSpec],
    index: usize,
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
    let new_rate = (descent_rate_m_s * RATE_INCREASE_FACTOR).min(maximum_rate);
    if new_rate <= descent_rate_m_s + f64::EPSILON {
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
