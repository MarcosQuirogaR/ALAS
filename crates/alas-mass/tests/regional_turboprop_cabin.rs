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
/// lands 14,608 kg, +8.6 % [M]: the wing that closes the TCDS 2.303 m MAC on
/// the 61 m^2 area tapers 0.62 rather than the 0.23 of the earlier
/// estimated chords, which raises the FLOPS bending-material term. The band
/// fails if the model regresses beyond 10 % [E], and the sign guard keeps the
/// model from drifting below the technical-specification figure.
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
    // seat count feeds the cabin, service and systems terms. DC-10 was
    // re-recorded again when its wing became the Douglas reference trapezoid
    // and its tailplane the DAC-67803A Figure 2.2 planform (wing and
    // horizontal-tail areas and tapers feed the FLOPS structure terms). A320-200
    // (planar 34.10 m planform), B787-9, A340-300 and A220-300 (published
    // sweeps entered at the leading edge, B787 12 % root) and A380-800
    // (outboard-rising camber sections) were re-recorded for the sourced
    // geometry: span, sweep and section thickness feed the FLOPS wing terms.
    // B787-9 was re-recorded when its engines moved to the D6-58333 Rev Q
    // 9.91 m station: the engine span feeds the FLOPS main-gear oleo length
    // (equation 66) and the detailed wing-bending engine-relief stations.
    // A340-300 was re-recorded for the 2.5 m drawn tip chord: the taper,
    // kink station and converted sweep feed the FLOPS wing terms.
    // A380-800 was re-recorded for the Airbus AC Rev 20 drawing geometry:
    // the 0.222 taper feeds the FLOPS wing terms, the 134.8 m^2 fin and
    // 193.2 m^2 tailplane the tail terms, the 14.8/25.7 m engine stations the
    // engine relief and main-gear oleo.
    // B787-9 was re-recorded for the D6-58333 Rev Q body: the 62.00 m TCDS
    // length and the 5.77 m by 5.94 m section feed the FLOPS fuselage terms,
    // and the tail stations the tail moment arms.
    // A340-300 was re-recorded for the Airbus AC Rev 33 engine stations,
    // tail and ground-clearance heights: the engine span feeds the main-gear
    // oleo and the engine relief, the fin and tailplane the tail terms.
    // A220-300 was re-recorded for the ACP Issue 013 body height, belly,
    // engine station and tail: the 3.721 m height feeds the FLOPS fuselage
    // terms and the 36.6 m^2 tailplane the horizontal-tail term.
    // A320-200 was re-recorded for the drawn tailplane taper and sweep, which
    // feed the horizontal-tail term at unchanged area.
    // DC-10 was re-recorded for the printed 55.35 m length (FLOPS fuselage
    // terms) and the 8.18 m engine station (main-gear oleo, engine relief).
    // Fin roots returned to their estimated heights, the spans holding the
    // drawn tip heights; the fin span and area feed the vertical-tail term.
    // Re-recorded for it: A380-800 (whose wing kept its flight-shape
    // dihedral), B787-9, A340-300, A220-300 and A320-200.
    let baseline: [(&str, f64); 7] = [
        ("AVE", 177_804.434_127_178_86),
        ("A340-300", 131_264.021_721_927_86),
        ("A380-800", 266_243.402_191_617_06),
        ("B787-9", 128_382.144_822_188_86),
        ("A320-200", 41_423.351_222_965_47),
        ("A220-300", 37_140.353_090_840_26),
        ("DC-10", 121_922.460_596_526_86),
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
