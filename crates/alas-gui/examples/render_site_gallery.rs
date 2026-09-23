// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Regenerate the documentation gallery (`site/docs-site/docs/assets/ave-*`)
//! as light and dark PNGs from one AVE pipeline run of the default
//! configuration, through the same dispatch the CLI `--plots` path and the
//! GUI results view use (`alas_gui::scene::build_result_figure`).
//!
//! Figures this cannot reproduce are left to their own sources and must not
//! be overwritten from its output: the MSES and Patran figures need those
//! external tools, and `ave-mission-route` was drawn from a SimBrief flight
//! plan, whereas this run flies the default notional route.
//!
//! Usage (writes only to the two directories named):
//!   cargo run --release -p alas-gui --example render_site_gallery -- \
//!       --seed 42 --output <run-dir> --figures-out <png-dir>

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::error::Error;
use std::path::PathBuf;

use alas_config::AlasConfig;
use alas_exec::ToolLocator;
use alas_gui::scene::build_result_figure;
use alas_gui::AppState;
use alas_pipeline::{DesignPipeline, PipelineOptions};
use alas_report::families::stability::figure_stability_metrics;

/// (site-asset basename without theme/extension, RESULT_FIGURES id).
/// Special-cased ids that are not real RESULT_FIGURES entries are handled
/// directly below.
const FIGURE_MAP: &[(&str, &str)] = &[
    ("ave-aero-panel", "aero_panel"),
    ("ave-airfoil-evolution", "airfoil_evolution"),
    ("ave-airfoil-reynolds", "airfoil_reynolds"),
    ("ave-cabin-payload", "cabin_payload"),
    ("ave-cg-envelope", "cg_envelope"),
    ("ave-control-surfaces", "control_surfaces"),
    ("ave-design-evolution", "design_evolution"),
    ("ave-drag-breakdown", "drag_breakdown"),
    ("ave-dynamic-modes", "dynamic_modes"),
    ("ave-fuel-volume", "fuel_volume_check"),
    ("ave-landing-gear", "landing_gear_planform"),
    ("ave-lto-arrival", "lto_arrival"),
    ("ave-lto-departure", "lto_departure"),
    ("ave-mass-breakdown", "mass_breakdown"),
    ("ave-mass-distribution", "mass_distribution"),
    ("ave-matching-chart", "matching_chart"),
    ("ave-mission-aero-coefficients", "mission_aero_coefficients"),
    ("ave-mission-aero-forces", "mission_aero_forces"),
    ("ave-mission-drag-components", "mission_drag_components"),
    ("ave-mission-flight-path", "mission_flight_path"),
    ("ave-mission-profile", "mission_profile"),
    ("ave-mission-route", "mission_route_2d"),
    ("ave-mission-velocities", "mission_velocities"),
    ("ave-model-comparison", "model_comparison"),
    ("ave-optimization-history", "optimization_history"),
    ("ave-payload-range", "payload_range"),
    ("ave-planform-comparison", "planform_comparison"),
    ("ave-polar-comparison", "polar_comparison"),
    ("ave-propulsion-altitude", "propulsion_altitude_sweep"),
    ("ave-propulsion-bpr", "propulsion_bpr_sensitivity"),
    ("ave-propulsion-carpet", "propulsion_carpet_plot"),
    ("ave-propulsion-cycle", "propulsion_cycle_summary"),
    (
        "ave-propulsion-efficiency",
        "propulsion_efficiency_decomposition",
    ),
    ("ave-span-loading", "span_loading"),
    ("ave-stability-side-view", "stability_side_view"),
    ("ave-structures-loads", "structures_loads"),
    ("ave-structures-modes", "structures_modes"),
    ("ave-structures-sizing", "structures_sizing"),
    ("ave-structures-stress", "structures_stress"),
    ("ave-threeview-3d", "threeview"),
    ("ave-vlm-flow", "vlm_flow"),
    ("ave-vn-diagram", "vn_diagram"),
];

fn write_png(path: &std::path::Path, scene: alas_report::Scene) -> Result<(), Box<dyn Error>> {
    let png = alas_viz::raster::render_scene_png(&scene).map_err(std::io::Error::other)?;
    std::fs::write(path, png)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    let no_optimize = args.iter().any(|a| a == "--no-optimize");
    let output_dir = args
        .iter()
        .position(|a| a == "--output")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("outputs/site_gallery_run"));
    let figures_out = args
        .iter()
        .position(|a| a == "--figures-out")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("outputs/site_gallery_figures"));
    let seed: Option<u64> = args
        .iter()
        .position(|a| a == "--seed")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok());

    std::fs::create_dir_all(&output_dir)?;
    std::fs::create_dir_all(&figures_out)?;

    let config = AlasConfig::default();
    let pipeline = DesignPipeline::new(config.clone());
    let options = PipelineOptions {
        optimize: !no_optimize,
        compare_baseline: true,
        parallel: true,
        aerodynamic_solver: alas_pipeline::AerodynamicSolverMode::Vlm,
        optimization_solver: alas_pipeline::OptimizationSolverMode::Vlm,
        output_dir: Some(output_dir.clone()),
        save_plots: false,
        seed,
        quiet: false,
    };

    let locator = ToolLocator::for_current_process();
    let environment = locator.resolve_environment(
        std::path::Path::new(""),
        std::path::Path::new(""),
        std::path::Path::new(""),
        std::path::Path::new(""),
        std::path::Path::new(""),
    );

    eprintln!(
        "Running pipeline (optimize={}, compare_baseline=true)...",
        options.optimize
    );
    let result = pipeline.run_with_environment_and_events(&options, &environment, &|_event| {})?;
    eprintln!("Pipeline run complete.");

    let mut state = AppState::default();
    state.set_completed_pipeline_result(result.clone());

    let mut written = 0usize;
    let mut skipped: Vec<String> = Vec::new();

    for (site_name, id) in FIGURE_MAP {
        for theme in ["dark", "light"] {
            match build_result_figure(&state, id, &config, theme) {
                Some(Some(scene)) => {
                    let path = figures_out.join(format!("{site_name}-{theme}.png"));
                    write_png(&path, scene)?;
                    written += 1;
                    println!("wrote {}", path.display());
                }
                Some(None) => {
                    skipped.push(format!(
                        "{site_name} ({id}) [{theme}]: no data for this run"
                    ));
                }
                None => {
                    skipped.push(format!(
                        "{site_name} ({id}) [{theme}]: pipeline result missing required stage"
                    ));
                }
            }
        }
    }

    // stability_metrics is not registered in RESULT_FIGURES / build_result_figure's
    // dispatch table; call the family function directly with the same report
    // selection build_result_figure would use (optimized report, falling back
    // to baseline).
    if let Some(report) = result
        .optimized_report
        .as_ref()
        .or(result.baseline_analysis.as_ref())
    {
        for theme in ["dark", "light"] {
            let scene = figure_stability_metrics(report, Some(&config), Some(theme));
            let path = figures_out.join(format!("ave-stability-metrics-{theme}.png"));
            write_png(&path, scene)?;
            written += 1;
            println!("wrote {}", path.display());
        }
    } else {
        skipped.push("ave-stability-metrics: no analysis report available".to_owned());
    }

    println!("\nWrote {written} PNG(s) to {}", figures_out.display());
    if !skipped.is_empty() {
        println!("\nSkipped:");
        for s in &skipped {
            println!("  {s}");
        }
    }

    Ok(())
}
