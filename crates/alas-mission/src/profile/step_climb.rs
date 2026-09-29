// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! First-order cruise altitude-drift model for route-aware step-climb
//! recommendations. Split out of `profile.rs` to keep that file at its
//! frozen review size; see `docs/source-size-budgets.tsv`.

use alas_atmo::{pressure_isa, temperature_isa};
use alas_config::mission::{resolve_true_airspeed_m_s, MissionProfileConfig, SpeedReference};

/// ISA/US Standard Atmosphere 1976 pressure scale height in the stratosphere
/// (the isothermal 216.65 K layer from 11 km to 20 km; ICAO Doc 7488 tabulates
/// the same layer), `H_p = R T / g0`: the specific gas constant of air,
/// 287.05 J/(kg K), times 216.65 K, over standard gravity, 9.80665 m/s^2.
/// `alas-mission` keeps its own copy rather than reaching into `alas-atmo`'s
/// internal breakpoint table for it.
const STRATOSPHERE_PRESSURE_SCALE_HEIGHT_M: f64 = 287.05 * 216.65 / 9.80665;

/// Representative fractional mass burn per hour used when the editor has no
/// solved fuel-flow/mass history yet. This is a planning prior, not an
/// aircraft-specific engine-deck result. ADS-B observations are used only to
/// check the resulting route-scale behavior; they do not contain mass or
/// fuel-flow data from which this rate could be directly fitted.
const TYPICAL_CRUISE_FUEL_BURN_FRACTION_PER_HOUR: f64 = 0.0325;

/// Nominal cruise-level interval used by the route recommendation, 2,000 ft
/// expressed in SI units.
pub(super) const CRUISE_LEVEL_INTERVAL_M: f64 = 2000.0 * 0.3048;

/// Cruise time after a step needed to drift one level interval, in seconds.
/// This allows the aircraft to use the higher level and continue losing mass
/// before another step is considered. It is a planning estimate, not an
/// AFM/dispatch rule.
///
/// For a jet transport in the isothermal stratosphere, a first-order
/// Breguet-style altitude drift estimate is `dh/dt = H_p * (fuel flow / W)`.
/// With the current preflight prior, that is about 206 m/h (676 ft/h); a
/// 2,000 ft increment therefore needs about 2.96 h after the step; the first
/// step also needs half an interval (about 1.48 h) of prior cruise to reach
/// the midpoint to the next level.
/// The prior and threshold are intentionally centralized so calibration can
/// replace them when ALAS has a solved per-aircraft fuel-flow estimate.
pub(super) fn minimum_cruise_time_for_step_s(step_m: f64) -> f64 {
    if !step_m.is_finite() || step_m <= 0.0 {
        return 0.0;
    }
    let optimum_altitude_rise_rate_m_s =
        STRATOSPHERE_PRESSURE_SCALE_HEIGHT_M * TYPICAL_CRUISE_FUEL_BURN_FRACTION_PER_HOUR / 3600.0;
    step_m / optimum_altitude_rise_rate_m_s
}

/// Cruise time needed for optimum-altitude drift to reach the midpoint of
/// the next `step_m` level before the first step climb.
fn minimum_trigger_cruise_time_for_step_s(step_m: f64) -> f64 {
    0.5 * minimum_cruise_time_for_step_s(step_m)
}

/// The relative share of the post-climb/descent cruise remainder each active
/// cruise leg would fly, for a candidate with `count` active legs.
fn active_cruise_leg_fractions(profile: &MissionProfileConfig, count: usize) -> [f64; 3] {
    let count = count.clamp(1, 3);
    match count {
        1 => [1.0, 0.0, 0.0],
        // The first level occupies one third of a minimum step opportunity
        // (half-step trigger); the higher level gets two thirds for the full
        // interval needed after the climb.
        2 => [1.0 / 3.0, 2.0 / 3.0, 0.0],
        _ => {
            let weights = [
                profile.cruise_1_distance_fraction,
                profile.cruise_2_distance_fraction,
                profile.cruise_3_distance_fraction,
            ];
            if weights
                .iter()
                .all(|weight| weight.is_finite() && *weight > 1.0e-9)
            {
                let total = weights.iter().sum::<f64>();
                weights.map(|weight| weight / total)
            } else {
                let defaults = MissionProfileConfig::default();
                let weights = [
                    defaults.cruise_1_distance_fraction,
                    defaults.cruise_2_distance_fraction,
                    defaults.cruise_3_distance_fraction,
                ];
                let total = weights.iter().sum::<f64>();
                weights.map(|weight| weight / total)
            }
        }
    }
}

