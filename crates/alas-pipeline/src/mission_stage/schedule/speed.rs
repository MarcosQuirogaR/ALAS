// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The native schedule's side of the CAS/Mach schedule, the MMO limit and the
//! standard-day cruise speed on the flown day. The definitions are shared
//! with the MDO mission model through `alas_config::mission::SpeedCaps`.

use alas_atmo::us1976_try_compute_values;
use alas_config::mission::MissionProfileConfig;
use alas_mission::segments::{SegmentKind, SegmentSpec};

/// Give every climb (take-off included) and descent (landing included) leg
/// the profile's Mach hold and MMO/VMO limits, and every level leg the MMO/VMO
/// limits alone.
pub(in crate::mission_stage) fn apply_speed_caps(
    schedule: &mut [SegmentSpec],
    profile: &MissionProfileConfig,
) {
    for spec in schedule {
        match spec.kind {
            SegmentKind::Climb { .. } => spec.speed_caps = profile.climb_caps(),
            SegmentKind::Descent { .. } => spec.speed_caps = profile.descent_caps(),
            // A level leg holds no Mach schedule, but VMO and MMO bound it as
            // they bound a climb: the junction holds its speed, and the target
            // of a level speed change into or out of it, to them.
            SegmentKind::Cruise { .. } => spec.speed_caps = profile.certified_caps(),
        }
    }
}

/// Speed of sound at `cruise_altitude_m` on a day `isa_deviation_c` off
/// standard, m/s.
fn speed_of_sound_m_s(cruise_altitude_m: f64, isa_deviation_c: f64) -> Result<f64, String> {
    us1976_try_compute_values(cruise_altitude_m, isa_deviation_c)
        .map(|values| values.speed_of_sound_m_s)
        .map_err(|error| format!("mission cruise atmosphere is unusable: {error}"))
}

/// `a(T_ISA + dT) / a(T_ISA)` at `cruise_altitude_m`: the factor taking a
/// declared standard-day cruise true airspeed (`M a_ISA`) to the same Mach
/// number on the flown day. Exactly `1` on a standard day.
pub(in crate::mission_stage) fn standard_day_speed_ratio(
    cruise_altitude_m: f64,
    isa_deviation_c: f64,
) -> Result<f64, String> {
    if isa_deviation_c == 0.0 {
        return Ok(1.0);
    }
    Ok(speed_of_sound_m_s(cruise_altitude_m, isa_deviation_c)?
        / speed_of_sound_m_s(cruise_altitude_m, 0.0)?)
}

/// The Mach number of the declared standard-day cruise speed `tas_m_s` on the
/// flown day at `cruise_altitude_m`, to be held at or below MMO.
pub(in crate::mission_stage) fn cruise_mach(
    tas_m_s: f64,
    cruise_altitude_m: f64,
    isa_deviation_c: f64,
) -> Result<f64, String> {
    let ratio = standard_day_speed_ratio(cruise_altitude_m, isa_deviation_c)?;
    Ok(tas_m_s * ratio / speed_of_sound_m_s(cruise_altitude_m, isa_deviation_c)?)
}

/// Check one climb or descent leg's configured airspeed and vertical rate.
pub(super) fn validate_speed_rate(name: &str, speed_m_s: f64, rate_m_s: f64) -> Result<(), String> {
    if !speed_m_s.is_finite() || speed_m_s <= 0.0 {
        return Err(format!(
            "mission {name} airspeed must be finite and positive, got {speed_m_s}"
        ));
    }
    if !rate_m_s.is_finite() || rate_m_s <= 0.0 {
        return Err(format!(
            "mission {name} vertical rate must be finite and positive, got {rate_m_s}"
        ));
    }
    if rate_m_s >= speed_m_s {
        return Err(format!(
            "mission {name} vertical rate {rate_m_s} m/s must be below airspeed {speed_m_s} m/s"
        ));
    }
    Ok(())
}
