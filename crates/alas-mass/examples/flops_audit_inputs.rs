// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Emit the complete resolved input deck used by the pure FLOPS product.
//!
//! This is an audit fixture rather than a production entry point.  It keeps
//! the airframe inputs (including the geometry-derived t/c, tail areas, gear
//! oleo lengths and nacelle dimensions) beside the systems inputs already
//! emitted by `flops_preset_comparison`, so an independent Aviary replay can
//! use exactly the same deck.
//!
//! Usage:
//!
//! ```text
//! cargo run -p alas-mass --example flops_audit_inputs -- outputs/a320-flops-audit/alas-inputs.json
//! ```

#![allow(clippy::print_stdout)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{calculate_flops_mass_buildup, ProductMassBuildup};
use alas_mass::flops_transport::structure::WingBendingFactor;
use serde_json::{json, Value};
use std::{error::Error, fs, path::PathBuf};

fn systems_json(build: &alas_mass::breakdown::FlopsMassBuildup) -> Value {
    let systems = build.systems_and_operating_items.systems;
    let operating = build.systems_and_operating_items.operating_items;
    json!({
        "systems": {
            "surface_controls_kg": systems.surface_controls_kg,
            "apu_kg": systems.apu_kg,
            "instruments_kg": systems.instruments_kg,
            "hydraulics_kg": systems.hydraulics_kg,
            "electrical_kg": systems.electrical_kg,
            "avionics_kg": systems.avionics_kg,
            "furnishings_kg": systems.furnishings_kg,
            "air_conditioning_kg": systems.air_conditioning_kg,
            "anti_ice_kg": systems.anti_ice_kg,
            "total_kg": systems.total_kg,
        },
        "operating_items": {
            "cabin_crew_and_baggage_kg": operating.cabin_crew_and_baggage_kg,
            "flight_crew_and_baggage_kg": operating.flight_crew_and_baggage_kg,
            "unusable_fuel_kg": operating.unusable_fuel_kg,
            "engine_oil_kg": operating.engine_oil_kg,
            "passenger_service_kg": operating.passenger_service_kg,
            "cargo_containers_kg": operating.cargo_containers_kg,
            "total_kg": operating.total_kg,
        },
    })
}

fn bending_json(bending: &WingBendingFactor) -> Value {
    match bending {
        WingBendingFactor::Simplified => json!({"method": "simplified"}),
        WingBendingFactor::Detailed {
            bt,
            bte,
            pod_mass_kg,
        } => json!({
            "method": "detailed",
            "bt": bt,
            "bte": bte,
            "pod_mass_kg": pod_mass_kg,
        }),
    }
}

