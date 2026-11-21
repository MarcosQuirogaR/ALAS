// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Legal cruising levels and the step-climb trigger.
//!
//! The ICAO table of cruising levels (ICAO Annex 2, *Rules of the Air*,
//! Appendix 3, table a, RVSM and non-RVSM columns) gives the levels a trip
//! may be planned at, and transport practice holds a level and climbs one
//! same-direction separation as soon as the weight burned off lets the
//! aircraft hold the next level (Torenbeek, "Advanced Aircraft Design",
//! Wiley, 2013, Sec. 12.6, p. 384, stepped cruise/climb; Jenkinson, Simpkin
//! and Rhodes, "Civil Jet Aircraft Design", Arnold, 1999, Sec. 10.5, p. 268
//! and Fig. 10.30, step cruise profile about the optimum cruise altitude).
//! Every mission flown under the `OptimumStep` policy plans its levels with
//! these functions (`cruise_levels`), for every aircraft alike.

use alas_units::FOOT;

/// One flight level unit, 100 ft in metres (a flight level is a standard
/// pressure altitude in hundreds of feet, ICAO Annex 2, Appendix 3).
const FLIGHT_LEVEL_UNIT_M: f64 = 100.0 * FOOT;

/// Top of the reduced vertical separation band, FL410 (ICAO Annex 2,
/// App. 3, table a). Up to it the cruising levels of the two directions
/// alternate every 1,000 ft (eastbound FL290, 310, ..., 410; westbound
/// FL300, 320, ..., 400; odd and even thousands below FL290), so the levels
/// of both directions together lie on every 10 FL and one direction's
/// levels are 20 FL apart. Above it each direction's levels are 40 FL apart
/// (eastbound FL450, 490, ...; westbound FL430, 470, ...), 20 FL apart
/// together.
const RVSM_TOP_FL: f64 = 410.0;
const BOTH_DIRECTIONS_BELOW_TOP_FL: f64 = 10.0;
const BOTH_DIRECTIONS_ABOVE_TOP_FL: f64 = 20.0;
const SAME_DIRECTION_BELOW_TOP_FL: f64 = 20.0;
const SAME_DIRECTION_ABOVE_TOP_FL: f64 = 40.0;
/// Last westbound level of the band (FL400), whose next level of the same
/// direction is the first westbound one above it (FL430).
const LAST_WESTBOUND_IN_TOP_FL: f64 = 400.0;
const FIRST_WESTBOUND_ABOVE_TOP_FL: f64 = 430.0;

/// Tolerance, in flight levels, under which an altitude counts as lying on a
/// level (1e-6 FL is 30 micrometres: the metre-foot round trip only).
const LEVEL_SNAP_FL: f64 = 1.0e-6;

/// The flight level of `altitude_m`, snapped to the whole level when it lies
/// within the metre-foot round trip ([`LEVEL_SNAP_FL`]) of one, so a level
/// produced by this module steps and rounds from its exact whole value.
fn snapped_fl(altitude_m: f64) -> f64 {
    let raw = altitude_m / FLIGHT_LEVEL_UNIT_M;
    let whole = raw.round();
    if (raw - whole).abs() <= LEVEL_SNAP_FL {
        whole
    } else {
        raw
    }
}

/// The highest cruising level of the ICAO table at or below `altitude_m`, m.
///
/// The design mission, the payload-range corners and a still-air route
/// carry no track, so the level is taken from both directions together:
/// every 1,000 ft below FL410, every 2,000 ft from FL410 up (FL410, 430,
/// 450, ...; ICAO Annex 2, Appendix 3, table a). A non-finite altitude is
/// returned as it is.
#[must_use]
pub(crate) fn legal_level_at_or_below_m(altitude_m: f64) -> f64 {
    if !altitude_m.is_finite() {
        return altitude_m;
    }
    let fl = altitude_m / FLIGHT_LEVEL_UNIT_M + LEVEL_SNAP_FL;
    let legal_fl = if fl < RVSM_TOP_FL {
        (fl / BOTH_DIRECTIONS_BELOW_TOP_FL).floor() * BOTH_DIRECTIONS_BELOW_TOP_FL
    } else {
        RVSM_TOP_FL
            + ((fl - RVSM_TOP_FL) / BOTH_DIRECTIONS_ABOVE_TOP_FL).floor()
                * BOTH_DIRECTIONS_ABOVE_TOP_FL
    };
    legal_fl * FLIGHT_LEVEL_UNIT_M
}

