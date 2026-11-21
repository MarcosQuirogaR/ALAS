// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The FLOPS main-gear leg of a fuselage-mounted main gear comes from the
//! fuselage ground datum, not from the wing-nacelle form of equation 66.

// A failed unwrap or expectation is a failed test assertion.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{calculate_flops_mass_buildup, FlopsMassBuildup, ProductMassBuildup};
use serde_json::json;

fn buildup(name: &str) -> (FlopsMassBuildup, AlasConfig) {
    buildup_with(name, true)
}

/// The preset's build-up with its declared belly clearance kept or removed.
fn buildup_with(name: &str, declared: bool) -> (FlopsMassBuildup, AlasConfig) {
    let mut config =
        AlasConfig::from_value(&json!({ "preset": name })).expect("preset configuration");
    if !declared {
        config.landing_gear.fuselage_ground_clearance_m = None;
    }
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
    let strut_m = config.landing_gear.fuselage_ground_clearance_m.unwrap();
    let inputs = &build.airframe.structure_inputs;
    assert!((inputs.main_gear_oleo_length_m - strut_m).abs() < 1e-12);
    assert!((inputs.nose_gear_oleo_length_m - 0.7 * strut_m).abs() < 1e-12);
}

/// With no declared clearance the sponson leg is the large end of the class
/// sample (a longer leg is the heavier gear, FLOPS mass goes with the leg to
/// the power 0.43): the turboprop sample is ATR 0.223 D to A400M 0.216 D.
#[test]
fn an_undeclared_sponson_leg_is_the_large_end_of_the_class_sample() {
    let (build, config) = buildup_with("ATR72-600", false);
    let strut_m = 0.223 * config.geometry.fuselage.diameter_m;
    let inputs = &build.airframe.structure_inputs;
    assert!((inputs.main_gear_oleo_length_m - strut_m).abs() < 1e-12);
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

/// A military transport with sponson main gear takes the Beltramo military
/// alighting-gear relation (NASA CR-151970 pp. 5-22, 5-29) in place of
/// equations 63-64; a civil sponson gear keeps equations 63-64. Only the A400M
/// declares carbon brakes and low-pressure tyres (p. 5-29 adjustments) and a
/// kneeling main gear (Raymer K_mp on the main-gear share).
#[test]
fn military_sponson_gear_takes_the_military_transport_class_relation() {
    use alas_mass::flops_transport::military_gear::kneeling::KNEELING_MAIN_GEAR_FACTOR;
    use alas_mass::flops_transport::military_gear::{
        military_gear_kg, MilitaryGearFeatures, FLOPS_GEAR_SOURCE, MILITARY_GEAR_SOURCE,
    };
    use alas_mass::flops_transport::structure::{main_gear_kg, nose_gear_kg};
    let presets = [
        ("A400M", true, true, true, 5_453.4, 6_074.7),
        ("C295", false, false, false, 634.2, 634.2),
    ];
    for (name, carbon, low_pressure, kneeling, expected_kg, total_kg) in presets {
        let (build, config) = buildup(name);
        let declared = &config.mass_model.flops_structure;
        assert_eq!(declared.main_gear_carbon_brakes, carbon, "{name}");
        let declared_low_pressure = declared.main_gear_low_pressure_tyres;
        assert_eq!(declared_low_pressure, low_pressure, "{name}");
        assert_eq!(declared.main_gear_kneeling, kneeling, "{name}");
        assert_eq!(build.airframe.sources.gear_mass, MILITARY_GEAR_SOURCE, "{name}");
        assert_eq!(MILITARY_GEAR_SOURCE, "beltramo_cr151970_military");
        let features = MilitaryGearFeatures {
            carbon_brakes: carbon,
            low_pressure_tyres: low_pressure,
            kneeling_main_gear: kneeling,
        };
        let dg_kg = build.airframe.structure_inputs.wing.design_gross_mass_kg;
        let expected = military_gear_kg(dg_kg, features);
        // Hand evaluations at the declared design masses (141,000 and 21,000 kg).
        assert!((expected - expected_kg).abs() < 0.5, "{name} {expected}");
        // A kneeling main gear takes 0.126 of its main share on top of the class
        // total, the nose gear unchanged (A400M: 0.126 x 4,930.9 = 621.3 kg).
        let structure = build.airframe.structure.as_ref().unwrap();
        let factor = if kneeling {
            KNEELING_MAIN_GEAR_FACTOR
        } else {
            1.0
        };
        let main_before_kg = structure.main_gear_kg / factor;
        let class_total = main_before_kg + structure.nose_gear_kg;
        assert!((class_total - expected).abs() < 1e-6, "{name} {class_total} vs {expected}");
        let gear = structure.main_gear_kg + structure.nose_gear_kg;
        assert!((gear - total_kg).abs() < 0.5, "{name} {gear}");
        assert!((build.masses.gear - gear).abs() < 1e-6, "{name}");
    }
    let (atr, atr_config) = buildup("ATR72-600");
    let atr_declared = &atr_config.mass_model.flops_structure;
    assert!(!atr_declared.main_gear_low_pressure_tyres);
    assert_eq!(atr.airframe.sources.gear_mass, FLOPS_GEAR_SOURCE);
    let inputs = &atr.airframe.structure_inputs;
    let flops = main_gear_kg(inputs.design_landing_mass_kg, inputs.main_gear_oleo_length_m)
        + nose_gear_kg(inputs.design_landing_mass_kg, inputs.nose_gear_oleo_length_m);
    assert!((atr.masses.gear - flops).abs() < 1e-9, "{} vs {flops}", atr.masses.gear);
}
