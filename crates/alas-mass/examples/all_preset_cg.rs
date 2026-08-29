// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Generate first-hand weight-and-balance evidence for every aircraft preset.
//!
//! The artifact compares the frozen Python-compatible wing point with the
//! structural-wingbox centroid at OEW, MZFW, MTOW-residual fuel, and the
//! published-usable-fuel limit. Payload remains the mass model's explicit
//! lumped planning load; this example does not claim an operational loading
//! envelope or replace the aircraft WBM.

use std::error::Error;
use std::fs;
use std::path::PathBuf;

use alas_config::{presets, AlasConfig, PlanningMacReference};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{
    calculate_component_masses, calculate_physical_cg, define_mass_coordinates,
    define_mass_coordinates_with_model, MassBreakdown, MassCoordinateModel, MassCoordinates,
};
use alas_mass::wing_centroid::wing_structural_centroid;
use serde_json::{json, Value};

fn load_case(
    base_masses: MassBreakdown,
    coordinates: &MassCoordinates,
    payload_kg: f64,
    fuel_kg: f64,
    mac_le_x_m: f64,
    mac_m: f64,
    planning_reference: Option<PlanningMacReference>,
) -> Value {
    let mut masses = base_masses;
    masses.payload = payload_kg.max(0.0);
    masses.fuel = fuel_kg.max(0.0);
    let cg = calculate_physical_cg(&masses, coordinates);
    let total_mass_kg = masses
        .as_pairs()
        .into_iter()
        .map(|(_, mass_kg)| mass_kg.max(0.0))
        .sum::<f64>();
    json!({
        "mass_kg": total_mass_kg,
        "payload_kg": masses.payload,
        "fuel_kg": masses.fuel,
        "cg_x_m": cg[0],
        "model_cg_percent_mac": 100.0 * (cg[0] - mac_le_x_m) / mac_m,
        "public_planning_cg_percent_mac": planning_reference.map(|reference| {
            100.0 * (cg[0] - reference.lemac_from_aircraft_nose_m)
                / reference.mean_aerodynamic_chord_m
        }),
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut aircraft = Vec::new();
    for preset_name in presets::available() {
        let preset = presets::get(preset_name).map_err(std::io::Error::other)?;
        let mut config = AlasConfig {
            preset: preset.name.to_owned(),
            geometry: preset.geometry.clone(),
            requirements: preset.requirements.clone(),
            landing_gear: preset.landing_gear.clone(),
            ..Default::default()
        };
        if let Some(mass_model) = &preset.mass_model {
            config.mass_model = mass_model.clone();
        }
        config.geometry.engine.apply_engine_spec();

        let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .map_err(|error| {
                std::io::Error::other(format!("failed to build {preset_name}: {error:?}"))
            })?;
        let wing = airplane
            .wings
            .iter()
            .find(|wing| wing.name == "Main Wing")
            .ok_or_else(|| std::io::Error::other(format!("{preset_name} has no main wing")))?;
        let masses = calculate_component_masses(
            &airplane,
            &config.requirements,
            &config.geometry,
            Some(&config.mass_model),
        );
        let reference_coordinates = define_mass_coordinates(
            &airplane,
            &config.geometry,
            Some(&config.requirements),
            Some(&config.mass_model),
        );
        let structural_coordinates = define_mass_coordinates_with_model(
            &airplane,
            &config.geometry,
            Some(&config.requirements),
            Some(&config.mass_model),
            MassCoordinateModel::StructuralWingbox(&config.structures),
        )?;
        let structural_centroid =
            wing_structural_centroid(wing, &config.requirements, &config.structures)?;

        let mac_m = wing.mean_aerodynamic_chord();
        let mac_le_x_m = wing.aerodynamic_center(0.0)[0];
        let payload_kg = masses.payload.max(0.0);
        let mtow_residual_fuel_kg = masses.fuel.max(0.0);
        let published_capacity_kg = preset.reference.usable_fuel_mass_kg;
        let planning_reference = preset
            .reference
            .planning_cg_envelope
            .map(|envelope| envelope.mac_reference);
        let tank_limited_fuel_kg = published_capacity_kg
            .map(|capacity_kg| capacity_kg.min(mtow_residual_fuel_kg))
            .unwrap_or(mtow_residual_fuel_kg);
        let case_set = |coordinates: &MassCoordinates| {
            json!({
                "oew": load_case(
                    masses,
                    coordinates,
                    0.0,
                    0.0,
                    mac_le_x_m,
                    mac_m,
                    planning_reference,
                ),
                "mzfw_planning_payload": load_case(
                    masses,
                    coordinates,
                    payload_kg,
                    0.0,
                    mac_le_x_m,
                    mac_m,
                    planning_reference,
                ),
                "mtow_residual_fuel": load_case(
                    masses,
                    coordinates,
                    payload_kg,
                    mtow_residual_fuel_kg,
                    mac_le_x_m,
                    mac_m,
                    planning_reference,
                ),
                "published_tank_limited_full_fuel": load_case(
                    masses,
                    coordinates,
                    payload_kg,
                    tank_limited_fuel_kg,
                    mac_le_x_m,
                    mac_m,
                    planning_reference,
                ),
            })
        };

        aircraft.push(json!({
            "preset": preset_name,
            "identity": {
                "model": preset.identity.model,
                "weight_variant": preset.identity.weight_variant,
                "engine_model": preset.identity.engine_model,
                "modification_state": preset.identity.modification_state,
                "tank_configuration": preset.identity.tank_configuration,
            },
            "mac_m": mac_m,
            "mac_le_x_m": mac_le_x_m,
            "public_planning_mac_frame": planning_reference.map(|reference| json!({
                "lemac_from_aircraft_nose_m": reference.lemac_from_aircraft_nose_m,
                "mean_aerodynamic_chord_m": reference.mean_aerodynamic_chord_m,
                "source": {
                    "document": reference.source.document,
                    "revision": reference.source.revision,
                    "location": reference.source.location,
                },
            })),
            "planning_payload_kg": payload_kg,
            "mtow_residual_fuel_kg": mtow_residual_fuel_kg,
            "published_usable_fuel_capacity_kg": published_capacity_kg,
            "mtow_residual_fits_published_capacity": published_capacity_kg
                .map(|capacity_kg| mtow_residual_fuel_kg <= capacity_kg),
            "reference_compatibility": {
                "wing_cg_x_m": reference_coordinates.wing[0],
                "wing_model_cg_percent_mac": 100.0
                    * (reference_coordinates.wing[0] - mac_le_x_m) / mac_m,
                "load_cases": case_set(&reference_coordinates),
            },
            "structural_wingbox": {
                "wing_cg_x_m": structural_coordinates.wing[0],
                "wing_model_cg_percent_mac": 100.0
                    * (structural_coordinates.wing[0] - mac_le_x_m) / mac_m,
                "modeled_semiwing_mass_kg": structural_centroid.modeled_semiwing_mass_kg(),
                "normalizing_mass_breakdown_kg": {
                    "spar_caps": structural_centroid.cap_mass_kg,
                    "spar_webs": structural_centroid.web_mass_kg,
                    "wingbox_skins": structural_centroid.skin_mass_kg,
                    "ribs": structural_centroid.rib_mass_kg,
                },
                "load_cases": case_set(&structural_coordinates),
            },
        }));
    }

    let output = json!({
        "status": "preliminary_physical_model_not_certification_evidence",
        "generated_by": "cargo run -p alas-mass --example all_preset_cg",
        "coordinate_models": {
            "reference_compatibility": "Frozen alas/physics/mass.py wing point: aerodynamic center plus 20% root chord.",
            "structural_wingbox": "Integrated first moment of configured spar caps, webs, wingbox skins, and ribs; Torenbeek total wing mass remains unchanged.",
        },
        "load_case_semantics": "OEW excludes payload and fuel; MZFW adds the preset planning payload; MTOW residual adds max(0, MTOW-OEW-payload); published-tank-limited full fuel caps only that residual by the unchanged preset's published usable-fuel mass.",
        "limitations": [
            "The payload coordinate is the mass model's lumped planning coordinate, not a seat/cargo loading envelope.",
            "Published usable-fuel capacity applies only to the unchanged registered preset; edited geometry requires the product's separately labeled geometry estimate.",
            "Model CG and static margin retain the built wing MAC frame. Public planning comparisons use only the source LEMAC/MAC frame registered with that manufacturer envelope.",
            "No result is a certified aircraft CG limit; the applicable AFM/WBM remains authoritative.",
        ],
        "primary_structural_provenance": {
            "source": "NASA Technical Paper 1158, Jernell, 1978",
            "url": "https://ntrs.nasa.gov/citations/19780017136",
            "scope": "Supports deriving wing-box mass properties from structural-design data; the ALAS centroid is a preliminary model, not a reproduction of that aircraft.",
        },
        "aircraft": aircraft,
    });
    let output_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../outputs/all_preset_structural_cg.json");
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output_path, serde_json::to_vec_pretty(&output)?)?;
    Ok(())
}
