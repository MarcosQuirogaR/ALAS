// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;
use alas_config::SystemsMassMethod;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec};
use alas_geom::aircraft::wing::WingXSec;

fn naca(name: &str) -> Airfoil {
    Airfoil::from_name(name).expect("valid 4-digit NACA name")
}

fn simple_wing(name: &str) -> Wing {
    Wing::new(
        name,
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 3.0, 0.0, naca("naca2412")),
            WingXSec::new([1.0, 15.0, 0.0], 1.0, 0.0, naca("naca2412")),
        ],
        true,
    )
}

fn simple_fuselage(name: &str, x0: f64, x1: f64) -> Fuselage {
    Fuselage::new(
        name,
        vec![
            FuselageXSec::new([x0, 0.0, 0.0], Some(1.0), None, None, 2.0)
                .expect("radius alone is valid"),
            FuselageXSec::new([x1, 0.0, 0.0], Some(0.5), None, None, 2.0)
                .expect("radius alone is valid"),
        ],
    )
}

fn all_zero_breakdown() -> MassBreakdown {
    MassBreakdown {
        wing: 0.0,
        h_stab: 0.0,
        v_stab: 0.0,
        fuselage: 0.0,
        gear: 0.0,
        propulsion: 0.0,
        systems: 0.0,
        furnishings: 0.0,
        payload: 0.0,
        fuel: 0.0,
    }
}

fn all_zero_coordinates() -> MassCoordinates {
    MassCoordinates {
        wing: [1.0, 2.0, 3.0],
        h_stab: [4.0, 5.0, 6.0],
        v_stab: [7.0, 8.0, 9.0],
        fuselage: [10.0, 11.0, 12.0],
        gear: [13.0, 14.0, 15.0],
        propulsion: [16.0, 17.0, 18.0],
        systems: [19.0, 20.0, 21.0],
        furnishings: [22.0, 23.0, 24.0],
        payload: [25.0, 26.0, 27.0],
        fuel: [28.0, 29.0, 30.0],
    }
}

#[test]
fn an_all_zero_mass_input_returns_the_origin_rather_than_dividing_by_zero() {
    let cg = calculate_physical_cg(&all_zero_breakdown(), &all_zero_coordinates());
    assert_eq!(cg, [0.0, 0.0, 0.0]);
}

#[test]
fn a_negative_mass_is_clamped_to_zero_rather_than_pulling_the_cg_the_wrong_way() {
    let mut masses = all_zero_breakdown();
    masses.wing = -1000.0;
    masses.fuselage = 100.0;
    let coords = all_zero_coordinates();
    let cg = calculate_physical_cg(&masses, &coords);
    // Only the fuselage mass (positive) should contribute; the negative
    // wing mass is dropped rather than subtracted.
    assert_eq!(cg, coords.fuselage);
}

#[test]
fn the_cg_of_one_component_is_that_components_own_coordinate() {
    let mut masses = all_zero_breakdown();
    masses.gear = 500.0;
    let coords = all_zero_coordinates();
    let cg = calculate_physical_cg(&masses, &coords);
    assert_eq!(cg, coords.gear);
}

#[test]
fn get_resolves_every_canonical_name_and_nothing_else() {
    let masses = MassBreakdown {
        wing: 1.0,
        h_stab: 2.0,
        v_stab: 3.0,
        fuselage: 4.0,
        gear: 5.0,
        propulsion: 6.0,
        systems: 7.0,
        furnishings: 8.0,
        payload: 9.0,
        fuel: 10.0,
    };
    assert_eq!(masses.get(WING), Some(1.0));
    assert_eq!(masses.get(FUEL), Some(10.0));
    assert_eq!(masses.get("Not a component"), None);
}

#[test]
fn oew_keys_excludes_exactly_payload_and_fuel() {
    assert!(!OEW_KEYS.contains(&PAYLOAD));
    assert!(!OEW_KEYS.contains(&FUEL));
    assert_eq!(OEW_KEYS.len(), 8);
}

fn plane_with_fuselages(fuselages: Vec<Fuselage>) -> Airplane {
    Airplane {
        name: "Probe".to_owned(),
        xyz_ref: [0.0, 0.0, 0.0],
        wings: vec![simple_wing("Main Wing")],
        fuselages,
        s_ref: 100.0,
        c_ref: 5.0,
        b_ref: 30.0,
    }
}

