// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Render the propulsion figures that exercise ALAS engineering typography.
//!
//! This is a fast visual check for the glyphs most likely to be absent from a
//! general UI font: Greek efficiency labels with letter subscripts, dotted
//! mass flow, SI separators, and the T-s diagram's reference entropy.

use std::error::Error;
use std::path::{Path, PathBuf};

use alas_config::AlasConfig;
use alas_report::families::propulsion::{
    figure_engine_designer_preview, figure_propulsion_carpet_plot,
    figure_propulsion_efficiency_decomposition,
};
use alas_report::Scene;

fn write_scene(directory: &Path, name: &str, scene: &Scene) -> Result<(), Box<dyn Error>> {
    std::fs::write(
        directory.join(format!("{name}.png")),
        alas_viz::raster::render_scene_png(scene).map_err(std::io::Error::other)?,
    )?;
    std::fs::write(
        directory.join(format!("{name}.svg")),
        alas_report::render_svg(scene),
    )?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let directory = std::env::args_os().nth(1).map_or_else(
        || PathBuf::from("outputs/engineering_typography_audit"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&directory)?;

    let config = AlasConfig::default();
    for theme in ["dark-accessible", "light"] {
        write_scene(
            &directory,
            &format!("efficiency-decomposition-{theme}"),
            &figure_propulsion_efficiency_decomposition(&config, Some(theme)),
        )?;
        write_scene(
            &directory,
            &format!("carpet-plot-{theme}"),
            &figure_propulsion_carpet_plot(&config, Some(theme)),
        )?;
        write_scene(
            &directory,
            &format!("ts-preview-{theme}"),
            &figure_engine_designer_preview(&config, Some(theme)),
        )?;
    }
    Ok(())
}
