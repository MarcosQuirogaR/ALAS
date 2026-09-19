// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The same aircraft, same geometry, same cabin, priced once the way a
//! registered preset is priced and once the way a configuration built without
//! a preset is priced — component by component.
//!
//! This exists because the two used to disagree. `declared_cabin_equipment_method`
//! was reached only from the preset loader, so a configuration built from
//! `FlopsTransportConfig::working_default()` got the published FLOPS cabin
//! equations and a full unit-load-device charge, while every registered preset
//! above the LTH domain threshold got the LTH relations and no charge. The
//! identical 350-seat, 358.7 t aircraft came out **16,515 kg (+10.0 %)**
//! heavier as a preset than as a clean sheet, and any objective that compares
//! a preset-derived design against a clean-sheet one was comparing two
//! accounting systems rather than two aircraft.
//!
//! What this probe reports is the residual after the two paths were put on one
//! rule. A nonzero residual here is a finding, not a tolerance: the two modes
//! describe the same airframe.
//!
//! Usage:
//!
//! ```text
//! cargo run -p alas-mass --example cross_mode_cabin_method_parity -- out.json
//! ```

#![allow(clippy::print_stdout)]

use alas_config::{presets, AlasConfig, FlopsTransportConfig};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{
    calculate_flops_mass_buildup, FlopsMassBuildup, ProductMassBuildup, OEW_KEYS,
};
use serde_json::{json, Value};
use std::{error::Error, fs, path::PathBuf};

/// Every component the two modes could possibly differ in, in kilograms.
fn components(build: &FlopsMassBuildup) -> Vec<(&'static str, f64)> {
    let systems = build.systems_and_operating_items.systems;
    let items = build.systems_and_operating_items.operating_items;
    vec![
        ("furnishings", systems.furnishings_kg),
        ("air_conditioning", systems.air_conditioning_kg),
        ("surface_controls", systems.surface_controls_kg),
        ("apu", systems.apu_kg),
        ("instruments", systems.instruments_kg),
        ("hydraulics", systems.hydraulics_kg),
        ("electrical", systems.electrical_kg),
        ("avionics", systems.avionics_kg),
        ("anti_ice", systems.anti_ice_kg),
        ("cabin_crew_and_baggage", items.cabin_crew_and_baggage_kg),
        ("flight_crew_and_baggage", items.flight_crew_and_baggage_kg),
        ("passenger_service", items.passenger_service_kg),
        ("unusable_fuel", items.unusable_fuel_kg),
        ("engine_oil", items.engine_oil_kg),
        // Reported, and outside operating empty mass on both sides. It is
        // listed here because it is exactly the quantity that used to move
        // silently with the method selection.
        ("cargo_containers_outside_oew", items.cargo_containers_kg),
    ]
}

fn oew_kg(build: &FlopsMassBuildup) -> f64 {
    OEW_KEYS
        .iter()
        .filter_map(|name| build.masses.get(name))
        .sum()
}

fn evaluate(config: &AlasConfig) -> Result<FlopsMassBuildup, String> {
    let design_vector = presets::get(&config.preset)
        .map(|entry| entry.design_vector)
        .ok();
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(design_vector.as_ref(), true)
        .map_err(|error| error.to_string())?;
    match calculate_flops_mass_buildup(
        &plane,
        &config.requirements,
        &config.geometry,
        &config.control_surfaces,
        Some(&config.mass_model),
        &config.landing_gear,
        &config.cabin,
    )
    .map_err(|error| error.to_string())?
    {
        ProductMassBuildup::PureFlops(build) => Ok(*build),
        other => Err(format!("not a pure-FLOPS buildup: {other:?}")),
    }
}

/// The same configuration with the cabin-equipment method replaced by the one
/// a configuration built without a preset receives.
///
/// Only that field is substituted, and deliberately. The haul class and the
/// cargo-hold loading are *declared aircraft properties* — an A320-200 is a
/// short/medium-haul bulk-loaded aircraft whether or not a preset says so —
/// and `FlopsTransportConfig::working_default()` states its own because it is
/// a 350-seat, 7,600 nmi containerised study scenario, not because the mode
/// derived them. Substituting those would compare two different aircraft and
/// report the difference as an accounting defect.
///
/// The cabin-equipment method is the one field that used to be *derived*, and
/// derived by only one of the two paths. Everything else is held: same
/// geometry, same design vector, same requirements, same cabin, same engines.
fn as_clean_sheet(config: &AlasConfig) -> AlasConfig {
    let clean_sheet = FlopsTransportConfig::working_default();
    let mut config = config.clone();
    config.mass_model.flops_transport.cabin_equipment_method = clean_sheet.cabin_equipment_method;
    config
}

