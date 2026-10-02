// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::splash::{
    author_license_line, fit_within, splash_symbol_size, SPLASH_FOOTER_HEIGHT, SPLASH_MARGIN,
    SPLASH_SYMBOL_MAX_HEIGHT, SPLASH_SYMBOL_MAX_WIDTH,
};
use super::walkthrough::{
    walkthrough_panel_position, WALKTHROUGH_ORDER, WALKTHROUGH_WINDOW_HIGHLIGHT_ID,
};
use egui::{pos2, vec2, Pos2, Rect, Vec2};

#[test]
fn walkthrough_explanation_is_available_without_painting_inline_prose() {
    alas_i18n::set_language(Some("en"));
    let mut state = crate::state::AppState {
        show_walkthrough: true,
        ..Default::default()
    };
    state.walkthrough_step = 0;
    let ctx = egui::Context::default();
    let raw_input = || egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 700.0))),
        ..Default::default()
    };
    let _ = ctx.run(raw_input(), |ctx| super::show_walkthrough(&mut state, ctx));
    let output = ctx.run(raw_input(), |ctx| super::show_walkthrough(&mut state, ctx));
    fn collect_text<'a>(shape: &'a egui::Shape, painted: &mut Vec<&'a str>) {
        match shape {
            egui::Shape::Text(text) => painted.push(text.galley.job.text.as_str()),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_text(shape, painted);
                }
            }
            _ => {}
        }
    }
    let mut painted = Vec::new();
    for shape in &output.shapes {
        collect_text(&shape.shape, &mut painted);
    }
    assert!(
        painted.contains(&crate::views::tour_data::TOUR_STEPS[0].title),
        "{painted:?}"
    );
    assert!(!painted.contains(&crate::views::tour_data::TOUR_STEPS[0].body));
}

#[test]
fn walkthrough_panel_moves_below_a_target_when_room_exists() {
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1_280.0, 820.0));
    let target = Rect::from_min_size(pos2(20.0, 40.0), vec2(220.0, 180.0));
    let position = walkthrough_panel_position(Some(target), screen, vec2(420.0, 220.0));

    assert!(position.y > target.max.y);
    assert!(position.x >= 16.0);
}

#[test]
fn walkthrough_panel_moves_above_a_low_target() {
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1_280.0, 820.0));
    let target = Rect::from_min_size(pos2(20.0, 690.0), vec2(900.0, 100.0));
    let position = walkthrough_panel_position(Some(target), screen, vec2(420.0, 220.0));

    assert!(position.y + 220.0 < target.min.y);
}

#[test]
fn walkthrough_panel_stays_clickable_on_a_short_window() {
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(880.0, 560.0));
    let target = Rect::from_min_size(pos2(10.0, 390.0), vec2(860.0, 100.0));
    let position = walkthrough_panel_position(Some(target), screen, vec2(420.0, 280.0));

    assert!(position.y >= 16.0);
    assert!(position.y + 280.0 <= screen.max.y - 16.0);
}

#[test]
fn overlay_shell_strings_have_spanish_desktop_translations() {
    let catalog = alas_i18n::es::desktop_catalog();
    for key in [
        "Walkthrough",
        "Step {current} of {total}",
        "Skip",
        "Get started",
        "Next ->",
        "<- Back",
        "Advanced Walkthrough",
        "Manage storage",
        "Only ALAS-owned generated data is listed here. Tool installations and saved aircraft documents are not removed.",
        "Storage clearing is disabled while an analysis is running.",
        "Generated outputs",
        "Airfoil CFD cases",
        "Solver scratch files",
        "Downloaded navigation data",
        "Downloaded globe texture",
        "Results, exports and solver files written by runs; the next run recreates what it needs.",
        "Airfoil CFD studies with their meshes, solver logs and results; clearing removes every saved study.",
        "Work directories external solvers left in the system temporary folder after an interrupted run.",
        "Navigation data for airway routing; downloaded again on demand.",
        "Earth image for the route globe; downloaded again on demand.",
        "{size} * {files} files",
        "Cleared {category}.",
        "Cleared {removed} paths; {failed} could not be removed.",
        "Saved tool paths",
        "Resetting saved paths leaves installed tools untouched; ALAS will discover them again on the next run or launch.",
        "Reset saved tool paths",
        "Saved tool paths reset; installed tools were not removed.",
        "Could not reset saved tool paths: {error}",
        "Output directory: {path}",
        "{size} on disk",
        "Not created yet.",
        "Clear exported outputs",
        "Cleared {path}.",
        "Could not clear outputs: {error}",
        "About ALAS",
        "ALAS - Aircraft Layout and Analysis Suite",
        "Conceptual transport aircraft sizing, optimization, and multi-disciplinary analysis.",
    ] {
        assert!(catalog.contains_key(key), "missing overlay text: {key}");
    }
}

