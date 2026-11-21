// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! CPU cost of drawing the Payload page's two canvases once they are settled,
//! so a change to a view shows up as a number. Printed with `--nocapture`;
//! the bound is generous and only catches a per-frame raster or rebuild.

use super::*;

fn frame(state: &mut AppState, ctx: &egui::Context) -> f64 {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(1_400.0, 1_000.0),
        )),
        ..Default::default()
    };
    let start = std::time::Instant::now();
    let output = ctx.run(raw, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| show(state, ui));
    });
    let ms = start.elapsed().as_secs_f64() * 1_000.0;
    SHAPES.with(|cell| cell.set(output.shapes.len()));
    ms
}

thread_local! {
    static SHAPES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn percentile(samples: &mut [f64], p: f64) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[((samples.len() as f64 - 1.0) * p).round() as usize]
}

// reason: the timing is the output, read with --nocapture.
#[allow(clippy::print_stdout)]
fn settled_cost(mode: ViewMode) -> (f64, f64) {
    let mut state = AppState::default();
    let ctx = egui::Context::default();
    ctx.data_mut(|data| {
        data.insert_temp(
            ui_id(),
            EditorUi {
                view_mode: mode,
                ..EditorUi::default()
            },
        );
    });
    // Let the page build its views and any worker finish its raster.
    for _ in 0..80 {
        frame(&mut state, &ctx);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let rect: egui::Rect = ctx
        .data(|data| data.get_temp(canvas::canvas_rect_id()))
        .expect("the canvas was drawn");
    let projection = match mode {
        ViewMode::Front => front_render::Projection::Front,
        ViewMode::Side => front_render::Projection::Side,
    };
    assert!(
        front_render::has_texture(&ctx, projection),
        "the {mode:?} view's render has been delivered"
    );
    assert!(rect.height() > 100.0, "{rect:?}");
    let mut samples: Vec<f64> = (0..300).map(|_| frame(&mut state, &ctx)).collect();
    println!("shapes per frame: {}", SHAPES.with(std::cell::Cell::get));
    (
        percentile(&mut samples, 0.5),
        percentile(&mut samples, 0.95),
    )
}

#[test]
// reason: the timing is the output, read with --nocapture.
#[allow(clippy::print_stdout)]
fn the_settled_canvases_cost_little_cpu_per_frame() {
    for (mode, name) in [(ViewMode::Front, "front"), (ViewMode::Side, "side")] {
        let (p50, p95) = settled_cost(mode);
        println!("payload {name} view, settled frame CPU ms: p50 {p50:.2}, p95 {p95:.2}");
        assert!(p95 < 60.0, "{name}: p95 {p95} ms");
    }
}
