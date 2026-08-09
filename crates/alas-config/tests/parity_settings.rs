// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares the loading path against the reference implementation's: what
//! configuration a saved file turns into, and which files are refused.
//!
//! `parity_config.rs` already checks what a freshly constructed aggregate
//! holds. What is checked here is the step that turns a file into a run --
//! applying a named preset, then laying the file's own keys over it -- and it
//! is checked by comparing the whole resulting configuration rather than the
//! fields each file names. A port that applied the two in the other order, or
//! that skipped a preset's own mass-model and high-lift calibrations because
//! the graphical front end also applies them, would agree on every field the
//! file mentioned and disagree on the ones that decide the aircraft's weight
//! and its takeoff speeds.
//!
//! Compared at `exact`: nothing on this path computes anything. A value is
//! copied from a default, from a preset, or from the file.
//!
//! The rejections are compared as rejections and not as messages. Upstream
//! raises a `KeyError` carrying a sentence it composed; here the deserializer
//! composes its own, and reproducing the exact wording of a Python exception
//! would be reproducing a string rather than a behaviour. What has to agree is
//! that the same files are refused and that the message names the key that
//! caused it, which is what sends the author to the right line.

// This file is itself a test binary, so an unwrap that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::BTreeMap;

use alas_config::AlasConfig;
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;
use serde_json::Value;

/// One saved-file case: what it holds, and either the configuration it
/// produces or the message it was rejected with.
#[derive(Deserialize)]
struct Case {
    name: String,
    input: Value,
    result: Option<Value>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
    from_preset: BTreeMap<String, Value>,
}

fn fixture() -> Fixture {
    alas_testkit::load("config", "settings")
}

#[test]
fn every_saved_file_loads_into_the_configuration_the_reference_produces() {
    let fixture = fixture();
    let mut comparison = Comparison::new("alas-config::settings", Tier::Exact);

    for case in &fixture.cases {
        match (AlasConfig::from_value(&case.input), &case.result) {
            (Ok(config), Some(expected)) => {
                compare_values(&mut comparison, &case.name, &as_value(&config), expected);
            }
            (Ok(_), None) => {
                comparison.exact(&format!("{}: accepted", case.name), &true, &false);
            }
            (Err(error), Some(_)) => {
                comparison.exact(
                    &format!("{}: rejected with {error}", case.name),
                    &false,
                    &true,
                );
            }
            (Err(error), None) => {
                // Both refuse it. What has to agree beyond that is that the
                // message names the key responsible, since that is what the
                // author of the file has to be sent to.
                let key = offending_key(case.error.as_deref().unwrap_or_default());
                comparison.exact(
                    &format!("{}: the message names `{key}`", case.name),
                    &format!("{error}").contains(&key),
                    &true,
                );
            }
        }
    }
    comparison.finish();
}

#[test]
fn every_aircraft_preset_survives_the_loading_path() {
    let fixture = fixture();
    let mut comparison = Comparison::new("alas-config::settings preset loading", Tier::Exact);

    for (name, expected) in &fixture.from_preset {
        let loaded = AlasConfig::from_value(&serde_json::json!({"preset": name}));
        match loaded {
            Ok(config) => compare_values(&mut comparison, name, &as_value(&config), expected),
            Err(error) => {
                comparison.exact(&format!("{name}: {error}"), &false, &true);
            }
        }
    }
    comparison.exact(
        "preset count",
        &alas_config::presets::available().len(),
        &fixture.from_preset.len(),
    );
    comparison.finish();
}

fn as_value(config: &AlasConfig) -> Value {
    serde_json::to_value(config).expect("a configuration serializes")
}

/// The key named between single quotes in the reference's rejection message.
///
/// Its wording is Python's; what is portable about it is the identifier it
/// quotes, and that is what both implementations have to point at.
fn offending_key(message: &str) -> String {
    message
        .split('\'')
        .nth(1)
        .unwrap_or("<no quoted key>")
        .to_owned()
}

/// Compare two trees, reporting each disagreeing key by its path rather than
/// dumping both. Numbers are compared by value, since an integer upstream and
/// a float here denote the same quantity.
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
