// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sandbox viewport's tessellated output must stay inside the projected
//! geometry, and its floating controls must own the gestures made on them.
//!
//! Regression for the elongated triangles and rays seen in the sandbox
//! preview: lofted faces seen edge-on project to slivers, and the closed
//! path tessellation of egui places their feathering vertices at
//! `normal / |normal|^2`, which is unbounded for a near-reversing corner.

use alas_gui::sandbox::scene::{build_sandbox_scene, SANDBOX_CAMERA_ID};
use alas_gui::sandbox::viewport::{overlay_rect_tagged, overlay_rects, pointer_over_overlay};
use alas_gui::sandbox::workspace::show_sandbox_workspace;
use alas_gui::state::{AppState, PreviewCamera};
use alas_report::scene::{Color, Fill, Scene, SceneElement};
use alas_viz::raster::render_scene_png;
use alas_viz::{render_scene_to_shapes, to_egui_color, to_egui_stroke, ViewportTransform};
use egui::epaint::{tessellator::Tessellator, PathShape, TessellationOptions};
use egui::{pos2, vec2, Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect};

fn camera(elev: f64, azim: f64, zoom: f64) -> PreviewCamera {
    PreviewCamera {
        pitch_deg: elev,
        yaw_deg: azim,
        zoom,
    }
}

struct Tessellated {
    /// Bounds of the projected polygon vertices handed to the renderer.
    input: Rect,
    /// Bounds of the finite vertices the tessellator produced.
    output: Rect,
    non_finite: usize,
    vertices: usize,
}

fn tessellate(state: &AppState, rect: Rect) -> Tessellated {
    let (mut scene, _) = build_sandbox_scene(state).expect("AVE scene");
    // The desktop path paints the theme background itself and draws only
    // the vector elements as shapes.
    scene.hide_background_paint();
    let transform = ViewportTransform::fit(scene.width, scene.height, rect);
    let mut input = Rect::NOTHING;
    for element in &scene.elements {
        if let SceneElement::Polygon { points, .. } = element {
            for p in points {
                let s = transform.to_screen(*p);
                input = input.union(Rect::from_min_max(s, s));
            }
        }
    }
    let shapes = render_scene_to_shapes(&scene, &transform);
    let mut tessellator = Tessellator::new(1.0, TessellationOptions::default(), [0, 0], vec![]);
    let mut mesh = egui::epaint::Mesh::default();
    for shape in shapes {
        tessellator.tessellate_shape(shape, &mut mesh);
    }
    let mut output = Rect::NOTHING;
    let mut non_finite = 0usize;
    for v in &mesh.vertices {
        if !v.pos.x.is_finite() || !v.pos.y.is_finite() {
            non_finite += 1;
            continue;
        }
        output = output.union(Rect::from_min_max(v.pos, v.pos));
    }
    Tessellated {
        input,
        output,
        non_finite,
        vertices: mesh.vertices.len(),
    }
}

