// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! LOPA landmarks retained alongside the generated main-deck seat identifiers.

use alas_payload::layout::{ItemKind, PayloadLayout};

use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};

/// Draw the actual main-deck service bays and emergency exits over the seat
/// map. Rows leave these stations empty; exposing their physical footprints
/// makes that absence explainable instead of looking like a drawing gap.
pub(super) fn draw_main_deck_services(
    scene: &mut Scene,
    axes: &Axes2D,
    layout: &PayloadLayout,
    lo: f64,
    hi: f64,
    final_segment: bool,
) {
    for item in layout.by_deck("main") {
        if !matches!(
            item.kind,
            ItemKind::Galley
                | ItemKind::Lav
                | ItemKind::AccessibleLav
                | ItemKind::WheelchairStowage
                | ItemKind::Exit
        ) || item.x < lo
            || (item.x >= hi && !final_segment)
        {
            continue;
        }
        let p0 = axes.map_point(item.x - item.length * 0.5, item.y - item.width * 0.5);
        let p1 = axes.map_point(item.x + item.length * 0.5, item.y + item.width * 0.5);
        let left_edge = axes.left + 1.0;
        let right_edge = axes.left + axes.width - 1.0;
        let top_edge = axes.top + 1.0;
        let bottom_edge = axes.top + axes.height - 1.0;
        if item.kind == ItemKind::Exit {
            let x = ((p0[0] + p1[0]) * 0.5).clamp(left_edge, right_edge);
            scene.add(SceneElement::Line {
                p1: [x, p0[1].clamp(top_edge, bottom_edge)],
                p2: [x, p1[1].clamp(top_edge, bottom_edge)],
                stroke: Stroke::new(Color::from_hex("#e74c3c"), 2.0),
            });
        } else {
            let (color, label) = match item.kind {
                ItemKind::Galley => (Color::from_hex("#e67e22"), "G"),
                ItemKind::WheelchairStowage => (Color::from_hex("#f1c40f"), "W"),
                ItemKind::AccessibleLav => (Color::from_hex("#2471a3"), "AL"),
                _ => (Color::from_hex("#5dade2"), "L"),
            };
            let left = p0[0].min(p1[0]).clamp(left_edge, right_edge);
            let right = p0[0].max(p1[0]).clamp(left_edge, right_edge);
            let top = p0[1].min(p1[1]).clamp(top_edge, bottom_edge);
            let bottom = p0[1].max(p1[1]).clamp(top_edge, bottom_edge);
            let width = (right - left).max(1.0);
            let height = (bottom - top).max(1.0);
            scene.add(SceneElement::Rect {
                x: left,
                y: top,
                width,
                height,
                rx: 1.0,
                fill: Some(Fill::new(color)),
                stroke: Some(Stroke::new(Color::from_hex("#101214"), 0.5)),
            });
            scene.add(SceneElement::Text {
                text: label.to_owned(),
                pos: [left + width * 0.5, top + height * 0.5],
                font_size: 9.0,
                color: Color::rgb(255, 255, 255),
                align: TextAlign::Center,
                baseline: TextBaseline::Middle,
                angle_deg: 0.0,
                bold: true,
            });
        }
    }
}
