// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Headless export of the Weight & Balance results page as PNG (light theme).
//!
//! `cargo run --release -p alas-viz --example wb_page_png -- <out_dir> [PRESET ...]`
//! evaluates each preset as itself (no optimization) and writes
//! `<out_dir>/<preset>_<figure>.png` for the load-and-trim sheet, the mass
//! breakdown, the fuel-volume check and the plan-view mass distribution, the
//! four figures that state their mass basis.

#![allow(clippy::print_stderr)]

use std::path::PathBuf;

use alas_report::families::mass_balance::{figure_mass_breakdown, figure_mass_distribution};
use alas_report::scene::Scene;

fn run(out_dir: PathBuf, presets: Vec<String>) -> Result<(), String> {
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let theme = Some("light");
    for name in presets {
        let mut config = alas_config::AlasConfig::from_value(&serde_json::json!({ "preset": name }))
            .map_err(|e| e.to_string())?;
        config.structures.run_nastran = false;
        config.structures.run_patran_export = false;
        config.optimizer.design_space.mode = alas_config::optimizer::DesignMode::BaselineSandbox;
        let options = alas_pipeline::PipelineOptions {
            optimize: false,
            compare_baseline: false,
            parallel: true,
            aerodynamic_solver: Default::default(),
            optimization_solver: Default::default(),
            output_dir: None,
            save_plots: false,
            seed: Some(42),
            quiet: true,
        };
        let result = alas_pipeline::DesignPipeline::new(config)
            .run(&options, &alas_pipeline::RunEnvironment::default())
            .map_err(|e| e.to_string())?;
        let report = result
            .optimized_report
            .as_ref()
            .or(result.baseline_analysis.as_ref())
            .ok_or("the run has no analysis report")?;
        let mut figures: Vec<(&str, Scene)> = vec![
            ("mass_breakdown", figure_mass_breakdown(report, theme)),
            (
                "fuel_volume",
                alas_report::families::mass_balance_layout::figure_fuel_volume_check_for_loading(
                    &result.feasibility.fuel_loading,
                    theme,
                ),
            ),
            ("mass_distribution", figure_mass_distribution(report, theme)),
        ];
        if let Some(data) =
            alas_report::families::mass_balance::load_trim::data::load_trim_data_from_pipeline(
                &result,
            )
        {
            figures.push((
                "load_trim",
                alas_report::families::mass_balance::load_trim::figure_load_trim_sheet(
                    &data,
                    alas_report::theme::get_palette(theme),
                ),
            ));
        }
        for (label, scene) in figures {
            let path = out_dir.join(format!("{name}_{label}.png"));
            std::fs::write(&path, alas_viz::raster::render_scene_png(&scene)?)
                .map_err(|e| e.to_string())?;
            eprintln!("wrote {}", path.display());
        }
    }
    Ok(())
}

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let out_dir = PathBuf::from(args.next().unwrap_or_else(|| "wb_page".to_owned()));
    let mut presets: Vec<String> = args.collect();
    if presets.is_empty() {
        presets = vec!["A320-200".to_owned()];
    }
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || run(out_dir, presets))
        .map_err(|e| e.to_string())?
        .join()
        .map_err(|_| "export thread panicked".to_owned())?
}
