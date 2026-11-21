// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Climb-ladder altitude bands shared by trip and diversion legs.
//!
//! The three climb rungs are the altitudes at which the climb schedule changes (250 kt below
//! 10,000 ft, the type's CAS, then its Mach). `build_schedule`
//! (`alas-pipeline::mission_stage::schedule`) climbs through all of them continuously up to the
//! cruise level whatever the number of active cruise legs: a cruise leg is flown only at a cruise
//! level of the step ladder, the last one at the top, never at a rung boundary lower down.
//! [`bands`] mirrors that, so the ladder [`super::profile`] flies stops climbing at the same
//! altitude the native schedule does.

use alas_config::mission::MissionProfileConfig;

/// Climb-rung altitude bands `(takeoff top, initial top, step-one top,
/// ladder top)` for a cruise at `cruise_m` from `departure_m`.
///
/// `ladder top` is the altitude the ladder reaches: always `cruise_m`.
pub(crate) fn bands(
    profile: &MissionProfileConfig,
    departure_m: f64,
    cruise_m: f64,
) -> (f64, f64, f64, f64) {
    let takeoff_top = (departure_m + profile.takeoff_altitude_gain_m).min(cruise_m);
    let initial_top = (cruise_m * profile.initial_climb_altitude_fraction)
        .max(takeoff_top)
        .min(cruise_m);
    let mut initial_top = initial_top;
    let mut step_one_top = (cruise_m * profile.step_climb_1_altitude_fraction)
        .max(initial_top)
        .min(cruise_m);
    // As the native schedule (`schedule::reach::flown_cruise_legs`): a rung gaining less than
    // the 30 m reach tolerance is not flown, and the highest flown rung ends at the top.
    if step_one_top - initial_top < MINIMUM_RUNG_GAIN_M {
        step_one_top = initial_top;
    }
    if cruise_m - step_one_top < MINIMUM_RUNG_GAIN_M {
        if step_one_top > initial_top {
            step_one_top = cruise_m;
        } else {
            initial_top = cruise_m;
            step_one_top = cruise_m;
        }
    }
    (takeoff_top, initial_top, step_one_top, cruise_m)
}

/// A climb rung gaining less than this, m, is not flown (the native schedule's
/// `MINIMUM_RUNG_GAIN_M`, the 30 m reach tolerance of `Builder::reach`).
const MINIMUM_RUNG_GAIN_M: f64 = 30.0;

/// Altitude the climb ladder tops out at (and where cruise then flies) for a
/// cruise at `cruise_m`; see [`bands`].
pub(crate) fn top_m(profile: &MissionProfileConfig, departure_m: f64, cruise_m: f64) -> f64 {
    bands(profile, departure_m, cruise_m).3
}
