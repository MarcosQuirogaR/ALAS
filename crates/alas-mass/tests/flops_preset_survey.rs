// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Focused integration checks for the pure FLOPS production architecture.
//!
//! These tests exercise public configuration and mass entry points with real
//! registered preset geometry. They assert conservation and group ownership,
//! rather than pinning aircraft OEW values or fitting the equations to a
//! reference number. The legacy method appears only as an explicit control.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use alas_config::{
    presets, AlasConfig, MassArchitecture, MassArchitectureMigration, MassModelConfig,
    PropulsionMassMethod, StructuralMassMethod, SystemsMassMethod,
};
use alas_geom::{aircraft::airplane::Airplane, builder::AircraftBuilder};
use alas_mass::breakdown::{
    calculate_flops_mass_buildup, run_product_mass_analysis_with_groups, ComponentMassError,
    FlopsMassBuildup, MassBreakdown, MassCoordinateModel, PayloadLayoutSummary, ProductMassBuildup,
    OEW_KEYS,
};
use alas_mass::flops_transport::FlopsTransportUnverifiedReason;
use serde_json::json;

fn preset_case(name: &str) -> (AlasConfig, Airplane) {
    let config = AlasConfig::from_value(&json!({ "preset": name }))
        .expect("registered preset configuration must load");
    let preset = presets::get(name).expect("registered preset must exist");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("registered preset geometry must build");
    (config, plane)
}

fn pure_buildup(config: &AlasConfig, plane: &Airplane) -> FlopsMassBuildup {
    match calculate_flops_mass_buildup(
        plane,
        &config.requirements,
        &config.geometry,
        &config.control_surfaces,
        Some(&config.mass_model),
        &config.landing_gear,
        &config.cabin,
    )
    .expect("declared jet preset must produce a pure FLOPS buildup")
    {
        ProductMassBuildup::PureFlops(build) => *build,
        ProductMassBuildup::LegacyComparison(_) => {
            panic!("the production entry point returned the legacy control")
        }
    }
}

fn close(actual: f64, expected: f64) {
    let tolerance = 1.0e-9_f64.max(1.0e-12 * actual.abs().max(expected.abs()));
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} differs from {expected} by more than {tolerance}"
    );
}

fn oew(masses: &MassBreakdown) -> f64 {
    OEW_KEYS.iter().filter_map(|name| masses.get(name)).sum()
}

#[test]
fn the_product_default_is_pure_flops_with_a_complete_declared_contract() {
    let model = MassModelConfig::default();
    assert_eq!(
        model.mass_architecture,
        MassArchitecture::PureFlopsTransportV1
    );
    assert!(model.mass_architecture.is_production());
    assert!(model.architecture_is_coherent());
    assert!(!model.flops_transport.is_unspecified());
    assert!(model.flops_transport.provenance.is_complete());
    assert_eq!(
        model.systems_mass_method,
        SystemsMassMethod::FlopsTransportV1
    );
    assert_eq!(
        model.structural_mass_method,
        StructuralMassMethod::FlopsTransportV1
    );
    assert_eq!(
        model.propulsion_mass_method,
        PropulsionMassMethod::FlopsTransportV1
    );
}

