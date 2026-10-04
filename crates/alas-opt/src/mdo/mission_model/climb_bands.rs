// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Climb-ladder altitude-band gating shared by trip and diversion legs.
//!
//! `build_schedule` (`alas-pipeline::mission_stage::schedule`) only builds
//! `step_climb_1`/`cruise_step_2` when `cruise_2_distance_fraction > 0`, and
//! `step_climb_2`/`cruise_step_3` only when `cruise_3_distance_fraction > 0`;
//! a leg with a zero share is never built, regardless of its configured
//! altitude fraction. [`bands`] mirrors that gate so the ladder
//! [`super::profile`] flies stops climbing at the same altitude the native
//! schedule does.

use alas_config::mission::MissionProfileConfig;

/// Climb-rung altitude bands `(takeoff top, initial top, step-one top,
/// ladder top)` for a cruise at `cruise_m` from `departure_m`.
///
/// `ladder top` is the altitude the ladder actually reaches: `cruise_m` when
/// `step_climb_2` is active, `step_one_top` when only `step_climb_1` is
/// active, and `initial_top` when neither is.
pub(crate) fn bands(
    profile: &MissionProfileConfig,
    departure_m: f64,
    cruise_m: f64,
) -> (f64, f64, f64, f64) {
    let takeoff_top = (departure_m + profile.takeoff_altitude_gain_m).min(cruise_m);
    let initial_top = (cruise_m * profile.initial_climb_altitude_fraction)
        .max(takeoff_top)
        .min(cruise_m);
    let step_climb_1_active = profile.cruise_2_distance_fraction > 0.0;
    let step_climb_2_active = profile.cruise_3_distance_fraction > 0.0;
    let step_one_top = if step_climb_1_active {
        (cruise_m * profile.step_climb_1_altitude_fraction)
            .max(initial_top)
            .min(cruise_m)
    } else {
        initial_top
    };
    let ladder_top = if step_climb_2_active {
        cruise_m
    } else {
        step_one_top
    };
    (takeoff_top, initial_top, step_one_top, ladder_top)
}

/// Altitude the climb ladder actually tops out at (and where cruise then
/// flies) for a cruise at `cruise_m`; see [`bands`].
pub(crate) fn top_m(profile: &MissionProfileConfig, departure_m: f64, cruise_m: f64) -> f64 {
    bands(profile, departure_m, cruise_m).3
}
