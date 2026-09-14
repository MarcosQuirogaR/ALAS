// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Write a transparent cross-preset systems-mass audit.
//!
//! This compatibility audit measures the explicitly selected legacy
//! reference-compatible ALAS mass fractions against the separately translated
//! mission analysis model subsystem correlation without connecting either
//! result to the pure-FLOPS product feasibility path. Its two mission analysis
//! model accessory cases make the reference bridge's known `"long range"`
//! versus `"long-range"` mismatch visible.

use std::error::Error;
use std::fs;
use std::path::PathBuf;

use alas_config::{presets, AlasConfig, MassArchitecture, SystemsMassMethod};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{
    calculate_component_masses, calculate_component_masses_checked, ComponentMassError,
};
use alas_mass::transport_weight::{
    estimate_operating_items, estimate_systems, AccessoriesType, ControlSystemType,
};
use serde_json::{json, Value};

fn passenger_count(value: i64, preset: &str) -> Result<u32, std::io::Error> {
    u32::try_from(value).map_err(|_| {
        std::io::Error::other(format!("{preset} has an invalid passenger count {value}"))
    })
}

fn systems_json(breakdown: alas_mass::transport_weight::SystemsBreakdown) -> Value {
    json!({
        "control_systems_kg": breakdown.control_systems_kg,
        "apu_kg": breakdown.apu_kg,
        "electrical_kg": breakdown.electrical_kg,
        "avionics_kg": breakdown.avionics_kg,
        "hydraulics_kg": breakdown.hydraulics_kg,
        "furnishings_kg": breakdown.furnish_kg,
        "air_conditioning_kg": breakdown.air_conditioner_kg,
        "instruments_kg": breakdown.instruments_kg,
        "total_systems_and_equipment_kg": breakdown.total_kg,
    })
}

fn operating_items_json(breakdown: alas_mass::transport_weight::OperationalItems) -> Value {
    json!({
        "operating_items_less_crew_kg": breakdown.operating_items_less_crew_kg,
        "flight_crew_kg": breakdown.flight_crew_kg,
        "flight_attendants_kg": breakdown.flight_attendants_kg,
        "total_operating_items_kg": breakdown.total_kg,
    })
}

