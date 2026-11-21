// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The three leg constructors of the schedule: a climb, a cruise and a descent leg with the
//! defaults every scheduled leg shares (sixteen control points, full throttle, no speed blend).

use alas_config::mission::SpeedReference;
use alas_mission::segments::{SegmentKind, SegmentSpec};

use super::CONTROL_POINTS;

pub(in crate::mission_stage) fn climb(
    tag: &str,
    start: Option<f64>,
    end: f64,
    speed: f64,
    speed_reference: SpeedReference,
    rate: f64,
    temperature_deviation_k: f64,
) -> SegmentSpec {
    SegmentSpec {
        tag: tag.to_owned(),
        kind: SegmentKind::Climb {
            altitude_start_m: start,
            altitude_end_m: end,
            climb_rate_m_s: rate,
        },
        air_speed_m_s: speed,
        air_speed_reference: speed_reference,
        speed_caps: alas_config::mission::SpeedCaps::default(),
        true_course_rad: 0.0,
        temperature_deviation_k,
        number_control_points: CONTROL_POINTS,
        takeoff_rated: false,
        rating_limited: false,
        configuration_delta_cd: 0.0,
        load_factor: 1.0,
        entry_speed_m_s: 0.0,
        exit_speed_m_s: 0.0,
        speed_profile: Vec::new(),
    }
}

pub(in crate::mission_stage) fn cruise(
    tag: &str,
    altitude: Option<f64>,
    distance: f64,
    speed: f64,
    temperature_deviation_k: f64,
) -> SegmentSpec {
    SegmentSpec {
        tag: tag.to_owned(),
        kind: SegmentKind::Cruise {
            altitude_m: altitude,
            distance_m: distance,
        },
        air_speed_m_s: speed,
        // A cruise leg is always a literal true airspeed (or a Mach-derived
        // one for a preset's operational route): never calibrated airspeed,
        // regardless of the profile's climb/descent speed reference.
        air_speed_reference: SpeedReference::TrueAirspeed,
        speed_caps: alas_config::mission::SpeedCaps::default(),
        true_course_rad: 0.0,
        temperature_deviation_k,
        number_control_points: CONTROL_POINTS,
        takeoff_rated: false,
        rating_limited: false,
        configuration_delta_cd: 0.0,
        load_factor: 1.0,
        entry_speed_m_s: 0.0,
        exit_speed_m_s: 0.0,
        speed_profile: Vec::new(),
    }
}

pub(in crate::mission_stage) fn descent(
    tag: &str,
    start: Option<f64>,
    end: f64,
    speed: f64,
    speed_reference: SpeedReference,
    rate: f64,
    temperature_deviation_k: f64,
) -> SegmentSpec {
    SegmentSpec {
        tag: tag.to_owned(),
        kind: SegmentKind::Descent {
            altitude_start_m: start,
            altitude_end_m: end,
            descent_rate_m_s: rate,
        },
        air_speed_m_s: speed,
        air_speed_reference: speed_reference,
        speed_caps: alas_config::mission::SpeedCaps::default(),
        true_course_rad: 0.0,
        temperature_deviation_k,
        number_control_points: CONTROL_POINTS,
        takeoff_rated: false,
        rating_limited: false,
        configuration_delta_cd: 0.0,
        load_factor: 1.0,
        entry_speed_m_s: 0.0,
        exit_speed_m_s: 0.0,
        speed_profile: Vec::new(),
    }
}
