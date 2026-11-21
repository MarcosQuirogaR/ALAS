// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Changes of speed between legs.
//!
//! * **Along the path of a climb or descent.** A leg that starts at the speed
//!   the previous leg ended at (`SegmentSpec::entry_speed_m_s`) reaches its own
//!   schedule along its path (and one that ends at the speed the next leg starts at,
//!   `exit_speed_m_s`, leaves it along the end of its path), by the total-energy model of the EUROCONTROL BADA
//!   3.8 User Manual (Sec. 3.2, eqs. 3.2-1 to 3.2-6, printed pp. 14-16): with
//!   the available power split between climbing and accelerating by the energy
//!   share factor `f`, `dh/dt = f (T - D) V / (m g)` and
//!   `V dV/dt = (1 - f) (T - D) V / m`, hence `d(V^2/2)/dh = g (1 - f) / f`
//!   along the path. Where neither Mach nor CAS is held, BADA gives `f = 0.3`
//!   for acceleration in climb and deceleration in descent and `f = 1.7` for
//!   deceleration in climb and acceleration in descent.
//! * **Level**, a cruise leg with a `speed_profile`: a table of speed against
//!   the fraction of the leg's duration (the planner integrates the level
//!   energy balance `m dV/dt = T - D` for it).

/// Standard gravity, m/s^2 (the `g` of the BADA total-energy model).
const GRAVITY_M_S2: f64 = alas_atmo::STANDARD_GRAVITY;

/// BADA 3 energy share factor for a speed change along a climb or descent when neither Mach nor
/// CAS is held: 0.3 for an acceleration in a climb or a deceleration in a descent, 1.7 for a
/// deceleration in a climb or an acceleration in a descent. BADA 3.8 User Manual (EUROCONTROL,
/// public document) Sec. 3.2, printed p. 16; the relation is eq. 3.2-5, `V dV/dh = g (1 - f) / f`
/// (also eq. (2)-(3) of the EUROCONTROL BADA overview paper, `ESF = [1 + v/g dv/dh]^-1`).
pub fn energy_share_factor(climbing: bool, accelerating: bool) -> f64 {
    if climbing == accelerating {
        0.3
    } else {
        1.7
    }
}

/// Altitude gained, m, by a climb that accelerates from `from_m_s` to `to_m_s` with the BADA
/// energy share factor of an acceleration in climb: `d(V^2/2)/dh = g (1 - f) / f` with
/// `f = 0.3` (the relation [`along_path_speed`] follows), so
/// `h = f / (1 - f) (V_to^2 - V_from^2) / (2 g)`, independent of the thrust. Zero for a
/// speed that does not rise.
pub fn acceleration_climb_altitude_gain_m(from_m_s: f64, to_m_s: f64) -> f64 {
    if to_m_s <= from_m_s {
        return 0.0;
    }
    let share = energy_share_factor(true, true);
    share / (1.0 - share) * (to_m_s * to_m_s - from_m_s * from_m_s) / (2.0 * GRAVITY_M_S2)
}

/// True airspeed along the path of a climb or descent that starts at
/// `entry_m_s` at `start_altitude_m` and heads for its own schedule
/// `nominal_m_s` at `altitude_m` (`nominal_start_m_s` is that schedule at the
/// start). The change follows `V^2 = V0^2 + 2 g (1 - f)/f (h - h0)` until it
/// meets the schedule; from there the leg flies the schedule. No entry speed
/// (zero): the schedule itself.
pub fn along_path_speed(
    entry_m_s: f64,
    nominal_start_m_s: f64,
    start_altitude_m: f64,
    altitude_m: f64,
    nominal_m_s: f64,
) -> f64 {
    if entry_m_s <= 0.0 {
        return nominal_m_s;
    }
    let accelerating = nominal_start_m_s > entry_m_s;
    let climbing = altitude_m >= start_altitude_m;
    let share = energy_share_factor(climbing, accelerating);
    let squared = entry_m_s * entry_m_s
        + 2.0 * GRAVITY_M_S2 * (1.0 - share) / share * (altitude_m - start_altitude_m);
    let along = squared.max(0.0).sqrt();
    if accelerating {
        along.min(nominal_m_s)
    } else {
        along.max(nominal_m_s)
    }
}

