// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;
use crate::flops_transport::{FlopsOperatingItemsBreakdown, FlopsSystemsBreakdown};
use crate::ledger::{InertiaTensor, MassMethod, MassRole};
use crate::stations::ComponentStation;

fn close(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1.0e-9 * left.abs().max(right.abs()).max(1.0)
}

fn station(x: f64, z: f64) -> ComponentStation {
    ComponentStation {
        position_m: [x, 0.0, z],
        extent_m: [1.0, 1.0, 1.0],
        method: "test fixture",
    }
}

/// A hand-built, geometrically plausible station set: gear, tails and
/// systems all aft of the wing, no nacelle geometry (exercising the
/// legacy no-nacelle propulsion fallback).
fn sample_stations() -> ComponentStations {
    ComponentStations {
        wing: station(20.0, 0.0),
        horizontal_tail: station(38.0, 2.0),
        vertical_tail: station(37.0, 3.0),
        fuselage: ComponentStation {
            position_m: [18.0, 0.0, 0.0],
            extent_m: [40.0, 4.0, 4.0],
            method: "test fixture",
        },
        nose_gear: station(3.0, -2.0),
        main_gear: station(21.0, -2.0),
        propulsion_units: Vec::new(),
        systems: station(15.0, 0.0),
        furnishings: station(16.0, 0.0),
        operating_items: station(16.0, 0.0),
        payload_fallback: station(16.0, 0.0),
    }
}

fn sample_masses() -> MassBreakdown {
    MassBreakdown {
        wing: 8_000.0,
        h_stab: 800.0,
        v_stab: 600.0,
        fuselage: 9_000.0,
        gear: 3_000.0,
        propulsion: 4_000.0,
        systems: 3_000.0,
        furnishings: 4_000.0,
        payload: 15_000.0,
        fuel: 20_000.0,
    }
}

fn fuel_item(id: &str, mass_kg: f64, position_m: [f64; 3]) -> MassItem {
    MassItem {
        id: id.to_owned(),
        group: MassGroup::Fuel,
        role: MassRole::UsableFuel,
        mass_kg,
        position_m,
        local_inertia: InertiaTensor::ZERO,
        method: MassMethod::TankFill,
    }
}

fn build_statement(
    masses: &MassBreakdown,
    stations: &ComponentStations,
    takeoff_fuel_items: Vec<MassItem>,
) -> MassStatement {
    MassStatement::build(MassStatementInputs {
        masses,
        stations,
        payload_items: &[],
        takeoff_fuel_items,
        landing_fuel_items: Vec::new(),
        unusable_fuel_items: Vec::new(),
        flops: None,
    })
    .expect("a consistent hand-built statement validates")
}

#[test]
fn state_identities_match_the_legacy_breakdown_sums() {
    let masses = sample_masses();
    let stations = sample_stations();
    let takeoff_fuel_items = vec![fuel_item("fuel-takeoff", 12_000.0, [20.0, 0.0, 1.0])];
    let takeoff_fuel_mass: f64 = takeoff_fuel_items.iter().map(|item| item.mass_kg).sum();
    let statement = build_statement(&masses, &stations, takeoff_fuel_items);

    let oew_sum = masses.wing
        + masses.h_stab
        + masses.v_stab
        + masses.fuselage
        + masses.gear
        + masses.propulsion
        + masses.systems
        + masses.furnishings;
    let oew = statement.state(LoadState::OperatingEmpty);
    assert!(
        close(oew.mass_kg, oew_sum),
        "oew={} expected={oew_sum}",
        oew.mass_kg
    );

    let zfw = statement.state(LoadState::ZeroFuel);
    assert!(close(zfw.mass_kg, oew_sum + masses.payload));

    let takeoff = statement.state(LoadState::Takeoff);
    assert!(close(takeoff.mass_kg, zfw.mass_kg + takeoff_fuel_mass));
}

#[test]
fn forward_fuel_moves_the_takeoff_cg_forward_of_zero_fuel() {
    let masses = sample_masses();
    let stations = sample_stations();
    let zero_fuel_cg_x = build_statement(&masses, &stations, Vec::new())
        .state(LoadState::ZeroFuel)
        .cg_m[0];

    let forward_fuel = vec![fuel_item(
        "fuel-forward",
        5_000.0,
        [zero_fuel_cg_x - 15.0, 0.0, 0.0],
    )];
    let statement = build_statement(&masses, &stations, forward_fuel);
    let zero_fuel = statement.state(LoadState::ZeroFuel);
    let takeoff = statement.state(LoadState::Takeoff);
    assert!(takeoff.cg_m[0] < zero_fuel.cg_m[0]);
}

