// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Technology binding and preliminary installed-mass checks for the ATR preset.

// A failed unwrap or expectation is a failed test assertion.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, ActiveEngineModel};
use alas_mass::propulsion_mass::{turboprop_installed_mass, PropulsionMassEvidence};

#[test]
fn atr72_uses_power_not_its_zero_thrust_compatibility_field() {
    let preset = presets::get("ATR72-600").expect("ATR preset is registered");
    assert_eq!(preset.geometry.engine.thrust_kn(), 0.0);
    let ActiveEngineModel::Turboprop(spec) = preset
        .geometry
        .engine
        .active_model()
        .expect("ATR engine binding is coherent")
    else {
        panic!("ATR preset must bind a turboprop payload");
    };
    let estimate = turboprop_installed_mass(spec, preset.n_engines)
        .expect("positive shaft power yields an estimate");
    assert_eq!(
        estimate.evidence,
        PropulsionMassEvidence::SecondaryCalibration
    );
    assert!(estimate.total_kg > estimate.dry_engines_kg + estimate.propellers_kg);
    assert!((1_500.0..=1_800.0).contains(&estimate.total_kg));
    assert_eq!(estimate.relative_uncertainty, 0.25);
}