/// A configured cruise altitude read as the flight level it names, m: the
/// whole flight level nearest to `altitude_m`, so within half a flight level
/// (50 ft, 15.24 m) of it.
///
/// A configured level is a flight level that a preset or a user states in
/// metres, rounded by the unit conversion (FL170 = 5,181.6 m stored as
/// 5,180 m, FL169.95). Without this reading the legal level at or below the
/// stored value would drop a whole 1,000 ft (FL160) for a 1.6 m rounding.
/// The tolerance is the unit conversion's, the same for every aircraft; it
/// only bounds levels from above (the planned level is still the legal level
/// at or below it and at or below the capability). A non-finite altitude is
/// returned as it is.
#[must_use]
pub(crate) fn configured_level_m(altitude_m: f64) -> f64 {
    if !altitude_m.is_finite() {
        return altitude_m;
    }
    (altitude_m / FLIGHT_LEVEL_UNIT_M).round() * FLIGHT_LEVEL_UNIT_M
}

/// The spacing of the legal levels of both directions at `altitude_m`, m:
/// 1,000 ft below FL410, 2,000 ft from it up. (Read by the tests that bound
/// the rounding of the initial level.)
#[cfg(test)]
#[must_use]
pub(crate) fn legal_level_spacing_m(altitude_m: f64) -> f64 {
    let spacing_fl = if altitude_m / FLIGHT_LEVEL_UNIT_M + LEVEL_SNAP_FL < RVSM_TOP_FL {
        BOTH_DIRECTIONS_BELOW_TOP_FL
    } else {
        BOTH_DIRECTIONS_ABOVE_TOP_FL
    };
    spacing_fl * FLIGHT_LEVEL_UNIT_M
}

/// The next cruising level of the same direction of flight above the level
/// `altitude_m`, m (ICAO Annex 2, App. 3, table a): 2,000 ft up to FL410
/// (FL310 -> 330, FL390 -> 410), then the column's first level above the
/// band (eastbound FL410 -> 450, westbound FL400 -> 430) and 4,000 ft above
/// it (FL430 -> 470, FL450 -> 490). A step climb is a climb to this level.
///
/// A legal level is snapped to its whole flight level before stepping, so
/// the result is bit-identical to [`legal_level_at_or_below_m`] of itself.
/// An altitude that is not a legal level (a prescribed `Declared` or
/// `Design` level lowered on a 1 m grid, `prescribed_level`) steps by the
/// same-direction separation of the band it lies in.
#[must_use]
pub(crate) fn next_same_direction_level_m(altitude_m: f64) -> f64 {
    let fl = snapped_fl(altitude_m);
    let next_fl = if fl + SAME_DIRECTION_BELOW_TOP_FL <= RVSM_TOP_FL {
        fl + SAME_DIRECTION_BELOW_TOP_FL
    } else if fl == LAST_WESTBOUND_IN_TOP_FL {
        FIRST_WESTBOUND_ABOVE_TOP_FL
    } else {
        fl + SAME_DIRECTION_ABOVE_TOP_FL
    };
    next_fl * FLIGHT_LEVEL_UNIT_M
}

/// Whether a step climb to the next level is due at the current mass: the
/// next level qualifies (300 ft/min residual climb at maximum-climb thrust,
/// under the maximum-cruise-thrust ceiling, 1.3 g to buffet onset:
/// `SegmentMissionModel::level_qualifies`) and its specific air range
/// exceeds the current level's.
///
/// Stepped cruise holds a level at constant speed and climbs one
/// same-direction separation as soon as the weight reduction lets the
/// aircraft hold the next level, toward the altitude of best range at the
/// current weight (Torenbeek, "Advanced Aircraft Design", Wiley, 2013,
/// Sec. 12.6, p. 384); the levels flown so bracket the optimum cruise
/// altitude, which rises as fuel burns off (Jenkinson, Simpkin and Rhodes,
/// "Civil Jet Aircraft Design", Arnold, 1999, Sec. 10.5, p. 268, Fig. 10.30,
/// where the 300 ft/min initial-cruise criterion is also stated). The
/// specific-range comparison is the "toward best range" half of that rule:
/// a step that would lower the specific range is not taken. The steps a plan
/// carries stop at the configured level (`ProfileGeometry::step_levels`), so
/// no step goes above it.
#[must_use]
pub(crate) fn step_climb_due(
    next_level_qualifies: bool,
    specific_air_range_here_m_kg: f64,
    specific_air_range_next_m_kg: f64,
) -> bool {
    next_level_qualifies
        && specific_air_range_here_m_kg.is_finite()
        && specific_air_range_next_m_kg > specific_air_range_here_m_kg
}

#[cfg(test)]
#[path = "flight_levels_tests.rs"]
mod tests;
