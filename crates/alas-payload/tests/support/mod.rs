// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the payload parity tests share: building a fixture case's aircraft the
//! way its generator did, and reading a value out of a recorded summary.
//!
//! `golden/generators/gen_payload.py` describes each case as an `ALASConfig`
//! overlay plus the name of a shipped preset, and builds the aircraft from that
//! preset's own design vector. Rebuilding it the same way here is what makes
//! the comparison one between two interiors of the same aeroplane rather than
//! between two aeroplanes.

// Each test binary compiles its own copy of this module and uses the part of
// it that its own section of the fixture needs.
#![allow(dead_code)]
// A test binary's failed unwrap or expect is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{AlasConfig, DesignVector};
use alas_geom::asb::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use serde_json::Value;

/// A case's configuration, the aircraft it builds, and the design vector it
/// was built from: the generator's `_config_and_plane`.
pub fn config_and_plane(input: &Value) -> (AlasConfig, Airplane, Option<DesignVector>) {
    let defaults: Value = alas_testkit::load("config", "defaults");
    let presets: Value = alas_testkit::load("config", "aircraft_presets");
    let mut frozen = defaults["types"]["ALASConfig"]["defaults"].clone();
    frozen.as_object_mut().unwrap().retain(|key, _| {
        matches!(
            key.as_str(),
            "geometry" | "requirements" | "cabin" | "landing_gear" | "mass_model" | "performance"
        )
    });
    let preset_name = input
        .get("preset")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let design_vector: Option<DesignVector> = if preset_name.is_empty() {
        None
    } else {
        let preset = presets["presets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|preset| preset["name"] == preset_name)
            .expect("the fixture names a frozen preset");
        for key in [
            "geometry",
            "requirements",
            "landing_gear",
            "mass_model",
            "performance",
        ] {
            if preset.get(key).is_some_and(|value| !value.is_null()) {
                merge(&mut frozen[key], &preset[key]);
            }
        }
        Some(serde_json::from_value(preset["design_vector"].clone()).unwrap())
    };
    merge(&mut frozen, input);
    let config = AlasConfig::from_value(&frozen).expect("the frozen overlay loads");
    // Payload fixtures are frozen Python translations.  Use the explicit
    // compatibility contract so the product transport-planform correction
    // cannot change the fixture's cabin frame implicitly.
    let builder = AircraftBuilder::new_reference_compatibility(Some(config.geometry.clone()));
    let plane = builder
        .build(design_vector.as_ref(), false)
        .expect("the case's aircraft builds");
    (config, plane, design_vector)
}

fn merge(target: &mut Value, overlay: &Value) {
    if let (Some(target), Some(overlay)) = (target.as_object_mut(), overlay.as_object()) {
        for (key, value) in overlay {
            merge(target.entry(key.clone()).or_insert(Value::Null), value);
        }
    } else {
        *target = overlay.clone();
    }
}

/// A number recorded under `key`.
pub fn number(record: &Value, key: &str) -> f64 {
    record
        .get(key)
        .and_then(Value::as_f64)
        .unwrap_or_else(|| panic!("the fixture has no number at {key}"))
}

/// An integer recorded under `key`.
pub fn integer(record: &Value, key: &str) -> i64 {
    record
        .get(key)
        .and_then(Value::as_i64)
        .unwrap_or_else(|| panic!("the fixture has no integer at {key}"))
}

/// A string recorded under `key`.
pub fn text(record: &Value, key: &str) -> String {
    record
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("the fixture has no string at {key}"))
        .to_owned()
}

/// A boolean recorded under `key`.
pub fn flag(record: &Value, key: &str) -> bool {
    record
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or_else(|| panic!("the fixture has no boolean at {key}"))
}
