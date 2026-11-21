// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The take-off climb: from the safety speed to the climb-out speed.
//!
//! The aeroplane clears the 35 ft screen at the take-off safety speed `V2`
//! (CS 25.107(c): `V2 >= 1.13 VSR`, `VSR >= V_S1g`), not at the 250 kt CAS the
//! OpenAP climb ladders open their take-off band with. It climbs at `V2` with the
//! take-off rating to the acceleration height
//! ([`alas_config::mission::TAKEOFF_ACCELERATION_HEIGHT_M`], 1,000 ft above the field; the
//! constant gives what its choice rests on), then accelerates
//! to the climb-out speed *along the climb* with BADA 3.8's energy share for an
//! acceleration in climb (`f = 0.3`, see `energy_share_factor`), which fixes the height gained
//! (`alas_mission::segments::acceleration_climb_altitude_gain_m`). The priced route model
//! charges that climb by the energy method (`alas-opt` `takeoff_acceleration`); the native
//! flight flies it as two legs, the climb at `V2` and the take-off band whose entry speed
//! is `V2`, which the junction logic turns into the acceleration piece. Both legs are
//! take-off-rated: their command is the share of the take-off rating's force at each point
//! (`SegmentSpec::takeoff_rated`), and both are flown at that rating
//! (`SegmentSpec::rating_limited`), their vertical rate from the excess power.

use alas_mission::segments::{SegmentKind, SegmentSpec};

/// Tag of the constant-speed climb at `V2` that opens the flight.
pub(crate) const TAKEOFF_CLIMB_TAG: &str = "takeoff_climb";

/// What the priced route flew in front of its take-off band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TakeoffAcceleration {
    /// Take-off safety speed, m/s.
    pub v2_m_s: f64,
    /// Take-off rating over maximum-climb rating of the deck at the climb's
    /// condition: the climb is take-off-rated when it exceeds 1.
    pub thrust_ratio: f64,
}

/// Passes of the fixed point for the height the acceleration ends at.
const FIXED_POINT_PASSES: usize = 12;

/// Cut the first leg of `schedule` (the take-off band, at the band speed `to_m_s`, from the
/// field) into the climb at `acceleration.v2_m_s` to the acceleration height and the band from
/// there, whose top is raised to the end of the acceleration climb when the profile's own
/// take-off altitude gain does not reach it.
pub(super) fn split_takeoff_climb(
    schedule: &mut Vec<SegmentSpec>,
    acceleration: &TakeoffAcceleration,
    departure_elevation_m: f64,
    to_m_s: f64,
) {
    let SegmentKind::Climb {
        altitude_end_m,
        climb_rate_m_s,
        ..
    } = schedule[0].kind
    else {
        return;
    };
    let acceleration_m =
        departure_elevation_m + alas_config::mission::TAKEOFF_ACCELERATION_HEIGHT_M;
    // The band holds a calibrated airspeed, whose true airspeed rises with the height the
    // acceleration climbs to, so the end of the acceleration is where the energy-share speed meets
    // the band's speed at that height (a fixed point; it converges in a few passes).
    let mut end_m = acceleration_m
        + alas_mission::segments::acceleration_climb_altitude_gain_m(acceleration.v2_m_s, to_m_s);
    for _ in 0..FIXED_POINT_PASSES {
        let Some(band_m_s) = super::speed_change::leg_speed_m_s(&schedule[0], end_m) else {
            break;
        };
        let next_m = acceleration_m
            + alas_mission::segments::acceleration_climb_altitude_gain_m(
                acceleration.v2_m_s,
                band_m_s,
            );
        let moved_m = (next_m - end_m).abs();
        end_m = next_m;
        if moved_m < 0.01 {
            break;
        }
    }
    let rated = acceleration.thrust_ratio > 1.0;
    let mut climb = schedule[0].clone();
    climb.tag = TAKEOFF_CLIMB_TAG.to_owned();
    climb.kind = SegmentKind::Climb {
        altitude_start_m: None,
        altitude_end_m: acceleration_m,
        climb_rate_m_s,
    };
    climb.air_speed_m_s = acceleration.v2_m_s;
    climb.air_speed_reference = alas_config::mission::SpeedReference::TrueAirspeed;
    climb.speed_caps = alas_config::mission::SpeedCaps::default();
    climb.entry_speed_m_s = 0.0;
    climb.exit_speed_m_s = 0.0;
    climb.takeoff_rated = rated;
    // The band keeps a short stretch at its own speed after the acceleration meets it (a piece
    // shorter than the junction logic's minimum is not cut off).
    let band_top_m = altitude_end_m.max(end_m + alas_config::mission::TAKEOFF_BAND_STRETCH_M);
    schedule[0].kind = SegmentKind::Climb {
        altitude_start_m: Some(acceleration_m),
        altitude_end_m: band_top_m,
        climb_rate_m_s,
    };
    schedule[0].takeoff_rated = rated;
    schedule.insert(0, climb);
}

