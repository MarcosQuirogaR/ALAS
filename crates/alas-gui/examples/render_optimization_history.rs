// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Render the optimization history of a small seeded run, exactly as the
//! results view builds it, plus a 40 000-request synthetic trace, as SVG and
//! PNG in both themes, and time the egui shape conversion of the large one.
//!
//! Usage: `render_optimization_history [output directory] [screening budget]
//! [refinement budget] [preset]`.

use std::path::Path;
use std::time::Instant;

use alas_opt::{EvaluationTrace, TraceClass, TraceStage};

fn write_scene(
    directory: &Path,
    name: &str,
    scene: &alas_report::scene::Scene,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::write(
        directory.join(format!("{name}.svg")),
        alas_report::svg::render_svg(scene),
    )?;
    std::fs::write(
        directory.join(format!("{name}.png")),
        alas_viz::raster::render_scene_png(scene).map_err(std::io::Error::other)?,
    )?;
    Ok(())
}

/// A deterministic 40 000-request trace shaped like a long run: a screening
/// quarter, a refinement, a restoration tail and three verifications.
fn synthetic_trace(n: usize) -> EvaluationTrace {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut uniform = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut trace = EvaluationTrace {
        screening_separate: true,
        ..EvaluationTrace::default()
    };
    for index in 0..n {
        let stage = if index < n / 4 {
            TraceStage::Screening
        } else if index + 400 < n {
            TraceStage::Refinement
        } else if index + 3 < n {
            TraceStage::Restoration
        } else {
            TraceStage::Verification
        };
        let progress = index as f64 / n as f64;
        let fuel = 18_000.0 + 9_000.0 * (1.0 - progress).powi(2) * uniform() + 800.0 * uniform();
        let draw = uniform();
        let (class, objective) = if draw < 0.01 || stage == TraceStage::Verification {
            (TraceClass::Valid, Some(fuel as f32))
        } else if draw < 0.08 {
            (TraceClass::Failed, None)
        } else if draw < 0.35 {
            (TraceClass::Rejected, None)
        } else {
            (TraceClass::Rejected, Some((fuel * 1.15) as f32))
        };
        trace.push(stage, class, objective, objective.map(|v| v / 20_000.0));
    }
    trace
}

// AppState owns private caches, so external callers initialize it through
// Default; the diagnostic writes its counts and timings to standard output.
#[allow(clippy::field_reassign_with_default, clippy::print_stdout)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let output = args.next().unwrap_or_else(|| {
        std::env::temp_dir()
            .join("alas-shots")
            .to_string_lossy()
            .into_owned()
    });
    let screening: i64 = args.next().map_or(Ok(600), |value| value.parse())?;
    let refinement: i64 = args.next().map_or(Ok(400), |value| value.parse())?;
    let preset = args.next().unwrap_or_else(|| "A220-300".to_owned());
    let directory = Path::new(&output);
    std::fs::create_dir_all(directory)?;

    let mut config = alas_config::AlasConfig::from_value(&serde_json::json!({"preset": preset}))?;
    config.optimizer.solver.screening.max_evaluations = screening;
    config.optimizer.solver.refinement.max_evaluations = refinement;
    config.optimizer.solver.stop_on_evaluations_only = true;
    config.structures.enabled = false;
    let draft = &alas_config::fidelity_presets::get("draft")
        .map_err(|error| format!("{error:?}"))?
        .analysis;
    let analysis = &mut config.analysis;
    analysis.sweep_n_points = draft.sweep_n_points;
    analysis.spanwise_resolution = draft.spanwise_resolution;
    analysis.chordwise_resolution = draft.chordwise_resolution;
    analysis.fine_spanwise_resolution = draft.fine_spanwise_resolution;
    analysis.fine_chordwise_resolution = draft.fine_chordwise_resolution;
    let options = alas_pipeline::PipelineOptions {
        optimize: true,
        compare_baseline: false,
        parallel: true,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };
    let started = Instant::now();
    let result = alas_pipeline::DesignPipeline::new(config)
        .run(&options, &alas_pipeline::RunEnvironment::default())?;
    println!("pipeline: {:.1} s", started.elapsed().as_secs_f64());
    if let Some(trace) = result
        .optimization_result
        .as_ref()
        .map(alas_opt::OptimizationResult::evaluation_trace)
    {
        let per_stage: Vec<(TraceStage, usize)> = TraceStage::ALL
            .iter()
            .map(|&stage| {
                let count = trace
                    .evaluations
                    .iter()
                    .filter(|evaluation| evaluation.stage == stage)
                    .count();
                (stage, count)
            })
            .collect();
        println!(
            "trace: {} requests, valid {}, rejected {}, failed {}, per stage {per_stage:?}",
            trace.len(),
            trace.count(TraceClass::Valid),
            trace.count(TraceClass::Rejected),
            trace.count(TraceClass::Failed)
        );
    }
    let mut state = alas_gui::AppState::default();
    state.config_values = serde_json::to_value(&result.config)?;
    let config = result.config.clone();
    state.set_completed_pipeline_result(result);
    for theme in ["light", "dark"] {
        let scene =
            alas_gui::scene::build_result_figure(&state, "optimization_history", &config, theme)
                .flatten()
                .ok_or("optimization history unavailable")?;
        write_scene(directory, &format!("history_run_{theme}"), &scene)?;
    }

    let trace = synthetic_trace(40_000);
    for theme in ["light", "dark"] {
        let started = Instant::now();
        let scene = alas_report::families::optimization::figure_evaluation_trace(
            &trace,
            Some(alas_config::ObjectiveKind::BlockFuel),
            Some(theme),
        );
        let built = started.elapsed();
        let transform = alas_viz::ViewportTransform::fit(
            scene.width,
            scene.height,
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440.0, 960.0)),
        );
        let context = egui::Context::default();
        let _ = context.run(egui::RawInput::default(), |_| {});
        let started = Instant::now();
        let shapes = alas_viz::render_scene_to_shapes_with_context(&scene, &transform, &context);
        println!(
            "synthetic {theme}: {} elements, scene {:.1} ms, {} egui shapes {:.1} ms",
            scene.elements.len(),
            built.as_secs_f64() * 1e3,
            shapes.len(),
            started.elapsed().as_secs_f64() * 1e3
        );
        write_scene(directory, &format!("history_synthetic40k_{theme}"), &scene)?;
    }
    Ok(())
}
