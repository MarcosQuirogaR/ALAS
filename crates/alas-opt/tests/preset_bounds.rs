// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The optimizer is the one place that derives a preset's search envelope, so a
//! front end that passes the anchored box and one that passes nothing search
//! the same one.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig, DESIGN_VARIABLE_SPECS};
use alas_opt::DesignOptimizer;

/// The envelope a front end would compute for itself, in design-variable
/// order.
fn caller_anchored_bounds(
    config: &AlasConfig,
    nominal: &alas_config::DesignVector,
) -> Vec<(f64, f64)> {
    let envelope = config.optimizer.design_space.envelope(nominal);
    DESIGN_VARIABLE_SPECS
        .iter()
        .map(|spec| {
            let variable = envelope
                .iter()
                .find(|variable| variable.name == spec.name)
                .expect("the envelope names every design variable");
            (variable.lower, variable.upper)
        })
        .collect()
}

#[test]
fn a_caller_anchored_box_and_no_box_search_the_same_envelope_for_every_preset() {
    for preset in presets::registry() {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset.name }))
            .expect("a registered preset loads");
        let optimizer = DesignOptimizer::new(config.clone());
        let nominal = preset.design_vector;

        let headless = optimizer
            .resolved_bounds(None, Some(&nominal))
            .unwrap_or_else(|error| panic!("{}: {error}", preset.name));
        let anchored = caller_anchored_bounds(&config, &nominal);
        let desktop = optimizer
            .resolved_bounds(Some(&anchored), Some(&nominal))
            .unwrap_or_else(|error| panic!("{}: {error}", preset.name));

        assert_eq!(headless, desktop, "{}", preset.name);
        // The search box contains the aircraft it adapts.
        for ((lower, upper), value) in headless.iter().zip(nominal.to_array()) {
            assert!(
                *lower <= value && value <= *upper,
                "{}: {value} outside [{lower}, {upper}]",
                preset.name
            );
        }
    }
}
