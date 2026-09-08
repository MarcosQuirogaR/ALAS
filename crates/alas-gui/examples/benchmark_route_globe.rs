// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Measure route-globe scene construction and rasterization at GUI densities.
//!
//! This is intentionally a small wall-clock harness rather than a criterion
//! benchmark: it can run in the same release profile as the desktop app and
//! reports the operations that changed when fullscreen zoom moved from canvas
//! scaling to camera-driven globe reconstruction.

use std::hint::black_box;
use std::time::{Duration, Instant};

use alas_report::families::mission::figure_mission_route_3d;
use alas_report::scene::Camera3D;
use alas_route::route::{Route, RouteSource, Waypoint};

const SAMPLES: usize = 30;

fn route() -> Route {
    Route::new(
        vec![
            Waypoint::named(51.4700, -0.4543, "EGLL"),
            Waypoint::named(50.0, 8.0, "SULUS"),
            Waypoint::named(44.0, 22.0, "BALIK"),
            Waypoint::named(37.0, 38.0, "TUMAK"),
            Waypoint::named(25.2532, 55.3657, "OMDB"),
        ],
        RouteSource::GreatCircle,
    )
}

fn scene(camera: Camera3D) -> alas_report::Scene {
    figure_mission_route_3d(
        &route(),
        Some(&[254_000.0, 248_000.0, 238_000.0, 224_000.0, 215_000.0]),
        Some(&[0.0, 10_000.0, 11_000.0, 8_000.0, 0.0]),
        Some(camera),
        Some("dark"),
    )
}

fn summary(samples: &mut [Duration]) -> (f64, f64, f64) {
    samples.sort_unstable();
    let mean = samples.iter().map(Duration::as_secs_f64).sum::<f64>() / samples.len() as f64;
    let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)].as_secs_f64();
    (mean * 1_000.0, p95 * 1_000.0, 1.0 / p95)
}

fn main() -> Result<(), String> {
    let camera = Camera3D::isometric();
    let mut build_samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let start = Instant::now();
        black_box(scene(camera));
        build_samples.push(start.elapsed());
    }
    let (build_mean, build_p95, _) = summary(&mut build_samples);
    println!("operation,scale,mean_ms,p95_ms,p95_fps");
    println!("scene_build,1.00,{build_mean:.3},{build_p95:.3},-");

    for scale in [1.0, 1.25, 1.5, 2.0] {
        let source = scene(camera);
        let mut samples = Vec::with_capacity(SAMPLES);
        for _ in 0..SAMPLES {
            let start = Instant::now();
            let image = alas_viz::raster::render_scene_rgba_scaled(&source, scale)?;
            black_box(image);
            samples.push(start.elapsed());
        }
        let (mean, p95, fps) = summary(&mut samples);
        println!("raster,{scale:.2},{mean:.3},{p95:.3},{fps:.2}");
    }

    let mut frames = Vec::with_capacity(SAMPLES);
    for index in 0..SAMPLES {
        let start = Instant::now();
        let mut camera = camera;
        camera.azim_deg += index as f64 * 1.2;
        let source = scene(camera);
        let image = alas_viz::raster::render_scene_rgba_scaled(&source, 1.5)?;
        black_box(image);
        frames.push(start.elapsed());
    }
    let (mean, p95, fps) = summary(&mut frames);
    println!("camera_rebuild_and_raster,1.50,{mean:.3},{p95:.3},{fps:.2}");

    // Approximate the old embedded-card behavior: the cached raster remains
    // unchanged while wheel input only changes the canvas transform. Warm the
    // cache first so the measured frames represent scrolling, not first paint.
    let context = egui::Context::default();
    let source = scene(camera);
    let mut view_state = alas_viz::SceneViewState::default();
    let warmup = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(860.0, 540.0),
        )),
        ..egui::RawInput::default()
    };
    let _ = context.run(warmup, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            alas_viz::SceneView::new(&source, &mut view_state)
                .desired_size(egui::vec2(860.0, 540.0))
                .orbit_only()
                .wheel_zoom(true)
                .raster_scale(1.0)
                .cache_key("old-canvas-zoom")
                .show(ui);
        });
    });
    let mut cached_zoom_frames = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(860.0, 540.0),
            )),
            events: vec![
                egui::Event::PointerMoved(egui::pos2(430.0, 270.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, 40.0),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..egui::RawInput::default()
        };
        let start = Instant::now();
        let _ = context.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                alas_viz::SceneView::new(&source, &mut view_state)
                    .desired_size(egui::vec2(860.0, 540.0))
                    .orbit_only()
                    .wheel_zoom(true)
                    .raster_scale(1.0)
                    .cache_key("old-canvas-zoom")
                    .show(ui);
            });
        });
        cached_zoom_frames.push(start.elapsed());
    }
    let (mean, p95, fps) = summary(&mut cached_zoom_frames);
    println!("cached_canvas_wheel_frame,1.00,{mean:.3},{p95:.3},{fps:.2}");
    Ok(())
}
