// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The wing is priced at the discrete-gust load relative to the registered
//! aircraft's, so a stretched wing pays for its gust load and the registered
//! aircraft itself is priced exactly as before.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]
// The test prints its diagnostic table of measured values.
#![allow(clippy::print_stdout)]

use alas_config::{presets, AlasConfig, DesignVector};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{calculate_flops_mass_buildup, ProductMassBuildup};
use serde_json::json;

struct Priced {
    ultimate_load_factor: f64,
    wing_kg: f64,
}

fn priced(name: &str, vector: &DesignVector, with_gust_term: bool) -> Priced {
    priced_at(name, vector, with_gust_term, None)
}

/// `limit_manoeuvre_g` replaces the requirement's CS-25.337 limit load factor.
fn priced_at(
    name: &str,
    vector: &DesignVector,
    with_gust_term: bool,
    limit_manoeuvre_g: Option<f64>,
) -> Priced {
    let mut config = AlasConfig::from_value(&json!({ "preset": name })).expect("preset loads");
    if let Some(n) = limit_manoeuvre_g {
        config.requirements.ultimate_load_factor_from_mtow = false;
        config.requirements.ultimate_load_factor = 1.5 * n;
    }
    if !with_gust_term {
        config.mass_model.flops_structure.gust_reference_preset = None;
    }
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(vector), true)
        .expect("candidate builds");
    let build = calculate_flops_mass_buildup(
        &plane,
        &config.requirements,
        &config.geometry,
        &config.control_surfaces,
        Some(&config.mass_model),
        &config.landing_gear,
        &config.cabin,
    )
    .expect("candidate evaluates");
    let ProductMassBuildup::PureFlops(build) = build else {
        panic!("{name} is a pure FLOPS product");
    };
    Priced {
        ultimate_load_factor: build.airframe.structure_inputs.wing.ultimate_load_factor,
        wing_kg: build
            .airframe
            .structure
            .as_ref()
            .expect("structure")
            .wing
            .total_kg,
    }
}

fn stretched(name: &str) -> DesignVector {
    let mut vector = presets::get(name).unwrap().design_vector;
    vector.span_m *= 1.10;
    vector.root_chord_m *= 0.90;
    vector.break_chord_m *= 0.90;
    vector.tip_chord_m *= 0.90;
    vector
}

#[test]
fn the_registered_aircraft_is_priced_at_the_manoeuvre_load_exactly() {
    for name in ["A320-200", "ATR72-600", "B747-400", "LJ60"] {
        let preset = presets::get(name).unwrap();
        let config = AlasConfig::from_value(&json!({ "preset": name })).unwrap();
        let with = priced(name, &preset.design_vector, true);
        let without = priced(name, &preset.design_vector, false);
        assert_eq!(
            with.ultimate_load_factor.to_bits(),
            config.requirements.design_ultimate_load_factor().to_bits(),
            "{name}"
        );
        assert_eq!(with.wing_kg.to_bits(), without.wing_kg.to_bits(), "{name}");
    }
}

#[test]
fn a_higher_aspect_ratio_candidate_pays_for_its_gust_load_when_the_gust_governs() {
    // The registered ATR 72-600 critical gust is n = 2.171 (V_C case) and the
    // span +10 % / chord -10 % candidate's is 2.185, under the 2.5 g manoeuvre
    // limit, so the shipped requirement is unchanged (next test). With a 2.0 g
    // manoeuvre limit the gust governs and the stretched wing pays the ratio.
    let name = "ATR72-600";
    let vector = stretched(name);
    let with = priced_at(name, &vector, true, Some(2.0));
    let without = priced_at(name, &vector, false, Some(2.0));
    println!(
        "{name} ulf {} -> {}, wing {} -> {} kg",
        without.ultimate_load_factor, with.ultimate_load_factor, without.wing_kg, with.wing_kg
    );
    assert!(with.ultimate_load_factor > without.ultimate_load_factor);
    assert!(with.wing_kg > without.wing_kg);
    // The registered wing is still unchanged under the same limit.
    let registered = presets::get(name).unwrap().design_vector;
    let a = priced_at(name, &registered, true, Some(2.0));
    let b = priced_at(name, &registered, false, Some(2.0));
    assert_eq!(a.wing_kg.to_bits(), b.wing_kg.to_bits());
}

#[test]
fn a_gust_below_the_manoeuvre_load_keeps_the_manoeuvre_load_factor() {
    // Every registered transport's gusts lie under its 2.5 g manoeuvre limit
    // (ATR 2.17, A320 1.81, 747 1.74), the stretched ATR's included.
    let name = "ATR72-600";
    let vector = stretched(name);
    let with = priced(name, &vector, true);
    let without = priced(name, &vector, false);
    assert_eq!(
        with.ultimate_load_factor.to_bits(),
        without.ultimate_load_factor.to_bits()
    );
}

/// Wing mass of the stretched candidate with and without the reference term,
/// for a preset with an Al-alloy wing, plus the structural-group totals.
#[test]
fn a_stretched_metallic_wing_pays_the_excess_stiffness_weight() {
    for name in ["A320-200", "B747-400"] {
        let vector = stretched(name);
        let with = priced(name, &vector, true);
        let without = priced(name, &vector, false);
        let excess = with.wing_kg - without.wing_kg;
        println!(
            "{name} wing {:.1} -> {:.1} kg (+{:.1} kg, {:.2} %)",
            without.wing_kg,
            with.wing_kg,
            excess,
            100.0 * excess / without.wing_kg
        );
        assert!(excess > 0.0, "{name}");
    }
}

#[test]
fn the_stretched_atr_wing_pays_the_excess_stiffness_weight() {
    let name = "ATR72-600";
    let vector = stretched(name);
    let with = priced(name, &vector, true);
    let without = priced(name, &vector, false);
    let excess = with.wing_kg - without.wing_kg;
    println!(
        "{name} wing {:.1} -> {:.1} kg (+{:.1} kg, {:.2} %)",
        without.wing_kg,
        with.wing_kg,
        excess,
        100.0 * excess / without.wing_kg
    );
    assert!(excess > 0.0);
    assert_eq!(with.ultimate_load_factor, without.ultimate_load_factor);
}
