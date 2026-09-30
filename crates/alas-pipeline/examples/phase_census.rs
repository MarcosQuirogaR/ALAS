// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mass, balance and payload-range census of every registered preset, from
//! the baseline full analysis.
//!
//! Per preset it reports the OEW and the reference OEW, the planning seats,
//! the model-frame CG (% MAC) at OEW, analyzed ZFW, analyzed takeoff and
//! landing with each state's phase-scoped forward/aft limit and governing
//! mechanism, the MTOW, MZFW and MLW (declared or derived, labelled), and the
//! payload-range corners B, C and D. Masses are kg, ranges nautical miles,
//! CG and limits % MAC of the model frame (x aft of the nose tip, LEMAC
//! reference).
//!
//! ```text
//! cargo run -p alas-pipeline --release --example phase_census -- out/census
//! ```
//!
//! writes `<prefix>.json` and `<prefix>.md`; without an argument both are
//! printed to standard output.
#![allow(clippy::print_stdout)]

use std::error::Error;
use std::fmt::Write as _;
use std::fs;

use alas_config::{oew_reference, presets, AlasConfig};
use alas_opt::ModelCgLoadingState;
use alas_pipeline::feasibility::assess_physical_feasibility;
use alas_pipeline::quick_analysis::payload_range_corners;
use alas_pipeline::FullAnalysis;
use serde_json::{json, Value};

const METRES_PER_NMI: f64 = 1_852.0;

/// A declared value, or the derived one when the preset declares none.
fn declared_or_derived(declared: Option<f64>, derived: f64) -> Value {
    match declared {
        Some(value) => json!({ "kg": value, "basis": "declared" }),
        None => json!({ "kg": derived, "basis": "derived" }),
    }
}

fn state_row(state: &alas_opt::ModelCgLoadingAssessment, label: &str) -> (Value, String) {
    let limits = &state.physical_limits;
    let json = json!({
        "state": label,
        "mass_kg": state.mass_kg,
        "cg_pct_mac": state.cg_pct_mac,
        "fwd_limit_pct_mac": limits.fwd_limit_pct_mac,
        "fwd_mechanism": format!("{:?}", limits.fwd_limit_governance),
        "aft_limit_pct_mac": limits.aft_limit_pct_mac,
        "aft_mechanism": format!("{:?}", limits.aft_limit_governance),
    });
    let md = format!(
        "| {label} | {:.0} | {:.1} | {:.1} ({:?}) | {:.1} ({:?}) |",
        state.mass_kg,
        state.cg_pct_mac,
        limits.fwd_limit_pct_mac,
        limits.fwd_limit_governance,
        limits.aft_limit_pct_mac,
        limits.aft_limit_governance,
    );
    (json, md)
}

