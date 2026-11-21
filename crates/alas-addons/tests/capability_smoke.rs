// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Capability smoke test, family `addons`: the shipped script example installs,
//! is discovered from its manifest and runs inside the sandbox.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;

use alas_addons::{discover, install_examples, run_addon, AddonSettings};

#[test]
fn capability_smoke_addons_example_installs_discovers_and_runs() {
    let dir = std::env::temp_dir().join(format!("alas-addons-smoke-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    install_examples(&dir).unwrap();
    let found = discover(&dir);
    let addon = found
        .addons
        .iter()
        .find(|addon| addon.id == "raymer_wing_weight")
        .expect("the shipped example is discovered");
    let outcome = run_addon(
        addon,
        &BTreeMap::new(),
        &serde_json::json!({}),
        &AddonSettings::default(),
    )
    .unwrap();
    let mass_kg = outcome.outputs["wing_mass_kg"];
    assert!(mass_kg > 1_000.0 && mass_kg < 20_000.0, "{mass_kg}");
    let _ = std::fs::remove_dir_all(&dir);
}
