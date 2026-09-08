// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/mission_config.py (`MissionProfileConfig`)
// Reference: alas @ rust-port-baseline.

//! The speed, rate and altitude profile the mission flies.
//!
//! Takeoff, an initial climb, two step climbs, three cruise legs, a
//! four-rung descent ladder and a landing. The defaults reproduce the
//! reference implementation's original worked example, which is a long-range
//! widebody profile; a different aircraft usually wants different numbers,
//! which is why they are here and not in the mission builder.
//!
//! Two of the quantities are fractions rather than absolutes, and that is
//! deliberate. The step-climb altitudes are fractions of the cruise altitude,
//! so raising the cruise level moves the steps with it instead of leaving
//! them stranded below. The cruise legs' distances are relative shares of the
//! route left after the climb and descent profile legs; active shares are
//! normalized so the schedule closes on the requested route.
//!
//! A descent rung whose altitude is below the arrival field's elevation is
//! skipped rather than flown into the ground.
//!
//! The fields declare no explanations upstream; the ones here are this
//! port's, as CONTRIBUTING.md requires, and they change no value.

use serde::{Deserialize, Serialize};

use crate::{ConfigNode, Kind, Leaf};

/// Which airspeed definition the takeoff, climb, descent and landing legs'
/// `*_air_speed_m_s` fields are stated in.
///
/// Only those legs: the three cruise legs are always a true airspeed (or, for
/// a preset's operational route, a true airspeed derived from a commanded
/// Mach at the configured cruise altitude -- see
/// [`crate::AircraftPreset::operational_mission_defaults`]) regardless of
/// this setting, so switching an aircraft's climb schedule to calibrated
/// airspeed cannot silently reinterpret an already-correct cruise value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeedReference {
    /// The legacy, and still default, semantics: every `*_air_speed_m_s`
    /// field on the takeoff/climb/descent/landing legs is a true airspeed
    /// flown along the path, independent of local density.
    #[default]
    TrueAirspeed,
    /// Each field is a calibrated airspeed: the true airspeed actually flown
    /// is resolved from it and the real ambient pressure and temperature at
    /// that leg's own altitude (via [`alas_atmo::airspeed`]), so it rises
    /// through a climb or descent rung instead of staying constant. A jet's
    /// widebody climb schedule and a turboprop's are both stated in knots
    /// CAS in real operating manuals; this is what lets a preset reproduce
    /// that instead of literally flying the jet schedule's true-airspeed
    /// numbers.
    CalibratedAirspeed,
}

impl Leaf for SpeedReference {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}

/// How many equal-altitude sub-rungs a [`SpeedReference::CalibratedAirspeed`]
/// climb or descent leg is split into, in *both* the MDO mission model
/// (`alas-opt::mdo::mission_model::profile`) and the native pseudospectral
/// schedule (`alas-pipeline::mission_stage::schedule`).
///
/// Defined once, here, and read by both consumers, so "resolve CAS at each
/// live altitude" means the same discretization in both paths rather than
/// two paths that each pick their own rung count and happen to agree. This
/// is a discretized approximation of constant calibrated airspeed, not the
/// continuous, exact quantity: within one sub-rung the true airspeed is
/// still held constant at the value resolved from the sub-rung's own
/// midpoint altitude, not literally recomputed at every integration step.
/// Finer than this constant reduces that discretization error further; see
/// the refinement test next to this constant's two consumers for how much.
pub const CAS_SPEED_SUBDIVISIONS: usize = 8;

