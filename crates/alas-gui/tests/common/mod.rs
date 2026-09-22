// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Headless frame driving and rasterization shared by the sandbox tests.

#![allow(dead_code)]

use alas_gui::sandbox::workspace::show_sandbox_workspace;
use alas_gui::state::AppState;
use alas_report::scene::{Color, Fill, Scene, SceneElement};
use alas_viz::raster::render_scene_rgba;
use egui::{pos2, vec2, Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect};

/// Run one workspace frame on a screen of `size` points.
pub fn frame_on(
    ctx: &Context,
    state: &mut AppState,
    size: (f32, f32),
    events: Vec<Event>,
) -> egui::FullOutput {
    let input = RawInput {
        screen_rect: Some(Rect::from_min_max(Pos2::ZERO, pos2(size.0, size.1))),
        events,
        ..Default::default()
    };
    ctx.run(input, |ctx| show_sandbox_workspace(state, ctx))
}

pub fn press(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

/// A primary-button drag of (120, 48) points starting at `from`.
pub fn drag_on(ctx: &Context, state: &mut AppState, size: (f32, f32), from: Pos2) {
    frame_on(ctx, state, size, vec![Event::PointerMoved(from)]);
    frame_on(ctx, state, size, vec![press(from, true)]);
    for step in 1..=4 {
        let to = from + vec2(30.0 * step as f32, 12.0 * step as f32);
        frame_on(ctx, state, size, vec![Event::PointerMoved(to)]);
    }
    let end = from + vec2(120.0, 48.0);
    frame_on(ctx, state, size, vec![press(end, false)]);
    frame_on(ctx, state, size, vec![]);
}

/// A click at `pos`.
pub fn click_on(ctx: &Context, state: &mut AppState, size: (f32, f32), pos: Pos2) {
    frame_on(ctx, state, size, vec![Event::PointerMoved(pos)]);
    frame_on(ctx, state, size, vec![press(pos, true)]);
    frame_on(ctx, state, size, vec![press(pos, false)]);
    frame_on(ctx, state, size, vec![]);
}

/// Rasterize one full workspace frame headlessly: solid triangles by
/// colour, glyph triangles by their mean font-atlas coverage (legible as
/// text blocks, not as glyphs). Returns `(width, height, rgba)`.
pub fn rasterize_frame(
    ctx: &Context,
    output: egui::FullOutput,
    size: (f32, f32),
    background: egui::Color32,
) -> (u32, u32, Vec<u8>) {
    let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
    let atlas = ctx.fonts(|f| f.image());
    let [atlas_w, atlas_h] = atlas.size;
    let mut scene = Scene::new(
        f64::from(size.0),
        f64::from(size.1),
        Some(Color::rgba(
            background.r(),
            background.g(),
            background.b(),
            255,
        )),
    );
    scene.render_title = false;
    for primitive in primitives {
        let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else {
            continue;
        };
        let textured = mesh.texture_id == egui::TextureId::default();
        for triangle in mesh.indices.chunks(3) {
            let vertices: Vec<_> = triangle
                .iter()
                .map(|&i| mesh.vertices[i as usize])
                .collect();
            if vertices
                .iter()
                .any(|v| !v.pos.x.is_finite() || !v.pos.y.is_finite())
            {
                continue;
            }
            let c = vertices[0].color;
            let mut alpha = f32::from(c.a()) / 255.0;
            if textured {
                let u0 = vertices.iter().map(|v| v.uv.x).fold(1.0, f32::min);
                let u1 = vertices.iter().map(|v| v.uv.x).fold(0.0, f32::max);
                let v0 = vertices.iter().map(|v| v.uv.y).fold(1.0, f32::min);
                let v1 = vertices.iter().map(|v| v.uv.y).fold(0.0, f32::max);
                let x0 = ((u0 * atlas_w as f32) as usize).min(atlas_w - 1);
                let x1 = ((u1 * atlas_w as f32) as usize).min(atlas_w - 1);
                let y0 = ((v0 * atlas_h as f32) as usize).min(atlas_h - 1);
                let y1 = ((v1 * atlas_h as f32) as usize).min(atlas_h - 1);
                let mut sum = 0.0;
                let mut count = 0usize;
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        sum += atlas.pixels[y * atlas_w + x];
                        count += 1;
                    }
                }
                if count > 0 {
                    alpha *= sum / count as f32;
                }
            }
            if alpha < 0.02 {
                continue;
            }
            scene.add(SceneElement::Polygon {
                points: vertices
                    .iter()
                    .map(|v| [f64::from(v.pos.x), f64::from(v.pos.y)])
                    .collect(),
                fill: Some(Fill::new(Color::rgba(
                    c.r(),
                    c.g(),
                    c.b(),
                    (alpha * 255.0) as u8,
                ))),
                stroke: None,
            });
        }
    }
    render_scene_rgba(&scene).expect("rgba")
}

/// Encode an RGBA buffer as PNG through the raster path.
pub fn write_png(path: &std::path::Path, width: u32, height: u32, rgba: &[u8]) {
    let png = alas_viz::raster::encode_png_rgba(width, height, rgba).expect("png");
    std::fs::write(path, png).expect("write png");
}
