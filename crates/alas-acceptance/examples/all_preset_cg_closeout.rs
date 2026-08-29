// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Generate all-preset CG closeout evidence through the real two-pass payload path.

use std::error::Error;
use std::fs;
use std::path::PathBuf;

use alas_config::{presets, AlasConfig, PlanningMacReference};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{
    calculate_physical_cg, run_mass_analysis, run_mass_analysis_with_model, MassBreakdown,
    MassCoordinateModel, MassCoordinates, PayloadLayoutSummary,
};
use alas_payload::build::build_payload_layout;
use alas_payload::oew::oew_and_cg;
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
    let mass_kg = masses
        .as_pairs()
        .into_iter()
        .map(|(_, value)| value.max(0.0))
        .sum::<f64>();
    json!({
        "mass_kg": mass_kg,
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

fn cases(
    masses: MassBreakdown,
    coordinates: &MassCoordinates,
    tank_capacity_kg: Option<f64>,
    mac_le_x_m: f64,
    mac_m: f64,
    planning_reference: Option<PlanningMacReference>,
) -> Value {
    let payload_kg = masses.payload.max(0.0);
    let mtow_residual_fuel_kg = masses.fuel.max(0.0);
    let tank_limited_fuel_kg = tank_capacity_kg
        .map(|capacity_kg| capacity_kg.min(mtow_residual_fuel_kg))
        .unwrap_or(mtow_residual_fuel_kg);
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
        "mzfw_detailed_payload": load_case(
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
}

fn payload_summary(
    airplane: &alas_geom::aircraft::airplane::Airplane,
    config: &AlasConfig,
    masses: &MassBreakdown,
    coordinates: &MassCoordinates,
) -> Result<PayloadLayoutSummary, Box<dyn Error>> {
    let (oew_kg, oew_cg_x_m) = oew_and_cg(masses, coordinates);
    let layout = build_payload_layout(airplane, config, oew_kg, oew_cg_x_m)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    Ok(PayloadLayoutSummary {
        total_mass: layout.total_mass,
        cg_x: layout.cg_x,
        cg_y: layout.cg_y,
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut aircraft = Vec::new();
    for name in presets::available() {
        let preset = presets::get(name).map_err(std::io::Error::other)?;
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
        if let Some(performance) = &preset.performance {
            config.performance = performance.clone();
        }
        config.geometry.engine.apply_engine_spec();
        let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .map_err(|error| std::io::Error::other(format!("{name}: {error:?}")))?;
        let wing = airplane
            .wings
            .iter()
            .find(|wing| wing.name == "Main Wing")
            .ok_or_else(|| std::io::Error::other(format!("{name}: no main wing")))?;
        let mac_m = wing.mean_aerodynamic_chord();
        let mac_le_x_m = wing.aerodynamic_center(0.0)[0];
        let tank_capacity_kg = preset.reference.usable_fuel_mass_kg;
        let planning_reference = preset
            .reference
            .planning_cg_envelope
            .map(|envelope| envelope.mac_reference);

        let (reference_initial_masses, reference_initial_coordinates, _) = run_mass_analysis(
            &airplane,
            &config.requirements,
            &config.geometry,
            Some(&config.mass_model),
            None,
        );
        let reference_payload = payload_summary(
            &airplane,
            &config,
            &reference_initial_masses,
            &reference_initial_coordinates,
        )?;
        let (reference_masses, reference_coordinates, reference_cg) = run_mass_analysis(
            &airplane,
            &config.requirements,
            &config.geometry,
            Some(&config.mass_model),
            Some(&reference_payload),
        );

        let coordinate_model = MassCoordinateModel::StructuralWingbox(&config.structures);
        let (structural_initial_masses, structural_initial_coordinates, _) =
            run_mass_analysis_with_model(
                &airplane,
                &config.requirements,
                &config.geometry,
                Some(&config.mass_model),
                None,
                coordinate_model,
            )?;
        let structural_payload = payload_summary(
            &airplane,
            &config,
            &structural_initial_masses,
            &structural_initial_coordinates,
        )?;
        let (structural_masses, structural_coordinates, structural_cg) =
            run_mass_analysis_with_model(
                &airplane,
                &config.requirements,
                &config.geometry,
                Some(&config.mass_model),
                Some(&structural_payload),
                coordinate_model,
            )?;

        aircraft.push(json!({
            "preset": name,
            "published_usable_fuel_capacity_kg": tank_capacity_kg,
            "model_mac_frame": {
                "lemac_from_aircraft_nose_m": mac_le_x_m,
                "mean_aerodynamic_chord_m": mac_m,
            },
            "public_planning_mac_frame": planning_reference.map(|reference| json!({
                "lemac_from_aircraft_nose_m": reference.lemac_from_aircraft_nose_m,
                "mean_aerodynamic_chord_m": reference.mean_aerodynamic_chord_m,
                "source": {
                    "document": reference.source.document,
                    "revision": reference.source.revision,
                    "location": reference.source.location,
                },
            })),
            "reference_compatibility": {
                "wing_model_cg_percent_mac": 100.0
                    * (reference_coordinates.wing[0] - mac_le_x_m) / mac_m,
                "payload_model_cg_percent_mac": 100.0
                    * (reference_coordinates.payload[0] - mac_le_x_m) / mac_m,
                "loaded_model_cg_percent_mac": 100.0
                    * (reference_cg[0] - mac_le_x_m) / mac_m,
                "loaded_public_planning_cg_percent_mac": planning_reference.map(|reference| {
                    100.0 * (reference_cg[0] - reference.lemac_from_aircraft_nose_m)
                        / reference.mean_aerodynamic_chord_m
                }),
                "load_cases": cases(
                    reference_masses,
                    &reference_coordinates,
                    tank_capacity_kg,
                    mac_le_x_m,
                    mac_m,
                    planning_reference,
                ),
            },
            "structural_wingbox": {
                "wing_model_cg_percent_mac": 100.0
                    * (structural_coordinates.wing[0] - mac_le_x_m) / mac_m,
                "payload_model_cg_percent_mac": 100.0
                    * (structural_coordinates.payload[0] - mac_le_x_m) / mac_m,
                "loaded_model_cg_percent_mac": 100.0
                    * (structural_cg[0] - mac_le_x_m) / mac_m,
                "loaded_public_planning_cg_percent_mac": planning_reference.map(|reference| {
                    100.0 * (structural_cg[0] - reference.lemac_from_aircraft_nose_m)
                        / reference.mean_aerodynamic_chord_m
                }),
                "load_cases": cases(
                    structural_masses,
                    &structural_coordinates,
                    tank_capacity_kg,
                    mac_le_x_m,
                    mac_m,
                    planning_reference,
                ),
            },
            "loaded_model_cg_shift_percent_mac": 100.0
                * (structural_cg[0] - reference_cg[0]) / mac_m,
            "loaded_public_planning_cg_shift_percent_mac": planning_reference.map(|reference| {
                100.0 * (structural_cg[0] - reference_cg[0])
                    / reference.mean_aerodynamic_chord_m
            }),
            "mtow_residual_fits_published_capacity": tank_capacity_kg.map(|capacity_kg| {
                structural_masses.fuel.max(0.0) <= capacity_kg
            }),
        }));
    }

    let output = json!({
        "status": "preliminary_model_evidence_not_afm_wbm_limits",
        "generated_by": "cargo run -p alas-acceptance --example all_preset_cg_closeout",
        "comparison": "Both paths run the real two-pass cabin/cargo builder. The reference path preserves alas/physics/mass.py; the product path integrates the configured structural wingbox. Component mass correlations and preset values are unchanged.",
        "tank_semantics": "For an unchanged registered aircraft, the published usable-fuel mass is authoritative. MTOW-residual fuel is separately reported and capped only in the tank-limited loading case; edited designs use the product's separately labeled geometry estimate.",
        "limitations": [
            "The four states are planning OEW/MZFW/MTOW/full-fuel cases, not an operational loading envelope.",
            "Seat, cargo, and ULD placements come from the current detailed payload model; operator WBM data remain authoritative.",
            "Model CG and static margin retain the built wing MAC frame. Public planning comparisons use only the source LEMAC/MAC frame registered with that manufacturer envelope.",
            "The structural component masses normalize the wing first moment only; Torenbeek remains the aircraft wing total-mass correlation.",
        ],
        "aircraft": aircraft,
    });
    let output_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../outputs/all_preset_cg_closeout.json");
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output_path, serde_json::to_vec_pretty(&output)?)?;
    Ok(())
}
