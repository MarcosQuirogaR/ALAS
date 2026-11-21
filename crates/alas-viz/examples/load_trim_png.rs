// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Headless export of the load-and-trim sheet as PNG (light theme).
//!
//! `cargo run --release -p alas-viz --example load_trim_png -- <out_dir> [PRESET ...]`
//! evaluates each preset as itself (no optimization) and writes
//! `<out_dir>/<preset>_load_trim.png`. The pipeline runs on a large stack,
//! as the desktop analysis worker does.

#![allow(clippy::print_stderr)]

use std::path::PathBuf;

fn run(out_dir: PathBuf, presets: Vec<String>) -> Result<(), String> {
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    for name in presets {
        let mut config =
            alas_config::AlasConfig::from_value(&serde_json::json!({ "preset": name }))
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
        let Some(data) =
            alas_report::families::mass_balance::load_trim::data::load_trim_data_from_pipeline(
                &result,
            )
        else {
            eprintln!("{name}: no CG assessment in this run");
            continue;
        };
        let scene = alas_report::families::mass_balance::load_trim::figure_load_trim_sheet(
            &data,
            alas_report::theme::get_palette(Some("light")),
        );
        let path = out_dir.join(format!("{name}_load_trim.png"));
        std::fs::write(&path, alas_viz::raster::render_scene_png(&scene)?)
            .map_err(|e| e.to_string())?;
        eprintln!("wrote {}", path.display());
    }
    Ok(())
}

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let out_dir = PathBuf::from(args.next().unwrap_or_else(|| "load_trim".to_owned()));
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
