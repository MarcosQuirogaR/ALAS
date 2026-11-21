// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The desktop interface and the command line analyse the same case for the
//! same preset: the configuration the interface holds after loading a preset
//! equals the one the command-line loader builds from `{"preset": name}`,
//! apart from what the interface sets for interactive use (the optimizer
//! solver budget and seed), the machine-local tool paths and the seven
//! external-tool switches.

// This file is itself a test binary, so an unwrap or expect that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_gui::state::AppState;
use serde_json::{json, Value};

/// JSON pointers the two front ends are allowed to differ in.
const EXEMPT: &[&str] = &[
    "/optimizer/solver",
    "/mses/enabled",
    "/mses/mses_dir",
    "/mses/osmap_path",
    "/downstream/openvsp",
    "/downstream/vspaero",
    "/downstream/avl",
    "/downstream/flowunsteady",
    "/structures/run_nastran",
    "/structures/run_patran_export",
    "/structures/nastran_exe_path",
    "/structures/nastran_solver_path",
    "/structures/nastran_memory_mb",
    "/structures/nastran95_dir_path",
    "/structures/nastran95_runtime_path",
    "/structures/nastran95_rf_stage_path",
    "/structures/nastran95_open_core_words",
    "/structures/patran_exe_path",
    "/mission/navdata_dir",
    "/mission/routes_dir",
    "/mission/texture_path",
];

fn masked(config: &alas_config::AlasConfig) -> Value {
    let mut value = serde_json::to_value(config).unwrap();
    for pointer in EXEMPT {
        if let Some((parent, key)) = pointer.rsplit_once('/') {
            if let Some(map) = value.pointer_mut(parent).and_then(Value::as_object_mut) {
                map.remove(key);
            }
        }
    }
    value
}

fn differing_leaves(path: &str, a: &Value, b: &Value, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: std::collections::BTreeSet<_> = x.keys().chain(y.keys()).collect();
            for key in keys {
                let child = format!("{path}/{key}");
                differing_leaves(
                    &child,
                    x.get(key).unwrap_or(&Value::Null),
                    y.get(key).unwrap_or(&Value::Null),
                    out,
                );
            }
        }
        _ if a == b => {}
        _ => out.push(format!("{path}: {a} | {b}")),
    }
}

#[test]
fn every_preset_loads_the_same_case_in_the_interface_and_on_the_command_line() {
    let mut failures = Vec::new();
    for name in alas_config::presets::available() {
        let cli = alas_app::config_load::load_config_value(&json!({ "preset": name })).unwrap();
        let mut state = AppState::default();
        state.load_preset(name);
        let gui = state
            .typed_config()
            .expect("the interface holds a valid configuration");
        let mut leaves = Vec::new();
        differing_leaves("", &masked(&gui), &masked(&cli), &mut leaves);
        if !leaves.is_empty() {
            failures.push(format!(
                "{name}: {} differing\n  {}",
                leaves.len(),
                leaves.join("\n  ")
            ));
        }
    }
    assert!(failures.is_empty(), "GUI | CLI\n{}", failures.join("\n"));
}

#[test]
fn an_explicit_profile_in_a_document_is_kept_and_the_preset_default_is_route_derived() {
    let stored = alas_config::AlasConfig::from_value(&json!({ "preset": "A320-200" }))
        .unwrap()
        .mission
        .profile;
    let derived = alas_app::config_load::load_config_value(&json!({ "preset": "A320-200" }))
        .unwrap()
        .mission
        .profile;
    assert_ne!(stored, derived, "the preset default is now route-derived");
    let explicit = alas_app::config_load::load_config_value(&json!({
        "preset": "A320-200",
        "mission": { "profile": serde_json::to_value(&stored).unwrap() },
    }))
    .unwrap()
    .mission
    .profile;
    assert_eq!(explicit, stored, "an explicit profile is honoured");
}
