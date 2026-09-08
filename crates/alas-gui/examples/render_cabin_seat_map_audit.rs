// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Render generated cabin maps and sections for every registered aircraft preset.

use std::error::Error;
use std::path::PathBuf;

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_gui::scene::localize_scene_for_display;
use alas_payload::build::build_payload_layout;
use alas_report::families::geometry::{figure_cabin_payload, figure_main_deck_seat_map};
use alas_report::families::mass_balance_layout::figure_cabin_cross_section;
use alas_report::render_svg;

fn write_scene(
    directory: &std::path::Path,
    name: &str,
    scene: &alas_report::Scene,
) -> Result<(), Box<dyn Error>> {
    std::fs::write(
        directory.join(format!("{name}.png")),
        alas_viz::raster::render_scene_png(scene).map_err(std::io::Error::other)?,
    )?;
    std::fs::write(directory.join(format!("{name}.svg")), render_svg(scene))?;
    Ok(())
}

fn render_preset(directory: &std::path::Path, name: &str) -> Result<(), Box<dyn Error>> {
    let preset = presets::get(name)?;
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))?;
    let aircraft = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .map_err(|error| std::io::Error::other(format!("{name} geometry: {error:?}")))?;
    let layout = build_payload_layout(&aircraft, &config, 0.0, 0.0)?;
    let stem = name.to_ascii_lowercase().replace('-', "_");
    write_scene(
        directory,
        &format!("{stem}_seat_map_es_dark"),
        &localize_scene_for_display(figure_main_deck_seat_map(
            &layout,
            &aircraft,
            &config,
            Some("dark-accessible"),
        )),
    )?;
    write_scene(
        directory,
        &format!("{stem}_cabin_payload_es_dark"),
        &localize_scene_for_display(figure_cabin_payload(
            &layout,
            &aircraft,
            &config,
            Some("dark-accessible"),
        )),
    )?;
    write_scene(
        directory,
        &format!("{stem}_cabin_section_es_dark"),
        &localize_scene_for_display(figure_cabin_cross_section(
            &layout,
            &aircraft,
            &config,
            Some("dark-accessible"),
        )),
    )
}

fn main() -> Result<(), Box<dyn Error>> {
    let directory = std::env::args_os().nth(1).map_or_else(
        || PathBuf::from("outputs/cabin_seat_map_audit"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&directory)?;
    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    for preset in presets::available() {
        render_preset(&directory, preset)?;
    }
    Ok(())
}
