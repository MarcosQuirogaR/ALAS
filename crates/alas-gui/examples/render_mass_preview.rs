// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Export the Mass navigation preview and surface-figure raster samples.

// AppState owns private caches, so external callers initialize it through Default.
#[allow(clippy::field_reassign_with_default)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("output SVG path required")?;
    let mut config =
        alas_config::AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"}))?;
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
        .run(&options, &alas_pipeline::RunEnvironment::default())?;
    let mut state = alas_gui::AppState::default();
    state.config_values = serde_json::to_value(&result.config)?;
    state.set_completed_pipeline_result(result);
    let scene =
        alas_gui::scene::build_page_preview(&state, "mass_cg").ok_or("mass preview unavailable")?;
    let path = std::path::Path::new(&path);
    std::fs::write(path, alas_report::svg::render_svg(&scene))?;
    std::fs::write(
        path.with_extension("png"),
        alas_viz::raster::render_scene_png(&scene).map_err(std::io::Error::other)?,
    )?;
    let plane = alas_geom::builder::AircraftBuilder::new(None).build(None, true)?;
    for (name, scene) in [
        (
            "wireframe_wing_light.png",
            alas_report::families::geometry::figure_wireframe_wing(&plane, Some("light")),
        ),
        (
            "wireframe_empennage_light.png",
            alas_report::families::geometry::figure_wireframe_empennage(&plane, Some("light")),
        ),
    ] {
        std::fs::write(
            path.with_file_name(name),
            alas_viz::raster::render_scene_png(&scene).map_err(std::io::Error::other)?,
        )?;
    }
    Ok(())
}
