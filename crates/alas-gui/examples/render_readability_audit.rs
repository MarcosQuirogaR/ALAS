// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Render representative dark-mode figures for first-hand readability review.

use std::error::Error;
use std::path::{Path, PathBuf};

use alas_config::AlasConfig;
use alas_pipeline::{DesignPipeline, PipelineOptions, RunEnvironment};
use alas_report::families::{mission, performance, stability};
use alas_report::Scene;

fn write_scene(directory: &Path, name: &str, scene: &Scene) -> Result<(), Box<dyn Error>> {
    let png = alas_viz::raster::render_scene_png(scene).map_err(std::io::Error::other)?;
    std::fs::write(directory.join(format!("{name}.png")), png)?;
    std::fs::write(
        directory.join(format!("{name}.svg")),
        alas_report::render_svg(scene),
    )?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let directory = std::env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("outputs/readability_audit"), PathBuf::from);
    std::fs::create_dir_all(&directory)?;

    let config = AlasConfig::default();
    let result = DesignPipeline::new(config.clone())
        .run(
            &PipelineOptions {
                optimize: false,
                compare_baseline: false,
                parallel: false,
                aerodynamic_solver: Default::default(),
                optimization_solver: Default::default(),
                output_dir: None,
                save_plots: false,
                seed: Some(42),
                quiet: true,
            },
            &RunEnvironment::default(),
        )
        .map_err(std::io::Error::other)?;
    let report = result
        .optimized_report
        .as_ref()
        .ok_or_else(|| std::io::Error::other("pipeline produced no report"))?;
    let theme = Some("dark-accessible");

    write_scene(
        &directory,
        "matching_chart_dark",
        &performance::figure_matching_chart(report, &config, theme),
    )?;
    write_scene(
        &directory,
        "lto_departure_dark",
        &performance::figure_lto_departure(report, &config, theme),
    )?;
    write_scene(
        &directory,
        "stability_metrics_dark",
        &stability::figure_stability_metrics(report, Some(&config), theme),
    )?;
    if let Some(mission_result) = result.mission_result.as_ref() {
        write_scene(
            &directory,
            "mission_profile_dark",
            &mission::figure_mission_profile(mission_result, theme),
        )?;
    }
    Ok(())
}