fn flops_selection_json(
    airplane: &alas_geom::aircraft::airplane::Airplane,
    config: &AlasConfig,
) -> Value {
    let mut mass_model = config.mass_model.clone();
    mass_model.mass_architecture = MassArchitecture::PureFlopsTransportV1;
    mass_model.systems_mass_method = SystemsMassMethod::FlopsTransportV1;
    mass_model.apply_architecture();
    match calculate_component_masses_checked(
        airplane,
        &config.requirements,
        &config.geometry,
        &config.cabin,
        &config.control_surfaces,
        Some(&mass_model),
    ) {
        Ok(masses) => json!({
            "method": "flops_transport_v1",
            "status": "verified",
            "systems_kg": masses.systems,
            "furnishings_and_operating_items_kg": masses.furnishings,
        }),
        Err(ComponentMassError::FlopsUnverified { reasons, .. }) => json!({
            "method": "flops_transport_v1",
            "status": "unverified",
            "reasons": reasons.iter().map(|reason| reason.as_str()).collect::<Vec<_>>(),
            "fallback_used": false,
        }),
        Err(ComponentMassError::Geometry(error)) => json!({
            "method": "flops_transport_v1",
            "status": "unverified",
            "reasons": [format!("geometry: {error}")],
            "fallback_used": false,
        }),
        Err(ComponentMassError::FlopsIncompleteAirframe) => json!({
            "method": "flops_transport_v1",
            "status": "unverified",
            "reasons": ["incomplete_flops_airframe"],
            "fallback_used": false,
        }),
        Err(ComponentMassError::IncoherentMassArchitecture { .. }) => json!({
            "method": "flops_transport_v1",
            "status": "unverified",
            "reasons": ["incoherent_mass_architecture"],
            "fallback_used": false,
        }),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut aircraft = Vec::new();
    for preset_name in presets::available() {
        let preset = presets::get(preset_name).map_err(std::io::Error::other)?;
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset_name }))?;

        let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .map_err(|error| {
                std::io::Error::other(format!("failed to build {preset_name}: {error:?}"))
            })?;
        let main_wing = airplane
            .wings
            .iter()
            .find(|wing| wing.name == "Main Wing")
            .ok_or_else(|| std::io::Error::other(format!("{preset_name} has no main wing")))?;
        let hstab = airplane
            .wings
            .iter()
            .find(|wing| wing.name == "Horizontal Stabilizer")
            .ok_or_else(|| {
                std::io::Error::other(format!("{preset_name} has no horizontal stabilizer"))
            })?;
        let vstab = airplane
            .wings
            .iter()
            .find(|wing| wing.name == "Vertical Stabilizer")
            .ok_or_else(|| {
                std::io::Error::other(format!("{preset_name} has no vertical stabilizer"))
            })?;
        let passengers = passenger_count(config.requirements.num_passengers, preset_name)?;
        // The audit reports projected aircraft planform for the main and
        // horizontal tail. A vertical fin lies in XZ, so its physical fin
        // planform must remain explicit rather than collapsing under XY
        // projection.
        let tail_area_m2 = hstab.reference_area() + vstab.unfolded_area();

        let frozen_mission = estimate_systems(
            passengers,
            ControlSystemType::FullyPowered,
            AccessoriesType::Other,
            airplane.s_ref,
            tail_area_m2,
            main_wing.reference_area(),
        );
        let intended_long_range = estimate_systems(
            passengers,
            ControlSystemType::FullyPowered,
            AccessoriesType::LongRange,
            airplane.s_ref,
            tail_area_m2,
            main_wing.reference_area(),
        );
        let frozen_operating_items = estimate_operating_items(passengers, AccessoriesType::Other);
        let mut legacy_mass_model = config.mass_model.clone();
        legacy_mass_model.mass_architecture = MassArchitecture::LegacyReferenceCompatibleComparison;
        legacy_mass_model.apply_architecture();
        let product = calculate_component_masses(
            &airplane,
            &config.requirements,
            &config.geometry,
            Some(&legacy_mass_model),
        );
        let product_group_kg = product.systems + product.furnishings;
        let default_systems_kg = alas_config::MassModelConfig::default().systems_mass_fraction
            * config.requirements.mtow_kg;
        let default_furnishings_kg = alas_config::MassModelConfig::default()
            .furnishings_mass_fraction
            * config.requirements.mtow_kg;
        let default_group_kg = default_systems_kg + default_furnishings_kg;
        let frozen_mission_group_kg = frozen_mission.total_kg + frozen_operating_items.total_kg;
        let fraction_method_status = if preset_name == "A220-300" {
            "compatibility_only_outcome_calibration"
        } else {
            "reference_compatible_fraction_baseline"
        };

        aircraft.push(json!({
            "preset": preset_name,
            "identity": {
                "model": preset.identity.model,
                "weight_variant": preset.identity.weight_variant,
                "engine_model": preset.identity.engine_model,
            },
            "inputs": {
                "mtow_kg": config.requirements.mtow_kg,
                "passenger_count": passengers,
                "main_wing_area_m2": main_wing.reference_area(),
                "tail_area_m2": tail_area_m2,
            },
            "product_fraction_buildup": {
                "method": "reference_compatible_mass_fraction_of_mtow",
                "method_status": fraction_method_status,
                "mass_architecture": legacy_mass_model.mass_architecture.as_str(),
                "systems_fraction_of_mtow": legacy_mass_model.systems_mass_fraction,
                "furnishings_fraction_of_mtow": legacy_mass_model.furnishings_mass_fraction,
                "systems_kg": product.systems,
                "furnishings_and_operations_kg": product.furnishings,
                "combined_kg": product_group_kg,
                "combined_fraction_of_mtow": product_group_kg / config.requirements.mtow_kg,
                "oew_kg": product.wing + product.h_stab + product.v_stab + product.fuselage
                    + product.gear + product.propulsion + product_group_kg,
                "published_oew_kg": preset.reference.oew_kg,
                "oew_difference_vs_published_kg": preset.reference.oew_kg.map(|published| {
                    product.wing + product.h_stab + product.v_stab + product.fuselage
                        + product.gear + product.propulsion + product_group_kg - published
                }),
                "global_default_fraction_counterfactual": {
                    "systems_kg": default_systems_kg,
                    "furnishings_and_operations_kg": default_furnishings_kg,
                    "combined_kg": default_group_kg,
                    "delta_to_selected_preset_kg": product_group_kg - default_group_kg,
                    "purpose": "Shows a preset mass-model override without asserting that either result is a source-backed subsystem mass.",
                },
            },
            "translated_mission_correlation": {
                "product_wired": false,
                "scope": "Correlation projection from the product-built planform and preset passenger count; not a mission analysis model mission execution or a validated replacement mass.",
                "reference_bridge_accessories": "Other (the source bridge supplies the unmatched string long range)",
                "systems": systems_json(frozen_mission),
                "operating_items": operating_items_json(frozen_operating_items),
                "systems_plus_operating_items_kg": frozen_mission_group_kg,
                "systems_plus_operating_items_fraction_of_mtow": frozen_mission_group_kg / config.requirements.mtow_kg,
                "difference_from_product_combined_kg": frozen_mission_group_kg - product_group_kg,
                "long_range_accessory_counterfactual": {
                    "systems": systems_json(intended_long_range),
                    "delta_from_reference_bridge_systems_kg": intended_long_range.total_kg - frozen_mission.total_kg,
                    "purpose": "Makes the documented reference string mismatch visible only; it is not applied to a product result.",
                },
            },
            "flops_product_selection": flops_selection_json(&airplane, &config),
        }));
    }

    let output = json!({
        "status": "compatibility_audit_with_explicit_flops_boundary",
        "generated_by": "cargo run -p alas-mass --example systems_mass_audit",
        "findings": [
            "The compatibility product column is the explicitly selected legacy reference-compatible buildup; it is not the pure-FLOPS production result.",
            "The legacy buildup uses Torenbeek-style wing, stabilizer and fuselage terms plus its configured fraction groups.",
            "The translated mission analysis model subsystem correlation is independently parity-tested but remains diagnostic only.",
            "The NASA FLOPS transport method has a checked product entry point; missing architecture data are typed as unverified and never fall back to fractions.",
            "The source mission analysis model bridge's long range accessory string falls through to Other; the counterfactual records the magnitude without correcting or using it.",
            "The A220 preset's 13% systems and 12% furnishings fractions close an OEW gap; that is outcome calibration, not independent subsystem evidence.",
        ],
        "method_provenance": {
            "product": "alas/physics/mass.py calculate_component_masses; explicit legacy_reference_compatible_comparison baseline",
            "mission": "mission analysis model 2.5.2 New mission analysis model transport systems and operating-items correlations; translated at Tier::Closed",
            "flops_transport_v1": {
                "source": "NASA/TM-2017-219627/Vol. I, Wells, Horvath, McCullers, 2017",
                "url": "https://ntrs.nasa.gov/api/citations/20170005851/downloads/20170005851.pdf",
                "relevant_pages": "pp.36-41 (PDF pp.41-46): transport surface controls, APU, instruments, hydraulics, electrical, avionics, furnishings, air conditioning, anti-ice",
                "scope": "NASA documents a component buildup driven by geometry, maximum Mach, design range, crew, passenger cabin/class, engines, fuel capacity, and system architecture. The Rust checked entry point evaluates it only with declared inputs and returns typed unverified blockers otherwise; it is not selected by the frozen default presets.",
            },
        },
        "category_boundary": "The product combines furnishings with operational items, while mission analysis model keeps cabin furnishings inside systems and exposes operating items separately. The two combined totals are diagnostic only and not interchangeable certified OEW groups.",
        "aircraft": aircraft,
    });
    let output_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../outputs/systems_mass_audit.json");
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output_path, serde_json::to_vec_pretty(&output)?)?;
    Ok(())
}
