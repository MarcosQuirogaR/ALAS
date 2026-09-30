// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The regional turboprop cabin method on registered presets: selection,
//! group closure against the Torenbeek relation, and no change to any jet.

// A failed unwrap or expectation is a failed test assertion.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig, CabinEquipmentMethod};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{calculate_flops_mass_buildup, FlopsMassBuildup, ProductMassBuildup};
use serde_json::json;

fn buildup(name: &str) -> FlopsMassBuildup {
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
        ProductMassBuildup::PureFlops(build) => *build,
        ProductMassBuildup::LegacyComparison(_) => panic!("production entry returned the control"),
    }
}

fn oew_kg(build: &FlopsMassBuildup) -> f64 {
    build.masses.as_pairs()[..8]
        .iter()
        .map(|(_, mass)| mass)
        .sum()
}

#[test]
fn the_atr_takes_the_regional_method_and_its_group_closes_on_the_torenbeek_relation() {
    let build = buildup("ATR72-600");
    assert_eq!(
        build.inputs.cabin_equipment_method,
        CabinEquipmentMethod::RegionalTurbopropV1
    );
    let systems = &build.systems_and_operating_items.systems;
    let mtow = build.inputs.design_gross_mass_kg;
    let seats = build.inputs.passenger_count() as f64;
    let expected = 0.11 * mtow + 0.768 * 0.88 * mtow.powf(2.0 / 3.0) + 15.0 * seats;
    assert!(
        (systems.total_kg - expected).abs() < 1.0e-9,
        "{} vs {expected}",
        systems.total_kg
    );
    assert!(systems.furnishings_kg > 0.0);
    let others = systems.total_kg - systems.furnishings_kg;
    let recomputed = systems.surface_controls_kg
        + systems.apu_kg
        + systems.instruments_kg
        + systems.hydraulics_kg
        + systems.electrical_kg
        + systems.avionics_kg
        + systems.air_conditioning_kg
        + systems.anti_ice_kg;
    assert!((others - recomputed).abs() < 1.0e-9);
}

/// The modeled ATR 72-600 operating empty mass against the published typical
/// in-service OEW of 13,450 kg [S ATR 72-600 factsheet; the technical
/// specification lists 13,010 kg]. The regional turboprop method currently
/// lands 14,275 kg, +6.1 % [M]; the band fails if the model regresses beyond
/// 8 % [E], and the sign guard keeps the model from drifting below the
/// technical-specification figure.
#[test]
fn the_atr_operating_empty_mass_stays_within_the_published_band() {
    const PUBLISHED_TYPICAL_OEW_KG: f64 = 13_450.0;
    const PUBLISHED_SPEC_OEW_KG: f64 = 13_010.0;
    const MAX_RELATIVE_ERROR: f64 = 0.08;
    let oew = oew_kg(&buildup("ATR72-600"));
    let error = (oew - PUBLISHED_TYPICAL_OEW_KG) / PUBLISHED_TYPICAL_OEW_KG;
    assert!(
        error.abs() < MAX_RELATIVE_ERROR,
        "ATR OEW {oew:.0} kg is {:+.1} % from the published {PUBLISHED_TYPICAL_OEW_KG} kg",
        100.0 * error
    );
    assert!(oew > PUBLISHED_SPEC_OEW_KG, "ATR OEW {oew:.0} kg");
}

#[test]
fn every_other_preset_keeps_its_method_and_operating_empty_mass() {
    // Operating empty mass in kg of the registered design vector, recorded
    // from the evaluation with the same inputs before the regional method
    // existed. Compared bit for bit: no jet may move. AVE was re-recorded
    // when its default landing ratio became the 777-9 benchmark (0.7574
    // instead of 0.92): the FLOPS main gear goes as WLDG^0.95, and the lighter
    // design landing mass takes 3,425.7 kg off the gear and so off the OEW. The
    // AVE inboard chords (root 16.0 m, break 8.0 m) that keep its root-to-kink
    // trailing edge running aft add wing area and move it again. A340-300,
    // A380-800, A220-300 and DC-10 were re-recorded when the registered cabin
    // became the published planning cabin (335, 555, 140 and 255 seats): the
    // seat count feeds the cabin, service and systems terms.
    let baseline: [(&str, f64); 7] = [
        ("AVE", 177_804.434_127_178_86),
        ("A340-300", 128124.74144473174),
        ("A380-800", 257065.07687024234),
        ("B787-9", 127_020.392_124_507_4),
        ("A320-200", 41_388.795_550_037_08),
        ("A220-300", 36319.44412183819),
        ("DC-10", 117883.55953574172),
    ];
    for (name, expected) in baseline {
        let build = buildup(name);
        assert_ne!(
            build.inputs.cabin_equipment_method,
            CabinEquipmentMethod::RegionalTurbopropV1,
            "{name}"
        );
        let oew = oew_kg(&build);
        assert_eq!(oew.to_bits(), expected.to_bits(), "{name}: {oew:?}");
    }
}