#[test]
fn walkthrough_window_highlight_has_a_dedicated_foreground_layer() {
    let layer = egui::LayerId::new(
        WALKTHROUGH_ORDER,
        egui::Id::new(WALKTHROUGH_WINDOW_HIGHLIGHT_ID),
    );
    assert_eq!(layer.order, egui::Order::Foreground);
    assert_eq!(layer.id, egui::Id::new(WALKTHROUGH_WINDOW_HIGHLIGHT_ID));
}

#[test]
fn fit_within_preserves_aspect_ratio_on_the_tighter_axis() {
    // A wide source in a square box: width is the binding constraint.
    let size = fit_within(vec2(1000.0, 400.0), vec2(500.0, 500.0));
    assert!((size.x - 500.0).abs() < 1e-6);
    assert!((size.y - 200.0).abs() < 1e-6);

    // The same source in a short, wide box: height binds instead.
    let size = fit_within(vec2(1000.0, 400.0), vec2(500.0, 100.0));
    assert!((size.x - 250.0).abs() < 1e-6);
    assert!((size.y - 100.0).abs() < 1e-6);
}

#[test]
fn fit_within_degrades_to_zero_for_a_degenerate_input() {
    assert_eq!(fit_within(vec2(0.0, 400.0), vec2(500.0, 500.0)), Vec2::ZERO);
    assert_eq!(
        fit_within(vec2(1000.0, 400.0), vec2(0.0, 500.0)),
        Vec2::ZERO
    );
}

#[test]
fn author_license_line_names_the_author_and_license_without_the_email() {
    let line = author_license_line();
    assert_eq!(line, "Marcos Quiroga Rodriguez \u{b7} AGPL-3.0-or-later");
    assert!(!line.contains('@'));
    assert!(!line.contains('<'));
}

#[test]
fn splash_symbol_is_capped_and_keeps_the_footer_clear() {
    let panel = Rect::from_min_size(pos2(0.0, 0.0), vec2(1_280.0, 820.0));
    let footer_top = panel.max.y - SPLASH_FOOTER_HEIGHT;
    let natural = vec2(1_511.0, 692.0);
    let size = splash_symbol_size(natural, panel, footer_top);
    let symbol = Rect::from_center_size(panel.center(), size);

    assert_eq!(symbol.center(), panel.center());
    assert!(size.x <= SPLASH_SYMBOL_MAX_WIDTH + f32::EPSILON);
    assert!(size.y <= SPLASH_SYMBOL_MAX_HEIGHT + f32::EPSILON);
    assert!(symbol.min.y >= panel.min.y + SPLASH_MARGIN - f32::EPSILON);
    assert!(symbol.max.y <= footer_top - SPLASH_MARGIN + f32::EPSILON);
}

#[test]
fn splash_symbol_shrinks_for_a_short_client_height() {
    let panel = Rect::from_min_size(pos2(0.0, 0.0), vec2(880.0, 320.0));
    let footer_top = panel.max.y - SPLASH_FOOTER_HEIGHT;
    let size = splash_symbol_size(vec2(1_511.0, 692.0), panel, footer_top);
    let symbol = Rect::from_center_size(panel.center(), size);

    assert!(symbol.min.y >= panel.min.y + SPLASH_MARGIN - f32::EPSILON);
    assert!(symbol.max.y <= footer_top - SPLASH_MARGIN + f32::EPSILON);
    assert!(size.y < SPLASH_SYMBOL_MAX_HEIGHT);
}