#[test]
fn nacelle_fuselages_overwrite_the_propulsion_coordinate_with_their_mean_position() {
    let geometry = GeometryConfig::default();
    let plane = plane_with_fuselages(vec![
        simple_fuselage("Fuselage", 0.0, 76.72),
        simple_fuselage("Nacelle L", 10.0, 18.0).translate([0.0, -9.8, -2.0]),
        simple_fuselage("Nacelle R", 10.0, 18.0).translate([0.0, 9.8, -2.0]),
    ]);
    let coords = define_mass_coordinates(&plane, &geometry, None, None);
    // Mean X of the two nacelles' (start + half length): both at
    // x_start=10, length=8, so midpoint 14 for each -- mean is 14.
    assert!((coords.propulsion[0] - 14.0).abs() < 1e-9);
    assert!((coords.propulsion[1] - 0.0).abs() < 1e-9); // symmetric L/R
    assert!((coords.propulsion[2] - (-2.0)).abs() < 1e-9);
}

#[test]
fn no_nacelle_fuselages_leaves_the_wing_relative_propulsion_coordinate() {
    let geometry = GeometryConfig::default();
    let plane = plane_with_fuselages(vec![simple_fuselage("Fuselage", 0.0, 76.72)]);
    let coords = define_mass_coordinates(&plane, &geometry, None, None);
    let wing = &plane.wings[0];
    let w_ac = wing.aerodynamic_center(AERODYNAMIC_CENTER_CHORD_FRACTION);
    let w_root_z = wing.xsecs[0].xyz_le[2];
    assert_eq!(coords.propulsion, [w_ac[0], 0.0, w_root_z - 1.0]);
}

#[test]
fn a_positive_payload_layout_replaces_the_lumped_payload_and_recomputes_fuel() {
    let geometry = GeometryConfig::default();
    let requirements = DesignRequirements::default();
    let plane = plane_with_fuselages(vec![simple_fuselage("Fuselage", 0.0, 76.72)]);

    let (baseline_masses, _, _) = run_mass_analysis(&plane, &requirements, &geometry, None, None);

    let layout = PayloadLayoutSummary {
        total_mass: 40_000.0,
        cg_x: 33.0,
        cg_y: 0.5,
    };
    let (masses, coords, _) =
        run_mass_analysis(&plane, &requirements, &geometry, None, Some(&layout));

    assert_eq!(masses.payload, 40_000.0);
    assert_eq!(coords.payload[0], 33.0);
    assert_eq!(coords.payload[1], 0.5);

    let m_oew: f64 = OEW_KEYS
        .iter()
        .map(|&key| baseline_masses.get(key).unwrap_or(0.0))
        .sum();
    let expected_fuel = requirements.mtow_kg - (m_oew + 40_000.0);
    assert!((masses.fuel - expected_fuel).abs() < 1e-9);
}

#[test]
fn a_zero_mass_payload_layout_is_ignored_like_the_python_falsy_check() {
    let geometry = GeometryConfig::default();
    let requirements = DesignRequirements::default();
    let plane = plane_with_fuselages(vec![simple_fuselage("Fuselage", 0.0, 76.72)]);

    let (without, _, _) = run_mass_analysis(&plane, &requirements, &geometry, None, None);
    let layout = PayloadLayoutSummary {
        total_mass: 0.0,
        cg_x: 99.0,
        cg_y: 99.0,
    };
    let (with_zero_layout, _, _) =
        run_mass_analysis(&plane, &requirements, &geometry, None, Some(&layout));
    assert_eq!(without, with_zero_layout);
}

#[test]
fn the_reference_mass_method_remains_the_explicit_checked_default() {
    let geometry = GeometryConfig::default();
    let requirements = DesignRequirements::default();
    let plane = plane_with_fuselages(vec![simple_fuselage("Fuselage", 0.0, 76.72)]);
    let model = MassModelConfig::default();

    let checked = calculate_component_masses_checked(
        &plane,
        &requirements,
        &geometry,
        &CabinConfig::default(),
        &ControlSurfacesConfig::default(),
        Some(&model),
    )
    .expect("reference-compatible method is always resolvable");
    let reference = calculate_component_masses(&plane, &requirements, &geometry, Some(&model));

    assert_eq!(checked, reference);
}

#[test]
fn the_selected_flops_method_returns_typed_unverified_without_architecture_inputs() {
    let geometry = GeometryConfig::default();
    let requirements = DesignRequirements::default();
    let plane = plane_with_fuselages(vec![simple_fuselage("Fuselage", 0.0, 76.72)]);
    let model = MassModelConfig {
        systems_mass_method: SystemsMassMethod::FlopsTransportV1,
        ..MassModelConfig::default()
    };

    let result = calculate_component_masses_checked(
        &plane,
        &requirements,
        &geometry,
        &CabinConfig::default(),
        &ControlSurfacesConfig::default(),
        Some(&model),
    );
    let Err(ComponentMassError::FlopsUnverified { reasons, .. }) = result else {
        panic!("incomplete FLOPS architecture must not fall back to fractions");
    };
    assert!(!reasons.is_empty());
    assert!(reasons.contains(&FlopsTransportUnverifiedReason::MovableSurfaceGeometry));
}
