// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Render the load-and-trim sheet of every registered transport preset from
//! one seeded analysis (no optimization) each, as PNG in the light theme and,
//! for the first preset, the dark theme, plus the Mass navigation preview of
//! the same run at the dock's default width, for visual review.
//!
//! Usage: `render_load_trim_presets <output directory> [preset ...]`.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::Path;

const PRESETS: [&str; 8] = [
    "A320-200",
    "A220-300",
    "A340-300",
    "A380-800",
    "B787-9",
    "DC-10",
    "ATR72-600",
    "AVE",
];

// AppState owns private caches, so it is initialized through Default.
#[allow(clippy::field_reassign_with_default)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let output = args.next().unwrap_or_else(|| {
        std::env::temp_dir()
            .join("alas-shots/qa")
            .to_string_lossy()
            .into_owned()
    });
    let chosen: Vec<String> = args.collect();
    let presets: Vec<String> = if chosen.is_empty() {
        PRESETS.iter().map(|p| (*p).to_owned()).collect()
    } else {
        chosen
    };
    let directory = Path::new(&output);
    std::fs::create_dir_all(directory)?;
    for (index, preset) in presets.iter().enumerate() {
        let config = alas_config::AlasConfig::from_value(&serde_json::json!({"preset": preset}))?;
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
        let result = match alas_pipeline::DesignPipeline::new(config)
            .run(&options, &alas_pipeline::RunEnvironment::default())
        {
            Ok(result) => result,
            Err(error) => {
                eprintln!("{preset}: pipeline failed: {error}");
                continue;
            }
        };
        let slug = preset.to_ascii_lowercase().replace(['-', ' '], "");
        let Some(data) =
            alas_report::families::mass_balance::load_trim::data::load_trim_data_from_pipeline(
                &result,
            )
        else {
            eprintln!("{preset}: no load-and-trim data");
            continue;
        };
        let themes: &[&str] = if index == 0 {
            &["light", "dark"]
        } else {
            &["light"]
        };
        for theme in themes {
            let scene = alas_report::families::mass_balance::load_trim::figure_load_trim_sheet(
                &data,
                alas_report::theme::get_palette(Some(theme)),
            );
            let path = directory.join(format!("loadtrim_{slug}_{theme}.png"));
            std::fs::write(
                &path,
                alas_viz::raster::render_scene_png(&scene).map_err(std::io::Error::other)?,
            )?;
            println!("wrote {}", path.display());
        }
        let mut state = alas_gui::AppState::default();
        state.theme = alas_gui::AppTheme::Light;
        state.config_values = serde_json::to_value(&result.config)?;
        state.set_completed_pipeline_result(result);
        if let Some(scene) = alas_gui::scene::build_page_preview(&state, "mass_cg") {
            let path = directory.join(format!("loadtrim_{slug}_mass_preview.png"));
            std::fs::write(
                &path,
                alas_viz::raster::render_scene_png(&scene).map_err(std::io::Error::other)?,
            )?;
            println!("wrote {} ({:?})", path.display(), scene.title);
        }
    }
    Ok(())
}