#[test]
fn sandbox_scene_tessellation_never_leaves_the_projected_geometry() {
    let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(920.0, 730.0));
    let mut state = AppState::default();
    let mut cameras = vec![
        camera(22.0, -125.0, 1.0),
        camera(22.0, -125.0, 1.8),
        PreviewCamera::top(),
        PreviewCamera::front(),
        PreviewCamera::side(),
    ];
    for azim in (-180..180).step_by(30) {
        for elev in [-60.0, -20.0, 0.0, 15.0, 45.0, 80.0] {
            cameras.push(camera(elev, f64::from(azim), 1.4));
        }
    }
    let mut failures = Vec::new();
    for cam in cameras {
        *state.preview_camera_mut(SANDBOX_CAMERA_ID) = cam;
        let t = tessellate(&state, rect);
        // Feathering and the outline stroke may add a few pixels around a
        // face; anything more is geometry the renderer invented.
        let allowed = t.input.expand(6.0);
        let fitted = cam.zoom > 1.0 || rect.expand(1.0).contains_rect(t.input);
        if t.non_finite > 0 || !allowed.contains_rect(t.output) || !fitted {
            failures.push(format!(
                "camera elev {} azim {} zoom {}: {} vertices, {} non-finite, input {:?}, output {:?}",
                cam.pitch_deg, cam.yaw_deg, cam.zoom, t.vertices, t.non_finite, t.input, t.output
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "tessellated sandbox geometry escaped its projected faces:\n{}",
        failures.join("\n")
    );
}

const SCREEN: Rect = Rect::from_min_max(Pos2::ZERO, pos2(1280.0, 820.0));

fn frame(ctx: &Context, state: &mut AppState, events: Vec<Event>) -> egui::FullOutput {
    let input = RawInput {
        screen_rect: Some(SCREEN),
        events,
        ..Default::default()
    };
    ctx.run(input, |ctx| show_sandbox_workspace(state, ctx))
}

fn press(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn drag(ctx: &Context, state: &mut AppState, from: Pos2) {
    frame(ctx, state, vec![Event::PointerMoved(from)]);
    frame(ctx, state, vec![press(from, true)]);
    for step in 1..=4 {
        let to = from + vec2(30.0 * step as f32, 12.0 * step as f32);
        frame(ctx, state, vec![Event::PointerMoved(to)]);
    }
    let end = from + vec2(120.0, 48.0);
    frame(ctx, state, vec![press(end, false)]);
    frame(ctx, state, vec![]);
}

#[test]
fn gestures_on_floating_controls_never_orbit_the_camera_or_drag_handles() {
    let mut state = AppState::default();
    assert!(state.enter_sandbox(true), "sandbox opens from AVE");
    let ctx = Context::default();
    frame(&ctx, &mut state, vec![]);
    frame(&ctx, &mut state, vec![]);
    let rects = overlay_rects(&ctx);
    assert!(
        rects.len() >= 12,
        "camera row, search and five category buttons are floating controls: {rects:?}"
    );
    let wing = overlay_rect_tagged(&ctx, "category:wing").expect("wing category button");
    assert!(pointer_over_overlay(&rects, Some(wing.center())));

    let before = *state.preview_camera_mut(SANDBOX_CAMERA_ID);
    drag(&ctx, &mut state, wing.center());
    let after = *state.preview_camera_mut(SANDBOX_CAMERA_ID);
    assert_eq!(before, after, "a drag starting on a control must not orbit");
    assert!(state.sandbox.drag.is_none());

    // A click on the category button opens its editor and isolates it.
    let center = wing.center();
    frame(&ctx, &mut state, vec![Event::PointerMoved(center)]);
    frame(&ctx, &mut state, vec![press(center, true)]);
    frame(&ctx, &mut state, vec![press(center, false)]);
    frame(&ctx, &mut state, vec![]);
    assert!(state
        .sandbox
        .layout
        .open_disciplines
        .iter()
        .any(|id| id == "wing"));
    assert_eq!(state.sandbox.focus().map(|d| d.id()), Some("wing"));

    // The same gesture on the empty design space still orbits.
    let empty = pos2(640.0, 560.0);
    assert!(!pointer_over_overlay(&overlay_rects(&ctx), Some(empty)));
    let before = *state.preview_camera_mut(SANDBOX_CAMERA_ID);
    drag(&ctx, &mut state, empty);
    let after = *state.preview_camera_mut(SANDBOX_CAMERA_ID);
    assert_ne!(before.yaw_deg, after.yaw_deg, "orbit outside the controls");
}

/// Rasterize tessellated shapes the way the GPU would, through the SVG
/// raster path, so the desktop output can be inspected headlessly.
fn rasterize(shapes: Vec<egui::Shape>, rect: Rect, background: Option<Color>) -> Vec<u8> {
    let mut tessellator = Tessellator::new(1.0, TessellationOptions::default(), [0, 0], vec![]);
    let mut mesh = egui::epaint::Mesh::default();
    for shape in shapes {
        tessellator.tessellate_shape(shape, &mut mesh);
    }
    let mut scene = Scene::new(
        f64::from(rect.width()),
        f64::from(rect.height()),
        background,
    );
    scene.render_title = false;
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
        scene.add(SceneElement::Polygon {
            points: vertices
                .iter()
                .map(|v| [f64::from(v.pos.x), f64::from(v.pos.y)])
                .collect(),
            fill: Some(Fill::new(Color::rgba(c.r(), c.g(), c.b(), c.a()))),
            stroke: None,
        });
    }
    render_scene_png(&scene).expect("png")
}

/// Writes headless before/after renders of the desktop tessellation to an
/// internal evidence directory (2026-09-14); run with `--ignored`.
#[test]
#[ignore = "writes evidence images"]
fn write_tessellation_evidence_images() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../out/evidence/sandbox-corrections-2026-09-14");
    std::fs::create_dir_all(&dir).expect("evidence directory");
    let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(920.0, 730.0));
    let mut state = AppState::default();
    let cameras = [
        ("screenshot-camera", camera(22.0, -125.0, 1.8)),
        ("iso-fit", camera(22.0, -125.0, 1.0)),
        ("front", PreviewCamera::front()),
    ];
    for (name, cam) in cameras {
        *state.preview_camera_mut(SANDBOX_CAMERA_ID) = cam;
        let (mut scene, _) = build_sandbox_scene(&state).expect("scene");
        let background = scene.background;
        scene.hide_background_paint();
        let transform = ViewportTransform::fit(scene.width, scene.height, rect);
        // The previous desktop path: whole-pixel positions and the feathered
        // convex polygon for every face.
        let before: Vec<egui::Shape> = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Polygon {
                    points,
                    fill,
                    stroke,
                } => {
                    let points = points
                        .iter()
                        .map(|p| {
                            let s = transform.to_screen(*p);
                            pos2(s.x.round(), s.y.round())
                        })
                        .collect();
                    let fill = fill
                        .map(|f| to_egui_color(&f.color))
                        .unwrap_or(egui::Color32::TRANSPARENT);
                    let stroke = stroke
                        .as_ref()
                        .map(|s| to_egui_stroke(s, transform.scale))
                        .unwrap_or(egui::Stroke::NONE);
                    Some(egui::Shape::Path(PathShape::convex_polygon(
                        points, fill, stroke,
                    )))
                }
                _ => None,
            })
            .collect();
        let after = render_scene_to_shapes(&scene, &transform);
        for (label, shapes) in [("before", before), ("after", after)] {
            let png = rasterize(shapes, rect, background);
            std::fs::write(dir.join(format!("tessellation-{name}-{label}.png")), png)
                .expect("write png");
        }
    }
}

