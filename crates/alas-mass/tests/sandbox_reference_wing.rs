// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A baseline sandbox with no registered preset is its own reference wing.
//!
//! The GUI opens the Sandbox from a loaded preset case with the preset identity cleared and
//! the registered design vector pinned as the drawn design. A frozen reference aircraft built
//! with the canonical default design vector in that case fails on the DC-10 (geometry
//! stations: `StructuralSizing`) and on the turboprops (reference mass buildup:
//! `MassCoordinates`), and refers every other aircraft to a wing it is not. The drawn design is
//! the reference of a baseline sandbox, so the cleared aircraft must reconcile against the same
//! complete wing the registered aircraft does.
//!
//! The sized primary box is not compared: its fuel relief reads the preset's declared tank
//! layout, which the cleared identity does not carry (a separate registry lookup).

// A test asserts on values it built from registered presets, so a failed unwrap there is the
// assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig, DesignMode};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{
    run_mass_analysis_with_model_checked_product_with_gear, MassCoordinateModel,
};
use alas_mass::wing_reconciliation::WingReconciliation;
use alas_mass::wing_reconciliation::{reconcile_with_design_box, size_design_wing_box};

/// The reconciliation of `name` as a baseline sandbox with the preset identity kept or cleared.
fn reconcile_of(name: &str, clear_preset: bool) -> Result<WingReconciliation, String> {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
        .map_err(|error| error.to_string())?;
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    if clear_preset {
        config.preset.clear();
    }
    let design = presets::get(name)
        .map_err(|error| error.to_string())?
        .design_vector;
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), true)
        .map_err(|error| error.to_string())?;
    let mass_model = config.analysis_mass_model(config.requirements.mtow_kg);
    let (masses, _, _) = run_mass_analysis_with_model_checked_product_with_gear(
        &plane,
        &config.requirements,
        &config.geometry,
        &config.cabin,
        &config.control_surfaces,
        Some(&mass_model),
        None,
        MassCoordinateModel::StructuralWingbox(&config.structures),
        &config.landing_gear,
    )
    .map_err(|error| format!("{error:?}"))?;
    let design_box =
        size_design_wing_box(&config, &design, &plane).map_err(|error| format!("{error:?}"))?;
    reconcile_with_design_box(&config, &design, &plane, None, masses.wing, design_box)
        .map_err(|error| format!("{error:?}"))
}

fn assert_close(name: &str, what: &str, drawn: f64, registered: f64) {
    assert!(
        (drawn - registered).abs() <= 1e-9 * registered.abs().max(1.0),
        "{name}: the cleared sandbox's {what} {drawn} differs from the registered aircraft's {registered}"
    );
}

#[test]
fn a_sandbox_opened_from_a_preset_reconciles_against_the_drawn_aircraft() {
    // The DC-10 (geometry stations), the two turboprops (reference mass buildup) and two jets
    // the default design vector could still build.
    for name in ["DC-10", "C295", "ATR72-600", "A320-200", "B747-400"] {
        let registered =
            reconcile_of(name, false).unwrap_or_else(|error| panic!("{name}: {error}"));
        let drawn = reconcile_of(name, true)
            .unwrap_or_else(|error| panic!("{name} with its preset cleared: {error}"));
        let (registered_reference, drawn_reference) = (
            registered.reference.as_ref().expect("registered reference"),
            drawn.reference.as_ref().expect("cleared reference"),
        );
        assert_close(
            name,
            "reference wing mass",
            drawn_reference.total_mass_kg,
            registered_reference.total_mass_kg,
        );
        for axis in 0..3 {
            assert_close(
                name,
                "reference wing centroid",
                drawn_reference.centroid_m[axis],
                registered_reference.centroid_m[axis],
            );
            assert_close(
                name,
                "reconciled wing centroid",
                drawn.feedback.centroid_m[axis],
                registered.feedback.centroid_m[axis],
            );
        }
        assert_close(
            name,
            "reconciled wing mass",
            drawn.feedback.total_wing_mass_kg,
            registered.feedback.total_wing_mass_kg,
        );
    }
}
