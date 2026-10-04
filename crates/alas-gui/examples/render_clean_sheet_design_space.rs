// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Drive the desktop clean-sheet path headlessly: load the A320-200, choose
//! New aircraft, enter a C919-class brief through the configuration tree the
//! forms edit, render the Design Space page (with its derived bounds) to a
//! PNG, then optionally start the run through the same `start_pipeline` the
//! Run button calls and report the search's feasible count.
//!
//! Usage:
//!   cargo run --release -p alas-gui --example render_clean_sheet_design_space -- \
//!       --png <file.png> [--run]
//!
//! External tools (MSES, OpenVSP, VSPAERO, AVL, FlowUnsteady, Nastran,
//! Patran) are disabled, outputs are not retained and the seed is fixed.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::error::Error;
use std::time::{Duration, Instant};

use alas_config::DesignMode;
use alas_gui::views::show_design_space_view;
use alas_gui::{apply_theme, AppState, AppTheme};
use alas_pipeline::AerodynamicSolverMode;
use egui::{pos2, Context, Pos2, RawInput, Rect};
use serde_json::{json, Value};

/// Window size of the rendered page, points, and its pixel scale.
const SIZE: (f32, f32) = (1280.0, 2400.0);
const PIXELS_PER_POINT: f32 = 1.0;

/// The C919-class brief entered on top of the A320-200 shape.
fn c919_brief() -> Value {
    json!({
        "requirements": {
            "cruise_mach": 0.785, "cruise_altitude_m": 11277.6, "mtow_kg": 72500.0,
            "num_passengers": 168, "max_structural_payload_kg": 18900.0,
            "cabin_preset": "Ryanair"
        },
        "geometry": {"fuselage": {"diameter_m": 3.96, "height_m": 4.166}},
        "mass_model": {"flops_transport": {
            "design_range_nmi": 2200.0, "maximum_fuel_capacity_kg": 20033.0,
            "haul_class": "short_medium_haul", "flight_attendant_count": 4
        }},
        "mses": {"enabled": false},
        "downstream": {"openvsp": false, "vspaero": false, "avl": false, "flowunsteady": false},
        "structures": {"run_nastran": false, "run_patran_export": false},
        "optimizer": {"objective": {"design_range_nmi": 2200.0, "mtow_sizing": "fixed_requirement"}},
        "departure_airport": "HKJK",
        "arrival_airport": "OMDB"
    })
}

fn merge(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(target), Value::Object(patch)) => {
            for (key, value) in patch {
                merge(target.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
        (target, patch) => *target = patch.clone(),
    }
}

fn frame(ctx: &Context, state: &mut AppState) -> egui::FullOutput {
    let input = RawInput {
        screen_rect: Some(Rect::from_min_max(Pos2::ZERO, pos2(SIZE.0, SIZE.1))),
        ..Default::default()
    };
    ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| show_design_space_view(state, ui));
    })
}

