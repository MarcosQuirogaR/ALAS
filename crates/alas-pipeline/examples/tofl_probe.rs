// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! ISA sea-level take-off field length breakdown per jet preset (diagnostic).
//!
//! `cargo run --release -p alas-pipeline --example tofl_probe [preset ...]`
//! prints one line per preset: masses, speeds, `VR`, `V1`, all-engine distance
//! (and its CS 25.113(a)(2) factor), accelerate-stop, continued take-off,
//! balanced field and the field length, plus the Vref of the same run.
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]
// Diagnostic over curated fixtures: a failure aborts the line.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_pipeline::field_reference::report_isa_sea_level_field_reference;
use alas_pipeline::full_analysis::FullAnalysis;
use serde_json::json;

fn main() {
    let wanted: Vec<String> = std::env::args().skip(1).collect();
    for name in presets::available() {
        if !wanted.is_empty() && !wanted.iter().any(|w| w == name) {
            continue;
        }
        match line(name) {
            Ok(text) => println!("{text}"),
            Err(error) => println!("{name}: ERROR {error}"),
        }
    }
}

fn line(name: &str) -> Result<String, String> {
    let preset = presets::get(name).map_err(|e| e.to_string())?;
    // TOFL_PERF='{"takeoff_gear_delta_cd":0.0}' overrides performance fields
    // (sensitivity runs); TOFL_CLMAX_SCALE multiplies the preset CLmax_TO.
    let mut value = json!({"preset": name});
    if let Ok(text) = std::env::var("TOFL_PERF") {
        value["performance"] = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    }
    let mut config = AlasConfig::from_value(&value).map_err(|e| e.to_string())?;
    if let Some(scale) = std::env::var("TOFL_CLMAX_SCALE")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
    {
        config.performance.cl_max_to *= scale;
    }
    let report = FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .map_err(|e| e.to_string())?;
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .map_err(|e| e.to_string())?;
    let mtow = config.requirements.mtow_kg;
    let mlw = config.landing_mass_limit_kg(mtow);
    let r = report_isa_sea_level_field_reference(&config, &report, airplane.s_ref, mtow, mlw)?;
    let Some(f) = r.takeoff_field else {
        return Ok(format!(
            "{name}: TOFL {:.0} m (propeller/replay method)",
            r.takeoff_field_length_m
        ));
    };
    Ok(format!(
        "{name}: mtow {mtow:.0} T/W {:.4} VSR {:.1} V2 {:.1} ({:.3} VSR) Vref {:.1} | VR {:.1} V1 {:.1} VEF {:.1} | AEO {:.0} x1.15 {:.0} | ASD {:.0} GO {:.0} BFL {:.0} | TOFL {:.0}",
        r.static_tw, r.v_stall_takeoff_m_s, r.v2_m_s, r.v2_m_s / r.v_stall_takeoff_m_s, r.vref_m_s,
        f.vr_m_s, f.v1_m_s, f.vef_m_s, f.all_engine_m, 1.15 * f.all_engine_m,
        f.accelerate_stop_m, f.continued_m, f.balanced_m, f.tofl_m,
    ))
}