/// True airspeed to fly for a configured takeoff/climb/descent/landing speed
/// `configured_speed_m_s`, under `reference`, at the given ambient state.
///
/// Centralizes the CAS-\>TAS resolution so the MDO mission model
/// (`alas-opt`) and the native pseudospectral mission (`alas-mission`/
/// `alas-pipeline`) consume the identical conversion rather than each
/// re-deriving it.
///
/// # Errors
///
/// [`alas_atmo::airspeed::AirspeedError`] when the calibrated airspeed and
/// ambient state imply an invalid or supersonic state; see
/// [`alas_atmo::airspeed::true_from_calibrated`]. Never returned in
/// [`SpeedReference::TrueAirspeed`] mode, which passes `configured_speed_m_s`
/// through unchanged.
pub fn resolve_true_airspeed_m_s(
    reference: SpeedReference,
    configured_speed_m_s: f64,
    ambient_pressure_pa: f64,
    ambient_temperature_k: f64,
) -> Result<f64, alas_atmo::airspeed::AirspeedError> {
    match reference {
        SpeedReference::TrueAirspeed => Ok(configured_speed_m_s),
        SpeedReference::CalibratedAirspeed => alas_atmo::airspeed::true_from_calibrated(
            configured_speed_m_s,
            ambient_pressure_pa,
            ambient_temperature_k,
        ),
    }
}

/// Whether `reference` is the legacy default, for
/// `#[serde(skip_serializing_if)]`: a saved file or wire format that never
/// asked for calibrated airspeed looks exactly as it did before this field
/// existed.
fn is_true_airspeed(reference: &SpeedReference) -> bool {
    *reference == SpeedReference::TrueAirspeed
}

