// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The FLOPS inputs the registered aircraft resolve from their built geometry:
//! where the tailplane sits and which engines relieve the wing.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig, FlopsWingBendingMethod};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{calculate_flops_mass_buildup, FlopsMassBuildup, ProductMassBuildup};
use serde_json::json;

fn buildup(name: &str, bending: FlopsWingBendingMethod) -> FlopsMassBuildup {
    let preset = presets::get(name).expect("registered preset");
    let mut config = AlasConfig::from_value(&json!({ "preset": name })).expect("preset loads");
    config.mass_model.flops_structure.wing_bending_method = bending;
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("preset builds");
    let build = calculate_flops_mass_buildup(
        &plane,
        &config.requirements,
        &config.geometry,
        &config.control_surfaces,
        Some(&config.mass_model),
        &config.landing_gear,
        &config.cabin,
    )
    .expect("preset evaluates");
    let ProductMassBuildup::PureFlops(build) = build else {
        panic!("{name} is a pure FLOPS product");
    };
    *build
}

#[test]
fn only_a_tailplane_on_the_fin_raises_the_fin_mass() {
    for name in ["ATR72-600", "A400M", "LJ60"] {
        let build = buildup(name, FlopsWingBendingMethod::Simplified);
        let factor = build.airframe.structure_inputs.fin_mounted_tailplane_factor;
        assert!((1.1..1.3).contains(&factor), "{name} {factor}");
    }
    for name in [
        "A320-200", "A350-900", "B747-400", "DC-10", "E195-E2", "C295",
    ] {
        let build = buildup(name, FlopsWingBendingMethod::Simplified);
        let factor = build.airframe.structure_inputs.fin_mounted_tailplane_factor;
        assert_eq!(factor, 1.0, "{name}");
    }
}

#[test]
fn the_detailed_relief_reads_the_pods_of_one_semi_wing() {
    let relief = |name: &str| {
        buildup(name, FlopsWingBendingMethod::Detailed)
            .airframe
            .structure
            .as_ref()
            .expect("structural group")
            .wing
            .inertia_relief_factor
    };
    // The rear-fuselage engines of the LJ60 relieve no wing.
    assert_eq!(relief("LJ60"), 1.0);
    // One pod a side: the range of the validated twin decks, 0.959 and 0.967,
    // not the doubled relief of counting both sides.
    let twin = relief("A320-200");
    assert!((0.945..0.975).contains(&twin), "{twin}");
}

#[test]
fn the_ramp_has_one_owner_by_the_fuselage_method_in_use() {
    use alas_mass::flops_transport::military_cargo::military_cargo_equipment;
    // The C295 fuselage is the pressurized regional method, which has no
    // cargo-floor term: its ramp and handling system are both charged items.
    // The A400M fuselage is FLOPS equation 56 with CARGF 1, which owns the ramp;
    // its handling system stays an item.
    for (name, pressurized, owns_ramp) in [("C295", true, false), ("A400M", false, true)] {
        let build = buildup(name, FlopsWingBendingMethod::Simplified);
        assert_eq!(
            build
                .airframe
                .structure_inputs
                .pressurized_fuselage
                .is_some(),
            pressurized,
            "{name}"
        );
        let hold = build.inputs.military_cargo.expect("declared freight hold");
        assert!(hold.rear_ramp, "{name}");
        assert_eq!(hold.fuselage_owns_ramp, owns_ramp, "{name}");
        let items = military_cargo_equipment(&hold, build.inputs.design_gross_mass_kg, 10_000.0);
        assert!(items.handling_system_kg > 0.0, "{name}");
        assert_eq!(items.ramp_kg > 0.0, !owns_ramp, "{name}");
        let charged = build
            .systems_and_operating_items
            .systems
            .military_cargo_equipment_kg;
        assert!(
            charged >= items.handling_system_kg + items.handling_gear_kg - 1e-9,
            "{name} {charged}"
        );
    }
}