fn row(name: &str) -> Result<Value, Box<dyn Error>> {
    let preset = presets::get(name).map_err(std::io::Error::other)?;
    let config = AlasConfig::from_value(&json!({"preset": name}))?;
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)?;
    let build = calculate_flops_mass_buildup(
        &plane,
        &config.requirements,
        &config.geometry,
        &config.control_surfaces,
        Some(&config.mass_model),
        &config.landing_gear,
        &config.cabin,
    )?;
    let ProductMassBuildup::PureFlops(build) = build else {
        return Err("audit deck requires the pure FLOPS product".into());
    };
    let structure = build
        .airframe
        .structure
        .as_ref()
        .ok_or("pure FLOPS audit deck has no structural group")?;
    let wing = &build.airframe.structure_inputs.wing;
    let prop = &build.airframe.propulsion_inputs;
    let inputs = &build.inputs;
    Ok(json!({
        "preset": name,
        "identity": {
            "model": preset.identity.model,
            "weight_variant": preset.identity.weight_variant,
            "engine_model": preset.identity.engine_model,
            "modification_state": preset.identity.modification_state,
            "tank_configuration": preset.identity.tank_configuration,
        },
        "systems_deck": systems_json(&build),
        "transport_inputs": {
            "maximum_mach": inputs.maximum_mach,
            "design_range_nmi": inputs.design_range_nmi,
            "design_gross_mass_kg": inputs.design_gross_mass_kg,
            "wing_area_m2": inputs.wing_area_m2,
            "movable_surface_area_m2": inputs.movable_surface_area_m2,
            "wing_span_m": inputs.wing_span_m,
            "quarter_chord_sweep_deg": inputs.quarter_chord_sweep_deg,
            "fuselage_length_m": inputs.fuselage_length_m,
            "fuselage_width_m": inputs.fuselage_width_m,
            "fuselage_depth_m": inputs.fuselage_depth_m,
            "fuselage_count": inputs.fuselage_count,
            "passenger_compartment_length_m": inputs.passenger_compartment_length_m,
            "first_class_passenger_count": inputs.first_class_passenger_count,
            "business_class_passenger_count": inputs.business_class_passenger_count,
            "tourist_class_passenger_count": inputs.tourist_class_passenger_count,
            "flight_crew_count": inputs.flight_crew_count,
            "flight_attendant_count": inputs.flight_attendant_count,
            "galley_crew_count": inputs.galley_crew_count,
            "wing_mounted_engine_count": inputs.wing_mounted_engine_count,
            "fuselage_mounted_engine_count": inputs.fuselage_mounted_engine_count,
            "engine_count": inputs.engine_count,
            "rated_thrust_per_engine_n": inputs.rated_thrust_per_engine_n,
            "nacelle_diameter_m": inputs.nacelle_diameter_m,
            "hydraulic_pressure_pa": inputs.hydraulic_pressure_pa,
            "variable_sweep_penalty": inputs.variable_sweep_penalty,
            "maximum_fuel_capacity_kg": inputs.maximum_fuel_capacity_kg,
            "fuel_tank_count": inputs.fuel_tank_count,
            "containerized_cargo_kg": inputs.containerized_cargo_kg,
        },
        "structure_inputs": {
            "wing": {
                "design_gross_mass_kg": wing.design_gross_mass_kg,
                "wing_area_m2": wing.wing_area_m2,
                "wing_span_m": wing.wing_span_m,
                "taper_ratio": wing.taper_ratio,
                "quarter_chord_sweep_deg": wing.quarter_chord_sweep_deg,
                "thickness_to_chord": wing.thickness_to_chord,
                "movable_surface_area_m2": wing.movable_surface_area_m2,
                "ultimate_load_factor": wing.ultimate_load_factor,
                "composite_utilization": wing.composite_utilization,
                "aeroelastic_tailoring": wing.aeroelastic_tailoring,
                "strut_bracing": wing.strut_bracing,
                "wing_load_fraction": wing.wing_load_fraction,
                "fuselage_count": wing.fuselage_count,
                "variable_sweep_penalty": wing.variable_sweep_penalty,
                "wing_mounted_engine_count": wing.wing_mounted_engine_count,
                "bending": bending_json(&wing.bending),
            },
            "horizontal_tail_area_m2": build.airframe.structure_inputs.horizontal_tail_area_m2,
            "horizontal_tail_taper_ratio": build.airframe.structure_inputs.horizontal_tail_taper_ratio,
            "vertical_tail_area_m2": build.airframe.structure_inputs.vertical_tail_area_m2,
            "vertical_tail_taper_ratio": build.airframe.structure_inputs.vertical_tail_taper_ratio,
            "vertical_tail_count": build.airframe.structure_inputs.vertical_tail_count,
            "fuselage_length_m": build.airframe.structure_inputs.fuselage_length_m,
            "fuselage_width_m": build.airframe.structure_inputs.fuselage_width_m,
            "fuselage_depth_m": build.airframe.structure_inputs.fuselage_depth_m,
            "scaled_fuselage_engines": build.airframe.structure_inputs.scaled_fuselage_engines,
            "military_cargo_floor": build.airframe.structure_inputs.military_cargo_floor,
            "design_landing_mass_kg": build.airframe.structure_inputs.design_landing_mass_kg,
            "main_gear_oleo_length_m": build.airframe.structure_inputs.main_gear_oleo_length_m,
            "nose_gear_oleo_length_m": build.airframe.structure_inputs.nose_gear_oleo_length_m,
            "total_nacelles": build.airframe.structure_inputs.total_nacelles,
            "nacelle_diameter_m": build.airframe.structure_inputs.nacelle_diameter_m,
            "nacelle_length_m": build.airframe.structure_inputs.nacelle_length_m,
            "rated_thrust_per_engine_n": build.airframe.structure_inputs.rated_thrust_per_engine_n,
            "paint_area_density_kg_m2": build.airframe.structure_inputs.paint_area_density_kg_m2,
            "painted_wetted_area_m2": build.airframe.structure_inputs.painted_wetted_area_m2,
        },
        "propulsion_inputs": {
            "engine_count": prop.engine_count,
            "wing_mounted_engine_count": prop.wing_mounted_engine_count,
            "fuselage_mounted_engine_count": prop.fuselage_mounted_engine_count,
            "rated_thrust_per_engine_n": prop.rated_thrust_per_engine_n,
            "baseline_thrust_n": prop.baseline_thrust_n,
            "baseline_engine_mass_kg": prop.baseline_engine_mass_kg,
            "scaling_exponent": prop.scaling_exponent,
            "baseline_inlet_mass_kg": prop.baseline_inlet_mass_kg,
            "inlet_scaling_exponent": prop.inlet_scaling_exponent,
            "baseline_nozzle_mass_kg": prop.baseline_nozzle_mass_kg,
            "nozzle_scaling_exponent": prop.nozzle_scaling_exponent,
            "thrust_reversers_installed": prop.thrust_reversers_installed,
            "maximum_mach": prop.maximum_mach,
            "nacelle_diameter_m": prop.nacelle_diameter_m,
            "maximum_fuel_capacity_kg": prop.maximum_fuel_capacity_kg,
            "misc_propulsion_mass_kg": prop.misc_propulsion_mass_kg,
        },
        "sources": {
            "landing_mass": build.airframe.sources.landing_mass,
            "main_gear_length": build.airframe.sources.main_gear_length,
            "nose_gear_length": build.airframe.sources.nose_gear_length,
            "baseline_engine_mass": build.airframe.sources.baseline_engine_mass,
        },
        "groups": {
            "structure": {
                "wing": {
                    "bending_factor": structure.wing.bending_factor,
                    "inertia_relief_factor": structure.wing.inertia_relief_factor,
                    "bending_material_kg": structure.wing.bending_material_kg,
                    "shear_and_control_kg": structure.wing.shear_and_control_kg,
                    "miscellaneous_kg": structure.wing.miscellaneous_kg,
                    "total_kg": structure.wing.total_kg,
                },
                "horizontal_tail_kg": structure.horizontal_tail_kg,
                "vertical_tail_kg": structure.vertical_tail_kg,
                "fuselage_kg": structure.fuselage_kg,
                "main_gear_kg": structure.main_gear_kg,
                "nose_gear_kg": structure.nose_gear_kg,
                "nacelle_kg": structure.nacelle_kg,
                "paint_kg": structure.paint_kg,
                "total_kg": structure.total_kg,
            },
            "propulsion": build.airframe.propulsion.as_ref().map(|p| json!({
                "total_nacelles": p.total_nacelles,
                "baseline_engine_mass_kg": p.baseline_engine_mass_kg,
                "engine_core_each_kg": p.engine_core_each_kg,
                "inlet_each_kg": p.inlet_each_kg,
                "nozzle_each_kg": p.nozzle_each_kg,
                "engine_each_kg": p.engine_each_kg,
                "engine_cores_kg": p.engine_cores_kg,
                "inlets_kg": p.inlets_kg,
                "nozzles_kg": p.nozzles_kg,
                "engines_kg": p.engines_kg,
                "thrust_reversers_kg": p.thrust_reversers_kg,
                "engine_controls_kg": p.engine_controls_kg,
                "starters_kg": p.starters_kg,
                "misc_kg": p.misc_kg,
                "fuel_system_kg": p.fuel_system_kg,
                "total_kg": p.total_kg,
            })),
            "turboprop_propulsion": build.airframe.turboprop_propulsion.as_ref().map(|p| json!({
                "engine_mass_source": p.engine_mass_source,
                "engine_each_kg": p.engine_each_kg,
                "engines_kg": p.engines_kg,
                "gearboxes_kg": p.gearboxes_kg,
                "propeller_each_kg": p.propeller_each_kg,
                "propellers_kg": p.propellers_kg,
                "nacelles_kg": p.nacelles_kg,
                "pylons_kg": p.pylons_kg,
                "engine_installation_kg": p.engine_installation_kg,
                "fuel_system_kg": p.fuel_system_kg,
                "unusable_fuel_kg": p.unusable_fuel_kg,
                "total_without_nacelles_kg": p.total_without_nacelles_kg,
            })),
            "nacelle_kg": build.nacelle_kg(),
            "propulsion_without_nacelles_kg": build.propulsion_without_nacelles_kg(),
            "masses_kg": {
                "wing": build.masses.wing,
                "h_stab": build.masses.h_stab,
                "v_stab": build.masses.v_stab,
                "fuselage": build.masses.fuselage,
                "gear": build.masses.gear,
                "propulsion": build.masses.propulsion,
                "systems": build.masses.systems,
                "furnishings": build.masses.furnishings,
                "payload": build.masses.payload,
                "fuel": build.masses.fuel,
            },
        },
    }))
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let output = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("outputs/a320-flops-audit/alas-inputs.json"));
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    // Any remaining arguments name the presets to audit. With none given the
    // deck covers every registered preset, so a preset that the pure FLOPS
    // product cannot evaluate is recorded with its refusal rather than
    // aborting the whole audit.
    let requested: Vec<String> = args.collect();
    let names: Vec<String> = if requested.is_empty() {
        presets::available()
            .into_iter()
            .map(str::to_owned)
            .collect()
    } else {
        requested
    };
    let rows: Vec<Value> = names
        .iter()
        .map(|name| match row(name) {
            Ok(value) => value,
            Err(error) => json!({
                "preset": name,
                "status": "unavailable",
                "reason": error.to_string(),
            }),
        })
        .collect();
    let report = json!({
        "schema_version": 2,
        "generated_by": "cargo run -p alas-mass --example flops_audit_inputs -- <out.json> [preset...]",
        "architecture": "pure_flops_transport_v1",
        "aircraft": rows,
    });
    fs::write(&output, serde_json::to_string_pretty(&report)? + "\n")?;
    println!("{}", output.display());
    Ok(())
}
