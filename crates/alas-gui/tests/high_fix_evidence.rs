// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Headless renders of the three High defects the 2026-09-17 native
//! screenshot review confirmed, before and after the corrections.
//!
//! Run with `--ignored`; the images land in
//! `.agent/reports/gui-high-fixes-20260917/`. Glyphs are rasterized as their
//! mean font-atlas coverage, so text reads as blocks rather than letters:
//! these images are evidence about layout, selection state and which
//! surfaces are present, not about typography.

use alas_gui::sandbox::fields::Discipline;
use alas_gui::sandbox::workspace::show_sandbox_workspace;
use alas_gui::state::AppState;
use alas_gui::views::show_inputs_view;
use alas_report::scene::{Color, Fill, Scene, SceneElement};
use alas_viz::raster::render_scene_png;
use egui::{vec2, Color32, Context, FullOutput, Margin, Pos2, RawInput, Rect};

const CONTENT_MARGIN: Margin = Margin {
    left: 26.0,
    right: 18.0,
    top: 16.0,
    bottom: 14.0,
};

fn evidence_dir() -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.agent/reports/gui-high-fixes-20260917");
    std::fs::create_dir_all(&dir).expect("evidence directory");
    dir
}

/// Rasterize one frame: solid triangles by colour, glyph triangles by their
/// mean atlas coverage, onto an opaque `background`.
fn frame_png(ctx: &Context, output: FullOutput, size: (f32, f32), background: Color32) -> Vec<u8> {
    let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
    let atlas = ctx.fonts(|fonts| fonts.image());
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
                .map(|&index| mesh.vertices[index as usize])
                .collect();
            if vertices
                .iter()
                .any(|vertex| !vertex.pos.x.is_finite() || !vertex.pos.y.is_finite())
            {
                continue;
            }
            let color = vertices[0].color;
            let mut alpha = f32::from(color.a()) / 255.0;
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
                    .map(|vertex| [f64::from(vertex.pos.x), f64::from(vertex.pos.y)])
                    .collect(),
                fill: Some(Fill::new(Color::rgba(
                    color.r(),
                    color.g(),
                    color.b(),
                    (alpha * 255.0) as u8,
                ))),
                stroke: None,
            });
        }
    }
    render_scene_png(&scene).expect("png")
}

fn write(name: &str, png: Vec<u8>) {
    let path = evidence_dir().join(name);
    std::fs::write(&path, png).expect("evidence image");
    println!("wrote {}", path.display());
}

/// Render the Inputs page, optionally reserving a right-hand dock first.
fn inputs_frame(state: &mut AppState, size: (f32, f32), dock_width: Option<f32>) -> Vec<u8> {
    let ctx = Context::default();
    alas_gui::apply_theme(alas_gui::AppTheme::Dark, &ctx);
    let background = ctx.style().visuals.window_fill();
    let mut output = None;
    for _ in 0..3 {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(size.0, size.1))),
            ..RawInput::default()
        };
        output = Some(ctx.run(input, |ctx| {
            if let Some(width) = dock_width {
                egui::SidePanel::right("preview_panel")
                    .resizable(false)
                    .exact_width(width)
                    .show(ctx, |ui| {
                        ui.label("3D Live Preview");
                    });
            }
            egui::TopBottomPanel::bottom("control_bar").show(ctx, |ui| {
                alas_gui::views::show_control_bar(state, ui);
            });
            egui::CentralPanel::default()
                .frame(egui::Frame::central_panel(ctx.style().as_ref()).inner_margin(CONTENT_MARGIN))
                .show(ctx, |ui| show_inputs_view(state, ui));
        }));
    }
    frame_png(&ctx, output.expect("rendered frame"), size, background)
}

/// F-01: the captured 466 x 893 window, with the dock forced to the width it
/// held in the capture and with the correction's responsive behaviour.
#[test]
#[ignore = "writes evidence images"]
fn write_narrow_window_evidence_images() {
    let size = (466.0, 893.0);
    let mut state = AppState::default();
    write(
        "f01-before-466x893-dock-fixed-317.png",
        inputs_frame(&mut state, size, Some(317.0)),
    );
    let mut state = AppState::default();
    write(
        "f01-after-466x893-dock-stands-down.png",
        inputs_frame(&mut state, size, None),
    );
    // The declared minimum window keeps both surfaces.
    let range = alas_gui::layout::preview_width_range(880.0).expect("dock fits at the minimum");
    let mut state = AppState::default();
    write(
        "f01-after-880-wide-form-beside-widest-dock.png",
        inputs_frame(&mut state, (880.0, 560.0), Some(*range.end())),
    );
}

/// F-03: cruise Mach 3.5 disables Run. The value area must read "3.5", the
/// marker must appear once, and the blocking reason must be on the card.
#[test]
#[ignore = "writes evidence images"]
fn write_invalid_mach_evidence_images() {
    let size = (1_600.0, 900.0);
    let mut state = AppState::default();
    write("f03-before-edit-valid-mach.png", inputs_frame(&mut state, size, None));
    state.config_values["requirements"]["cruise_mach"] = serde_json::Value::from(3.5);
    state.on_config_modified();
    assert!(state.blocked(), "Mach 3.5 must still block the run");
    write(
        "f03-after-invalid-mach-3p5-reason-shown.png",
        inputs_frame(&mut state, size, None),
    );
}

/// F-02: the sandbox component context. The category stack, the camera row's
/// context label and the isolated scene must agree, and the whole-aircraft
/// summary must not read as a component selection.
#[test]
#[ignore = "writes evidence images"]
fn write_sandbox_component_context_evidence_images() {
    let size = (1_600.0, 900.0);
    let mut state = AppState::default();
    assert!(state.enter_sandbox(true), "sandbox opens from AVE");
    let ctx = Context::default();
    alas_gui::apply_theme(alas_gui::AppTheme::Dark, &ctx);
    let background = ctx.style().visuals.window_fill();

    let mut shot = |state: &mut AppState, name: &str| {
        let mut output = None;
        for _ in 0..3 {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(size.0, size.1))),
                ..RawInput::default()
            };
            output = Some(ctx.run(input, |ctx| show_sandbox_workspace(state, ctx)));
        }
        write(
            name,
            frame_png(&ctx, output.expect("rendered frame"), size, background),
        );
    };

    shot(&mut state, "f02-overview-no-component-selected.png");
    state.set_sandbox_focus(Some(Discipline::Propulsion));
    shot(&mut state, "f02-propulsion-selected.png");
    state.sandbox.layout.summary_open = true;
    shot(&mut state, "f02-propulsion-selected-with-summary-open.png");
    state.set_sandbox_focus(Some(Discipline::Wing));
    assert!(
        state.sandbox.layout.summary_open,
        "the summary is independent of the selection"
    );
    shot(&mut state, "f02-wing-selected-summary-still-open.png");
}