/// Mission segment speeds, rates and altitudes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields, default)]
pub struct MissionProfileConfig {
    /// How the takeoff, climb, descent and landing legs' airspeeds below are
    /// defined. The three cruise legs are unaffected -- see
    /// [`SpeedReference`].
    #[serde(skip_serializing_if = "is_true_airspeed")]
    #[config(
        help = "Whether the takeoff/climb/descent/landing air speeds below are true airspeeds (legacy default) or calibrated airspeeds resolved against the real ambient pressure and temperature at each leg's altitude. Cruise legs are never affected by this."
    )]
    pub climb_descent_speed_reference: SpeedReference,

    /// Height above the field the takeoff segment climbs to.
    #[config(
        help = "Height above the departure field the takeoff segment climbs to before the initial climb takes over."
    )]
    pub takeoff_altitude_gain_m: f64,

    /// Airspeed flown during the takeoff climb.
    #[config(help = "Airspeed flown from liftoff to the end of the takeoff segment.")]
    pub takeoff_air_speed_m_s: f64,

    /// Rate of climb during the takeoff segment.
    #[config(help = "Rate of climb flown during the takeoff segment.")]
    pub takeoff_climb_rate_m_s: f64,

    /// Airspeed flown during the initial climb.
    #[config(
        help = "Airspeed flown from the end of the takeoff segment to the first cruise step."
    )]
    pub initial_climb_air_speed_m_s: f64,

    /// Rate of climb during the initial climb.
    #[config(
        help = "Rate of climb flown up to the first cruise step. The steepest of the three climb segments, since the aircraft is at its lightest thrust-to-weight advantage low down."
    )]
    pub initial_climb_rate_m_s: f64,

    /// Where the initial climb levels off, as a fraction of cruise altitude.
    #[config(
        help = "Altitude the initial climb levels off at, as a fraction of the cruise altitude, so raising the cruise level moves this step up with it."
    )]
    pub initial_climb_altitude_fraction: f64,

    /// Airspeed flown during the first step climb.
    #[config(help = "Airspeed flown during the first step climb.")]
    pub step_climb_1_air_speed_m_s: f64,

    /// Rate of climb during the first step climb.
    #[config(
        help = "Rate of climb during the first step climb. Much shallower than the initial climb: the aircraft is heavy, high, and trading only a little altitude for cruise efficiency."
    )]
    pub step_climb_1_rate_m_s: f64,

    /// Where the first step climb levels off, as a fraction of cruise
    /// altitude.
    #[config(
        help = "Altitude the first step climb levels off at, as a fraction of the cruise altitude."
    )]
    pub step_climb_1_altitude_fraction: f64,

    /// Airspeed flown during the second step climb.
    #[config(
        help = "Airspeed flown during the second step climb, which reaches the final cruise altitude."
    )]
    pub step_climb_2_air_speed_m_s: f64,

    /// Rate of climb during the second step climb.
    #[config(help = "Rate of climb during the second step climb, the shallowest of the profile.")]
    pub step_climb_2_rate_m_s: f64,

    /// Airspeed flown on the first cruise leg.
    #[config(help = "Airspeed flown on the first cruise leg.")]
    pub cruise_1_air_speed_m_s: f64,

    /// Relative share of the cruise remainder flown on the first cruise leg.
    #[config(
        help = "Relative share of the distance remaining after climb and descent legs flown on the first cruise leg. Active cruise fractions are normalized so the route closes exactly."
    )]
    pub cruise_1_distance_fraction: f64,

    /// Airspeed flown on the second cruise leg.
    #[config(
        help = "Airspeed flown on the second cruise leg, slower than the first as the aircraft climbs and lightens."
    )]
    pub cruise_2_air_speed_m_s: f64,

    /// Relative share of the cruise remainder flown on the second cruise leg.
    #[config(
        help = "Relative share of the distance remaining after profile climb and descent legs flown on the second cruise leg."
    )]
    pub cruise_2_distance_fraction: f64,

    /// Airspeed flown on the third cruise leg.
    #[config(help = "Airspeed flown on the third and final cruise leg.")]
    pub cruise_3_air_speed_m_s: f64,

    /// Relative share of the cruise remainder flown on the third cruise leg.
    #[config(
        help = "Relative share of the distance remaining after climb and descent legs flown on the third cruise leg. Active cruise fractions are normalized so the route closes exactly."
    )]
    pub cruise_3_distance_fraction: f64,

    /// Altitude the first descent rung levels off at.
    #[config(
        help = "Altitude the first descent rung levels off at. A rung below the arrival field's elevation is skipped rather than flown."
    )]
    pub descent_1_altitude_ft: f64,

    /// Airspeed flown on the first descent rung.
    #[config(help = "Airspeed flown down to the first descent rung's altitude.")]
    pub descent_1_air_speed_m_s: f64,

    /// Rate of descent on the first rung.
    #[config(help = "Rate of descent flown down to the first rung's altitude.")]
    pub descent_1_rate_m_s: f64,

    /// Altitude the second descent rung levels off at.
    #[config(help = "Altitude the second descent rung levels off at.")]
    pub descent_2_altitude_ft: f64,

    /// Airspeed flown on the second descent rung.
    #[config(help = "Airspeed flown down to the second descent rung's altitude.")]
    pub descent_2_air_speed_m_s: f64,

    /// Rate of descent on the second rung.
    #[config(help = "Rate of descent flown down to the second rung's altitude.")]
    pub descent_2_rate_m_s: f64,

    /// Altitude the third descent rung levels off at.
    #[config(
        help = "Altitude the third descent rung levels off at, the usual speed-limit altitude below which airspeed is restricted."
    )]
    pub descent_3_altitude_ft: f64,

    /// Airspeed flown on the third descent rung.
    #[config(help = "Airspeed flown down to the third descent rung's altitude.")]
    pub descent_3_air_speed_m_s: f64,

    /// Rate of descent on the third rung.
    #[config(help = "Rate of descent flown down to the third rung's altitude.")]
    pub descent_3_rate_m_s: f64,

    /// Altitude the fourth descent rung levels off at.
    #[config(help = "Altitude the fourth descent rung levels off at, where the approach begins.")]
    pub descent_4_altitude_ft: f64,

    /// Airspeed flown on the fourth descent rung.
    #[config(help = "Airspeed flown down to the fourth descent rung's altitude.")]
    pub descent_4_air_speed_m_s: f64,

    /// Rate of descent on the fourth rung.
    #[config(help = "Rate of descent flown down to the fourth rung's altitude.")]
    pub descent_4_rate_m_s: f64,

    /// Airspeed flown on the final descent to the field.
    #[config(
        help = "Airspeed flown on the final descent to the arrival field's elevation -- an approach speed, well below the descent rungs above it."
    )]
    pub landing_air_speed_m_s: f64,

    /// Rate of descent on the final descent to the field.
    #[config(
        help = "Rate of descent flown on the final descent to the arrival field's elevation."
    )]
    pub landing_descent_rate_m_s: f64,
}