#[test]
fn version_one_legacy_and_hybrid_selections_migrate_to_one_pure_architecture() {
    let legacy = json!({
        "mass_model": {
            "schema_version": 1,
            "systems_mass_method": "reference_compatible_fractions",
            "structural_mass_method": "reference_compatible",
            "propulsion_mass_method": "reference_compatible"
        }
    });
    let (legacy_config, legacy_migration) = AlasConfig::from_value_with_migration(&legacy)
        .expect("version-one legacy configuration must load");
    assert_eq!(
        legacy_migration,
        MassArchitectureMigration::LegacyDefaultsMovedToPureFlops
    );
    assert_eq!(
        legacy_config.mass_model.mass_architecture,
        MassArchitecture::PureFlopsTransportV1
    );
    assert!(legacy_config.mass_model.architecture_is_coherent());

    let hybrid = json!({
        "mass_model": {
            "schema_version": 1,
            "systems_mass_method": "flops_transport_v1",
            "structural_mass_method": "reference_compatible",
            "propulsion_mass_method": "reference_compatible"
        }
    });
    let (hybrid_config, hybrid_migration) = AlasConfig::from_value_with_migration(&hybrid)
        .expect("version-one hybrid configuration must load");
    assert_eq!(
        hybrid_migration,
        MassArchitectureMigration::LegacyHybridMigratedToPureFlops {
            systems_was_flops: true,
            structure_was_flops: false,
            propulsion_was_flops: false,
        }
    );
    assert_eq!(
        hybrid_config.mass_model.mass_architecture,
        MassArchitecture::PureFlopsTransportV1
    );
    assert!(hybrid_config.mass_model.architecture_is_coherent());
}

#[test]
fn pure_buildup_maps_each_flops_group_once_and_closes_the_mass_ledger() {
    let (config, plane) = preset_case("A320-200");
    let build = pure_buildup(&config, &plane);
    let masses = build.masses;
    let structure = build
        .airframe
        .structure
        .as_ref()
        .expect("pure buildup must contain the structural group");
    let propulsion = build
        .airframe
        .propulsion
        .as_ref()
        .expect("pure buildup must contain the propulsion group");
    let systems = build.systems_and_operating_items.systems;
    let operating = build.systems_and_operating_items.operating_items;

    assert_eq!(build.inputs.engine_count, 2);
    close(propulsion.total_nacelles, 2.0);
    close(build.nacelle_kg(), structure.nacelle_kg);
    close(build.propulsion_without_nacelles_kg(), propulsion.total_kg);
    close(
        masses.propulsion,
        propulsion.total_kg + structure.nacelle_kg,
    );
    close(
        masses.furnishings,
        systems.furnishings_kg + operating.total_kg,
    );

    // Equation 139 is explicit in the production slot. Reconstruct its base
    // from the independent groups so the test cannot pass by merely checking
    // a hard-coded expected OEW.
    let systems_without_margin = systems.total_kg - systems.furnishings_kg;
    let furnishings_with_operating = systems.furnishings_kg + operating.total_kg;
    let empty_mass_before_margin = masses.wing
        + masses.h_stab
        + masses.v_stab
        + masses.fuselage
        + masses.gear
        + masses.propulsion
        + systems_without_margin
        + furnishings_with_operating
        - operating.total_kg;
    let expected_systems = systems_without_margin
        + config.mass_model.flops_structure.empty_mass_margin_fraction * empty_mass_before_margin;
    close(masses.systems, expected_systems);

    let expected_oew = oew(&masses);
    assert!(expected_oew.is_finite() && expected_oew > 0.0);
    close(
        masses.signed_fuel_closure_kg(),
        config.requirements.mtow_kg - expected_oew - masses.payload,
    );
    close(
        expected_oew + masses.payload + masses.signed_fuel_closure_kg(),
        config.requirements.mtow_kg,
    );
    assert!(masses.physical_fuel_mass_kg().is_some());
    assert!(masses.as_pairs().iter().all(|(_, value)| value.is_finite()));
}

#[test]
fn grouped_product_analysis_returns_the_same_mass_buildup_used_for_coordinates() {
    let (config, plane) = preset_case("B787-9");
    let direct = pure_buildup(&config, &plane);
    let (masses, _coordinates, _cg, grouped) = run_product_mass_analysis_with_groups(
        &plane,
        &config.requirements,
        &config.geometry,
        &config.cabin,
        &config.control_surfaces,
        Some(&config.mass_model),
        None,
        MassCoordinateModel::ReferenceCompatibility,
        &config.landing_gear,
    )
    .expect("grouped product mass analysis must complete");
    let grouped = grouped.expect("pure architecture must retain its FLOPS groups");
    assert_eq!(masses, direct.masses);
    assert_eq!(
        grouped.systems_and_operating_items,
        direct.systems_and_operating_items
    );
    assert_eq!(grouped.airframe, direct.airframe);
}