// The fixture is built here; a failed unwrap is the assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::mission::{
        SpeedReference, TAKEOFF_ACCELERATION_HEIGHT_M, TAKEOFF_BAND_STRETCH_M,
    };

    /// The split of a 250 kt CAS band at a sea-level field: the climb at V2 ends at the
    /// acceleration height; the band starts there and reaches past the end of the acceleration, the
    /// height where the BADA energy-share speed meets the band's own speed (the same fixed point the
    /// priced model's `takeoff_climb` solves), plus the band stretch; both legs are take-off-rated.
    #[test]
    fn the_split_ends_the_acceleration_where_the_energy_share_speed_meets_the_band() {
        let band_cas_m_s = alas_config::mission::SPEED_RESTRICTION_CAS_M_S;
        let mut schedule = vec![super::super::specs::climb(
            "takeoff",
            None,
            457.2,
            band_cas_m_s,
            SpeedReference::CalibratedAirspeed,
            11.0,
            0.0,
        )];
        let v2_m_s = 65.0;
        let acceleration = TakeoffAcceleration {
            v2_m_s,
            thrust_ratio: 1.3,
        };
        let band_tas = |schedule: &[SegmentSpec], height_m: f64| {
            super::super::speed_change::leg_speed_m_s(&schedule[0], height_m).unwrap()
        };
        let to_m_s = band_tas(&schedule, 0.0);
        let before = schedule.clone();
        split_takeoff_climb(&mut schedule, &acceleration, 0.0, to_m_s);
        assert_eq!(schedule[0].tag, TAKEOFF_CLIMB_TAG);
        assert_eq!(schedule[0].air_speed_m_s, v2_m_s);
        let SegmentKind::Climb {
            altitude_end_m: climb_end_m,
            ..
        } = schedule[0].kind
        else {
            panic!("the take-off climb is a climb");
        };
        assert!((climb_end_m - TAKEOFF_ACCELERATION_HEIGHT_M).abs() < 1.0e-9);
        let SegmentKind::Climb {
            altitude_start_m,
            altitude_end_m: band_top_m,
            ..
        } = schedule[1].kind
        else {
            panic!("the take-off band is a climb");
        };
        assert_eq!(altitude_start_m, Some(TAKEOFF_ACCELERATION_HEIGHT_M));
        // The acceleration ends where its energy-share speed meets the band's speed at that height.
        let end_m = band_top_m - TAKEOFF_BAND_STRETCH_M;
        let met_m_s = band_tas(&before, end_m);
        let gain_m = alas_mission::segments::acceleration_climb_altitude_gain_m(v2_m_s, met_m_s);
        assert!(
            (end_m - (TAKEOFF_ACCELERATION_HEIGHT_M + gain_m)).abs() < 0.1,
            "ends at {end_m} m, the energy share gives {} m",
            TAKEOFF_ACCELERATION_HEIGHT_M + gain_m
        );
        assert!(schedule[0].takeoff_rated && schedule[1].takeoff_rated);
    }
}
