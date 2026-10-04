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
/// specification lists 13,010 kg]. The regional turboprop class currently
/// lands 13,376 kg, -0.6 % [M], with its pressurized fuselage priced at the
/// 25,000 ft cabin differential and its sponson main gear on the fuselage
/// ground datum. The band fails if the model regresses beyond 10 % [E], and
/// the sign guard keeps the model from drifting below the
/// technical-specification figure.
#[test]
fn the_atr_operating_empty_mass_stays_within_the_published_band() {
    const PUBLISHED_TYPICAL_OEW_KG: f64 = 13_450.0;
    const PUBLISHED_SPEC_OEW_KG: f64 = 13_010.0;
    const MAX_RELATIVE_ERROR: f64 = 0.10;
    let oew = oew_kg(&buildup("ATR72-600"));
    let error = (oew - PUBLISHED_TYPICAL_OEW_KG) / PUBLISHED_TYPICAL_OEW_KG;
    assert!(
        error.abs() < MAX_RELATIVE_ERROR,
        "ATR OEW {oew:.0} kg is {:+.1} % from the published {PUBLISHED_TYPICAL_OEW_KG} kg",
        100.0 * error
    );
    assert!(oew > PUBLISHED_SPEC_OEW_KG, "ATR OEW {oew:.0} kg");
}

/// The ATR 72-600 fuselage takes the pressure-bending method of its class at
/// the CS 25.841(a) differential for its certified 25,000 ft ceiling, and
/// lands on the Torenbeek class II fuselage of the same aircraft: 2,323 kg
/// (Torenbeek 1986 eq. 8.2.6 as evaluated in D. Scholz / HAW Hamburg,
/// "Aircraft Design Studies Based on the ATR 72", TextNita, Sect. 8.2, a
/// secondary source). FLOPS equation 56, fitted at the jet differential,
/// gave 3,235 kg [M]. The 5 % band is the agreement of two independent
/// class II relations [E], not a calibration: neither input was fitted.
#[test]
fn the_atr_fuselage_is_priced_at_its_own_cabin_pressure_differential() {
    const TORENBEEK_CLASS_II_FUSELAGE_KG: f64 = 2_323.0;
    const AGREEMENT: f64 = 0.05;
    let build = buildup("ATR72-600");
    let inputs = build
        .airframe
        .structure_inputs
        .pressurized_fuselage
        .expect("the regional turboprop class prices a pressurized fuselage");
    let psi = inputs.pressure_differential_pa / alas_units::PSI;
    assert!((psi - 5.46).abs() < 0.01, "{psi} psi");
    assert_eq!(inputs.design_zero_fuel_mass_kg, 21_000.0);
    let fuselage_kg = build.airframe.structure.expect("structure").fuselage_kg;
    let error = (fuselage_kg - TORENBEEK_CLASS_II_FUSELAGE_KG) / TORENBEEK_CLASS_II_FUSELAGE_KG;
    assert!(error.abs() < AGREEMENT, "fuselage {fuselage_kg:.0} kg");
}

/// Every jet keeps the transport cabin method, and its modelled operating
/// empty mass is finite and, where the OEW registry
/// (`alas_config::oew_reference`) holds a value comparable with the preset,
/// within that record's stated comparison uncertainty: A340-300 131,215 kg
/// +-2,000 kg (Airbus A340 AC Rev 33), A220-300 37,149 kg +-500 kg (Airbus
/// Canada A220 Aircraft Recovery Publication, weight and balance) and DC-10
/// 120,914 kg +-2,000 kg (Douglas ACAP Series 30 OWE at the
/// 572,000 lb MTOGW option). The band is the registry's own uncertainty, not
/// a fitted tolerance. AVE is notional and the A380-800, B787-9 and A320-200
/// records are source gaps with no comparable value.
#[test]
fn every_other_preset_keeps_its_method_and_its_sourced_operating_empty_mass() {
    let mut compared = 0;
    for name in [
        "AVE", "A340-300", "A380-800", "B787-9", "A320-200", "A220-300", "DC-10",
    ] {
        let build = buildup(name);
        assert_ne!(
            build.inputs.cabin_equipment_method,
            CabinEquipmentMethod::RegionalTurbopropV1,
            "{name}"
        );
        let oew = oew_kg(&build);
        assert!(oew.is_finite() && oew > 0.0, "{name}: {oew}");
        let record = alas_config::oew_reference::get(name).expect("every preset has a record");
        if let (Some(reference_kg), Some(uncertainty_kg)) =
            (record.preset_reference_oew_kg(), record.uncertainty_kg)
        {
            compared += 1;
            assert!(
                (oew - reference_kg).abs() <= uncertainty_kg,
                "{name}: modelled OEW {oew:.0} kg against {reference_kg} kg +- {uncertainty_kg} kg"
            );
        }
    }
    assert_eq!(
        compared, 3,
        "A340-300, A220-300 and DC-10 carry comparable values"
    );
}