/// Speed of a level leg's profile `table` (rows `[duration fraction, speed]`,
/// ascending) at `fraction` of its duration. An empty table is the
/// constant speed `constant_m_s`.
pub fn profile_speed(table: &[[f64; 2]], fraction: f64, constant_m_s: f64) -> f64 {
    let Some(first) = table.first() else {
        return constant_m_s;
    };
    if fraction <= first[0] {
        return first[1];
    }
    for pair in table.windows(2) {
        if fraction <= pair[1][0] {
            let span = pair[1][0] - pair[0][0];
            let t = if span > 0.0 {
                (fraction - pair[0][0]) / span
            } else {
                1.0
            };
            return pair[0][1] + t * (pair[1][1] - pair[0][1]);
        }
    }
    table.last().map_or(0.0, |last| last[1])
}

/// Mean speed of a level leg's profile `table` over its duration, as the
/// segment's own quadrature (`weights`, the last row of its integration operator,
/// at the `nodes`) integrates it: the leg then flies exactly its distance.
pub fn profile_mean_speed(
    table: &[[f64; 2]],
    nodes: &[f64],
    weights: &[f64],
    constant_m_s: f64,
) -> f64 {
    if table.is_empty() {
        return constant_m_s;
    }
    weights
        .iter()
        .zip(nodes)
        .map(|(weight, node)| weight * profile_speed(table, *node, constant_m_s))
        .sum()
}

/// Speed at `altitude_m` of a leg that ends at `exit_m_s` at `end_altitude_m`
/// while its own schedule is `nominal_m_s` there (`nominal_end_m_s` at the
/// end): the same energy relation as [`along_path_speed`], read backwards from
/// the end. Zero exit speed: the schedule itself.
pub fn toward_exit_speed(
    exit_m_s: f64,
    nominal_end_m_s: f64,
    end_altitude_m: f64,
    altitude_m: f64,
    nominal_m_s: f64,
) -> f64 {
    if exit_m_s <= 0.0 {
        return nominal_m_s;
    }
    let accelerating = exit_m_s > nominal_end_m_s;
    let climbing = altitude_m <= end_altitude_m;
    let share = energy_share_factor(climbing, accelerating);
    let squared = exit_m_s * exit_m_s
        + 2.0 * GRAVITY_M_S2 * (1.0 - share) / share * (altitude_m - end_altitude_m);
    let along = squared.max(0.0).sqrt();
    if accelerating {
        along.max(nominal_m_s)
    } else {
        along.min(nominal_m_s)
    }
}

/// The speeds a climb or descent changes between along its path: the speed it
/// starts at and the speed it ends at (zero: its own schedule), and its own
/// schedule at both ends.
#[derive(Debug, Clone, Copy, Default)]
pub struct PathEnds {
    /// Speed at the start, m/s (zero: the schedule).
    pub entry_m_s: f64,
    /// Speed at the end, m/s (zero: the schedule).
    pub exit_m_s: f64,
    /// Altitude at the start, m.
    pub start_altitude_m: f64,
    /// Altitude at the end, m.
    pub end_altitude_m: f64,
    /// The leg's own schedule at the start, m/s.
    pub nominal_start_m_s: f64,
    /// The leg's own schedule at the end, m/s.
    pub nominal_end_m_s: f64,
}

impl PathEnds {
    /// Ends from `[entry, exit]` speeds, `[start, end]` altitudes and the
    /// schedule `[at the start, at the end]`.
    pub fn new(speeds: [f64; 2], altitudes: [f64; 2], nominal: [f64; 2]) -> Self {
        Self {
            entry_m_s: speeds[0],
            exit_m_s: speeds[1],
            start_altitude_m: altitudes[0],
            end_altitude_m: altitudes[1],
            nominal_start_m_s: nominal[0],
            nominal_end_m_s: nominal[1],
        }
    }
}

/// True airspeed of a climb or descent at `altitude_m`, where its own schedule
/// is `nominal_m_s`: the schedule plus the deviations the entry and exit
/// changes make from it.
pub fn ramp_speed(ends: &PathEnds, altitude_m: f64, nominal_m_s: f64) -> f64 {
    let entry = along_path_speed(
        ends.entry_m_s,
        ends.nominal_start_m_s,
        ends.start_altitude_m,
        altitude_m,
        nominal_m_s,
    );
    let exit = toward_exit_speed(
        ends.exit_m_s,
        ends.nominal_end_m_s,
        ends.end_altitude_m,
        altitude_m,
        nominal_m_s,
    );
    nominal_m_s + (entry - nominal_m_s) + (exit - nominal_m_s)
}