/// Rasterize egui's tessellated meshes, sampling the font atlas per pixel so
/// text is legible. Premultiplied colours, source-over blending.
fn rasterize(ctx: &Context, output: egui::FullOutput) -> (u32, u32, Vec<u8>) {
    let scale = PIXELS_PER_POINT;
    let (w, h) = ((SIZE.0 * scale) as usize, (SIZE.1 * scale) as usize);
    let background = ctx.style().visuals.panel_fill;
    let mut buffer: Vec<[f32; 4]> = vec![
        [
            f32::from(background.r()) / 255.0,
            f32::from(background.g()) / 255.0,
            f32::from(background.b()) / 255.0,
            1.0,
        ];
        w * h
    ];
    let atlas = ctx.fonts(|f| f.image());
    let [atlas_w, atlas_h] = atlas.size;
    for primitive in ctx.tessellate(output.shapes, output.pixels_per_point) {
        let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else {
            continue;
        };
        let textured = mesh.texture_id == egui::TextureId::default();
        let clip = primitive.clip_rect;
        for tri in mesh.indices.chunks(3) {
            let v = [
                mesh.vertices[tri[0] as usize],
                mesh.vertices[tri[1] as usize],
                mesh.vertices[tri[2] as usize],
            ];
            let p: Vec<(f32, f32)> = v
                .iter()
                .map(|v| (v.pos.x * scale, v.pos.y * scale))
                .collect();
            let area =
                (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
            if area.abs() < 1e-9 || !area.is_finite() {
                continue;
            }
            let x0 = p
                .iter()
                .map(|q| q.0)
                .fold(f32::MAX, f32::min)
                .max(clip.min.x * scale)
                .max(0.0);
            let x1 = p
                .iter()
                .map(|q| q.0)
                .fold(f32::MIN, f32::max)
                .min(clip.max.x * scale)
                .min(w as f32);
            let y0 = p
                .iter()
                .map(|q| q.1)
                .fold(f32::MAX, f32::min)
                .max(clip.min.y * scale)
                .max(0.0);
            let y1 = p
                .iter()
                .map(|q| q.1)
                .fold(f32::MIN, f32::max)
                .min(clip.max.y * scale)
                .min(h as f32);
            if x0 >= x1 || y0 >= y1 {
                continue;
            }
            for py in (y0.floor() as usize)..(y1.ceil() as usize).min(h) {
                for px in (x0.floor() as usize)..(x1.ceil() as usize).min(w) {
                    let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
                    let w0 = ((p[1].0 - x) * (p[2].1 - y) - (p[2].0 - x) * (p[1].1 - y)) / area;
                    let w1 = ((p[2].0 - x) * (p[0].1 - y) - (p[0].0 - x) * (p[2].1 - y)) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                        continue;
                    }
                    let mut c = [0.0f32; 4];
                    for (k, weight) in [w0, w1, w2].into_iter().enumerate() {
                        let col = v[k].color;
                        c[0] += weight * f32::from(col.r()) / 255.0;
                        c[1] += weight * f32::from(col.g()) / 255.0;
                        c[2] += weight * f32::from(col.b()) / 255.0;
                        c[3] += weight * f32::from(col.a()) / 255.0;
                    }
                    if textured {
                        let u = w0 * v[0].uv.x + w1 * v[1].uv.x + w2 * v[2].uv.x;
                        let t = w0 * v[0].uv.y + w1 * v[1].uv.y + w2 * v[2].uv.y;
                        let ax = ((u * atlas_w as f32) as usize).min(atlas_w - 1);
                        let ay = ((t * atlas_h as f32) as usize).min(atlas_h - 1);
                        let coverage = atlas.pixels[ay * atlas_w + ax];
                        for channel in &mut c {
                            *channel *= coverage;
                        }
                    }
                    let dst = &mut buffer[py * w + px];
                    for i in 0..4 {
                        dst[i] = c[i] + dst[i] * (1.0 - c[3]);
                    }
                }
            }
        }
    }
    let rgba = buffer
        .iter()
        .flat_map(|px| {
            let [r, g, b, _] = *px;
            [r, g, b]
                .map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
                .into_iter()
                .chain([255])
        })
        .collect();
    (w as u32, h as u32, rgba)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    let png = args
        .iter()
        .position(|a| a == "--png")
        .and_then(|i| args.get(i + 1))
        .ok_or("--png <file> is required")?
        .clone();
    let run = args.iter().any(|a| a == "--run");

    let mut state = AppState::default();
    state.load_preset("A320-200");
    state.set_design_mode(DesignMode::CleanSheet);
    merge(&mut state.config_values, &c919_brief());
    state.set_engine("LEAP-1A");
    state.on_config_modified();
    // Committing an Optimizer setting is what rebuilds the bounds from the
    // typed envelope; a run start does the same when the brief changed.
    state.reset_design_space_bounds_to_mode();

    let config = state
        .typed_config()
        .ok_or("configuration does not decode")?;
    println!(
        "preset='{}' mode={} clean_sheet_brief={:?} aerodrome_code={} span_limit={:?}",
        config.preset,
        config.optimizer.design_space.mode.as_str(),
        config.optimizer.design_space.clean_sheet_brief,
        config.aerodrome_reference_code().as_str(),
        config.max_design_span_m()
    );
    let design = state.current_design().ok_or("design incomplete")?;
    let bounds = state.current_design_bounds().ok_or("bounds incomplete")?;
    for ((spec, value), (lower, upper)) in alas_config::DESIGN_VARIABLE_SPECS
        .iter()
        .zip(design.to_array())
        .zip(bounds)
    {
        println!(
            "{:24} {:9.3} [{:9.3}, {:9.3}]",
            spec.name, value, lower, upper
        );
    }

    let ctx = Context::default();
    ctx.set_pixels_per_point(PIXELS_PER_POINT);
    apply_theme(AppTheme::Light, &ctx);
    for _ in 0..3 {
        frame(&ctx, &mut state);
    }
    let output = frame(&ctx, &mut state);
    let (w, h, rgba) = rasterize(&ctx, output);
    std::fs::write(&png, alas_viz::raster::encode_png_rgba(w, h, &rgba)?)?;
    println!("wrote {png} ({w}x{h})");

    if run {
        state.run_options.write_outputs = false;
        state.pipeline_options.seed = Some(20_260_922);
        state.pipeline_options.aerodynamic_solver = AerodynamicSolverMode::Vlm;
        let started = Instant::now();
        state.start_pipeline(false);
        while state.is_running {
            std::thread::sleep(Duration::from_millis(500));
            state.poll_worker();
        }
        println!(
            "run finished in {:.0} s: {}",
            started.elapsed().as_secs_f64(),
            state.status_message
        );
        let Some(result) = state.pipeline_result.as_ref() else {
            for line in state.logs.iter().rev().take(5) {
                println!("log: {}", line.text);
            }
            return Ok(());
        };
        if let Some(search) = result
            .optimization_result
            .as_ref()
            .and_then(|o| o.search_diagnostics.as_ref())
        {
            let evaluations: usize = search.stages.iter().map(|s| s.analysis_evaluations).sum();
            let feasible: usize = search.stages.iter().map(|s| s.feasible).sum();
            println!("search: {evaluations} evaluations, {feasible} feasible");
        }
        if let Some(design) = result.optimized_design {
            println!(
                "delivered: span {:.2} m, root {:.2} m, fuselage {:.2} m",
                design.span_m, design.root_chord_m, design.fuselage_length_m
            );
        }
        if let Some(report) = result.optimized_report.as_ref() {
            println!(
                "sized takeoff mass {:?} kg, S_ref {:.1} m^2, physically feasible {} ({} findings)",
                report.sized_takeoff_mass_kg(),
                report.airplane.s_ref,
                result.feasibility.is_feasible(),
                result.feasibility.findings.len()
            );
        }
    }
    Ok(())
}
