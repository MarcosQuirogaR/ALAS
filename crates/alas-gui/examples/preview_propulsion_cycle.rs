// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Render both engine technologies for visual review without running a mission.
use alas_config::AlasConfig;
use alas_report::{families::propulsion::figure_engine_designer_preview, svg::render_svg};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.agent/reports/propulsion-cycle");
    std::fs::create_dir_all(&output)?;
    let turboprop = AlasConfig::from_value(&serde_json::json!({"preset":"ATR72-600"}))?;
    for (name, config) in [
        ("turbofan", AlasConfig::default()),
        ("turboprop", turboprop),
    ] {
        for theme in ["dark", "light"] {
            let scene = figure_engine_designer_preview(&config, Some(theme));
            std::fs::write(
                output.join(format!("{name}-{theme}.svg")),
                render_svg(&scene),
            )?;
            std::fs::write(
                output.join(format!("{name}-{theme}.png")),
                alas_viz::raster::render_scene_png(&scene).map_err(std::io::Error::other)?,
            )?;
        }
    }
    Ok(())
}