/// Configure the active cruise count and its matching altitude ladder.
///
/// Each extra cruise leg adds one step climb. Levels are separated by about
/// 2,000 ft and the final active level is just below `cruise_altitude_m`,
/// because the serialized altitude fractions must remain below one. This
/// keeps a one-leg profile at cruise altitude and prevents the selected leg
/// count from leaving an aircraft parked at an unrelated preset schedule
/// level. Other aircraft-specific phase speeds and rates are preserved.
pub fn configure_cruise_legs(
    profile: &mut MissionProfileConfig,
    count: usize,
    cruise_altitude_m: f64,
    departure_elevation_m: f64,
) {
    let fractions = active_cruise_leg_fractions(profile, count);
    profile.cruise_1_distance_fraction = fractions[0];
    profile.cruise_2_distance_fraction = fractions[1];
    profile.cruise_3_distance_fraction = fractions[2];

    if !cruise_altitude_m.is_finite() || cruise_altitude_m <= 0.0 {
        return;
    }
    let count = count.clamp(1, 3);
    let steps = count.saturating_sub(1);
    let final_level_m = (cruise_altitude_m - 0.5).max(0.0);
    let floor_m = departure_elevation_m + 3000.0;
    let available_step_m = if steps == 0 {
        CRUISE_LEVEL_INTERVAL_M
    } else {
        ((final_level_m - floor_m).max(0.0) / steps as f64).max(300.0)
    };
    let step_m = CRUISE_LEVEL_INTERVAL_M.min(available_step_m);
    let first_level_m = if steps == 0 {
        (final_level_m - 1.0).max(0.0)
    } else {
        (final_level_m - steps as f64 * step_m).max(floor_m)
    };
    let second_level_m = match count {
        1 => (final_level_m - 0.5).max(first_level_m + 0.001),
        2 => final_level_m,
        _ => (first_level_m + step_m).min(final_level_m),
    };
    let initial_fraction = (first_level_m / cruise_altitude_m).clamp(0.0, 1.0 - 2.0e-6);
    profile.initial_climb_altitude_fraction = initial_fraction;
    profile.step_climb_1_altitude_fraction =
        (second_level_m / cruise_altitude_m).clamp(initial_fraction + 1.0e-6, 1.0 - 1.0e-6);
}

/// Whether sequential cruise-leg durations support every step in this
/// candidate ladder, given the route distance left over after climb and
/// descent.
///
/// One cruise leg contains no step climb and is always available as the
/// fallback. The first cruise segment must accumulate enough weight loss to
/// reach the midpoint to the next level; each later segment must then provide
/// one full level interval of drift after its preceding climb. This respects
/// the order in which the profile flies its cruise legs.
pub(super) fn cruise_legs_have_enough_time_to_justify_the_ladder(
    profile: &MissionProfileConfig,
    route_distance_m: f64,
    non_cruise_distance_m: f64,
    active_cruise_legs: usize,
    cruise_altitude_m: f64,
    departure_elevation_m: f64,
) -> bool {
    if active_cruise_legs < 2 {
        return true;
    }
    let cruise_remainder_m = route_distance_m - non_cruise_distance_m;
    if !cruise_remainder_m.is_finite() || cruise_remainder_m <= 0.0 {
        return false;
    }
    let fractions = active_cruise_leg_fractions(profile, active_cruise_legs);
    let cruise_air_speeds_m_s = [
        profile.cruise_1_air_speed_m_s,
        profile.cruise_2_air_speed_m_s,
        profile.cruise_3_air_speed_m_s,
    ];
    let levels_m = step_climb_levels_m(profile, cruise_altitude_m, departure_elevation_m);
    let mut cruise_leg_durations_s = [0.0; 3];
    for (index, (&fraction, &speed_m_s)) in fractions
        .iter()
        .zip(cruise_air_speeds_m_s.iter())
        .take(active_cruise_legs)
        .enumerate()
    {
        if !fraction.is_finite() || fraction < 0.0 || !speed_m_s.is_finite() || speed_m_s <= 0.0 {
            return false;
        }
        cruise_leg_durations_s[index] = cruise_remainder_m * fraction / speed_m_s;
        if !cruise_leg_durations_s[index].is_finite() {
            return false;
        }
    }

    let step_count = active_cruise_legs - 1;
    let first_step_m = (levels_m[1] - levels_m[0]).max(0.0);
    if cruise_leg_durations_s[0] < minimum_trigger_cruise_time_for_step_s(first_step_m) {
        return false;
    }
    (0..step_count).all(|step_index| {
        let step_m = (levels_m[step_index + 1] - levels_m[step_index]).max(0.0);
        cruise_leg_durations_s[step_index + 1] >= minimum_cruise_time_for_step_s(step_m)
    })
}

