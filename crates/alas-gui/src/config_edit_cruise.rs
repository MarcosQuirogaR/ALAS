// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Keeps the mission form's cruise speeds in step with `requirements.cruise_mach`.
//!
//! The model owns the rule ([`alas_config::AlasConfig::with_cruise_mach`]): a
//! profile speed that is still the registered aircraft's automatic one
//! follows the requirement's Mach, an edited speed is kept. This module only
//! applies that rule to the stored form values after an edit, so the form
//! never shows a speed that disagrees with the Mach it was derived from.

use crate::state::AppState;

impl AppState {
    /// Write the model's follow-Mach speeds back into `config_values` when
    /// `requirements.cruise_mach` changed since the previous call.
    pub(crate) fn sync_cruise_speeds_to_requirement_mach(&mut self) {
        let Some(config) = self.typed_config() else {
            return;
        };
        let mach = config.requirements.cruise_mach;
        let previous = self.synced_cruise_mach.replace(mach).or_else(|| {
            // First edit of the session: the aircraft's own Mach is the
            // value the unedited profile speeds were derived from.
            alas_config::AlasConfig::from_value(&serde_json::json!({ "preset": config.preset }))
                .ok()
                .map(|preset| preset.requirements.cruise_mach)
        });
        let Some(previous) = previous.filter(|p| p.is_finite() && *p != mach) else {
            return;
        };
        let mut before = config.clone();
        before.requirements.cruise_mach = previous;
        if !before.cruise_profile_follows_requirement() {
            return;
        }
        let after = before.with_cruise_mach(Some(mach)).mission.profile;
        let Some(profile) = self
            .config_values
            .pointer_mut("/mission/profile")
            .and_then(|value| value.as_object_mut())
        else {
            return;
        };
        for (key, speed) in [
            ("cruise_1_air_speed_m_s", after.cruise_1_air_speed_m_s),
            ("cruise_2_air_speed_m_s", after.cruise_2_air_speed_m_s),
            ("cruise_3_air_speed_m_s", after.cruise_3_air_speed_m_s),
        ] {
            profile.insert(key.to_owned(), serde_json::json!(speed));
        }
    }
}

#[cfg(test)]
// A test builds the documents it asserts on; a failed unwrap is the assertion.
#[allow(clippy::unwrap_used)]
mod tests {
    use serde_json::json;

    use crate::state::AppState;

    fn speeds(state: &AppState) -> [f64; 3] {
        let config = state.typed_config().unwrap();
        let p = config.mission.profile;
        [
            p.cruise_1_air_speed_m_s,
            p.cruise_2_air_speed_m_s,
            p.cruise_3_air_speed_m_s,
        ]
    }

    #[test]
    fn editing_cruise_mach_moves_the_automatic_cruise_speeds() {
        let mut state = AppState::default();
        state.load_preset("A320-200");
        let first = speeds(&state);
        let mach0 = state.typed_config().unwrap().requirements.cruise_mach;
        // True airspeed over Mach is the speed of sound at the preset's
        // cruise level, whatever the Mach.
        let sound = first[0] / mach0;
        for mach in [0.70, 0.74] {
            state.config_values["requirements"]["cruise_mach"] = json!(mach);
            state.on_config_modified();
            for speed in speeds(&state) {
                assert!((speed - mach * sound).abs() < 1.0e-9, "{speed} at {mach}");
            }
        }
        assert!(speeds(&state)[0] != first[0]);
    }

    #[test]
    fn an_explicitly_edited_cruise_speed_is_kept() {
        let mut state = AppState::default();
        state.load_preset("A320-200");
        state.config_values["mission"]["profile"]["cruise_1_air_speed_m_s"] = json!(231.0);
        state.config_values["requirements"]["cruise_mach"] = json!(0.70);
        state.on_config_modified();
        assert_eq!(speeds(&state)[0], 231.0);
    }
}
