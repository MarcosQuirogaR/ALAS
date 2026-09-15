// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `alas-mission::profile` against `golden/mission/profile.json`.
//!
//! Each case names the inputs the reference used: a preset (empty is the
//! default configuration) and two airport codes, and the parity test rebuilds
//! those with `AlasConfig::from_value({"preset": name})` and
//! `airports::get(icao)`, supplying the independently recorded baseline profile
//! from `golden/config/defaults.json`. It then compares the
//! request `build_mission_request` assembles against the recorded one by walking
//! the two documents in parallel: strings (the mission tag) at `exact`, numbers
//! (every altitude, elevation, distance and profile speed) at `closed`. A
//! missing or extra key is its own finding, so a request that dropped a field
//! or grew one fails by name rather than by a value that never got compared.

// This file is itself a test binary, so an unwrap or expect that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_config::airports;
use alas_config::AlasConfig;
use alas_mission::build_mission_request;
use alas_testkit::{load_json, Comparison, Tier};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    preset: String,
    origin: String,
    dest: String,
    route_distance_m: f64,
    request: Value,
}

// The test builds inputs it recorded and asserts on them, so a failed expect is
// a broken fixture rather than a library invariant.
#[allow(clippy::expect_used)]
fn config_for(preset: &str) -> AlasConfig {
    if preset.is_empty() {
        return AlasConfig::default();
    }
    AlasConfig::from_value(&json!({ "preset": preset })).expect("the preset overlay loads")
}

/// Walk two request documents in parallel, routing each leaf to its tier.
fn compare(
    path: &str,
    actual: &Value,
    expected: &Value,
    numbers: &mut Comparison,
    strings: &mut Comparison,
) {
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) => {
            for (key, expected_value) in e {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                match a.get(key) {
                    Some(actual_value) => {
                        compare(&child, actual_value, expected_value, numbers, strings)
                    }
                    None => {
                        strings.exact(&child, &"<absent>".to_owned(), &"<present>".to_owned());
                    }
                }
            }
            for key in a.keys() {
                if !e.contains_key(key) {
                    let child = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    strings.exact(&child, &"<present>".to_owned(), &"<absent>".to_owned());
                }
            }
        }
        (Value::String(a), Value::String(e)) => {
            strings.exact(path, a, e);
        }
        _ => {
            // Numbers may be JSON ints on one side (an airport elevation) and
            // floats on the other, so both are read through `as_f64`.
            let a = actual.as_f64();
            let e = expected.as_f64();
            match (a, e) {
                (Some(a), Some(e)) => {
                    numbers.scalar(path, a, e);
                }
                _ => {
                    strings.exact(path, &actual.to_string(), &expected.to_string());
                }
            }
        }
    }
}

#[test]
fn mission_request_matches_the_reference() {
    let fixture: Fixture =
        serde_json::from_value(load_json("mission", "profile")).expect("fixture shape");

    let mut numbers = Comparison::new("mission request numbers", Tier::Closed);
    let mut strings = Comparison::new("mission request strings", Tier::Exact);

    for case in &fixture.cases {
        let mut config = config_for(&case.preset);
        // The Python request builder received the baseline explicit TAS profile.
        // Product presets derive their schedule from Mach; freeze the independent
        // reference input so that this test measures the builder, not preset drift.
        config.mission.profile = serde_json::from_value(
            load_json("config", "defaults")["types"]["ALASConfig"]["defaults"]["mission"]
                ["profile"]
                .clone(),
        )
        .expect("reference mission profile input");
        let origin = airports::get(&case.origin).expect("origin airport");
        let dest = airports::get(&case.dest).expect("destination airport");

        let request = build_mission_request(&config, origin, dest, case.route_distance_m);
        let actual = serde_json::to_value(&request).expect("the request serializes");

        let label = format!("{}:{}", case.preset, request.mission_tag);
        compare(&label, &actual, &case.request, &mut numbers, &mut strings);
    }

    numbers.finish();
    strings.finish();
}