/// Estimate the route's horizontal climb/descent footprint for a configured
/// cruise-level count. This deliberately stays first order: phase speeds and
/// rates are resolved from the editable profile, while the route solver later
/// closes the exact segment schedule.
pub(super) fn estimate_non_cruise_distance(
    profile: &MissionProfileConfig,
    cruise_altitude_m: f64,
    departure_elevation_m: f64,
    arrival_elevation_m: f64,
    active_cruise_legs: usize,
) -> Result<f64, String> {
    let mut current_m = departure_elevation_m;
    let mut distance_m = 0.0;
    let leg = |current_m: &mut f64,
               distance_m: &mut f64,
               target_m: f64,
               speed_m_s: f64,
               rate_m_s: f64,
               midpoint_m: f64| {
        let delta_m = (target_m - *current_m).abs();
        if delta_m <= 1.0e-9 {
            *current_m = target_m;
            return Ok::<(), String>(());
        }
        let speed_m_s = resolved_profile_speed(profile, speed_m_s, midpoint_m)?;
        if !speed_m_s.is_finite() || speed_m_s <= 0.0 || !rate_m_s.is_finite() || rate_m_s <= 0.0 {
            return Err(format!(
                "mission proposal needs positive finite speed/rate, got speed={speed_m_s}, rate={rate_m_s}"
            ));
        }
        if speed_m_s <= rate_m_s {
            return Err(format!(
                "mission proposal vertical rate {rate_m_s} m/s is not below airspeed {speed_m_s} m/s"
            ));
        }
        let horizontal_speed_m_s = (speed_m_s * speed_m_s - rate_m_s * rate_m_s).sqrt();
        *distance_m += delta_m / rate_m_s * horizontal_speed_m_s;
        *current_m = target_m;
        Ok(())
    };

    leg(
        &mut current_m,
        &mut distance_m,
        departure_elevation_m + profile.takeoff_altitude_gain_m,
        profile.takeoff_air_speed_m_s,
        profile.takeoff_climb_rate_m_s,
        departure_elevation_m,
    )?;
    // The same levels the ladder-duration check judges, so the footprint and
    // the check describe one schedule.
    let [first_level_m, second_level_m, _] =
        step_climb_levels_m(profile, cruise_altitude_m, departure_elevation_m);
    let first_level_midpoint_m = 0.5 * (current_m + first_level_m);
    leg(
        &mut current_m,
        &mut distance_m,
        first_level_m,
        profile.initial_climb_air_speed_m_s,
        profile.initial_climb_rate_m_s,
        first_level_midpoint_m,
    )?;
    if active_cruise_legs >= 2 {
        let second_level_midpoint_m = 0.5 * (current_m + second_level_m);
        leg(
            &mut current_m,
            &mut distance_m,
            second_level_m,
            profile.step_climb_1_air_speed_m_s,
            profile.step_climb_1_rate_m_s,
            second_level_midpoint_m,
        )?;
    }
    if active_cruise_legs >= 3 {
        let cruise_midpoint_m = 0.5 * (current_m + cruise_altitude_m);
        leg(
            &mut current_m,
            &mut distance_m,
            cruise_altitude_m,
            profile.step_climb_2_air_speed_m_s,
            profile.step_climb_2_rate_m_s,
            cruise_midpoint_m,
        )?;
    }

    for (altitude_ft, speed_m_s, rate_m_s) in [
        (
            profile.descent_1_altitude_ft,
            profile.descent_1_air_speed_m_s,
            profile.descent_1_rate_m_s,
        ),
        (
            profile.descent_2_altitude_ft,
            profile.descent_2_air_speed_m_s,
            profile.descent_2_rate_m_s,
        ),
        (
            profile.descent_3_altitude_ft,
            profile.descent_3_air_speed_m_s,
            profile.descent_3_rate_m_s,
        ),
        (
            profile.descent_4_altitude_ft,
            profile.descent_4_air_speed_m_s,
            profile.descent_4_rate_m_s,
        ),
    ] {
        let target_m = altitude_ft * 0.3048;
        if target_m > arrival_elevation_m && target_m < current_m {
            let descent_midpoint_m = 0.5 * (current_m + target_m);
            leg(
                &mut current_m,
                &mut distance_m,
                target_m,
                speed_m_s,
                rate_m_s,
                descent_midpoint_m,
            )?;
        }
    }
    if arrival_elevation_m < current_m {
        let landing_midpoint_m = 0.5 * (current_m + arrival_elevation_m);
        leg(
            &mut current_m,
            &mut distance_m,
            arrival_elevation_m,
            profile.landing_air_speed_m_s,
            profile.landing_descent_rate_m_s,
            landing_midpoint_m,
        )?;
    }
    Ok(distance_m)
}

fn resolved_profile_speed(
    profile: &MissionProfileConfig,
    configured_speed_m_s: f64,
    altitude_m: f64,
) -> Result<f64, String> {
    match profile.climb_descent_speed_reference {
        SpeedReference::TrueAirspeed => Ok(configured_speed_m_s),
        SpeedReference::CalibratedAirspeed => resolve_true_airspeed_m_s(
            SpeedReference::CalibratedAirspeed,
            configured_speed_m_s,
            pressure_isa(altitude_m),
            temperature_isa(altitude_m),
        )
        .map_err(|error| {
            format!("mission proposal could not resolve calibrated airspeed: {error}")
        }),
    }
}

/// The altitude ladder `estimate_non_cruise_distance` climbs through, up to
/// three levels: the initial climb-out level, the level after the first
/// step climb, and cruise altitude itself. Duplicated here (rather than
/// having `estimate_non_cruise_distance` return it) so
/// [`cruise_legs_have_enough_time_to_justify_the_ladder`] can size each step's
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
