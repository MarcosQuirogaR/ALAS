// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Quantify A380 CG sensitivity to the provisional wing-mass point model.
//!
//! This is evidence, not a product correction. The alternative coordinates
//! assume uniform structural areal density centered at a selected wingbox
//! chord fraction; real spanwise structure and the aircraft WBM remain needed.

use alas_config::{presets, AlasConfig};
use alas_mass::breakdown::{FUEL, OEW_KEYS, PAYLOAD, WING};
use alas_pipeline::full_analysis::FullAnalysis;
use serde_json::json;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

fn planform_chord_fraction_centroid_x(
    wing: &alas_geom::aircraft::wing::Wing,
    chord_fraction: f64,
) -> Option<f64> {
    let mut area = 0.0;
    let mut first_moment = 0.0;
    for pair in wing.xsecs.windows(2) {
        let root = &pair[0];
        let tip = &pair[1];
        let dy = tip.xyz_le[1] - root.xyz_le[1];
        let dz = tip.xyz_le[2] - root.xyz_le[2];
        let span = dy.hypot(dz);
        if !span.is_finite() || span <= 0.0 {
            continue;
        }
        let midpoint_chord = 0.5 * (root.chord + tip.chord);
        let root_x = root.xyz_le[0] + chord_fraction * root.chord;
        let tip_x = tip.xyz_le[0] + chord_fraction * tip.chord;
        let midpoint_x = 0.5 * (root_x + tip_x);
        area += span * (root.chord + tip.chord) / 2.0;
        first_moment += span
            * (root.chord * root_x + 4.0 * midpoint_chord * midpoint_x + tip.chord * tip_x)
            / 6.0;
    }
    (area > 0.0).then_some(first_moment / area)
}

fn condition_cg_x(
    masses: &std::collections::HashMap<String, f64>,
    coordinates: &std::collections::HashMap<String, [f64; 3]>,
    include_payload: bool,
    include_fuel: bool,
    wing_x: f64,
) -> Option<(f64, f64)> {
    let mut total_mass = 0.0;
    let mut moment = 0.0;
    for key in OEW_KEYS {
        let mass = *masses.get(key)?;
        let x = if key == WING {
            wing_x
        } else {
            coordinates.get(key)?[0]
        };
        total_mass += mass;
        moment += mass * x;
    }
    if include_payload {
        let mass = *masses.get(PAYLOAD)?;
        total_mass += mass;
        moment += mass * coordinates.get(PAYLOAD)?[0];
    }
    if include_fuel {
        let mass = (*masses.get(FUEL)?).max(0.0);
        total_mass += mass;
        moment += mass * coordinates.get(FUEL)?[0];
    }
    (total_mass > 0.0).then_some((moment / total_mass, total_mass))
}

fn main() -> Result<(), Box<dyn Error>> {
    let preset = presets::get("A380-800").map_err(std::io::Error::other)?;
    let mut config = AlasConfig {
        preset: preset.name.to_owned(),
        geometry: preset.geometry.clone(),
        requirements: preset.requirements.clone(),
        landing_gear: preset.landing_gear.clone(),
        ..AlasConfig::default()
    };
    if let Some(model) = &preset.mass_model {
        config.mass_model = model.clone();
    }
    if let Some(performance) = &preset.performance {
        config.performance = performance.clone();
    }
    let report = FullAnalysis::new_reference_compatibility(config)
        .run(&preset.design_vector, true)
        .map_err(std::io::Error::other)?;
    let wing = report
        .airplane
        .wings
        .iter()
        .find(|wing| wing.name == "Main Wing")
        .ok_or_else(|| std::io::Error::other("built A380 has no main wing"))?;
    let current_wing_x = report
        .mass_coordinates
        .get(WING)
        .ok_or_else(|| std::io::Error::other("wing mass coordinate is missing"))?[0];
    let mac = wing.mean_aerodynamic_chord();
    let lemac = wing.aerodynamic_center(0.0)[0];
    let to_percent_mac = |x: f64| 100.0 * (x - lemac) / mac;

    let mut alternatives = Vec::new();
    for fraction in [0.35, 0.40, 0.45] {
        let Some(wing_x) = planform_chord_fraction_centroid_x(wing, fraction) else {
            continue;
        };
        let Some((oew_x, oew_mass)) = condition_cg_x(
            &report.component_masses,
            &report.mass_coordinates,
            false,
            false,
            wing_x,
        ) else {
            continue;
        };
        let Some((mzfw_x, mzfw_mass)) = condition_cg_x(
            &report.component_masses,
            &report.mass_coordinates,
            true,
            false,
            wing_x,
        ) else {
            continue;
        };
        let Some((mtow_x, mtow_mass)) = condition_cg_x(
            &report.component_masses,
            &report.mass_coordinates,
            true,
            true,
            wing_x,
        ) else {
            continue;
        };
        alternatives.push(json!({
            "wingbox_chord_fraction": fraction,
            "wing_mass_x_m": wing_x,
            "wing_mass_x_percent_mac": to_percent_mac(wing_x),
            "oew_mass_kg": oew_mass,
            "oew_cg_percent_mac": to_percent_mac(oew_x),
            "mzfw_mass_kg": mzfw_mass,
            "mzfw_cg_percent_mac": to_percent_mac(mzfw_x),
            "mtow_mass_kg": mtow_mass,
            "mtow_cg_percent_mac": to_percent_mac(mtow_x),
        }));
    }

    let output = json!({
        "status": "sensitivity_only_not_product_correction",
        "aircraft": "A380-841 WV000",
        "model_warning": "Uniform planform areal density is not a validated structural mass distribution; AFM/WBM and a spanwise wingbox mass model are required.",
        "mac_m": mac,
        "lemac_x_m": lemac,
        "neutral_point_percent_mac": to_percent_mac(report.x_neutral_point),
        "current": {
            "wing_mass_x_m": current_wing_x,
            "wing_mass_x_percent_mac": to_percent_mac(current_wing_x),
            "mtow_cg_percent_mac": to_percent_mac(report.physical_cg[0]),
            "static_margin_percent_mac": report.static_margin * 100.0,
        },
        "alternatives": alternatives,
    });
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../outputs/a380_wing_cg_sensitivity.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(&output)?)?;
    Ok(())
}
