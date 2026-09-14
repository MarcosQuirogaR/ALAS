// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! A saved workspace file must remain a loadable configuration.

use alas_config::{AlasConfig, WORKSPACE_ENVELOPE_KEY};
use serde_json::json;

#[test]
fn the_workspace_envelope_is_ignored_by_the_configuration_loader() {
    let mut document = serde_json::to_value(AlasConfig::default()).expect("serializable");
    document["requirements"]["cruise_mach"] = json!(0.79);
    document[WORKSPACE_ENVELOPE_KEY] = json!({
        "version": 1,
        "mode": "sandbox",
        "layout": { "parameter_panel_width": 320.0 }
    });

    let loaded = AlasConfig::from_value(&document).expect("envelope must not reject the file");
    assert_eq!(loaded.requirements.cruise_mach, 0.79);
}

#[test]
fn a_configuration_without_an_envelope_loads_exactly_as_before() {
    let document = serde_json::to_value(AlasConfig::default()).expect("serializable");
    let loaded = AlasConfig::from_value(&document).expect("plain configuration");
    assert_eq!(loaded, AlasConfig::default());
}

#[test]
fn unknown_top_level_keys_other_than_the_envelope_are_still_rejected() {
    let mut document = serde_json::to_value(AlasConfig::default()).expect("serializable");
    document["not_a_group"] = json!(1);
    assert!(AlasConfig::from_value(&document).is_err());
}
