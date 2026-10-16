// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One source of truth for the cruise Mach of a registered aircraft.
//!
//! The mission flies `mission.profile.cruise_{1,2,3}_air_speed_m_s`, true
//! airspeeds declared for the route's cruise level and held as a constant
//! Mach by the jet deck. A registered preset derives them from
//! `requirements.cruise_mach` ([`crate::AircraftPreset::operational_mission_defaults`]).
//! Before this module a document that changed the requirement (the settings
//! file, the interactive form) left the profile at the preset's old speed,
//! and the range and fuel pricing silently ignored the edit. A profile
//! speed the document did not change follows the requirement; a profile
//! speed the document did change is the user's explicit choice and is kept.

use crate::AlasConfig;

/// The true airspeed the preset's automatic profile flies, m/s: the
/// requirement's Mach times the speed of sound at the preset's declared cruise
/// altitude. `None` for a configuration that names no registered aircraft.
fn preset_cruise_tas_m_s(config: &AlasConfig) -> Option<f64> {
    let preset = crate::presets::get(&config.preset).ok()?;
    let altitude_m = preset.operational_mission_defaults().cruise_altitude_m;
    let sound = alas_atmo::us1976_compute_values(altitude_m, 0.0).speed_of_sound_m_s;
    Some(config.requirements.cruise_mach * sound)
}

impl AlasConfig {
    /// Whether the mission profile's three cruise speeds are still the ones a
    /// registered aircraft derives from `requirements.cruise_mach` (not edited
    /// by the user). The mission model then re-derives them at the level it
    /// actually references, so the flown Mach is the requirement's whichever
    /// level the route is flown at; an edited speed is kept as written.
    pub fn cruise_profile_follows_requirement(&self) -> bool {
        preset_cruise_tas_m_s(self).is_some_and(|tas| {
            let profile = &self.mission.profile;
            [
                profile.cruise_1_air_speed_m_s,
                profile.cruise_2_air_speed_m_s,
                profile.cruise_3_air_speed_m_s,
            ]
            .iter()
            .all(|speed| speed.to_bits() == tas.to_bits())
        })
    }
}

impl AlasConfig {
    /// This configuration flown at `mach` (a chart's printed cruise Mach)
    /// instead of its requirement's, the profile speeds following when they
    /// are the automatic ones. `None` returns an unchanged copy.
    #[must_use]
    pub fn with_cruise_mach(&self, mach: Option<f64>) -> AlasConfig {
        let mut config = self.clone();
        if let Some(mach) = mach.filter(|m| m.is_finite() && *m > 0.0) {
            let follows = self.cruise_profile_follows_requirement();
            config.requirements.cruise_mach = mach;
            if let (true, Some(tas)) = (follows, preset_cruise_tas_m_s(&config)) {
                let profile = &mut config.mission.profile;
                profile.cruise_1_air_speed_m_s = tas;
                profile.cruise_2_air_speed_m_s = tas;
                profile.cruise_3_air_speed_m_s = tas;
            }
        }
        config
    }
}

/// Re-derive `loaded`'s unedited cruise speeds from its requirement's Mach.
///
/// `preset_config` is the configuration the preset alone produced, before the
/// document was laid over it. A cruise speed of `loaded` that is bit-equal to
/// the preset's is "not edited"; when the requirement's Mach was edited it
/// becomes `Mach x a(h)` at the preset's declared cruise altitude, the same
/// product the preset used.
pub(crate) fn follow_requirement_mach(loaded: &mut AlasConfig, preset_config: &AlasConfig) {
    let mach = loaded.requirements.cruise_mach;
    if preset_config.preset.is_empty()
        || !(mach.is_finite() && mach > 0.0)
        || mach == preset_config.requirements.cruise_mach
    {
        return;
    }
    if crate::presets::get(&preset_config.preset).is_err() {
        return;
    }
    let tas_m_s = preset_cruise_tas_m_s(loaded).unwrap_or(f64::NAN);
    let (profile, original) = (&mut loaded.mission.profile, &preset_config.mission.profile);
    for (speed, preset_speed) in [
        (
            &mut profile.cruise_1_air_speed_m_s,
            original.cruise_1_air_speed_m_s,
        ),
        (
            &mut profile.cruise_2_air_speed_m_s,
            original.cruise_2_air_speed_m_s,
        ),
        (
            &mut profile.cruise_3_air_speed_m_s,
            original.cruise_3_air_speed_m_s,
        ),
    ] {
        if speed.to_bits() == preset_speed.to_bits() {
            *speed = tas_m_s;
        }
    }
}

// A test builds the documents it asserts on; a failed unwrap is the assertion.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn flown_mach(config: &AlasConfig) -> f64 {
        let preset = crate::presets::get(&config.preset).unwrap();
        let altitude_m = preset.operational_mission_defaults().cruise_altitude_m;
        config.mission.profile.cruise_1_air_speed_m_s
            / alas_atmo::us1976_compute_values(altitude_m, 0.0).speed_of_sound_m_s
    }

    /// Every registered aircraft's operational schedule is the requirement's
    /// Mach at the declared cruise level, no second number.
    #[test]
    fn every_preset_profile_is_the_requirement_mach() {
        for name in crate::presets::available() {
            let config = AlasConfig::from_value(&json!({ "preset": name })).unwrap();
            let mach = flown_mach(&config);
            assert!(
                (mach - config.requirements.cruise_mach).abs() < 1.0e-12,
                "{name}: profile {mach} vs requirement {}",
                config.requirements.cruise_mach
            );
        }
    }

    /// Editing the requirement moves the flown speed with it; editing the
    /// profile speed is respected; editing neither changes nothing.
    #[test]
    fn an_edited_requirement_moves_the_unedited_profile_speed() {
        let edited = AlasConfig::from_value(
            &json!({"preset": "A380-800", "requirements": {"cruise_mach": 0.80}}),
        )
        .unwrap();
        assert!((flown_mach(&edited) - 0.80).abs() < 1.0e-12);
        assert_eq!(
            edited.mission.profile.cruise_1_air_speed_m_s,
            edited.mission.profile.cruise_3_air_speed_m_s
        );

        let pinned = AlasConfig::from_value(&json!({
            "preset": "A380-800",
            "requirements": {"cruise_mach": 0.80},
            "mission": {"profile": {"cruise_1_air_speed_m_s": 240.0}}
        }))
        .unwrap();
        assert_eq!(pinned.mission.profile.cruise_1_air_speed_m_s, 240.0);
        assert!((flown_mach(&edited) - flown_mach(&pinned)).abs() > 1.0e-3);
        let untouched = AlasConfig::from_value(&json!({"preset": "A380-800"})).unwrap();
        assert!((flown_mach(&untouched) - 0.85).abs() < 1.0e-12);
    }

    /// A saved document carries the whole profile: re-loading it with only the
    /// requirement edited still follows the requirement.
    #[test]
    fn a_saved_full_document_with_an_edited_requirement_follows_it() {
        let base = AlasConfig::from_value(&json!({"preset": "B787-9"})).unwrap();
        let mut document = serde_json::to_value(&base).unwrap();
        document["requirements"]["cruise_mach"] = json!(0.83);
        let reloaded = AlasConfig::from_value(&document).unwrap();
        assert!((flown_mach(&reloaded) - 0.83).abs() < 1.0e-12);
    }
}
