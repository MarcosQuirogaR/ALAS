// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


fn active_cruise_count(profile: &Value) -> usize {
    CRUISE_FRACTION_FIELDS
        .iter()
        .rposition(|name| profile_number(profile, name) > ACTIVE_FRACTION)
        .map_or(1, |index| index + 1)
}

fn active_descent_count(profile: &Value) -> usize {
    DESCENT_ALTITUDE_FIELDS
        .iter()
        .rposition(|name| profile_number(profile, name) > ACTIVE_FRACTION)
        .map_or(0, |index| index + 1)
}

fn active_cruise_share(profile: &Value, cruise_count: usize) -> f64 {
    CRUISE_FRACTION_FIELDS[..cruise_count]
        .iter()
        .map(|name| profile_number(profile, name))
        .sum()
}

fn profile_number(profile: &Value, name: &str) -> f64 {
    profile
        .get(name)
        .and_then(Value::as_f64)
        .unwrap_or_default()
}

fn set_active_cruise_count(profile: &mut Value, count: usize) {
    let count = count.clamp(1, CRUISE_FRACTION_FIELDS.len());
    let mut fractions = CRUISE_FRACTION_FIELDS.map(|name| profile_number(profile, name).max(0.0));
    let total = fractions[..count].iter().sum::<f64>();
    if total <= ACTIVE_FRACTION {
        fractions[..count].copy_from_slice(&DEFAULT_CRUISE_FRACTIONS[..count]);
    }
    let total = fractions[..count].iter().sum::<f64>();
    for (index, name) in CRUISE_FRACTION_FIELDS.iter().enumerate() {
        let value = if index < count {
            fractions[index] / total
        } else {
            0.0
        };
        set_profile_number(profile, name, value);
    }
}

fn set_active_descent_count(profile: &mut Value, count: usize) {
    let count = count.min(DESCENT_ALTITUDE_FIELDS.len());
    for (index, name) in DESCENT_ALTITUDE_FIELDS.iter().enumerate() {
        let current = profile_number(profile, name);
        let value = if index < count {
            if current > ACTIVE_FRACTION {
                current
            } else {
                DEFAULT_DESCENT_ALTITUDES_FT[index]
            }
        } else {
            0.0
        };
        set_profile_number(profile, name, value);
    }
}

fn set_profile_number(profile: &mut Value, name: &str, value: f64) {
    if let Some(object) = profile.as_object_mut() {
        object.insert(name.to_owned(), Value::from(value));
    }
}

#[cfg(test)]
mod tests {
    use super::{
        active_cruise_count, active_descent_count, set_active_cruise_count,
        set_active_descent_count,
    };
    use alas_config::MissionProfileConfig;

    fn profile_value() -> serde_json::Value {
        serde_json::to_value(MissionProfileConfig::default()).unwrap_or_default()
    }

    #[test]
    fn active_phase_controls_preserve_a_complete_cruise_share() {
        let mut profile = profile_value();
        set_active_cruise_count(&mut profile, 2);
        assert_eq!(active_cruise_count(&profile), 2);
        let total = profile["cruise_1_distance_fraction"]
            .as_f64()
            .unwrap_or_default()
            + profile["cruise_2_distance_fraction"]
                .as_f64()
                .unwrap_or_default();
        assert!((total - 1.0).abs() < 1.0e-12);
        assert_eq!(profile["cruise_3_distance_fraction"], 0.0);
    }

    #[test]
    fn descent_phase_controls_remove_unused_rungs_from_the_profile() {
        let mut profile = profile_value();
        set_active_descent_count(&mut profile, 2);
        assert_eq!(active_descent_count(&profile), 2);
        assert_eq!(profile["descent_3_altitude_ft"], 0.0);
        assert_eq!(profile["descent_4_altitude_ft"], 0.0);
    }
}

