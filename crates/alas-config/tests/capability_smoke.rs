// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Capability smoke test, family `config`: a registered preset loads, validates
//! without blocking errors, round-trips through its JSON form, and exposes the
//! settings schema every interface page is generated from.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{AlasConfig, ConfigNode, Severity};

#[test]
fn capability_smoke_config_preset_loads_validates_and_round_trips() {
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
    assert!(config.requirements.mtow_kg > 50_000.0);

    let blocking: Vec<_> = alas_config::validate(&config)
        .into_iter()
        .filter(|issue| issue.severity == Severity::Error)
        .map(|issue| format!("{}: {}", issue.field_path, issue.message))
        .collect();
    assert!(blocking.is_empty(), "{blocking:?}");

    let saved = serde_json::to_value(&config).unwrap();
    let reloaded = AlasConfig::from_value(&saved).unwrap();
    assert_eq!(reloaded, config);

    let schema = config.schema();
    assert_eq!(schema.type_name, "AlasConfig");
    assert!(schema.fields.len() >= 15, "{}", schema.fields.len());
}
