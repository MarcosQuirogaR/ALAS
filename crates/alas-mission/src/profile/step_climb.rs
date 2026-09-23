// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The physically derived minimum step-climb leg duration (physics review
//! v1.2, section 2.3). Split out of `profile.rs` to keep that file at its
//! frozen review size; see `docs/source-size-budgets.tsv`.

use alas_config::mission::MissionProfileConfig;

/// ISA/US Standard Atmosphere 1976 pressure scale height in the stratosphere
/// (the isothermal 216.65 K layer from 11 km to 20 km; ICAO Doc 7488 tabulates
/// the same layer), `H_p = R T / g0`: the specific gas constant of air,
/// 287.05 J/(kg K), times 216.65 K, over standard gravity, 9.80665 m/s^2.
/// [`minimum_cruise_leg_duration_for_step_s`] is the one place this number is
/// used, and `alas-mission` keeps its own copy rather than reaching into
/// `alas-atmo`'s internal breakpoint table for it.
const STRATOSPHERE_PRESSURE_SCALE_HEIGHT_M: f64 = 287.05 * 216.65 / 9.80665;

/// A representative cruise fuel-burn fraction, per hour of flight: the
/// middle of the 2.5-4 %/hour range a jet transport typically burns its
/// weight down at cruise (physics review v1.2, section 2.3). This function
/// has no live fuel-flow state to read (`propose_profile_for_route` builds a
/// candidate profile before anything has been flown), so it stands in for
/// one; a caller that does have the mission's own burn rate should derive
/// its own minimum instead of using this constant.
const TYPICAL_CRUISE_FUEL_BURN_FRACTION_PER_HOUR: f64 = 0.0325;

/// Minimum time a cruise leg reached by a step climb of `step_m` must be
/// flyable for before that climb is worth its own transition cost, in
/// seconds.
///
/// A step climb spends a climb segment now, at extra fuel and no forward
/// progress, in exchange for a lower specific fuel consumption at the new,
/// weight-reduced optimum altitude. That trade only pays back over time, and
/// how fast depends on how far the aircraft still has to climb to catch up
/// with the optimum: at constant `W/delta`, `dh = H_p dW/W`, so the optimum
/// altitude rises at `dh/dt = H_p (fuel flow / W)`
/// ([`STRATOSPHERE_PRESSURE_SCALE_HEIGHT_M`],
/// [`TYPICAL_CRUISE_FUEL_BURN_FRACTION_PER_HOUR`]) and a step of `step_m` is
/// worth flying to once the remaining leg can hold the new level for at
/// least `step_m / (dh/dt)`.
///
/// A single fixed duration for every step (a commonly quoted 30 minutes, on
/// the basis that the optimum altitude rises "1,000-2,000 ft per hour")
/// overstates the physical rate by roughly 2-3x: across the 2.5-4 %/hour
/// burn range the optimum altitude rises 520-830 ft/h. See the
/// `crates/alas-mission/src/profile.rs` tests
/// `a_short_declared_sector_does_not_receive_the_long_route_step_ladder` and
/// `a_long_route_keeps_the_two_leg_profile_under_the_corrected_minimum` for
/// where the margin falls under this rate.
pub(super) fn minimum_cruise_leg_duration_for_step_s(step_m: f64) -> f64 {
    if !step_m.is_finite() || step_m <= 0.0 {
        return 0.0;
    }
    let optimum_altitude_rise_rate_m_s =
        STRATOSPHERE_PRESSURE_SCALE_HEIGHT_M * TYPICAL_CRUISE_FUEL_BURN_FRACTION_PER_HOUR / 3600.0;
    step_m / optimum_altitude_rise_rate_m_s
}

/// The altitude ladder `estimate_non_cruise_distance` climbs through, up to
/// three levels: the initial climb-out level, the level after the first
/// step climb, and cruise altitude itself. Duplicated here (rather than
/// having `estimate_non_cruise_distance` return it) so
/// `cruise_legs_have_enough_time_to_justify_the_ladder` can size each step's
/// climb *before* a leg count is chosen, from the same two floors
/// `estimate_non_cruise_distance` applies: the same call, one number
/// forward, both places, is the alternative kept out to avoid restructuring
/// that function's control flow for a query only this caller needs.
pub(super) fn step_climb_levels_m(
    profile: &MissionProfileConfig,
    cruise_altitude_m: f64,
    departure_elevation_m: f64,
) -> [f64; 3] {
    let first_level_m = (cruise_altitude_m * profile.initial_climb_altitude_fraction)
        .max(departure_elevation_m + 3000.0);
    let second_level_m =
        (cruise_altitude_m * profile.step_climb_1_altitude_fraction).max(first_level_m + 300.0);
    [first_level_m, second_level_m, cruise_altitude_m]
}