#[test]
fn grouped_product_analysis_recloses_the_buildup_after_detailed_payload_layout() {
    let (config, plane) = preset_case("B787-9");
    let direct = pure_buildup(&config, &plane);
    let layout = PayloadLayoutSummary {
        total_mass: direct.masses.payload - 1_000.0,
        cg_x: 18.0,
        cg_y: 0.0,
    };
    let (masses, _coordinates, _cg, grouped) = run_product_mass_analysis_with_groups(
        &plane,
        &config.requirements,
        &config.geometry,
        &config.cabin,
        &config.control_surfaces,
        Some(&config.mass_model),
        Some(&layout),
        MassCoordinateModel::ReferenceCompatibility,
        &config.landing_gear,
    )
    .expect("grouped product mass analysis with detailed payload");
    let grouped = grouped.expect("pure architecture must retain its FLOPS groups");

    // The detailed cabin solver can seat a load case whose carried payload is
    // different from the nominal requirement. The grouped object is the
    // authoritative ledger consumed by the report/export path, so it must be
    // updated together with the returned masses and its signed fuel closure.
    close(masses.payload, layout.total_mass);
    close(grouped.masses.payload, masses.payload);
    close(grouped.masses.fuel, masses.fuel);
    close(
        masses.payload + masses.fuel,
        grouped.masses.payload + grouped.masses.fuel,
    );
}

#[test]
fn changing_built_geometry_and_engine_thrust_changes_the_same_product_path() {
    let (config, baseline_plane) = preset_case("A320-200");
    let baseline = pure_buildup(&config, &baseline_plane);

    let preset = presets::get("A320-200").expect("A320 preset must exist");
    let mut resized_design = preset.design_vector;
    resized_design.span_m *= 1.05;
    let resized_plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&resized_design), true)
        .expect("modified A320 geometry must build");
    let resized = pure_buildup(&config, &resized_plane);
    assert!((resized.inputs.wing_area_m2 - baseline.inputs.wing_area_m2).abs() > 1.0e-6);
    assert!((resized.masses.wing - baseline.masses.wing).abs() > 1.0e-6);

    let mut thrust_config = config.clone();
    let baseline_thrust_kn = thrust_config.geometry.engine.thrust_kn();
    thrust_config
        .geometry
        .engine
        .set_thrust_kn(baseline_thrust_kn * 1.08)
        .expect("positive turbofan thrust change must be accepted");
    let thrust_plane = AircraftBuilder::new(Some(thrust_config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("modified engine geometry must build");
    let thrust_changed = pure_buildup(&thrust_config, &thrust_plane);
    assert!(
        (thrust_changed.inputs.rated_thrust_per_engine_n
            - baseline.inputs.rated_thrust_per_engine_n)
            .abs()
            > 1.0
    );
    assert!((thrust_changed.masses.propulsion - baseline.masses.propulsion).abs() > 1.0e-6);
}

#[test]
fn atr_turboprop_is_explicitly_unsupported_without_a_fabricated_mass() {
    let (config, plane) = preset_case("ATR72-600");
    let result = calculate_flops_mass_buildup(
        &plane,
        &config.requirements,
        &config.geometry,
        &config.control_surfaces,
        Some(&config.mass_model),
        &config.landing_gear,
        &config.cabin,
    );
    let Err(ComponentMassError::FlopsUnverified { reasons, .. }) = result else {
        panic!("ATR must not publish a jet-equivalent FLOPS total");
    };
    assert!(reasons.contains(&FlopsTransportUnverifiedReason::UnsupportedPropulsionTechnology));
}
