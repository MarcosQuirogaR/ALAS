// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The takeoff, climb, descent and landing schedule of a clean-sheet
//! turboprop.
//!
//! `MissionProfileConfig::default()` is the long-range widebody worked
//! example: 128.6 to 250 m/s *true* airspeeds. A turboprop cruising at
//! M 0.44 cannot fly them (the mission deck refuses them with a typed climb
//! energy deficit, which is what rejected 558 of 558 candidates of a
//! 70-seat clean-sheet turboprop brief as
//! `dispatch_model_failed`). A preset that is a turboprop carries its own
//! schedule; a brief with no preset gets this one, derived from the brief
//! instead of from a registered aircraft:
//!
//! * **Takeoff speed** `V2 = 1.13 V_SR`, the CS 25.107(b) / 14 CFR 25.107(b)
//!   minimum for a two-engine propeller aeroplane, with `V_SR` the
//!   sea-level standard-day stall speed at the brief's MTOW, the class wing
//!   area of the brief ([`crate::AlasConfig::class_wing`]) and
//!   `performance.cl_max_to`. Exactly the form of
//!   `atr72_600_takeoff_speed_m_s`, with the class wing in place of the
//!   registered reference area.
//! * **Approach speed** `V_REF = 1.23 V_SR0` at the landing mass
//!   (`mlw_fraction_mtow x MTOW`) and `performance.cl_max_land`: the CS 25.125(b)(2)
//!   / 14 CFR 25.125(b)(2)(i) minimum landing reference speed.
//! * **En-route climb speed** 1.26 `V2`, calibrated: the ratio of the ATR 72-600
//!   factsheet's optimum climb speed (170 KCAS) to the V2 of the same
//!   aircraft (135 KCAS at its own mass, area and lift limit). One data
//!   point; the only retained turboprop climb statement.
//! * **Cruise** at the requirement's Mach times the speed of sound at the
//!   brief's cruise altitude, flown as one assigned level (a regional sector
//!   does not step-climb).
//! * **Descent ladder, vertical rates, takeoff-segment top**: the
//!   *unsourced* representative values of the ATR 72-600 schedule
//!   (`apply_atr72_600_speed_schedule`), i.e. 250 KIAS-class limit with
//!   margin, 1,500 ft takeoff-segment top, 3-degree-like approach rate. They
//!   are assumptions of operating practice, not measurements, and are not
//!   calibrated to any block-fuel or block-time figure.
//!
//! Every value is an operating-practice assumption of the model, not a
//! validated schedule; an explicit `mission.profile` key in the brief is laid
//! over it afterwards and always wins.

use crate::{AlasConfig, MissionProfileConfig, SpeedReference};

/// International knot, m/s.
const KNOT_M_S: f64 = 1852.0 / 3600.0;
/// Feet per minute, m/s.
const FT_MIN_M_S: f64 = 0.3048 / 60.0;
/// ISA sea-level density, kg/m^3.
const ISA_SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;
/// CS 25.107(b): V2 not below 1.13 V_SR for a two-engine propeller aeroplane.
const V2_OVER_VSR: f64 = 1.13;
/// CS 25.125(b)(2): landing reference speed not below 1.23 V_SR0.
const VREF_OVER_VSR0: f64 = 1.23;
/// ATR 72-600 factsheet optimum climb speed over its derived V2 (170 / 135).
const CLIMB_OVER_V2: f64 = 1.26;
/// Takeoff-segment climb gradient over true airspeed, the ATR 72-600
/// factsheet's sea-level rate of climb (1,355 ft/min) at its derived V2.
const TAKEOFF_CLIMB_GRADIENT: f64 = 0.10;

/// Stall speed, m/s, at `mass_kg`, `area_m2` and `cl_max`, sea level ISA.
fn stall_speed_m_s(mass_kg: f64, gravity_m_s2: f64, area_m2: f64, cl_max: f64) -> f64 {
    (2.0 * mass_kg * gravity_m_s2 / (ISA_SEA_LEVEL_DENSITY_KG_M3 * area_m2 * cl_max)).sqrt()
}

/// The mass, area and lift basis the class schedule is derived from.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TurbopropScheduleBasis {
    /// Takeoff mass the V2 is read at, kg.
    pub takeoff_mass_kg: f64,
    /// Landing mass the V_REF is read at, kg.
    pub landing_mass_kg: f64,
    /// Gravitational acceleration, m/s^2.
    pub gravity_m_s2: f64,
    /// Reference wing area, m^2.
    pub wing_area_m2: f64,
    /// Takeoff maximum lift coefficient.
    pub cl_max_takeoff: f64,
    /// Landing maximum lift coefficient.
    pub cl_max_landing: f64,
}