/// The business jet prices Torenbeek's executive cabin at its own delivery
/// empty mass: the flight-deck term's `W_DE` is the empty mass this buildup
/// returns, not the design gross mass it starts from.
#[test]
fn the_business_jet_cabin_reads_its_own_delivery_empty_mass() {
    let build = buildup("LJ60", FlopsWingBendingMethod::Simplified);
    assert_eq!(
        build.inputs.cabin_equipment_method,
        alas_config::CabinEquipmentMethod::ExecutiveJetV1
    );
    let cabin = build.inputs.executive_cabin.expect("declared interior");
    let m = &build.masses;
    let empty = m.wing + m.h_stab + m.v_stab + m.fuselage + m.gear + m.propulsion + m.systems
        + m.furnishings
        - build
            .systems_and_operating_items
            .operating_items
            .total_kg;
    let delivery = cabin.delivery_empty_mass_kg.expect("converged");
    assert!((delivery - empty).abs() < 1e-6, "{delivery} {empty}");
    for name in ["A320-200", "ATR72-600", "C295"] {
        let other = buildup(name, FlopsWingBendingMethod::Simplified);
        assert!(other.inputs.executive_cabin.is_none(), "{name}");
    }
}

/// NASA CR-151970 (1977) class relations for the propeller and military
/// transports: the anti-icing of every wing-mounted turboprop installation is
/// `W12 = 0.520 Sw` lb (p. 5-96), and the avionics of a declared military hold
/// within the report's military data (body wetted area from 3,339 ft^2) is
/// `W16 = 0.10 Sb + 2,330` lb (p. 5-117). The jets keep FLOPS equation 115 and
/// the FLOPS avionics equation; the C295 body lies below the military data and
/// keeps the FLOPS avionics; the regional-turboprop cabins still close their
/// systems group on the Torenbeek relation, so their empty mass is unchanged.
#[test]
fn turboprop_anti_icing_and_military_avionics_follow_their_class() {
    const FT2_PER_M2: f64 = 1.0 / (0.3048 * 0.3048);
    const KG_PER_LB: f64 = 0.453_592_37;
    for name in ["ATR72-600", "C295", "A400M"] {
        let build = buildup(name, FlopsWingBendingMethod::Simplified);
        let systems = &build.systems_and_operating_items.systems;
        let expected = 0.520 * build.inputs.wing_area_m2 * FT2_PER_M2 * KG_PER_LB;
        assert!((systems.anti_ice_kg - expected).abs() < 1e-6, "{name} {}", systems.anti_ice_kg);
    }
    for name in ["A320-200", "B747-400", "LJ60"] {
        let build = buildup(name, FlopsWingBendingMethod::Simplified);
        let anti_ice = build.systems_and_operating_items.systems.anti_ice_kg;
        let turboprop_line = 0.520 * build.inputs.wing_area_m2 * FT2_PER_M2 * KG_PER_LB;
        assert!((anti_ice - turboprop_line).abs() > 1.0, "{name} keeps equation 115");
    }
    let a400m = buildup("A400M", FlopsWingBendingMethod::Simplified);
    let hold = a400m.inputs.military_cargo.expect("declared freight hold");
    assert!(hold.military);
    let body_ft2 = hold.body_wetted_area_m2 * FT2_PER_M2;
    assert!((3_339.0..16_507.0).contains(&body_ft2), "A400M Sb {body_ft2} ft^2");
    let avionics = a400m.systems_and_operating_items.systems.avionics_kg;
    assert!((avionics - (0.10 * body_ft2 + 2_330.0) * KG_PER_LB).abs() < 1e-6, "{avionics}");
    let c295 = buildup("C295", FlopsWingBendingMethod::Simplified);
    let hold = c295.inputs.military_cargo.expect("declared freight hold");
    assert!(hold.military);
    assert!(hold.body_wetted_area_m2 * FT2_PER_M2 < 3_339.0, "C295 below the military data");
    let avionics = c295.systems_and_operating_items.systems.avionics_kg;
    assert!(avionics < 2_330.0 * KG_PER_LB, "C295 keeps the FLOPS avionics: {avionics}");
}
