// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Diagnostic: the product's payload-range corners of every registered
//! preset (`quick_analysis::payload_range_corners`), one JSON line each, for
//! a before/after comparison of a corner-pricing change.
//!
//! Usage: `corner_survey [PRESET ...]`
#![allow(
    clippy::expect_used,
    clippy::print_stderr,
    clippy::print_stdout,
    missing_docs
)]

use alas_config::AlasConfig;
use alas_pipeline::fuel_model::report_mission_model;
use alas_pipeline::quick_analysis::payload_range_corners;
use alas_pipeline::FullAnalysis;

fn main() {
    let requested: Vec<String> = std::env::args().skip(1).collect();
    let names: Vec<String> = if requested.is_empty() {
        alas_config::presets::available()
            .iter()
            .map(|name| (*name).to_owned())
            .collect()
    } else {
        requested
    };
    for name in names {
        let Ok(preset) = alas_config::presets::get(&name) else {
            continue;
        };
        let config =
            AlasConfig::from_value(&serde_json::json!({ "preset": name })).expect("config");
        let report = match FullAnalysis::new(config.clone()).run(&preset.design_vector, true) {
            Ok(report) => report,
            Err(error) => {
                println!(
                    "{}",
                    serde_json::json!({ "preset": name, "error": error.to_string() })
                );
                continue;
            }
        };
        let altitudes = report_mission_model(&config, &report)
            .map(|model| (model.cruise_altitude_m, model.design_cruise_altitude_m))
            .unwrap_or((f64::NAN, f64::NAN));
        match payload_range_corners(&config, &report) {
            Ok(corners) => println!(
                "{}",
                serde_json::json!({
                    "preset": name,
                    "declared_altitude_m": altitudes.0,
                    "design_altitude_m": altitudes.1,
                    "oew_kg": corners.oew_kg,
                    "mtow_kg": corners.mtow_kg,
                    "points_km_kg": corners.points.iter().map(|(r, p)| [r / 1000.0, *p]).collect::<Vec<_>>(),
                    "reserve_kg": corners.reserve_fuel_kg,
                })
            ),
            Err(error) => println!(
                "{}",
                serde_json::json!({ "preset": name, "unavailable": format!("{error:?}") })
            ),
        }
    }
}