/// Overwrite the takeoff, climb, descent and landing legs of `profile` with
/// the turboprop class schedule of this module's documentation, flown as one
/// assigned cruise level. The cruise speeds are left as they are. `None`,
/// leaving `profile` untouched, when the basis is not physical.
pub(crate) fn apply_class_schedule(
    profile: &mut MissionProfileConfig,
    basis: TurbopropScheduleBasis,
) -> Option<()> {
    let valid = |x: f64| x.is_finite() && x > 0.0;
    let b = basis;
    if ![
        b.takeoff_mass_kg,
        b.landing_mass_kg,
        b.gravity_m_s2,
        b.wing_area_m2,
        b.cl_max_takeoff,
        b.cl_max_landing,
    ]
    .into_iter()
    .all(valid)
    {
        return None;
    }
    let v2 = V2_OVER_VSR
        * stall_speed_m_s(
            b.takeoff_mass_kg,
            b.gravity_m_s2,
            b.wing_area_m2,
            b.cl_max_takeoff,
        );
    let v_ref = VREF_OVER_VSR0
        * stall_speed_m_s(
            b.landing_mass_kg,
            b.gravity_m_s2,
            b.wing_area_m2,
            b.cl_max_landing,
        );
    let climb_cas = CLIMB_OVER_V2 * v2;
    profile.climb_descent_speed_reference = SpeedReference::CalibratedAirspeed;
    // One assigned cruise level; see `atr72_600_schedule_fix`. 0.999, not 1.0,
    // because the mission model rejects a level fraction at or above 1.
    profile.initial_climb_altitude_fraction = 0.999;
    profile.cruise_2_distance_fraction = 0.0;
    profile.cruise_3_distance_fraction = 0.0;
    profile.takeoff_altitude_gain_m = 1_500.0 * 0.3048;
    profile.takeoff_air_speed_m_s = v2;
    profile.takeoff_climb_rate_m_s = TAKEOFF_CLIMB_GRADIENT * v2;
    profile.initial_climb_air_speed_m_s = climb_cas;
    profile.initial_climb_rate_m_s = 1_000.0 * FT_MIN_M_S;
    profile.step_climb_1_air_speed_m_s = climb_cas;
    profile.step_climb_1_rate_m_s = 600.0 * FT_MIN_M_S;
    profile.step_climb_2_air_speed_m_s = climb_cas;
    profile.step_climb_2_rate_m_s = 600.0 * FT_MIN_M_S;
    profile.descent_1_altitude_ft = 10_000.0;
    profile.descent_1_air_speed_m_s = 220.0 * KNOT_M_S;
    profile.descent_1_rate_m_s = 1_500.0 * FT_MIN_M_S;
    profile.descent_2_altitude_ft = 6_000.0;
    profile.descent_2_air_speed_m_s = 200.0 * KNOT_M_S;
    profile.descent_2_rate_m_s = 1_200.0 * FT_MIN_M_S;
    profile.descent_3_altitude_ft = 3_000.0;
    profile.descent_3_air_speed_m_s = 170.0 * KNOT_M_S;
    profile.descent_3_rate_m_s = 1_000.0 * FT_MIN_M_S;
    profile.descent_4_altitude_ft = 1_500.0;
    profile.descent_4_air_speed_m_s = (140.0 * KNOT_M_S).max(v_ref);
    profile.descent_4_rate_m_s = 800.0 * FT_MIN_M_S;
    profile.landing_air_speed_m_s = v_ref;
    profile.landing_descent_rate_m_s = 600.0 * FT_MIN_M_S;
    Some(())
}

