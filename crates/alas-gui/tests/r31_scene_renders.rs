// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Headless renders of the Aircraft-page scenes for review.
//! The pictures go to the folder named by `ALAS_RENDER_DIR` when it is set and exists.

// A render test panics when its fixture is unusable; that is the failure.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_gui::AppState;

fn out_dir() -> Option<std::path::PathBuf> {
    let dir = std::path::PathBuf::from(std::env::var("ALAS_RENDER_DIR").unwrap_or_default());
    dir.is_dir().then_some(dir)
}

fn write(name: &str, scene: &alas_report::Scene) {
    if let Some(dir) = out_dir() {
        let png = alas_viz::raster::render_scene_png(scene).expect("png");
        std::fs::write(dir.join(name), png).expect("write");
    }
}

fn ave_state() -> AppState {
    let mut state = AppState::default();
    state.config_values = serde_json::json!({"preset": "AVE"});
    state
}

#[test]
fn three_view_renders() {
    let state = ave_state();
    for (name, canvas) in [
        ("a3-threeview-grid.png", (800.0, 700.0)),
        ("a3-threeview-row.png", (1700.0, 460.0)),
    ] {
        let scene = alas_gui::scene::build_page_preview_sized(&state, "geometry", Some(canvas))
            .expect("scene");
        write(name, &scene);
    }
}

#[test]
fn load_trim_renders() {
    let config =
        alas_config::AlasConfig::from_value(&serde_json::json!({"preset": "AVE"})).unwrap();
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
        .unwrap();
    let data =
        alas_report::families::mass_balance::load_trim::data::load_trim_data_from_pipeline(&result)
            .unwrap();
    for width in [640.0, 900.0, 1000.0, 1100.0, 1300.0] {
        let scene = alas_report::families::mass_balance::load_trim::figure_load_trim_sheet_sized(
            &data,
            alas_report::theme::get_palette(Some("dark")),
            width,
        );
        write(&format!("a6-loadtrim-{width}.png"), &scene);
    }
}

#[test]
fn ts_renders() {
    let state = ave_state();
    for (name, canvas) in [
        ("a8-ts-800.png", (800.0, 470.0)),
        ("a8-ts-1400.png", (1400.0, 520.0)),
    ] {
        let scene = alas_gui::scene::build_page_preview_sized(&state, "engine", Some(canvas))
            .expect("scene");
        write(name, &scene);
    }
}

#[test]
fn cabin_renders() {
    let state = ave_state();
    for (name, canvas) in [
        ("a5-cabin-tall.png", (300.0, 900.0)),
        ("a5-cabin-wide.png", (900.0, 420.0)),
        ("a5-cabin-dock.png", (420.0, 700.0)),
    ] {
        let scene = alas_gui::scene::build_page_preview_sized(&state, "cabin_3d", Some(canvas))
            .expect("scene");
        write(name, &scene);
    }
}
