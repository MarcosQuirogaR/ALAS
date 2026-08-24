// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Render a localized GUI-dispatch scene for first-hand Spanish text review.

use std::error::Error;
use std::path::PathBuf;

use alas_gui::scene::build_page_preview;
use alas_gui::state::AppState;

fn write_scene(
    directory: &std::path::Path,
    name: &str,
    scene: &alas_report::Scene,
) -> Result<(), Box<dyn Error>> {
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
    let directory = std::env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("outputs/spanish_ui_audit"), PathBuf::from);
    std::fs::create_dir_all(&directory)?;

    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    let state = AppState::default();
    let three_view = build_page_preview(&state, "geometry")
        .ok_or_else(|| std::io::Error::other("Spanish geometry preview was unavailable"))?;
    write_scene(&directory, "threeview_es", &three_view)?;
    let drag = build_page_preview(&state, "drag")
        .ok_or_else(|| std::io::Error::other("Spanish drag preview was unavailable"))?;
    write_scene(&directory, "drag_es", &drag)?;
    Ok(())
}
