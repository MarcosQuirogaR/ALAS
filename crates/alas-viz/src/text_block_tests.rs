// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The interactive renderer wraps [`SceneElement::TextBlock`] paragraphs with
//! the real glyph metrics of the painting context, so a status diagnostic
//! never runs past its content box however the scene is scaled into a card.

use alas_report::scene::SceneElement;
use alas_report::status_figure::{figure_status_message, STATUS_MARGIN, STATUS_WIDTH};
use egui::epaint::TextShape;
use egui::Shape;

use crate::render::{render_scene_to_shapes_with_context, ViewportTransform};

fn context() -> egui::Context {
    let context = egui::Context::default();
    let _ = context.run(egui::RawInput::default(), |_| {});
    context
}

fn text_shapes(shapes: &[Shape]) -> Vec<&TextShape> {
    shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Text(text) => Some(text),
            _ => None,
        })
        .collect()
}

fn long_diagnostic() -> String {
    let long_path = format!(
        r"C:\Users\Marcos\ALAS\runs\{}\aircraft.history",
        "diagnostic".repeat(30)
    );
    format!("{long_path}: The system cannot find the file specified. (os error 2)")
}

#[test]
fn a_long_diagnostic_wraps_inside_the_block_at_every_card_scale() {
    let message = long_diagnostic();
    let scene = figure_status_message(
        "VSPAERO wake history unavailable",
        &message,
        false,
        Some("dark"),
    );
    assert!(scene
        .elements
        .iter()
        .any(|element| matches!(element, SceneElement::TextBlock { .. })));
    let context = context();
    for card_width in [320.0_f32, 560.0, 900.0, 1400.0] {
        let target = egui::Rect::from_min_size(
            egui::pos2(10.0, 20.0),
            egui::vec2(card_width, card_width * 0.6),
        );
        let transform = ViewportTransform::fit(scene.width, scene.height, target);
        let shapes = render_scene_to_shapes_with_context(&scene, &transform, &context);
        let texts = text_shapes(&shapes);
        let body = texts
            .iter()
            .find(|text| text.galley.text() == message)
            .unwrap_or_else(|| panic!("body galley at card width {card_width}"));
        let right_edge = transform.to_screen([STATUS_WIDTH - STATUS_MARGIN, 0.0]).x;
        let bottom_edge = transform.to_screen([0.0, scene.height]).y;
        assert!(
            body.pos.x + body.galley.rect.width() <= right_edge + 0.5,
            "card {card_width}: body reaches {} past the content box at {right_edge}",
            body.pos.x + body.galley.rect.width()
        );
        assert!(
            body.galley.rows.len() > 1,
            "card {card_width}: the diagnostic should wrap"
        );
        assert!(
            body.pos.y + body.galley.rect.height() <= bottom_edge + 0.5,
            "card {card_width}: the wrapped body must stay inside the scene"
        );
        let title = texts
            .iter()
            .find(|text| text.galley.text() == "VSPAERO wake history unavailable")
            .expect("title galley");
        assert_eq!(title.galley.rows.len(), 1);
    }
}

#[test]
fn status_figure_evidence_renders_when_requested() {
    let Ok(dir) = std::env::var("ALAS_STATUS_FIGURE_EVIDENCE_DIR") else {
        return;
    };
    std::fs::create_dir_all(&dir).expect("evidence directory");
    let message = long_diagnostic();
    for theme in ["light", "grey", "dark"] {
        let scene = figure_status_message(
            "VSPAERO wake history unavailable",
            &message,
            false,
            Some(theme),
        );
        let png = crate::raster::render_scene_png(&scene).expect("status figure PNG");
        std::fs::write(format!("{dir}/status-figure-{theme}.png"), png).expect("write PNG");
        std::fs::write(
            format!("{dir}/status-figure-{theme}.svg"),
            alas_report::render_svg(&scene),
        )
        .expect("write SVG");
    }
}
