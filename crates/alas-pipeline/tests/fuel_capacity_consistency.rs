// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Final-report tank capacity must agree with the inventory used for its CG.

// A test asserts on values it built here, so a failed unwrap or expect is the
// assertion failing rather than a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_mass::tanks::resolve_product_layout;
use alas_pipeline::feasibility::{
    assess_fuel_capacity, assess_physical_feasibility, FuelCapacityEvidence,
};
use alas_pipeline::full_analysis::FullAnalysis;

#[test]
fn redesigned_dc10_dispatch_and_balance_use_all_installed_tanks() {
    let preset = presets::get("DC-10").unwrap();
    let mut config = AlasConfig::from_value(&serde_json::json!({"preset":"DC-10"})).unwrap();
    config.mission.enabled = false;
    config.structures.enabled = false;
    // Candidate from the rejected saved run, independent of its result gates.
    let mut design = preset.design_vector;
    design.span_m = 45.96236315230242;
    design.root_chord_m = 11.83652753680161;
    design.break_chord_m = 7.423927757870449;
    design.tip_chord_m = 1.9618290937525737;
    design.sweep_deg = 38.944483085295055;
    design.airfoil_thickness_scale = 0.9626060060394632;
    let report = FullAnalysis::new(config.clone())
        .run(&design, true)
        .unwrap();
    let layout = resolve_product_layout(&config, &design, &report.airplane).unwrap();
    let capacity = assess_fuel_capacity(&config, &design, &report);
    assert_eq!(capacity.evidence, FuelCapacityEvidence::GeometryEstimate);
    assert_eq!(capacity.capacity_kg, Some(layout.usable_capacity_kg()));
    let physical = assess_physical_feasibility(&config, &design, &report, None);
    let balance = physical.mass_balance.unwrap();
    assert!((balance.usable_capacity_kg - layout.usable_capacity_kg()).abs() < 1e-7);
    let sum: f64 = balance
        .tanks
        .iter()
        .map(|tank| tank.usable_capacity_kg)
        .sum();
    assert!((sum - layout.usable_capacity_kg()).abs() < 1e-7);
    assert!(balance
        .tanks
        .iter()
        .any(|tank| tank.id == "auxiliary" && tank.usable_capacity_kg > 0.0));

    // A custom request must not regain the published total or a deleted cell.
    config.fuel_tanks.auxiliary.enabled = false;
    let custom = assess_fuel_capacity(&config, &design, &report);
    let custom_layout = resolve_product_layout(&config, &design, &report.airplane).unwrap();
    assert_eq!(custom.capacity_kg, Some(custom_layout.usable_capacity_kg()));
    assert!(custom.capacity_kg.unwrap() < sum);
    assert!(custom_layout.distribute(sum).is_err());

    // Even at exact preset dimensions, a disabled cell is not published-full.
    let custom_nominal = assess_fuel_capacity(&config, &preset.design_vector, &report);
    assert_ne!(
        custom_nominal.evidence,
        FuelCapacityEvidence::PublishedPreset
    );
    config.fuel_tanks = alas_config::preset_fuel_tanks::layout_for("DC-10").unwrap();
    config.mass_model.fuel_density_kg_m3 = 700.0;
    let changed_density = assess_fuel_capacity(&config, &preset.design_vector, &report);
    assert_eq!(
        changed_density.evidence,
        FuelCapacityEvidence::GeometryEstimate
    );
    config.mass_model.fuel_density_kg_m3 = -1.0;
    let invalid = assess_fuel_capacity(&config, &preset.design_vector, &report);
    assert_eq!(invalid.evidence, FuelCapacityEvidence::Unavailable);
    assert_eq!(invalid.capacity_kg, None);
}