impl Default for MissionProfileConfig {
    fn default() -> Self {
        Self {
            climb_descent_speed_reference: SpeedReference::TrueAirspeed,
            takeoff_altitude_gain_m: 3048.0,
            takeoff_air_speed_m_s: 128.6,
            takeoff_climb_rate_m_s: 10.0,
            initial_climb_air_speed_m_s: 170.0,
            initial_climb_rate_m_s: 12.0,
            initial_climb_altitude_fraction: 0.795,
            step_climb_1_air_speed_m_s: 250.0,
            step_climb_1_rate_m_s: 3.0,
            step_climb_1_altitude_fraction: 0.897,
            step_climb_2_air_speed_m_s: 248.0,
            step_climb_2_rate_m_s: 2.5,
            cruise_1_air_speed_m_s: 253.5,
            cruise_1_distance_fraction: 0.28169,
            cruise_2_air_speed_m_s: 249.4,
            cruise_2_distance_fraction: 0.33803,
            cruise_3_air_speed_m_s: 247.8,
            cruise_3_distance_fraction: 0.38028,
            descent_1_altitude_ft: 30000.0,
            descent_1_air_speed_m_s: 220.0,
            descent_1_rate_m_s: 4.5,
            descent_2_altitude_ft: 17000.0,
            descent_2_air_speed_m_s: 195.0,
            descent_2_rate_m_s: 5.0,
            descent_3_altitude_ft: 10000.0,
            descent_3_air_speed_m_s: 170.0,
            descent_3_rate_m_s: 5.0,
            descent_4_altitude_ft: 6500.0,
            descent_4_air_speed_m_s: 150.0,
            descent_4_rate_m_s: 5.0,
            landing_air_speed_m_s: 83.6,
            landing_descent_rate_m_s: 3.0,
        }
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cruise_legs_provide_relative_weights() {
        // The defaults provide the relative weights used to split the cruise
        // remainder; the mission scheduler normalizes active weights.
        let profile = MissionProfileConfig::default();
        let total = profile.cruise_1_distance_fraction
            + profile.cruise_2_distance_fraction
            + profile.cruise_3_distance_fraction;
        assert!((total - 1.0).abs() < 1e-4, "the fractions sum to {total}");
    }

    #[test]
    fn the_descent_ladder_steps_down() {
        let profile = MissionProfileConfig::default();
        let rungs = [
            profile.descent_1_altitude_ft,
            profile.descent_2_altitude_ft,
            profile.descent_3_altitude_ft,
            profile.descent_4_altitude_ft,
        ];
        for pair in rungs.windows(2) {
            assert!(pair[1] < pair[0], "the ladder does not descend: {rungs:?}");
        }
    }

    #[test]
    fn the_step_climbs_are_ordered_and_below_the_cruise_altitude() {
        let profile = MissionProfileConfig::default();
        assert!(profile.initial_climb_altitude_fraction < profile.step_climb_1_altitude_fraction);
        assert!(profile.step_climb_1_altitude_fraction < 1.0);
    }

    #[test]
    fn speeds_and_rates_take_their_units_from_their_names() {
        let schema = MissionProfileConfig::default().schema();
        assert_eq!(schema.field("takeoff_air_speed_m_s").unwrap().unit, "m/s");
        assert_eq!(schema.field("takeoff_altitude_gain_m").unwrap().unit, "m");
        assert_eq!(
            schema.field("takeoff_air_speed_m_s").unwrap().label,
            "Takeoff air speed"
        );
    }
}