#[test]
fn the_flops_split_preserves_systems_and_furnishings_totals() {
    let systems = FlopsSystemsBreakdown {
        surface_controls_kg: 900.0,
        apu_kg: 400.0,
        instruments_kg: 250.0,
        hydraulics_kg: 500.0,
        electrical_kg: 700.0,
        avionics_kg: 600.0,
        furnishings_kg: 1_800.0,
        air_conditioning_kg: 550.0,
        anti_ice_kg: 150.0,
        total_kg: 900.0 + 400.0 + 250.0 + 500.0 + 700.0 + 600.0 + 1_800.0 + 550.0 + 150.0,
    };
    let operating_items = FlopsOperatingItemsBreakdown {
        cabin_crew_and_baggage_kg: 380.0,
        flight_crew_and_baggage_kg: 190.0,
        unusable_fuel_kg: 120.0,
        engine_oil_kg: 60.0,
        passenger_service_kg: 240.0,
        cargo_containers_kg: 90.0,
        total_kg: 380.0 + 190.0 + 120.0 + 60.0 + 240.0 + 90.0,
    };
    let flops = FlopsTransportBreakdown {
        systems,
        operating_items,
    };
    let masses = MassBreakdown {
        systems: systems.total_kg,
        furnishings: systems.furnishings_kg + operating_items.total_kg,
        ..sample_masses()
    };
    let stations = sample_stations();

    let statement = MassStatement::build(MassStatementInputs {
        masses: &masses,
        stations: &stations,
        payload_items: &[],
        takeoff_fuel_items: Vec::new(),
        landing_fuel_items: Vec::new(),
        unusable_fuel_items: Vec::new(),
        flops: Some(&flops),
    })
    .expect("a consistent FLOPS-split statement validates");

    let totals = statement.group_totals(LoadState::ZeroFuel);
    let systems_total: f64 = totals
        .iter()
        .filter(|(group, _)| *group == MassGroup::Systems)
        .map(|(_, mass_kg)| *mass_kg)
        .sum();
    let furnishings_and_operating_total: f64 = totals
        .iter()
        .filter(|(group, _)| {
            *group == MassGroup::Furnishings || *group == MassGroup::OperatingItems
        })
        .map(|(_, mass_kg)| *mass_kg)
        .sum();

    assert!(
        (systems_total - masses.systems).abs() < 1.0e-6,
        "systems_total={systems_total} expected={}",
        masses.systems
    );
    assert!(
        (furnishings_and_operating_total - masses.furnishings).abs() < 1.0e-6,
        "furnishings_and_operating_total={furnishings_and_operating_total} expected={}",
        masses.furnishings
    );
}

// Building the default product aircraft and running the legacy mass
// analysis are assertions that the default configuration is valid, so a
// failed expect here is that assertion failing, not a library invariant
// being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[test]
fn the_default_aircraft_ledger_tensor_is_physical_and_plausible() {
    use alas_config::{AlasConfig, DesignVector};
    use alas_geom::builder::AircraftBuilder;

    use crate::breakdown::run_mass_analysis;
    use crate::stations::component_stations;

    let config = AlasConfig::default();
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&DesignVector::default()), true)
        .expect("the default geometry configuration builds");
    let (masses, _coordinates, _cg) = run_mass_analysis(
        &plane,
        &config.requirements,
        &config.geometry,
        Some(&config.mass_model),
        None,
    );
    let stations = component_stations(
        &plane,
        &config.geometry,
        &config.requirements,
        &config.mass_model,
        &config.structures,
    )
    .expect("the default aircraft resolves every station");

    let statement = MassStatement::build(MassStatementInputs {
        masses: &masses,
        stations: &stations,
        payload_items: &[],
        takeoff_fuel_items: Vec::new(),
        landing_fuel_items: Vec::new(),
        unusable_fuel_items: Vec::new(),
        flops: None,
    })
    .expect("the default aircraft ledger validates");

    let zero_fuel = statement.state(LoadState::ZeroFuel);
    assert!(zero_fuel.inertia_cg.is_physical());

    let span_m = plane.wings[0].reference_span();
    let fuselage_length_m = stations.fuselage.extent_m[0];
    let comparison =
        statement.radii_of_gyration_check(LoadState::ZeroFuel, span_m, fuselage_length_m);
    assert!(
        (0.5..=2.0).contains(&comparison.ratio[1]),
        "pitch ratio={} ledger={} reference={}",
        comparison.ratio[1],
        comparison.ledger_radii_m[1],
        comparison.reference_radii_m[1]
    );
    assert!(
        (0.5..=2.0).contains(&comparison.ratio[2]),
        "yaw ratio={} ledger={} reference={}",
        comparison.ratio[2],
        comparison.ledger_radii_m[2],
        comparison.reference_radii_m[2]
    );
}
