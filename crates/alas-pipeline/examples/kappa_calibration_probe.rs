// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Probe for the lift-dependent viscous drag calibration (kappa fixed, the
//! viscous margin `m` fitted).
//!
//! For each preset of a targets file this builds the baseline full analysis
//! once, then for every margin of the grid prices two payload-range corners
//! on the report's own segment mission model at the CHART masses given in the
//! targets file (not at the modelled OEW):
//!
//! - corner C: takeoff mass `tow_kg`, fuel `fuel_tank_kg - taxi`;
//! - corner B: same takeoff mass, fuel `fuel_tank_kg - taxi - delta_payload_kg`.
//!
//! Both corners lie on the same takeoff-mass line, so the ranges are
//! independent of the modelled empty mass. The output holds model-side
//! numbers only. No reference range is read here; the fit script owns the
//! chart targets, so this example cannot leak them into the model.
//!
//! Usage:
//! `kappa_calibration_probe --targets T.json --kappa 0.43 --m-grid 0.9:1.2:0.05 --out OUT.json [--rerun]`
//!
//! `--rerun` repeats the full analysis for every margin (slow) so the
//! reported cruise point and polar fit follow the margin; without it the
//! analysis runs once and only the fuel artifacts (drag table, mission
//! model) are rebuilt per margin.
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]

use std::path::PathBuf;

use alas_config::{presets, AlasConfig};
use alas_pipeline::fuel_model::ReportFuel;
use alas_pipeline::full_analysis::{AnalysisReport, FullAnalysis};
use alas_pipeline::quick_analysis::corners::{chart_mass_corners, ChartMasses, ChartReserveRule};
use serde_json::{json, Value};

struct Args {
    targets: PathBuf,
    kappa: f64,
    grid: Vec<f64>,
    out: PathBuf,
    rerun: bool,
}

