// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares the three named-preset registries against the ones the reference
//! implementation registers.
//!
//! What is checked for each preset is its whole resulting configuration, not
//! only the fields its constructor named. A preset is defined as much by what
//! it leaves alone as by what it sets: the performance registry must not
//! move the matching chart's axes, the fidelity registry must not touch an
//! assumption the user tuned, and a preset that overreached would agree
//! field for field on everything it meant to set while silently resetting
//! something else. Comparing the full configuration is what makes that
//! visible.
//!
//! Registration order is compared too. It is the order the dropdown lists
//! them in, and the first entry is what a user who does not choose ends up
//! running.
//!
//! Compared at `exact`: a preset's values are copied, not computed, so any
//! difference at all is a transposed digit, and one here produces a
//! plausible aircraft rather than a failure.//!
//! One deliberate divergence: the four vortex-lattice mesh resolutions.
//! The frozen registry meshes the optimizer loop at one chordwise panel,
//! which samples the mean camber line only at the leading and trailing
//! edges (where it is zero) so every section is a flat plate, and it
//! spends its high-fidelity budget spanwise, where the builder has already
//! converged the discretisation. Measured in an internal VLM
//! resolution-sensitivity study (2026-09-11) and cross-checked against
//! AeroSandbox on identical geometry. The frozen
//! values stay pinned by `mesh_resolution_correction`; the product values
//! are pinned by property in `fidelity_presets`' own unit tests.

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
                // Only the balanced preset's worker count diverges, and it
                // diverges the way the configuration default does: the frozen
                // literal `1` became `0`, meaning "resolve against this
                // machine", which the product L-SHADE search uses to
                // evaluate each generation's batch in parallel without
                // changing which points it evaluates or which one it
                // returns. The other three presets
                // ask for four workers explicitly and are unchanged. The
                // product value is asserted here so it is pinned on both
                // sides, and the frozen literal is then compared as it stands.
                if preset.name == "balanced" {
                    assert_eq!(
                        settings.get("workers").and_then(Value::as_i64),
                        Some(0),
                        "the balanced preset resolves its worker count against the machine"
                    );
                    settings.insert("workers".to_owned(), serde_json::json!(1));
                }
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
        let actual_value = actual.get(key).unwrap_or(&Value::Null);
        if let Some(upstream) = mesh_resolution_correction(path, key) {
            // The mesh resolutions diverge from the frozen registry on
            // purpose; both sides stay pinned. See the module doc.
            comparison.exact(
                &format!("{path}.{key}: frozen Python value"),
                expected_value,
                &upstream,
            );
            continue;
        }
        comparison.exact(&format!("{path}.{key}"), actual_value, expected_value);
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

/// The frozen fidelity-registry value for a vortex-lattice mesh field this
/// port deliberately moved, or `None` for every other field.
///
/// The registry's own unit tests pin what the product values must satisfy,
/// no preset may mesh a section as a flat plate, none may exceed the spanwise
/// resolution `validation` accepts, and the three must be ordered coarsest to
/// finest, so the product side is checked by property here rather than by a
/// second copy of the literals. What remains worth pinning is the frozen
/// value, so the divergence stays a recorded decision.
fn mesh_resolution_correction(path: &str, key: &str) -> Option<Value> {
    if !path.starts_with("fidelity.") {
        return None;
    }
    let preset = path.trim_start_matches("fidelity.");
    let frozen = match (preset, key) {
        ("draft", "chordwise_resolution") => 1,
        ("draft", "fine_spanwise_resolution") => 2,
        ("standard", "chordwise_resolution") => 1,
        ("standard", "fine_chordwise_resolution") => 8,
        ("standard", "fine_spanwise_resolution") => 2,
        ("high_fidelity", "spanwise_resolution") => 3,
        ("high_fidelity", "chordwise_resolution") => 3,
        ("high_fidelity", "fine_chordwise_resolution") => 8,
        ("high_fidelity", "fine_spanwise_resolution") => 2,
        _ => return None,
    };
    Some(serde_json::json!(frozen))
}

fn to_value<T: Serialize>(settings: &T) -> serde_json::Map<String, Value> {
    match serde_json::to_value(settings).expect("a configuration serializes") {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    }
}
