// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Render the manufacturer-style load-and-trim sheet for registered presets.
//!
//! `cargo run --release -p alas-report --example load_trim_sheet -- <out_dir> [PRESET ...]`
//! evaluates each preset as itself (BaselineSandbox) through the full
//! pipeline and writes `<preset>_load_trim_{light,dark}.svg`.

#![allow(clippy::print_stderr)]

use std::path::PathBuf;

use alas_config::optimizer::DesignMode;
use alas_config::AlasConfig;
use alas_pipeline::{DesignPipeline, PipelineOptions, RunEnvironment};
use alas_report::families::mass_balance::load_trim::{
    data::load_trim_data_from_pipeline, figure_load_trim_sheet,
};
use alas_report::svg::render_svg;
use alas_report::theme::{PALETTE_DARK, PALETTE_LIGHT};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let out_dir = PathBuf::from(args.next().unwrap_or_else(|| "load_trim".to_owned()));
    let mut presets: Vec<String> = args.collect();
    if presets.is_empty() {
        presets = vec!["A320-200".to_owned()];
    }
    std::fs::create_dir_all(&out_dir)?;
    for name in presets {
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))?;
        config.structures.run_nastran = false;
        config.structures.run_patran_export = false;
        config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
        let options = PipelineOptions {
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
        let result = DesignPipeline::new(config).run(&options, &RunEnvironment::default())?;
        let Some(data) = load_trim_data_from_pipeline(&result) else {
            eprintln!("{name}: no CG assessment in this run");
            continue;
        };
        for (theme, pal) in [("light", &PALETTE_LIGHT), ("dark", &PALETTE_DARK)] {
            let path = out_dir.join(format!("{name}_load_trim_{theme}.svg"));
            std::fs::write(&path, render_svg(&figure_load_trim_sheet(&data, pal)))?;
            eprintln!("wrote {}", path.display());
        }
    }
    Ok(())
}