fn parse_args() -> Result<Args, String> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut targets = None;
    let mut kappa = None;
    let mut grid = None;
    let mut out = None;
    let mut rerun = false;
    let mut index = 0;
    while index < argv.len() {
        let value = |index: usize| {
            argv.get(index + 1)
                .cloned()
                .ok_or_else(|| format!("{} needs a value", argv[index]))
        };
        match argv[index].as_str() {
            "--targets" => {
                targets = Some(PathBuf::from(value(index)?));
                index += 2;
            }
            "--kappa" => {
                kappa = Some(
                    value(index)?
                        .parse::<f64>()
                        .map_err(|error| error.to_string())?,
                );
                index += 2;
            }
            "--m-grid" => {
                let text = value(index)?;
                let parts: Vec<f64> = text
                    .split(':')
                    .map(|part| part.parse::<f64>().map_err(|error| error.to_string()))
                    .collect::<Result<_, _>>()?;
                if parts.len() != 3 || parts[2] <= 0.0 || parts[1] < parts[0] {
                    return Err("--m-grid is lo:hi:step with step > 0 and hi >= lo".to_owned());
                }
                let count = ((parts[1] - parts[0]) / parts[2] + 1e-9).floor() as usize + 1;
                grid = Some((0..count).map(|k| parts[0] + parts[2] * k as f64).collect());
                index += 2;
            }
            "--out" => {
                out = Some(PathBuf::from(value(index)?));
                index += 2;
            }
            "--rerun" => {
                rerun = true;
                index += 1;
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(Args {
        targets: targets.ok_or("--targets is required")?,
        kappa: kappa.ok_or("--kappa is required")?,
        grid: grid.ok_or("--m-grid is required")?,
        out: out.ok_or("--out is required")?,
        rerun,
    })
}

fn config_for(name: &str, kappa: f64, margin: f64) -> Result<AlasConfig, String> {
    let mut config =
        AlasConfig::from_value(&json!({ "preset": name })).map_err(|error| error.to_string())?;
    config.structures.run_nastran = false;
    config.structures.run_patran_export = false;
    config.drag_model.lift_dependent_viscous_fraction = kappa;
    config.drag_model.viscous_margin = margin;
    Ok(config)
}

fn run_report(name: &str, config: &AlasConfig) -> Result<AnalysisReport, String> {
    let preset = presets::get(name).map_err(|error| error.to_string())?;
    FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .map_err(|error| error.to_string())
}

fn corner(
    config: &AlasConfig,
    report: &AnalysisReport,
    tow_kg: f64,
    tank_kg: f64,
    delta_payload_kg: f64,
    reserve_rule: ChartReserveRule,
) -> Result<Value, String> {
    let corners = chart_mass_corners(
        config,
        report,
        &ChartMasses {
            tow_kg,
            tank_kg,
            delta_payload_kg,
            reserve_rule,
            cruise_mach: None,
        },
    )?;
    Ok(json!({
        "reserve_scheme": corners.scheme.as_str(),
        "alternate_distance_nmi": corners.alternate_distance_nmi,
        "final_reserve_hold_min": corners.final_reserve_hold_min,
        "taxi_kg": corners.taxi_kg,
        "fuel_b_kg": corners.fuel_b_kg,
        "fuel_c_kg": corners.fuel_c_kg,
        "range_b_m": corners.range_b_m,
        "range_c_m": corners.range_c_m,
        "reserve_b_kg": corners.reserve_b_kg,
        "reserve_c_kg": corners.reserve_c_kg,
    }))
}

fn point(report: &AnalysisReport) -> Value {
    let design = &report.design_point;
    let trimmed = report.trimmed_design_point.as_ref();
    json!({
        "design_cl": design.cl,
        "design_cd": design.cd,
        "design_ld": design.l_over_d,
        "trim_cl": trimmed.map(|point| point.cl),
        "trim_cd": trimmed.map(|point| point.cd),
        "trim_ld": trimmed.map(|point| point.l_over_d),
        "polar_cd0": report.polar_fit.cd0,
        "polar_k": report.polar_fit.k,
        "polar_e": report.polar_fit.oswald_e,
        "aspect_ratio": report.polar_fit.aspect_ratio,
        "component_mass_sum_kg": report.component_masses.values().copied().sum::<f64>(),
    })
}

fn main() {
    if let Err(error) = run() {
        eprintln!("kappa_calibration_probe: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let text = std::fs::read_to_string(&args.targets).map_err(|error| error.to_string())?;
    let targets: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let entries = targets
        .get("presets")
        .and_then(Value::as_array)
        .ok_or("targets file needs a presets array")?;
    let number = |entry: &Value, key: &str| -> Result<f64, String> {
        entry
            .get(key)
            .and_then(Value::as_f64)
            .ok_or_else(|| format!("target field {key} is missing"))
    };
    let mut results = Vec::new();
    for entry in entries {
        let name = entry
            .get("preset")
            .and_then(Value::as_str)
            .ok_or("target without preset name")?;
        let tow_kg = number(entry, "tow_kg")?;
        let tank_kg = number(entry, "fuel_tank_kg")?;
        let delta_kg = number(entry, "delta_payload_kg")?;
        let reserve_rule = ChartReserveRule::from_json(entry.get("reserve_rule"))?;
        eprintln!("[kappa probe] {name}");
        let first = config_for(name, args.kappa, args.grid[0])?;
        let mut report = run_report(name, &first)?;
        let mut rows = Vec::new();
        for (index, &margin) in args.grid.iter().enumerate() {
            let config = config_for(name, args.kappa, margin)?;
            if args.rerun && index > 0 {
                report = run_report(name, &config)?;
            }
            // The fuel artifacts cache the drag table built from the margin
            // in force; a fresh cache prices the corners on this margin.
            report.fuel = ReportFuel::default();
            let corners = corner(&config, &report, tow_kg, tank_kg, delta_kg, reserve_rule)?;
            rows.push(json!({ "m": margin, "corners": corners, "report": point(&report) }));
            eprintln!("  m = {margin:.4}");
        }
        results.push(json!({
            "preset": name,
            "tow_kg": tow_kg,
            "fuel_tank_kg": tank_kg,
            "delta_payload_kg": delta_kg,
            "config_mtow_kg": first.requirements.mtow_kg,
            "sweep_c4_deg": entry.get("sweep_c4_deg"),
            "rows": rows,
        }));
    }
    let document = json!({
        "kappa": args.kappa,
        "rerun_per_margin": args.rerun,
        "results": results,
    });
    if let Some(parent) = args.out.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(
        &args.out,
        serde_json::to_string_pretty(&document).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    println!("wrote {}", args.out.display());
    Ok(())
}
