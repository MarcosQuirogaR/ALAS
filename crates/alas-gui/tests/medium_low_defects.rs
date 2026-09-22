// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Headless regression tests for the Medium and Low findings of the
//! 2026-09-17 native screenshot review.
//!
//! Each test renders the real view and inspects the shapes egui emits, which
//! is the same evidence the review measured on the captured bitmaps. These are
//! headless renders, not desktop screenshots: they prove colour, placement and
//! composition, not typography.

use alas_gui::state::AppState;
use egui::{vec2, Color32, Context, FullOutput, Pos2, RawInput, Rect, Shape};

/// WCAG 2.1 relative-luminance contrast ratio.
fn contrast_ratio(a: Color32, b: Color32) -> f32 {
    fn luminance(color: Color32) -> f32 {
        let channel = |value: u8| {
            let value = f32::from(value) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    }
    let (lighter, darker) = if luminance(a) >= luminance(b) {
        (luminance(a), luminance(b))
    } else {
        (luminance(b), luminance(a))
    };
    (lighter + 0.05) / (darker + 0.05)
}

/// Every emitted (text, colour) pair of a frame, taken from the glyph mesh.
///
/// The colour that reaches the screen is the one on the galley's mesh
/// vertices, not the one in the layout job: `Ui::disable` fades a scope by
/// rewriting exactly those vertices (`epaint::shape_transform::adjust_colors`)
/// and leaves the layout job's section colour untouched. Reading the section
/// colour would therefore report an active page's colours for a disabled one.
fn text_colors(output: &FullOutput) -> Vec<(String, Color32)> {
    fn painted_color(text: &egui::epaint::TextShape) -> Option<Color32> {
        text.galley
            .rows
            .iter()
            .flat_map(|row| row.visuals.mesh.vertices.iter())
            .map(|vertex| vertex.color)
            .find(|color| color.a() > 0)
    }
    fn walk(shape: &Shape, found: &mut Vec<(String, Color32)>) {
        match shape {
            Shape::Text(text) => {
                let painted = painted_color(text);
                for section in &text.galley.job.sections {
                    let color = painted.unwrap_or_else(|| {
                        text.override_text_color.unwrap_or_else(|| {
                            if section.format.color == Color32::PLACEHOLDER {
                                text.fallback_color
                            } else {
                                section.format.color
                            }
                        })
                    });
                    found.push((
                        text.galley.job.text[section.byte_range.clone()].to_owned(),
                        color,
                    ));
                }
            }
            Shape::Vec(shapes) => {
                for shape in shapes {
                    walk(shape, found);
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for shape in &output.shapes {
        walk(&shape.shape, &mut found);
    }
    found
}

/// The colour a given string was painted in, or `None` when it was not drawn.
fn color_of(colors: &[(String, Color32)], needle: &str) -> Option<Color32> {
    colors
        .iter()
        .find(|(text, _)| text.contains(needle))
        .map(|(_, color)| *color)
}

fn run_frame(size: (f32, f32), add: impl FnMut(&mut egui::Ui)) -> (FullOutput, Context) {
    let ctx = Context::default();
    alas_gui::apply_theme(alas_gui::AppTheme::Dark, &ctx);
    let mut add = add;
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(size.0, size.1))),
        ..RawInput::default()
    };
    let output = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| add(ui));
    });
    (output, ctx)
}

/// F-11: a preset-locked page dimmed its own headings and labels.
#[test]
fn a_locked_page_keeps_its_title_and_labels_at_full_contrast() {
    let page = alas_gui::nav::page("control_surfaces").expect("the control-surface page exists");
    let mut locked_colors = Vec::new();
    let mut open_colors = Vec::new();
    let mut panel = Color32::BLACK;
    for (locked, sink) in [(true, &mut locked_colors), (false, &mut open_colors)] {
        let mut state = AppState::default();
        let (output, ctx) = run_frame((1200.0, 900.0), |ui| {
            alas_gui::views::form_page::show_form_page_locked(&mut state, ui, page, locked);
        });
        panel = ctx.style().visuals.panel_fill;
        *sink = text_colors(&output);
    }
    let title = color_of(&locked_colors, "Control Surfaces").expect("page title is drawn");
    let values = color_of(&locked_colors, "Parameter values").expect("section label is drawn");
    for (what, color) in [("title", title), ("Parameter values", values)] {
        let ratio = contrast_ratio(color, panel);
        assert!(
            ratio >= 4.5,
            "locked page {what} {color:?} measures {ratio:.2}:1 on {panel:?}"
        );
    }
    // The prose is identical whether or not the page is locked: only the
    // editors are disabled.
    assert_eq!(
        color_of(&open_colors, "Control Surfaces"),
        Some(title),
        "an unlocked page must paint its title the same way"
    );
    // ... and the lock notice is drawn on the locked page only.
    assert!(color_of(&locked_colors, "Preset geometry is protected").is_some());
    assert!(color_of(&open_colors, "Preset geometry is protected").is_none());
}

