// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Capability smoke tests, families `cli` and `filetypes`: the command line
//! parses its flags, and a configuration file is read from either of its two
//! formats with the command-line overrides applied on top.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_app::{load_config, parse_args};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn capability_smoke_cli_flags_parse_into_run_options() {
    let parsed = parse_args(&args(&[
        "--config",
        "design.json",
        "--no-optimize",
        "--no-mission",
        "--seed",
        "7",
        "--quiet",
    ]))
    .unwrap()
    .unwrap();
    assert_eq!(
        parsed.config.as_deref(),
        Some(std::path::Path::new("design.json"))
    );
    assert!(parsed.no_optimize && parsed.no_mission && parsed.quiet);
    assert_eq!(parsed.seed, Some(7));
    assert!(parse_args(&args(&["--save-config"])).is_err());
}

#[test]
fn capability_smoke_filetypes_config_loads_from_json_and_yaml() {
    let dir = std::env::temp_dir().join(format!("alas-cli-smoke-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let json = dir.join("design.json");
    let yaml = dir.join("design.yaml");
    std::fs::write(&json, r#"{"preset": "A320-200"}"#).unwrap();
    std::fs::write(&yaml, "preset: A320-200\n").unwrap();

    let mut mtow_kg = Vec::new();
    for path in [&json, &yaml] {
        let parsed = parse_args(&args(&["--config", path.to_str().unwrap(), "--no-mission"]))
            .unwrap()
            .unwrap();
        let config = load_config(&parsed).unwrap();
        assert!(!config.mission.enabled, "--no-mission overrides the file");
        mtow_kg.push(config.requirements.mtow_kg);
    }
    assert!(mtow_kg[0] > 50_000.0);
    assert_eq!(mtow_kg[0], mtow_kg[1]);
    let _ = std::fs::remove_dir_all(&dir);
}
