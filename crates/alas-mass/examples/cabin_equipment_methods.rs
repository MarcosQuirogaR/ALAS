// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The cabin-equipment and operating-item group of every registered aircraft,
//! under both implemented methods, against the published references.
//!
//! The group is FLOPS `WFURN` plus the occupant-driven operating items, which
//! is the boundary a manufacturer's own accounting uses (passenger seats and
//! galley structure are operational items, ATA 60-3 and 60-2, not
//! furnishings). It is the single largest identified contributor to the
//! remaining operating-empty-mass deficits, and the two methods disagree about
//! it by a factor that grows with the aircraft, so the comparison is published
//! rather than resolved by picking whichever lands closer.
//!
//! The unusable fuel and the engine oil are excluded from the group on both
//! sides: they are propulsion-side fluids that the LTH relations do not price.
//!
//! Usage:
//!
//! ```text
//! cargo run -p alas-mass --example cabin_equipment_methods -- out.json
//! ```

#![allow(clippy::print_stdout)]

use alas_config::{presets, AlasConfig, CabinEquipmentMethod};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{
    calculate_flops_mass_buildup, calculate_physical_cg, run_product_mass_analysis_with_groups,
    MassCoordinateModel, ProductMassBuildup, OEW_KEYS,
};
use serde_json::{json, Value};
use std::{error::Error, fs, path::PathBuf};

struct Row {
    preset: String,
    passengers: usize,
    flops_group_kg: f64,
    lth_group_kg: f64,
    flops_oew_kg: f64,
    lth_oew_kg: f64,
    flops_cg_percent_mac: f64,
    lth_cg_percent_mac: f64,
    reference_oew_kg: Option<f64>,
}

