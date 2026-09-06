// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares the three named-preset registries against the ones the reference
//! implementation registers.
//!
//! What is checked for each preset is its whole resulting configuration, not
//! only the fields its constructor named. A preset is defined as much by what
//! it leaves alone as by what it sets -- the performance registry must not
//! move the matching chart's axes, the fidelity registry must not touch an
//! assumption the user tuned -- and a preset that overreached would agree
//! field for field on everything it meant to set while silently resetting
//! something else. Comparing the full configuration is what makes that
//! visible.
//!
//! Registration order is compared too. It is the order the dropdown lists
//! them in, and the first entry is what a user who does not choose ends up
//! running.
//!
//! Compared at `exact`: a preset's values are copied, not computed, so any
//! difference at all is a transposed digit -- and one here produces a
//! plausible aircraft rather than a failure.

// This file is itself a test binary, so an unwrap that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_config::{fidelity_presets, performance_presets, solver_presets};
use alas_testkit::{Comparison, Tier};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One preset as the fixture records it. `settings` is the whole
/// configuration, whatever the preset's own dataclass called the field
/// holding it.
#[derive(Deserialize)]
struct PresetFixture {
    name: String,
    display_name: String,
    description: String,
    settings: Value,
}

#[derive(Deserialize)]
struct Fixture {
    solver: Vec<PresetFixture>,
    performance: Vec<PresetFixture>,
    fidelity: Vec<PresetFixture>,
}

fn fixture() -> Fixture {
    alas_testkit::load("config", "presets")
}

#[test]
fn every_solver_preset_matches_the_reference() {
    let mut comparison = Comparison::new("alas-config::solver_presets", Tier::Exact);
    compare_registry(
        &mut comparison,
        "solver",
        &fixture().solver,
        &solver_presets::registry()
            .iter()
            .map(|preset| {
                let mut settings = to_value(&preset.settings);
                // The product search-method selector has no Python field.
                // Solver presets preserve the selected method while changing
                // only the historical effort/budget settings.
                settings.remove("method");
                assert_eq!(
                    settings.remove("enforce_physical_constraints"),
                    Some(Value::Bool(false))
                );
                (
                    preset.name,
                    preset.display_name,
                    preset.description,
                    settings,
                )
            })
            .collect::<Vec<_>>(),
    );
    comparison.finish();
}

#[test]
fn every_performance_preset_matches_the_reference() {
    let mut comparison = Comparison::new("alas-config::performance_presets", Tier::Exact);
    compare_registry(
        &mut comparison,
        "performance",
        &fixture().performance,
        &performance_presets::registry()
            .iter()
            .map(|preset| {
                (
                    preset.name,
                    preset.display_name,
                    preset.description,
                    to_value(&preset.settings),
                )
            })
            .collect::<Vec<_>>(),
    );
    comparison.finish();
}

#[test]
fn every_fidelity_preset_matches_the_reference() {
    let mut comparison = Comparison::new("alas-config::fidelity_presets", Tier::Exact);
    compare_registry(
        &mut comparison,
        "fidelity",
        &fixture().fidelity,
        &fidelity_presets::registry()
            .iter()
            .map(|preset| {
                (
                    preset.name,
                    preset.display_name,
                    preset.description,
                    to_value(&preset.analysis),
                )
            })
            .collect::<Vec<_>>(),
    );
    comparison.finish();
}

#[test]
fn the_name_lists_keep_the_registration_order_the_dropdowns_read() {
    let fixture = fixture();
    let mut comparison = Comparison::new("alas-config preset ordering", Tier::Exact);

    let expected = |presets: &[PresetFixture]| {
        presets
            .iter()
            .map(|preset| preset.name.clone())
            .collect::<Vec<_>>()
    };
    let actual =
        |names: Vec<&'static str>| names.into_iter().map(str::to_owned).collect::<Vec<_>>();

    comparison.exact(
        "solver_presets::available",
        &actual(solver_presets::available()),
        &expected(&fixture.solver),
    );
    comparison.exact(
        "performance_presets::available",
        &actual(performance_presets::available()),
        &expected(&fixture.performance),
    );
    comparison.exact(
        "fidelity_presets::available",
        &actual(fidelity_presets::available()),
        &expected(&fixture.fidelity),
    );
    comparison.finish();
}

type Registered = (
    &'static str,
    &'static str,
    &'static str,
    serde_json::Map<String, Value>,
);

fn compare_registry(
    comparison: &mut Comparison,
    kind: &str,
    expected: &[PresetFixture],
    actual: &[Registered],
) {
    let names: Vec<&str> = actual.iter().map(|&(name, ..)| name).collect();
    let expected_names: Vec<&str> = expected.iter().map(|preset| preset.name.as_str()).collect();
    comparison.exact(
        &format!("{kind}: registration order"),
        &names,
        &expected_names,
    );
    if names != expected_names {
        return;
    }

    for (registered, expected) in actual.iter().zip(expected) {
        let (name, display_name, description, settings) = registered;
        comparison.exact(
            &format!("{kind}.{name}.display_name"),
            &(*display_name).to_owned(),
            &expected.display_name,
        );
        comparison.exact(
            &format!("{kind}.{name}.description"),
            &(*description).to_owned(),
            &expected.description,
        );
        compare_settings(
            comparison,
            &format!("{kind}.{name}"),
            settings,
            &expected.settings,
        );
    }
}

/// Compare one preset's whole configuration, reporting each disagreeing field
/// by name rather than dumping both structs.
fn compare_settings(
    comparison: &mut Comparison,
    path: &str,
    actual: &serde_json::Map<String, Value>,
    expected: &Value,
) {
    let Some(expected) = expected.as_object() else {
        comparison.exact(path, &"an object".to_owned(), &"not an object".to_owned());
        return;
    };

    for (key, expected_value) in expected {
        comparison.exact(
            &format!("{path}.{key}"),
            actual.get(key).unwrap_or(&Value::Null),
            expected_value,
        );
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

fn to_value<T: Serialize>(settings: &T) -> serde_json::Map<String, Value> {
    match serde_json::to_value(settings).expect("a configuration serializes") {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    }
}
