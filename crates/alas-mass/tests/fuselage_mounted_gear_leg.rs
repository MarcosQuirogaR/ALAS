// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The FLOPS main-gear leg of a fuselage-mounted main gear comes from the
//! fuselage ground datum, not from the wing-nacelle form of equation 66.

// A failed unwrap or expectation is a failed test assertion.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{calculate_flops_mass_buildup, FlopsMassBuildup, ProductMassBuildup};
use alas_mass::stations::FALLBACK_BELLY_CLEARANCE_DIAMETER_FRACTION;
use serde_json::json;

fn buildup(name: &str) -> (FlopsMassBuildup, AlasConfig) {
    let config = AlasConfig::from_value(&json!({ "preset": name })).expect("preset configuration");
    let preset = presets::get(name).expect("registered preset");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("preset geometry");
    match calculate_flops_mass_buildup(
        &plane,
        &config.requirements,
        &config.geometry,
        &config.control_surfaces,
        Some(&config.mass_model),
        &config.landing_gear,
        &config.cabin,
    )
    .expect("pure FLOPS buildup")
    {
        ProductMassBuildup::PureFlops(build) => (*build, config),
        ProductMassBuildup::LegacyComparison(_) => panic!("production entry returned the control"),
    }
}

/// The ATR 72-600 carries its main gear on fuselage sponsons, laterally
/// inboard of its wing nacelles (4.10 m track [S ATR Aircraft Recovery
/// Manual 1-10-01 Figure 1-2] against the 8.10 m propeller-axis separation
/// [S ATR 72-600 factsheet 2020 p.22]), under a wing above the fuselage
/// crown. Its leg is the shared ground-datum strut: the fuselage belly
/// clearance the gear stations stand on.
#[test]
fn the_sponson_gear_leg_is_the_fuselage_ground_datum() {
    let (build, config) = buildup("ATR72-600");
    let sources = &build.airframe.sources;
    assert_eq!(sources.main_gear_length, "fuselage_mounted_ground_datum");
    assert_eq!(sources.nose_gear_length, "flops_equation_67");
    let strut_m = config.landing_gear.fuselage_ground_clearance_m.unwrap_or(
        FALLBACK_BELLY_CLEARANCE_DIAMETER_FRACTION * config.geometry.fuselage.diameter_m,
    );
    let inputs = &build.airframe.structure_inputs;
    assert!((inputs.main_gear_oleo_length_m - strut_m).abs() < 1e-12);
    assert!((inputs.nose_gear_oleo_length_m - 0.7 * strut_m).abs() < 1e-12);
}

/// Low-wing transports carry the main gear in the wing root, so equation 66
/// still applies to every one of them.
#[test]
fn wing_mounted_main_gear_keeps_equation_66() {
    for name in ["A320-200", "A220-300", "B787-9", "DC-10"] {
        let (build, _) = buildup(name);
        assert_eq!(
            build.airframe.sources.main_gear_length, "flops_equation_66",
            "{name}"
        );
    }
}