/// What the method selection is worth on this aircraft, whichever way it goes.
///
/// Reported because a residual of zero only means something next to the
/// magnitude it would have had: on the AVE study configuration the two paths
/// used to differ by 16,515 kg, and a reader needs to see that the parity
/// above is a real agreement rather than a quantity that was always small.
fn method_selection_gap_kg(config: &AlasConfig) -> Option<f64> {
    use alas_config::CabinEquipmentMethod;
    let mut flops = config.clone();
    flops.mass_model.flops_transport.cabin_equipment_method =
        CabinEquipmentMethod::FlopsTransportV1;
    let mut lth = config.clone();
    lth.mass_model.flops_transport.cabin_equipment_method =
        CabinEquipmentMethod::LthCivilTransportV1;
    match (evaluate(&flops), evaluate(&lth)) {
        (Ok(left), Ok(right)) => Some(oew_kg(&right) - oew_kg(&left)),
        _ => None,
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let output_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("outputs/cross-mode-cabin-method-parity.json"));
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut records = Vec::new();
    let mut worst_residual_kg: f64 = 0.0;
    println!(
        "{:<11} {:>12} {:>12} {:>10} {:>8} {:>11}  {}",
        "preset",
        "preset OEW",
        "clean OEW",
        "delta kg",
        "delta %",
        "at stake kg",
        "components that differ"
    );
    for name in presets::available() {
        let preset_config = AlasConfig::from_value(&json!({ "preset": name }))?;
        let clean_sheet_config = as_clean_sheet(&preset_config);
        let (preset_build, clean_build) =
            match (evaluate(&preset_config), evaluate(&clean_sheet_config)) {
                (Ok(left), Ok(right)) => (left, right),
                (left, right) => {
                    records.push(json!({
                        "preset": name,
                        "preset_mode_error": left.err(),
                        "clean_sheet_mode_error": right.err(),
                    }));
                    continue;
                }
            };

        let preset_oew = oew_kg(&preset_build);
        let clean_oew = oew_kg(&clean_build);
        let residual_kg = preset_oew - clean_oew;
        worst_residual_kg = worst_residual_kg.max(residual_kg.abs());

        let mut differing = Vec::new();
        for ((component, preset_kg), (_, clean_kg)) in components(&preset_build)
            .into_iter()
            .zip(components(&clean_build))
        {
            if (preset_kg - clean_kg).abs() > 1.0e-9 {
                differing.push(json!({
                    "component": component,
                    "preset_mode_kg": preset_kg,
                    "clean_sheet_mode_kg": clean_kg,
                    "delta_kg": preset_kg - clean_kg,
                    "inside_operating_empty_mass": component != "cargo_containers_outside_oew",
                }));
            }
        }
        let labels = differing
            .iter()
            .filter_map(|entry| entry["component"].as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let at_stake_kg = method_selection_gap_kg(&preset_config);
        println!(
            "{:<11} {:>12.1} {:>12.1} {:>10.1} {:>7.2}% {:>11}  {}",
            name,
            preset_oew,
            clean_oew,
            residual_kg,
            if clean_oew > 0.0 {
                100.0 * residual_kg / clean_oew
            } else {
                f64::NAN
            },
            at_stake_kg.map_or_else(|| "-".to_owned(), |value| format!("{value:+.0}")),
            if labels.is_empty() {
                "none".to_owned()
            } else {
                labels
            }
        );
        records.push(json!({
            "preset": name,
            "passengers": preset_config.requirements.num_passengers,
            "reference_mtow_kg": preset_config.requirements.mtow_kg,
            "preset_mode": {
                "cabin_equipment_method": preset_config
                    .mass_model
                    .flops_transport
                    .cabin_equipment_method
                    .as_str(),
                "haul_class": preset_config
                    .mass_model
                    .flops_transport
                    .haul_class
                    .map(|class| class.as_str()),
                "cargo_loading": preset_config
                    .mass_model
                    .flops_transport
                    .cargo_loading
                    .map(|loading| loading.as_str()),
                "oew_kg": preset_oew,
            },
            "clean_sheet_mode": {
                "cabin_equipment_method": clean_sheet_config
                    .mass_model
                    .flops_transport
                    .cabin_equipment_method
                    .as_str(),
                "haul_class": clean_sheet_config
                    .mass_model
                    .flops_transport
                    .haul_class
                    .map(|class| class.as_str()),
                "cargo_loading": clean_sheet_config
                    .mass_model
                    .flops_transport
                    .cargo_loading
                    .map(|loading| loading.as_str()),
                "oew_kg": clean_oew,
            },
            "operating_empty_mass_residual_kg": residual_kg,
            "method_selection_gap_kg": at_stake_kg,
            "differing_components": Value::Array(differing),
        }));
    }

    let document = json!({
        "schema_version": "alas-mass/cross-mode-cabin-method-parity-v1",
        "question": "does the same aircraft get the same operating empty mass through the preset path and through the no-preset path",
        "matched": "geometry, design vector, requirements, cabin, control surfaces, landing gear, engines, haul class and cargo-hold loading are identical on both sides; only the cabin-equipment method, the one field the two paths used to derive differently, is taken from FlopsTransportConfig::working_default() on the clean-sheet side",
        "method_selection_gap_kg": "operating empty mass under the LTH relations minus operating empty mass under the published FLOPS equations, on this aircraft, as the magnitude the parity above is a statement about",
        "physical_validation": "not_performed",
        "worst_operating_empty_mass_residual_kg": worst_residual_kg,
        "presets": Value::Array(records),
    });
    fs::write(&output_path, serde_json::to_vec_pretty(&document)?)?;
    println!("\nworst operating-empty residual: {worst_residual_kg:.3} kg");
    println!("{}", output_path.display());
    Ok(())
}