/// Rasterize one full workspace frame headlessly: solid triangles by colour,
/// glyph triangles by their mean font-atlas coverage (legible as text blocks,
/// not as glyphs), so the layout of the floating controls can be inspected.
fn rasterize_frame(ctx: &Context, output: egui::FullOutput, background: egui::Color32) -> Vec<u8> {
    let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
    let atlas = ctx.fonts(|f| f.image());
    let [atlas_w, atlas_h] = atlas.size;
    let mut scene = Scene::new(
        f64::from(SCREEN.width()),
        f64::from(SCREEN.height()),
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
    render_scene_png(&scene).expect("png")
}

/// Writes headless renders of the sandbox workspace with the floating
/// controls (Wing button hovered) in the dark and light themes, plus one
/// with an active search; run with `--ignored`.
#[test]
#[ignore = "writes evidence images"]
fn write_workspace_layout_evidence_images() {
    use alas_gui::theme::AppTheme;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../out/evidence/sandbox-corrections-2026-09-14");
    std::fs::create_dir_all(&dir).expect("evidence directory");
    for (name, theme, search) in [
        ("dark", AppTheme::Dark, ""),
        ("light", AppTheme::Light, ""),
        ("dark-search", AppTheme::Dark, "sweep"),
    ] {
        let mut state = AppState::default();
        state.theme = theme;
        assert!(state.enter_sandbox(true));
        state.sandbox.search = search.to_owned();
        let ctx = Context::default();
        alas_gui::apply_theme(theme, &ctx);
        for _ in 0..3 {
            frame(&ctx, &mut state, vec![]);
        }
        let wing = overlay_rect_tagged(&ctx, "category:wing").expect("wing button");
        frame(&ctx, &mut state, vec![Event::PointerMoved(wing.center())]);
        let output = frame(&ctx, &mut state, vec![Event::PointerMoved(wing.center())]);
        let background = ctx.style().visuals.panel_fill;
        let png = rasterize_frame(&ctx, output, background);
        std::fs::write(dir.join(format!("workspace-layout-{name}.png")), png).expect("write png");
    }
}