/// Run the same egui widget path as the desktop splash and return the
/// tessellated bounds of its two embedded image textures.  This catches
/// widget-level overflow that a pure `splash_symbol_size` test cannot see.
fn rendered_splash_bounds(
    viewport_size: Vec2,
    native_pixels_per_point: f32,
    zoom_factor: f32,
) -> (Rect, Vec<Rect>) {
    let mut state = crate::state::AppState {
        boot_frames_remaining: 1,
        ..Default::default()
    };
    let ctx = egui::Context::default();
    let raw_input = || {
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, viewport_size)),
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .expect("root viewport")
            .native_pixels_per_point = Some(native_pixels_per_point);
        input
    };

    // A real native zoom change takes effect at the next egui pass.  Feed
    // one setup pass so the test exercises that same DPI/zoom transition.
    if (zoom_factor - 1.0).abs() > f32::EPSILON {
        let _ = ctx.run(raw_input(), |_| {});
        ctx.set_zoom_factor(zoom_factor);
    }
    let output = ctx.run(raw_input(), |ctx| super::show_splash(&mut state, ctx));
    let panel = ctx.screen_rect();
    let mut bounds = Vec::new();
    for primitive in ctx.tessellate(output.shapes, output.pixels_per_point) {
        let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else {
            continue;
        };
        // TextureId::default() is egui's font atlas.  The remaining two
        // meshes are the supplied wordmark and main symbol.
        if mesh.texture_id == egui::TextureId::default() {
            continue;
        }
        let Some(first) = mesh.vertices.first() else {
            continue;
        };
        let mut rect = Rect::from_min_max(first.pos, first.pos);
        for vertex in &mesh.vertices[1..] {
            rect = rect.union(Rect::from_min_max(vertex.pos, vertex.pos));
        }
        bounds.push(rect);
    }
    (panel, bounds)
}

#[test]
fn rendered_splash_contains_the_main_symbol_at_supported_sizes_and_zooms() {
    let natural = crate::branding::logo_natural_size().expect("embedded logo size");
    for (viewport_size, native_ppp, zoom_factor) in [
        (vec2(640.0, 360.0), 1.0, 1.0),
        (vec2(1_280.0, 820.0), 1.0, 1.0),
        (vec2(1_920.0, 1_080.0), 1.0, 1.0),
        (vec2(1_280.0, 820.0), 1.5, 1.0),
        (vec2(1_280.0, 820.0), 2.0, 1.5),
    ] {
        let (panel, mut bounds) = rendered_splash_bounds(viewport_size, native_ppp, zoom_factor);
        assert_eq!(bounds.len(), 2, "expected main symbol and footer image");

        let footer_top = (panel.max.y - SPLASH_FOOTER_HEIGHT).max(panel.min.y);
        let expected_size = splash_symbol_size(natural, panel, footer_top);
        let expected_rect = Rect::from_center_size(panel.center(), expected_size);
        let main_index = bounds
            .iter()
            .position(|rect| (rect.center().y - panel.center().y).abs() < 2.0)
            .expect("main symbol mesh centered in the client area");
        let main = bounds.swap_remove(main_index);
        let footer = bounds.pop().expect("footer mesh");

        // Tessellation rounds image vertices to roughly half a point; a
        // large excess here means the Image widget escaped its ui.put box.
        assert!(
            expected_rect.expand(1.5).contains_rect(main),
            "main image {:?} escaped its fitted rect {:?} for {:?}, dpi {}, zoom {}",
            main,
            expected_rect,
            viewport_size,
            native_ppp,
            zoom_factor
        );
        assert!((main.center().x - panel.center().x).abs() < 1.0);
        assert!((main.center().y - panel.center().y).abs() < 1.0);
        assert!(
            ((main.width() / main.height()) - (natural.x / natural.y)).abs() < 0.02,
            "main image aspect ratio changed: {:?} vs {:?}",
            main.size(),
            natural
        );

        assert!((footer.center().x - panel.center().x).abs() < 1.0);
        assert!(footer.max.y <= panel.max.y - SPLASH_MARGIN + 1.0);
        assert!(
            main.max.y + 1.0 <= footer.min.y,
            "main symbol {:?} overlaps footer {:?} for {:?}, dpi {}, zoom {}",
            main,
            footer,
            viewport_size,
            native_ppp,
            zoom_factor
        );
    }
}