/// The schedule for `config`, a clean-sheet brief whose engine is a
/// turboprop; `None` when the brief cannot be sized or is not physical.
pub(super) fn profile(config: &AlasConfig) -> Option<MissionProfileConfig> {
    let wing = config.class_wing()?;
    let r = &config.requirements;
    let p = &config.performance;
    let cruise_tas = r.cruise_mach
        * alas_atmo::us1976_compute_values(r.cruise_altitude_m, 0.0).speed_of_sound_m_s;
    if !(cruise_tas.is_finite() && cruise_tas > 0.0) {
        return None;
    }
    let mut profile = MissionProfileConfig::default();
    apply_class_schedule(
        &mut profile,
        TurbopropScheduleBasis {
            takeoff_mass_kg: r.mtow_kg,
            landing_mass_kg: r.mtow_kg * config.mass_model.mlw_fraction_mtow,
            gravity_m_s2: r.gravity_m_s2,
            wing_area_m2: wing.area_m2,
            cl_max_takeoff: p.cl_max_to,
            cl_max_landing: p.cl_max_land,
        },
    )?;
    profile.cruise_1_air_speed_m_s = cruise_tas;
    profile.cruise_2_air_speed_m_s = cruise_tas;
    profile.cruise_3_air_speed_m_s = cruise_tas;
    Some(profile)
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn brief() -> AlasConfig {
        AlasConfig::from_value(&serde_json::json!({
            "requirements": {
                "mtow_kg": 23000.0, "num_passengers": 70,
                "cruise_mach": 0.44, "cruise_altitude_m": 5180.0
            },
            "geometry": {
                "fuselage": { "diameter_m": 2.865 },
                "engine": { "engine_name": "PW127M" }
            }
        }))
        .unwrap_or_else(|error| panic!("the turboprop brief loads: {error}"))
    }

    #[test]
    fn the_brief_flies_a_turboprop_class_schedule() {
        let config = brief();
        let profile = &config.mission.profile;
        assert_eq!(
            profile.climb_descent_speed_reference,
            SpeedReference::CalibratedAirspeed
        );
        // V2 of a 23 t, ~60 m^2 aeroplane sits in the 60 to 80 m/s class of
        // the registered ATR (about 135 KCAS = 69 m/s), not at 128.6 m/s.
        assert!(
            (55.0..85.0).contains(&profile.takeoff_air_speed_m_s),
            "{}",
            profile.takeoff_air_speed_m_s
        );
        assert!(profile.landing_air_speed_m_s < profile.takeoff_air_speed_m_s);
        // A single assigned level, flown at the requirement's Mach.
        assert_eq!(profile.cruise_2_distance_fraction, 0.0);
        let a = alas_atmo::us1976_compute_values(5180.0, 0.0).speed_of_sound_m_s;
        assert!((profile.cruise_1_air_speed_m_s - 0.44 * a).abs() < 1e-9);
    }

    #[test]
    fn an_explicit_profile_key_wins_and_a_jet_brief_is_untouched() {
        let mut document = serde_json::json!({
            "requirements": { "mtow_kg": 23000.0, "num_passengers": 70,
                "cruise_mach": 0.44, "cruise_altitude_m": 5180.0 },
            "geometry": { "fuselage": { "diameter_m": 2.865 },
                "engine": { "engine_name": "PW127M" } },
            "mission": { "profile": { "takeoff_air_speed_m_s": 77.0 } }
        });
        let pinned = AlasConfig::from_value(&document)
            .unwrap_or_else(|error| panic!("the brief loads: {error}"));
        assert_eq!(pinned.mission.profile.takeoff_air_speed_m_s, 77.0);
        document["geometry"]["engine"]["engine_name"] = serde_json::json!("LEAP-1A");
        document["requirements"]["cruise_mach"] = serde_json::json!(0.78);
        document["requirements"]["cruise_altitude_m"] = serde_json::json!(11277.6);
        document["mission"] = serde_json::json!({});
        let jet = AlasConfig::from_value(&document)
            .unwrap_or_else(|error| panic!("the jet brief loads: {error}"));
        // The jet does not take this turboprop schedule; it takes the jet
        // default rule (CAS/Mach climb to the brief's cruise Mach, cruise at
        // that Mach at its level), the registered presets' rule, exactly.
        let mut expected = MissionProfileConfig::default();
        let tas_m_s = 0.78
            * alas_atmo::us1976_try_compute_values(11277.6, 0.0)
                .unwrap()
                .speed_of_sound_m_s;
        expected.cruise_1_air_speed_m_s = tas_m_s;
        expected.cruise_2_air_speed_m_s = tas_m_s;
        expected.cruise_3_air_speed_m_s = tas_m_s;
        assert!(crate::presets::apply_default_rule(
            &mut expected,
            0.78,
            jet.requirements.design_cruise_speed_eas_m_s(),
            11277.6
        ));
        assert_eq!(jet.mission.profile, expected);
        assert_eq!(jet.mission.profile.climb_mach, 0.78);
        assert_ne!(
            jet.mission.profile.takeoff_air_speed_m_s,
            pinned.mission.profile.takeoff_air_speed_m_s
        );
    }
}
