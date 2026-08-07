// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares the aircraft preset registry against the one the reference
//! implementation registers.
//!
//! Every field of every preset is compared, not the ones a reader would think
//! to check. A preset is several hundred dimensions read off a specification
//! sheet, and one of them transposed produces an aeroplane that flies and is
//! not the aeroplane on the sheet -- which is precisely the failure this
//! project exists to catch, and the only thing that catches it is comparing
//! all of them.
//!
//! Registration order is compared too, since it is the order the dropdown
//! lists them in, along with the display-name mapping the dropdown is built
//! from and the spanwise mount positions read through the accessor rather than
//! off the geometry.
//!
//! Compared at `exact`: a preset's values are copied, not computed.
//!
//! Numbers are compared as numbers rather than as JSON documents. A takeoff
//! weight is a Python `int` in this table and an `f64` here, so `275000` and
//! `275000.0` are a difference in the text and not in the data -- the same
//! distinction `parity_databases.rs` makes, for the same reason.

// This file is itself a test binary, so an unwrap that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_config::{presets, AircraftPreset};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Fixture {
    presets: Vec<Value>,
    available: Vec<String>,
    display_names: std::collections::BTreeMap<String, String>,
}

fn fixture() -> Fixture {
    alas_testkit::load("config", "aircraft_presets")
}

#[test]
fn every_aircraft_preset_matches_the_reference() {
    let fixture = fixture();
    let mut comparison = Comparison::new("alas-config::presets", Tier::Exact);

    let names: Vec<&str> = presets::available();
    let expected_names: Vec<&str> = fixture.available.iter().map(String::as_str).collect();
    comparison.exact("registration order", &names, &expected_names);
    if names != expected_names {
        comparison.finish();
        return;
    }

    for (preset, expected) in presets::registry().iter().zip(&fixture.presets) {
        compare_values(&mut comparison, preset.name, &as_value(preset), expected);
    }
    comparison.finish();
}

#[test]
fn the_dropdown_labels_match_the_reference() {
    let fixture = fixture();
    let mut comparison = Comparison::new("alas-config::presets display names", Tier::Exact);
    for (name, display_name) in presets::display_names() {
        comparison.exact(
            name,
            &display_name.to_owned(),
            fixture
                .display_names
                .get(name)
                .unwrap_or(&"absent upstream".to_owned()),
        );
    }
    comparison.exact(
        "count",
        &presets::display_names().len(),
        &fixture.display_names.len(),
    );
    comparison.finish();
}

/// One preset in the shape the fixture records it.
///
/// The two calibrations are `null` upstream when the preset does not carry
/// one, and `None` here; both serialize to the same absence, which is what
/// makes "this type uses the global default" comparable at all.
fn as_value(preset: &AircraftPreset) -> Value {
    serde_json::json!({
        "name": preset.name,
        "display_name": preset.display_name,
        "description": preset.description,
        "engine_name": preset.engine_name,
        "n_engines": preset.n_engines,
        "design_vector": preset.design_vector,
        "geometry": preset.geometry,
        "requirements": preset.requirements,
        "mass_model": preset.mass_model,
        "performance": preset.performance,
        "engine_spanwise_positions": preset.engine_spanwise_positions(),
    })
}

/// Compare two trees, reporting each disagreeing key by its path rather than
/// dumping both.
///
/// Two numbers are compared by value, so an integer upstream and a float here
/// agree when they denote the same quantity.
fn compare_values(comparison: &mut Comparison, path: &str, actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => {
            for (key, expected_value) in expected {
                let child = format!("{path}.{key}");
                match actual.get(key) {
                    Some(actual_value) => {
                        compare_values(comparison, &child, actual_value, expected_value);
                    }
                    None => {
                        comparison.exact(&child, &Value::Null, expected_value);
                    }
                }
            }
            for key in actual.keys() {
                if !expected.contains_key(key) {
                    comparison.exact(
                        &format!("{path}.{key}"),
                        &"present".to_owned(),
                        &"absent upstream".to_owned(),
                    );
                }
            }
        }
        (Value::Array(actual), Value::Array(expected)) => {
            if actual.len() != expected.len() {
                comparison.exact(&format!("{path}.len"), &actual.len(), &expected.len());
                return;
            }
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                compare_values(comparison, &format!("{path}[{index}]"), actual, expected);
            }
        }
        (Value::Number(actual), Value::Number(expected)) => {
            comparison.exact(path, &actual.as_f64(), &expected.as_f64());
        }
        (actual, expected) => {
            comparison.exact(path, actual, expected);
        }
    }
}