/// The negative control for the test above: the previous structure wrapped the
/// whole page in a disabled scope, and egui fades a disabled scope's every
/// painted colour toward the background. Without it the test above would pass
/// on any implementation.
#[test]
fn wrapping_a_whole_page_in_a_disabled_scope_is_what_dimmed_its_title() {
    let page = alas_gui::nav::page("control_surfaces").expect("the control-surface page exists");
    let mut state = AppState::default();
    let (output, ctx) = run_frame((1200.0, 900.0), |ui| {
        ui.add_enabled_ui(false, |ui| {
            alas_gui::views::form_page::show_form_page(&mut state, ui, page);
        });
    });
    let panel = ctx.style().visuals.panel_fill;
    let colors = text_colors(&output);
    let title = color_of(&colors, "Control Surfaces").expect("page title is drawn");
    let ratio = contrast_ratio(title, panel);
    assert!(
        ratio < 4.5,
        "the old disabled-page rendering must be observable; measured {ratio:.2}:1"
    );
}

/// The drawn extent of a scene's line work, in scene coordinates.
fn line_bounds(scene: &alas_report::scene::Scene) -> Option<(f64, f64, f64, f64)> {
    use alas_report::scene::SceneElement;
    let mut bounds: Option<(f64, f64, f64, f64)> = None;
    let mut include = |p: &[f64; 2]| {
        bounds = Some(match bounds {
            None => (p[0], p[1], p[0], p[1]),
            Some((x0, y0, x1, y1)) => (x0.min(p[0]), y0.min(p[1]), x1.max(p[0]), y1.max(p[1])),
        });
    };
    for element in &scene.elements {
        match element {
            SceneElement::Line { p1, p2, .. } => {
                include(p1);
                include(p2);
            }
            SceneElement::Polyline { points, .. } | SceneElement::Polygon { points, .. } => {
                for point in points {
                    include(point);
                }
            }
            _ => {}
        }
    }
    bounds
}

/// F-13: the live preview's model must be centred in, and fill, its canvas.
#[test]
fn the_exterior_preview_centres_and_fills_its_own_canvas() {
    let state = AppState::default();
    let scene = alas_gui::scene::build_page_preview(&state, "exterior_3d")
        .expect("the exterior preview builds from the default configuration");
    let (x0, y0, x1, y1) = line_bounds(&scene).expect("the wireframe draws line work");
    let centre = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let canvas_centre = (scene.width / 2.0, scene.height / 2.0);
    let offset_x = (centre.0 - canvas_centre.0).abs() / scene.width;
    let offset_y = (centre.1 - canvas_centre.1).abs() / scene.height;
    assert!(
        offset_x < 0.12 && offset_y < 0.12,
        "model centre ({:.0},{:.0}) is off the {:.0}x{:.0} canvas centre by \
         ({:.1}%, {:.1}%)",
        centre.0,
        centre.1,
        scene.width,
        scene.height,
        offset_x * 100.0,
        offset_y * 100.0
    );
    let fill = ((x1 - x0) / scene.width).max((y1 - y0) / scene.height);
    assert!(
        fill > 0.6,
        "the model fills only {:.0}% of its canvas",
        fill * 100.0
    );
}

/// F-13: the dock's canvas must stay inside the panel that hosts it.
#[test]
fn the_preview_dock_canvas_stays_inside_its_panel() {
    let mut state = AppState::default();
    state.update_preview_scene();
    let ctx = Context::default();
    alas_gui::apply_theme(alas_gui::AppTheme::Dark, &ctx);
    let mut panel_rect = Rect::NOTHING;
    let mut canvas = Rect::NOTHING;
    for _ in 0..2 {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 962.0))),
            ..RawInput::default()
        };
        let output = ctx.run(input, |ctx| {
            let response = egui::SidePanel::right("preview_panel")
                .resizable(false)
                .exact_width(317.0)
                .show(ctx, |ui| alas_gui::views::show_preview_dock(&mut state, ui));
            panel_rect = response.response.rect;
        });
        // The viewport paints the scene background across its own rectangle.
        let background = state
            .preview_scene
            .as_ref()
            .and_then(|scene| scene.background)
            .map(|color| Color32::from_rgb(color.r, color.g, color.b));
        let mut widest = Rect::NOTHING;
        fn walk_rects(shape: &Shape, want: Option<Color32>, widest: &mut Rect) {
            match shape {
                Shape::Rect(rect) => {
                    let matches = want.is_none_or(|fill| {
                        rect.fill.to_array()[..3] == fill.to_array()[..3] && rect.fill.a() > 0
                    });
                    let larger = !widest.is_finite() || rect.rect.area() > widest.area();
                    if matches && rect.rect.is_finite() && larger {
                        *widest = rect.rect;
                    }
                }
                Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk_rects(shape, want, widest);
                    }
                }
                _ => {}
            }
        }
        for shape in &output.shapes {
            walk_rects(&shape.shape, background, &mut widest);
        }
        canvas = widest;
    }
    assert!(canvas.is_finite(), "the dock painted a scene canvas");
    assert!(
        canvas.bottom() <= panel_rect.bottom() + 1.0,
        "canvas {canvas:?} overflows the panel {panel_rect:?}"
    );
    assert!(canvas.top() >= panel_rect.top() - 1.0);
}
