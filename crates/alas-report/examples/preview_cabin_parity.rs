// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Does the interface preview now price the same cabin the full analysis does?
//!
//! The preview and `alas_pipeline::full_analysis` run the same two mass passes
//! and must apply the same one-cabin-per-case rule. This prints the operating
//! empty mass and the seated passenger count from both, per preset, so the
//! agreement is a measured number and not an inference from the call site.

// A probe that reports to the console and stops at the first broken fixture.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use alas_config::presets;
use alas_config::AlasConfig;
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{
    run_mass_analysis_with_model_checked_product_with_gear, MassCoordinateModel, OEW_KEYS,
};
use alas_payload::build::build_payload_layout;
use alas_payload::oew::oew_and_cg;
use alas_pipeline::full_analysis::FullAnalysis;
use alas_report::families::mass_balance::quick_preview_report;

const PROBE: &[&str] = &["A320-200", "A220-300", "ATR72-600", "A380-800", "B787-9"];

fn oew(masses: &std::collections::HashMap<String, f64>) -> f64 {
    OEW_KEYS
        .iter()
        .filter_map(|key| masses.get(*key))
        .sum::<f64>()
}

fn main() {
    println!("preset,unsynchronized_oew_kg,preview_oew_kg,full_oew_kg,preview_minus_full_kg");
    for name in PROBE {
        let Ok(preset) = presets::get(name) else {
            continue;
        };
        let Ok(config) = AlasConfig::from_value(&serde_json::json!({ "preset": name })) else {
            continue;
        };
        let design = preset.design_vector;
        let Ok(plane) =
            AircraftBuilder::new(Some(config.geometry.clone())).build(Some(&design), true)
        else {
            println!("{name}: geometry did not build");
            continue;
        };
        let preview = match quick_preview_report(plane.clone(), &config, design) {
            Ok(report) => report,
            Err(error) => {
                println!("{name}: preview failed: {error}");
                continue;
            }
        };
        let full = match FullAnalysis::new(config.clone()).run_on_airplane(&design, plane) {
            Ok(report) => report,
            Err(error) => {
                println!("{name}: full analysis failed: {error}");
                continue;
            }
        };
        let preview_oew = oew(&preview.component_masses);
        let full_oew = oew(&full.component_masses);
        // What the preview reported before it applied the shared rule: both
        // passes priced `requirements.num_passengers` while the payload beside
        // them priced the layout.
        let unsynchronized = unsynchronized_oew(&config, &design);
        println!(
            "{name},{},{preview_oew:.3},{full_oew:.3},{:.3}",
            unsynchronized.map_or_else(|| "n/a".to_owned(), |value| format!("{value:.3}")),
            preview_oew - full_oew
        );
    }
}

/// The operating empty mass the preview reported before it applied the shared
/// cabin rule: the one-cabin-per-case rule omitted from both passes, so the
/// FLOPS operating items price `requirements.num_passengers` while the payload
/// drawn beside them prices the seats the layout placed.
fn unsynchronized_oew(
    config: &AlasConfig,
    design: &alas_config::design_variables::DesignVector,
) -> Option<f64> {
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(design), true)
        .ok()?;
    let model = config.analysis_mass_model(config.requirements.mtow_kg);
    let run = |layout: Option<&alas_mass::breakdown::PayloadLayoutSummary>| {
        run_mass_analysis_with_model_checked_product_with_gear(
            &plane,
            &config.requirements,
            &config.geometry,
            &config.cabin,
            &config.control_surfaces,
            Some(&model),
            layout,
            MassCoordinateModel::StructuralWingbox(&config.structures),
            &config.landing_gear,
        )
        .ok()
    };
    let (first_masses, first_coords, _) = run(None)?;
    let (operating_empty, x_operating_empty) = oew_and_cg(&first_masses, &first_coords);
    let layout = build_payload_layout(&plane, config, operating_empty, x_operating_empty).ok()?;
    let summary = alas_mass::breakdown::PayloadLayoutSummary {
        total_mass: layout.total_mass,
        cg_x: layout.cg_x,
        cg_y: layout.cg_y,
    };
    let (masses, _, _) = run(Some(&summary))?;
    Some(
        OEW_KEYS
            .iter()
            .filter_map(|key| {
                masses
                    .as_pairs()
                    .into_iter()
                    .find(|(name, _)| name == key)
                    .map(|(_, value)| value)
            })
            .sum::<f64>(),
    )
}
