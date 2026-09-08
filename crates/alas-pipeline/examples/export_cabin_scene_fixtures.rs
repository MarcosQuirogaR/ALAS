// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez
//! Generate preset-design cabin-scene fixtures for renderer development.
//!
//! These are validation inputs evaluated at each preset's registered design
//! vector. They are not optimization results. Production runs write their
//! optimized scene through `DesignPipeline` instead.

// Standalone fixture diagnostics fail immediately when their curated inputs are invalid.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use std::{env, error::Error, fs, path::PathBuf};

use alas_config::{presets, AlasConfig};
use alas_pipeline::{export_cabin_scene, FullAnalysis, CABIN_SCENE_SCHEMA_VERSION};
use serde::Serialize;

const PRESETS: &[&str] = &[
    "AVE", "A220-300", "A320-200", "A340-300", "A380-800", "B787-9", "DC-10",
];

#[derive(Serialize)]
struct FixtureManifest {
    fixture_kind: &'static str,
    warning: &'static str,
    schema_version: &'static str,
    files: Vec<FixtureEntry>,
}

#[derive(Serialize)]
struct FixtureEntry {
    preset: &'static str,
    file: String,
    source: &'static str,
}

fn main() -> Result<(), Box<dyn Error>> {
    let output = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("outputs/cabin_scene_v2_fixtures"));
    fs::create_dir_all(&output)?;
    let mut files = Vec::new();

    for &name in PRESETS {
        let preset = presets::get(name)?;
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))?;
        let report = FullAnalysis::new(config.clone()).run(&preset.design_vector, false)?;
        let filename = format!("{}_cabin_scene_v2.json", slug(name));
        export_cabin_scene(&config, &report, &output.join(&filename))?;
        println!("{name}: {}", output.join(&filename).display());
        files.push(FixtureEntry {
            preset: name,
            file: filename,
            source: "FullAnalysis evaluated at registered preset design vector; not optimized",
        });
    }

    let manifest = FixtureManifest {
        fixture_kind: "preset-design renderer validation",
        warning: "These files are not optimization results and must not be used as optimized-run evidence.",
        schema_version: CABIN_SCENE_SCHEMA_VERSION,
        files,
    };
    serde_json::to_writer_pretty(
        fs::File::create(output.join("fixture_manifest.json"))?,
        &manifest,
    )?;
    Ok(())
}

fn slug(name: &str) -> String {
    name.to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}