fn census(preset_name: &str) -> Result<(Value, String), Box<dyn Error>> {
    let config = AlasConfig::from_value(&json!({ "preset": preset_name }))?;
    let preset = presets::get(preset_name)?;
    let report = FullAnalysis::new(config.clone()).run(&preset.design_vector, true)?;
    let feasibility = assess_physical_feasibility(&config, &preset.design_vector, &report, None);
    let reference = &preset.reference;
    let mtow_config_kg = config.requirements.mtow_kg;

    let mut states_json = Vec::new();
    let mut md = format!("### {preset_name}\n\n");
    let mut oew_kg = f64::NAN;
    let mut zfw_kg = f64::NAN;
    let mut rows = Vec::new();
    if let Some(model_cg) = feasibility.model_cg.as_ref() {
        for (wanted, label) in [
            (ModelCgLoadingState::OperatingEmpty, "OEW"),
            (ModelCgLoadingState::AnalyzedZeroFuel, "ZFW"),
            (ModelCgLoadingState::AnalyzedTakeoff, "TOW"),
            (ModelCgLoadingState::AnalyzedLanding, "landing"),
        ] {
            let Some(state) = model_cg.loading_states.iter().find(|s| s.state == wanted) else {
                continue;
            };
            match wanted {
                ModelCgLoadingState::OperatingEmpty => oew_kg = state.mass_kg,
                ModelCgLoadingState::AnalyzedZeroFuel => zfw_kg = state.mass_kg,
                _ => {}
            }
            let (json, row) = state_row(state, label);
            states_json.push(json);
            rows.push(row);
        }
    }
    let reference_oew_kg = reference
        .oew_kg
        .or_else(|| oew_reference::preset_reference_oew_kg(preset_name));
    let oew_residual_pct = reference_oew_kg.map(|r| 100.0 * (oew_kg - r) / r);
    let derived_mzfw_kg = zfw_kg;
    let mzfw = declared_or_derived(reference.mzfw_kg, derived_mzfw_kg);
    let mlw = declared_or_derived(
        reference.mlw_kg,
        config.landing_mass_limit_kg(mtow_config_kg),
    );
    let mtow = declared_or_derived(reference.mtow_kg, mtow_config_kg);

    let corners = match payload_range_corners(&config, &report) {
        Ok(quick) => {
            let labels = ["A", "B", "C", "D"];
            let points = quick
                .points
                .iter()
                .enumerate()
                .map(|(index, &(range_m, payload_kg))| {
                    json!({
                        "corner": labels.get(index).copied().unwrap_or("?"),
                        "range_nmi": range_m / METRES_PER_NMI,
                        "payload_kg": payload_kg,
                        "reserve_fuel_kg": quick.reserve_fuel_kg.get(index).copied(),
                        "range_basis": format!("{:?}", quick.range_basis),
                    })
                })
                .collect::<Vec<_>>();
            Value::Array(points)
        }
        Err(error) => json!({ "unavailable": format!("{error:?}") }),
    };

    let _ = writeln!(
        md,
        "OEW {:.0} kg (reference {}, residual {}); planning seats {}; MTOW {:.0} kg ({}), \
         MZFW {:.0} kg ({}), MLW {:.0} kg ({}); sized takeoff mass {}.\n",
        oew_kg,
        reference_oew_kg.map_or_else(|| "n/a".to_owned(), |v| format!("{v:.0} kg")),
        oew_residual_pct.map_or_else(|| "n/a".to_owned(), |v| format!("{v:+.1} %")),
        reference
            .planning_seats
            .map_or_else(|| "n/a".to_owned(), |v| v.to_string()),
        mtow["kg"].as_f64().unwrap_or(f64::NAN),
        mtow["basis"].as_str().unwrap_or("?"),
        mzfw["kg"].as_f64().unwrap_or(f64::NAN),
        mzfw["basis"].as_str().unwrap_or("?"),
        mlw["kg"].as_f64().unwrap_or(f64::NAN),
        mlw["basis"].as_str().unwrap_or("?"),
        report.sized_takeoff_mass_kg().map_or_else(
            || "n/a (declared mass kept)".to_owned(),
            |v| format!("{v:.0} kg")
        ),
    );
    md.push_str("| state | mass (kg) | CG (% MAC) | forward limit (% MAC) | aft limit (% MAC) |\n");
    md.push_str("|---|---|---|---|---|\n");
    for row in rows {
        md.push_str(&row);
        md.push('\n');
    }
    md.push('\n');
    if let Some(points) = corners.as_array() {
        md.push_str("| corner | range (nmi) | payload (kg) | reserve fuel (kg) | range basis |\n");
        md.push_str("|---|---|---|---|---|\n");
        for point in points {
            let _ = writeln!(
                md,
                "| {} | {:.0} | {:.0} | {} | {} |",
                point["corner"].as_str().unwrap_or("?"),
                point["range_nmi"].as_f64().unwrap_or(f64::NAN),
                point["payload_kg"].as_f64().unwrap_or(f64::NAN),
                point["reserve_fuel_kg"]
                    .as_f64()
                    .map_or_else(|| "n/a".to_owned(), |v| format!("{v:.0}")),
                point["range_basis"].as_str().unwrap_or("?"),
            );
        }
        md.push('\n');
    }

    let json = json!({
        "preset": preset_name,
        "oew_kg": oew_kg,
        "reference_oew_kg": reference_oew_kg,
        "oew_residual_pct": oew_residual_pct,
        "planning_seats": reference.planning_seats,
        "mtow": mtow,
        "mzfw": mzfw,
        "mlw": mlw,
        "sized_takeoff_mass_kg": report.sized_takeoff_mass_kg(),
        "cg_states": states_json,
        "payload_range_corners": corners,
    });
    Ok((json, md))
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut all = Vec::new();
    let mut markdown = String::from("# Phase census\n\n");
    for name in presets::available() {
        match census(name) {
            Ok((json, md)) => {
                all.push(json);
                markdown.push_str(&md);
            }
            Err(error) => {
                all.push(json!({ "preset": name, "error": error.to_string() }));
                let _ = writeln!(markdown, "### {name}\n\nfailed: {error}\n");
            }
        }
    }
    let json = serde_json::to_string_pretty(&all)?;
    match std::env::args().nth(1) {
        Some(prefix) => {
            fs::write(format!("{prefix}.json"), json)?;
            fs::write(format!("{prefix}.md"), markdown)?;
        }
        None => {
            println!("{json}");
            println!("{markdown}");
        }
    }
    Ok(())
}
