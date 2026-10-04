// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Export the shared live-preview surface renderer as standalone report SVGs.

use std::path::PathBuf;

use alas_geom::builder::AircraftBuilder;
use alas_report::families::geometry::{
    figure_exterior_3d, figure_wireframe_empennage, figure_wireframe_fuselage,
    figure_wireframe_wing,
};
use alas_report::svg::render_svg;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args().nth(1).map_or_else(
        || std::env::temp_dir().join("wireframe_figures"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&output)?;
    let plane = AircraftBuilder::new(None).build(None, true)?;
    for theme in ["light", "dark"] {
        for (name, scene) in [
            ("wing", figure_wireframe_wing(&plane, Some(theme))),
            ("empennage", figure_wireframe_empennage(&plane, Some(theme))),
            ("fuselage", figure_wireframe_fuselage(&plane, Some(theme))),
            ("exterior", figure_exterior_3d(&plane, None, Some(theme))),
        ] {
            std::fs::write(
                output.join(format!("wireframe_{name}_{theme}.svg")),
                render_svg(&scene),
            )?;
        }
    }
    Ok(())
}