/// The cabin group, the operating empty mass and the operating-empty centre
/// of gravity, in percent of mean aerodynamic chord.
///
/// The centre of gravity is reported because this is the one comparison where
/// a method change moves several tonnes into a single cabin-stationed slot:
/// whoever decides between the two methods needs the balance consequence in
/// front of them, not only the mass.
fn evaluate(config: &AlasConfig, method: CabinEquipmentMethod) -> Result<(f64, f64, f64), String> {
    let mut config = config.clone();
    config.mass_model.flops_transport.cabin_equipment_method = method;
    // The registered design vector, exactly as `flops_preset_comparison`
    // builds it: without it the builder re-derives a geometry the aircraft
    // does not have.
    let design_vector = presets::get(&config.preset)
        .map(|entry| entry.design_vector)
        .ok();
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(design_vector.as_ref(), true)
        .map_err(|error| error.to_string())?;
    let build = match calculate_flops_mass_buildup(
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
        ProductMassBuildup::PureFlops(build) => build,
        other => return Err(format!("not a pure-FLOPS buildup: {other:?}")),
    };
    let items = build.systems_and_operating_items.operating_items;
    let group_kg = build.systems_and_operating_items.systems.furnishings_kg + items.total_kg
        - items.unusable_fuel_kg
        - items.engine_oil_kg;
    let oew_kg: f64 = OEW_KEYS
        .iter()
        .filter_map(|name| build.masses.get(name))
        .sum();

    // The operating-empty balance on the same buildup: payload and fuel are
    // cleared so the centre of gravity is the empty aircraft's own.
    let (_, coordinates, _, _) = run_product_mass_analysis_with_groups(
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
    .map_err(|error| error.to_string())?;
    let mut empty = build.masses;
    empty.payload = 0.0;
    empty.fuel = 0.0;
    let cg_x_m = calculate_physical_cg(&empty, &coordinates);
    let wing = plane
        .wings
        .iter()
        .find(|wing| wing.name == "Main Wing")
        .ok_or_else(|| "no main wing".to_owned())?;
    let mac_m = wing.mean_aerodynamic_chord();
    let mac_le_x_m = wing.aerodynamic_center(0.0)[0];
    let cg_percent_mac = if mac_m > 0.0 {
        100.0 * (cg_x_m[0] - mac_le_x_m) / mac_m
    } else {
        f64::NAN
    };
    Ok((group_kg, oew_kg, cg_percent_mac))
}

fn main() -> Result<(), Box<dyn Error>> {
    let output_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("outputs/cabin-equipment-methods.json"));
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut rows = Vec::new();
    let mut records = Vec::new();
    for name in presets::available() {
        let config = AlasConfig::from_value(&json!({ "preset": name }))?;
        let passengers = usize::try_from(config.requirements.num_passengers).unwrap_or(0);
        let flops = evaluate(&config, CabinEquipmentMethod::FlopsTransportV1);
        let lth = evaluate(&config, CabinEquipmentMethod::LthCivilTransportV1);
        let reference_oew_kg =
            alas_config::oew_reference::get(name).and_then(|record| record.visible_value_kg());
        match (flops, lth) {
            (
                Ok((flops_group_kg, flops_oew_kg, flops_cg_percent_mac)),
                Ok((lth_group_kg, lth_oew_kg, lth_cg_percent_mac)),
            ) => {
                records.push(json!({
                    "preset": name,
                    "passengers": passengers,
                    "haul_class": config
                        .mass_model
                        .flops_transport
                        .haul_class
                        .map(|class| class.as_str()),
                    "flops_cabin_group_kg": flops_group_kg,
                    "lth_cabin_group_kg": lth_group_kg,
                    "flops_cabin_group_kg_per_seat": flops_group_kg / passengers.max(1) as f64,
                    "lth_cabin_group_kg_per_seat": lth_group_kg / passengers.max(1) as f64,
                    "flops_oew_kg": flops_oew_kg,
                    "lth_oew_kg": lth_oew_kg,
                    "flops_oew_cg_percent_mac": flops_cg_percent_mac,
                    "lth_oew_cg_percent_mac": lth_cg_percent_mac,
                    "reference_oew_kg": reference_oew_kg,
                }));
                rows.push(Row {
                    preset: name.to_owned(),
                    passengers,
                    flops_group_kg,
                    lth_group_kg,
                    flops_oew_kg,
                    lth_oew_kg,
                    flops_cg_percent_mac,
                    lth_cg_percent_mac,
                    reference_oew_kg,
                });
            }
            (flops, lth) => records.push(json!({
                "preset": name,
                "flops_error": flops.err(),
                "lth_error": lth.err(),
            })),
        }
    }

    println!(
        "{:<11} {:>5} {:>11} {:>11} {:>8} {:>8} {:>11} {:>11} {:>10} {:>9} {:>9}",
        "preset",
        "seats",
        "cabin FLOPS",
        "cabin LTH",
        "kg/seat",
        "kg/seat",
        "OEW FLOPS",
        "OEW LTH",
        "reference",
        "err FLOPS",
        "err LTH"
    );
    println!(
        "{:<11} {:>5} {:>11} {:>11}",
        "", "", "OEW cg %MAC", "OEW cg %MAC"
    );
    for row in &rows {
        let seats = row.passengers.max(1) as f64;
        let (flops_error, lth_error) = match row.reference_oew_kg {
            Some(reference) if reference > 0.0 => (
                format!(
                    "{:+8.2}%",
                    100.0 * (row.flops_oew_kg - reference) / reference
                ),
                format!("{:+8.2}%", 100.0 * (row.lth_oew_kg - reference) / reference),
            ),
            _ => ("        -".to_owned(), "        -".to_owned()),
        };
        println!(
            "{:<11} {:>5} {:>11.0} {:>11.0} {:>8.1} {:>8.1} {:>11.0} {:>11.0} {:>10} {:>9} {:>9}",
            row.preset,
            row.passengers,
            row.flops_group_kg,
            row.lth_group_kg,
            row.flops_group_kg / seats,
            row.lth_group_kg / seats,
            row.flops_oew_kg,
            row.lth_oew_kg,
            row.reference_oew_kg
                .map_or_else(|| "-".to_owned(), |value| format!("{value:.0}")),
            flops_error,
            lth_error,
        );
        println!(
            "{:<11} {:>5} {:>11.2} {:>11.2}",
            "", "", row.flops_cg_percent_mac, row.lth_cg_percent_mac
        );
    }

    let document = json!({
        "schema_version": "alas-mass/cabin-equipment-method-comparison-v1",
        "group_definition": "FLOPS WFURN plus the occupant-driven operating items (cabin crew, flight crew, passenger service); the unusable fuel and engine oil are excluded on both sides because the LTH relations do not price them, and the unit-load-device tare is excluded because it is outside the operating-empty boundary every reference here is stated on",
        "methods": {
            "flops_transport_v1": "NASA/TM-2017-219627 Vol. I equation 110 and equations 119-126",
            "lth_civil_transport_v1": "LTH MA 401 12-01 B furnishings and operating items, coefficients from Pape 2018 equations 2.14-2.16, evaluated on the average fuselage diameter (width+depth)/2 its own worked examples require; stated validity is a civil transport of at least 40 t maximum takeoff mass or at least 70 passenger seats"
        },
        "physical_validation": "not_performed",
        "presets": Value::Array(records),
    });
    fs::write(&output_path, serde_json::to_vec_pretty(&document)?)?;
    println!("{}", output_path.display());
    Ok(())
}
