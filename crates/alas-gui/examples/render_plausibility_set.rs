// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Dump every result figure of one aircraft case for plausibility review:
//! one PNG per figure, the raw plot data of every figure (the numbers plotted)
//! and a mission segment table. The case is a registered preset or a clean-sheet
//! brief (a JSON patch merged onto a base preset), flown in one MTOW mode on the
//! registered design (no optimization), native analyses only.
//!
//! Usage:
//!   cargo run -p alas-gui --example render_plausibility_set -- \
//!       --preset <name> | --brief <file.json> [--base <preset>] \
//!       --mode fixed_requirement|mtow_band|payload_adjusted --out <dir>

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::error::Error;
use std::fmt::Write as _;
use std::path::PathBuf;

use alas_config::AlasConfig;
use alas_gui::export::collect_plot_data;
use alas_gui::scene::build_result_figure;
use alas_gui::AppState;
use alas_pipeline::{DesignPipeline, PipelineOptions, RunEnvironment};
use serde_json::{json, Value};

fn arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn merge(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(target), Value::Object(patch)) => {
            for (key, value) in patch {
                merge(target.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
        (target, patch) => *target = patch.clone(),
    }
}

fn mission_table(result: &alas_pipeline::PipelineResult) -> String {
    let mut text = String::new();
    let Some(mission) = result.mission_result.as_ref() else {
        return "no mission result\n".to_owned();
    };
    let _ = writeln!(
        text,
        "scheduled segments {}, flown {}, fuel exhaustion {:?}\n",
        mission.scheduled_segment_count,
        mission.segments.len(),
        mission.fuel_exhaustion.as_ref().map(|f| &f.segment_tag)
    );
    let _ = writeln!(
        text,
        "| # | tag | t0 min | t1 min | dt min | h0 ft | h1 ft | roc ft/min | range0 km | range1 km | M0 | M1 | mass0 kg | mass1 kg |"
    );
    let _ = writeln!(
        text,
        "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"
    );
    for (index, segment) in mission.segments.iter().enumerate() {
        let c = &segment.conditions;
        let (Some(&t0), Some(&t1)) = (c.time_s.first(), c.time_s.last()) else {
            continue;
        };
        let (h0, h1) = (c.altitude_m[0], *c.altitude_m.last().unwrap_or(&0.0));
        let dt = t1 - t0;
        let roc = if dt > 0.0 { (h1 - h0) / dt } else { 0.0 };
        let _ = writeln!(
            text,
            "| {index} | {} | {:.1} | {:.1} | {:.1} | {:.0} | {:.0} | {:.0} | {:.1} | {:.1} | {:.3} | {:.3} | {:.0} | {:.0} |",
            segment.spec.tag,
            t0 / 60.0,
            t1 / 60.0,
            dt / 60.0,
            h0 / 0.3048,
            h1 / 0.3048,
            roc / 0.3048 * 60.0,
            c.aircraft_range_m[0] / 1e3,
            c.aircraft_range_m.last().copied().unwrap_or(0.0) / 1e3,
            c.mach[0],
            c.mach.last().copied().unwrap_or(0.0),
            c.total_mass_kg[0],
            c.total_mass_kg.last().copied().unwrap_or(0.0),
        );
    }
    text
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    let out = PathBuf::from(arg(&args, "--out").ok_or("--out required")?);
    let mode = arg(&args, "--mode").unwrap_or_else(|| "fixed_requirement".to_owned());
    let (label, mut value) = if let Some(name) = arg(&args, "--preset") {
        (name.clone(), json!({ "preset": name }))
    } else {
        let file = arg(&args, "--brief").ok_or("--preset or --brief required")?;
        let base = arg(&args, "--base").unwrap_or_else(|| "A320-200".to_owned());
        let mut value = json!({ "preset": base });
        let patch: Value = serde_json::from_str(&std::fs::read_to_string(&file)?)?;
        merge(&mut value, &patch);
        let stem = std::path::Path::new(&file)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "brief".to_owned());
        (format!("cs-{stem}"), value)
    };
    merge(
        &mut value,
        &json!({
            "optimizer": {"objective": {"mtow_sizing": mode}},
            "mses": {"enabled": false},
            "downstream": {"openvsp": false, "vspaero": false, "avl": false, "flowunsteady": false},
            "structures": {"run_nastran": false, "run_patran_export": false}
        }),
    );
    let config = AlasConfig::from_value(&value)?;
    let dir = out.join(format!("{label}@{mode}"));
    std::fs::create_dir_all(dir.join("figures"))?;

    let options = PipelineOptions {
        optimize: false,
        compare_baseline: true,
        parallel: true,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };
    let result = DesignPipeline::new(config.clone()).run(&options, &RunEnvironment::default())?;
    std::fs::write(dir.join("mission.md"), mission_table(&result))?;

    let mut state = AppState::default();
    state.set_completed_pipeline_result(result.clone());
    let mut written = 0usize;
    let mut skipped = Vec::new();
    for descriptor in alas_report::RESULT_FIGURES {
        match build_result_figure(&state, descriptor.id, &result.config, "light") {
            Some(Some(scene)) => {
                let png =
                    alas_viz::raster::render_scene_png(&scene).map_err(std::io::Error::other)?;
                std::fs::write(
                    dir.join("figures").join(format!("{}.png", descriptor.id)),
                    png,
                )?;
                written += 1;
            }
            _ => skipped.push(descriptor.id),
        }
    }
    if let Ok(plots) = collect_plot_data(&state) {
        let data_dir = dir.join("plot-data");
        std::fs::create_dir_all(&data_dir)?;
        for plot in &plots {
            std::fs::write(
                data_dir.join(format!("{}.json", plot.plot_id)),
                plot.to_json(),
            )?;
        }
    }
    std::fs::write(
        dir.join("inventory.json"),
        serde_json::to_string_pretty(&json!({
            "case": label, "mode": mode, "figures_written": written,
            "figures_skipped": skipped,
            "registry_total": alas_report::RESULT_FIGURES.len(),
        }))?,
    )?;
    println!(
        "{label}@{mode}: {written} figures, {} skipped",
        skipped.len()
    );
    Ok(())
}
