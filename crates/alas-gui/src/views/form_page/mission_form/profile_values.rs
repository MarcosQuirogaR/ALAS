// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reading and writing the active-segment counts and numbers of a mission profile value.

use serde_json::Value;

use super::{
    ACTIVE_FRACTION, CRUISE_FRACTION_FIELDS, DEFAULT_CRUISE_FRACTIONS,
    DEFAULT_DESCENT_ALTITUDES_FT, DESCENT_ALTITUDE_FIELDS,
};

pub(super) fn active_cruise_count(profile: &Value) -> usize {
    CRUISE_FRACTION_FIELDS
        .iter()
        .rposition(|name| profile_number(profile, name) > ACTIVE_FRACTION)
        .map_or(1, |index| index + 1)
}

pub(super) fn active_descent_count(profile: &Value) -> usize {
    DESCENT_ALTITUDE_FIELDS
        .iter()
        .rposition(|name| profile_number(profile, name) > ACTIVE_FRACTION)
        .map_or(0, |index| index + 1)
}

pub(super) fn active_cruise_share(profile: &Value, cruise_count: usize) -> f64 {
    CRUISE_FRACTION_FIELDS[..cruise_count]
        .iter()
        .map(|name| profile_number(profile, name))
        .sum()
}

pub(super) fn profile_number(profile: &Value, name: &str) -> f64 {
    profile
        .get(name)
        .and_then(Value::as_f64)
        .unwrap_or_default()
}

pub(super) fn set_active_cruise_count(profile: &mut Value, count: usize) {
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

pub(super) fn set_active_descent_count(profile: &mut Value, count: usize) {
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

pub(super) fn set_profile_number(profile: &mut Value, name: &str, value: f64) {
    if let Some(object) = profile.as_object_mut() {
        object.insert(name.to_owned(), Value::from(value));
    }
}
