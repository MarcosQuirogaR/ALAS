// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! LTH total ownership and crew first moments, kg and nose-based metres.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{run_product_mass_analysis_with_groups, MassCoordinateModel};
use alas_mass::flops_transport::FlopsTransportBreakdown;
use alas_mass::statement::{LoadState, MassStatement, MassStatementInputs};
use alas_mass::stations::component_stations_with_gear;

#[test]
fn lth_crew_is_allocated_to_cockpit_without_changing_operating_total() {
    let config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
    let design = presets::get("A320-200").unwrap().design_vector;
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), true)
        .unwrap();
    let (masses, _, _, groups) = run_product_mass_analysis_with_groups(
        &plane,
        &config.requirements,
        &config.geometry,
        &config.cabin,
        &config.control_surfaces,
        Some(&config.mass_model),
        None,
        MassCoordinateModel::StructuralWingbox(&config.structures),
        &config.landing_gear,
    )
    .unwrap();
    let groups = groups.unwrap();
    let stations = component_stations_with_gear(
        &plane,
        &config.geometry,
        &config.requirements,
        &config.mass_model,
        &config.structures,
        &config.landing_gear,
    )
    .unwrap();
    let build = |flops: &FlopsTransportBreakdown| {
        MassStatement::build(MassStatementInputs {
            masses: &masses,
            stations: &stations,
            payload_items: &[],
            takeoff_fuel_items: vec![],
            landing_fuel_items: vec![],
            unusable_fuel_items: vec![],
            flops: Some(flops),
            flops_gear_split_kg: groups.gear_split_kg(),
        })
        .unwrap()
    };
    let flops = &groups.systems_and_operating_items;
    assert!((flops.operating_items.total_kg - 32.907 * 150_f64.powf(1.021)).abs() < 1e-8);
    // Eq.120: two pilots at225lb/person including baggage; the LTH source
    // includes crew, so allocating this known count does not add OEW.
    let pilot_mass = 2.0 * 225.0 * 0.453_592_37;
    assert!((flops.operating_items.flight_crew_and_baggage_kg - pilot_mass).abs() < 1e-9);
    let statement = build(flops);
    let crew_row = statement
        .ledger()
        .items()
        .iter()
        .find(|item| item.id == "operating-flight_crew")
        .unwrap();
    assert_eq!(
        crew_row.method.label(),
        "FLOPS crew allocation within LTH total"
    );
    let allocated = statement.state(LoadState::OperatingEmpty);
    let mut bundled = *flops;
    bundled.operating_items.passenger_service_kg += pilot_mass;
    bundled.operating_items.flight_crew_and_baggage_kg = 0.0;
    let bundled_placement = build(&bundled).state(LoadState::OperatingEmpty);
    assert!((allocated.mass_kg - bundled_placement.mass_kg).abs() < 1e-8);
    let cockpit_x = 0.05 * design.fuselage_length_m;
    let expected_moment_change = pilot_mass * (cockpit_x - stations.operating_items.position_m[0]);
    let actual_moment_change = allocated.mass_kg * allocated.cg_m[0]
        - bundled_placement.mass_kg * bundled_placement.cg_m[0];
    assert!((actual_moment_change - expected_moment_change).abs() < 1e-7);
    assert!(allocated.cg_m[0] < bundled_placement.cg_m[0]);
}
