// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reproducible preset aerodynamic and design-point fuel diagnostics.

use std::collections::BTreeMap;
use std::path::PathBuf;

use alas_config::{presets, AlasConfig};
use alas_mass::fuel_policy::plan_fuel;
use alas_opt::mdo::{baseline_fuel_artifacts, candidate_mission_model, plan_for_mission};
use alas_pipeline::FullAnalysis;
use serde_json::{json, Value};

fn measure(name: &str, prior: Option<&Value>) -> Result<Value, String> {
    let preset = presets::get(name).map_err(|error| error.to_string())?;
    let config =
        AlasConfig::from_value(&json!({ "preset": name })).map_err(|error| error.to_string())?;
    let design = &preset.design_vector;
    let report = FullAnalysis::new(config.clone())
        .run(design, true)
        .map_err(|error| error.to_string())?;
    let cl = report.design_point.cl;
    let i = report
        .polar
        .cl
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - cl).abs().total_cmp(&(*b - cl).abs()))
        .map(|(i, _)| i)
        .ok_or("empty polar")?;
    let ar = report.airplane.b_ref.powi(2) / report.airplane.s_ref;
    let span_cl = report.polar.cl[i];
    let span_e = span_cl.powi(2) / (std::f64::consts::PI * ar * report.polar.cd_induced[i]);
    let artifacts = baseline_fuel_artifacts(&config, design);
    let mission_comparison = prior.and_then(|prior| {
        let point = prior.pointer("/aerodynamics/cruise_drag_breakdown")?;
        let cl = point.get("cl")?.as_f64()?;
        let mach = point.get("mach")?.as_f64()?;
        let table = artifacts.as_ref().ok()?.drag.table()?;
        Some(json!({
            "cl": cl, "mach": mach,
            "prior_wave_cd": point.get("wave_compressibility"),
            "shared_table_wave_cd": table.wave_cd(cl, mach),
        }))
    });
    let table = match &artifacts {
        Ok(artifacts) => artifacts.drag.table().map(|table| {
            let cl = table.design_cl();
            json!({
                "cl": cl, "mach": table.design_mach(),
                "span_e": table.effective_oswald(cl),
                "ld": cl / table.cd(cl, table.design_mach(), table.reference_altitude_m()),
                "wave_cd": table.wave_cd(cl, table.design_mach()),
                "induced_check_relative": table.induced_check().relative_error(),
            })
        }),
        Err(error) => Some(json!({ "error": error })),
    };
    let design_fuel = (|| -> Result<Value, String> {
        let point = preset
            .reference
            .design_point
            .ok_or("no charted design point")?;
        let artifacts = artifacts.as_ref().map_err(Clone::clone)?;
        let model = candidate_mission_model(&config, artifacts)?;
        let range_m = point.range_nmi * 1852.0;
        let mass_kg = config.requirements.mtow_kg;
        let planned =
            plan_for_mission(&model, mass_kg, range_m).map_err(|error| format!("{error:?}"))?;
        let fuel = plan_fuel(&config.fuel_policy, &planned, mass_kg, range_m)
            .map_err(|error| format!("{error:?}"))?;
        Ok(json!({
            "range_nmi": point.range_nmi, "charted_payload_kg": point.payload_kg,
            "source": point.source, "takeoff_mass_kg": mass_kg,
            "takeoff_fuel_kg": fuel.takeoff_fuel_kg(), "trip_fuel_kg": fuel.trip.kg,
        }))
    })();
    Ok(json!({
        "cl": cl, "mach": config.requirements.cruise_mach,
        "altitude_m": config.requirements.cruise_altitude_m,
        "ar": ar, "sweep_le_deg": design.sweep_deg,
        "span_e": span_e, "span_cl": span_cl, "fitted_e": report.polar_fit.oswald_e,
        "cruise_ld": report.design_point.l_over_d,
        "trimmed_cruise": report.trimmed_design_point,
        "wave_cd": report.polar.cd_wave[i], "table": table,
        "mission_same_state_wave": mission_comparison,
        "reference_oew_kg": preset.reference.oew_kg,
        "design_fuel": design_fuel.unwrap_or_else(|error| json!({ "error": error })),
    }))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("output path required")?;
    let prior = std::env::args()
        .nth(2)
        .map(std::fs::read)
        .transpose()?
        .map(|bytes| serde_json::from_slice::<Value>(&bytes))
        .transpose()?;
    let rows: BTreeMap<_, _> = presets::available()
        .iter()
        .map(|&name| {
            (
                name,
                measure(name, prior.as_ref().and_then(|prior| prior.get(name)))
                    .unwrap_or_else(|error| json!({ "error": error })),
            )
        })
        .collect();
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, serde_json::to_vec_pretty(&rows)?)?;
    Ok(())
}
