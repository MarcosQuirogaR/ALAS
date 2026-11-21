// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The padlock that marks a non-editable field.

use egui::{vec2, Response, Rounding, Sense, Stroke, Ui};

/// A small padlock marking a non-editable field. Hovering or focusing it
/// shows `reason`, one of the closed set of lock reasons.
pub fn lock_icon(ui: &mut Ui, reason: &str) -> Response {
    let size = vec2(16.0, 16.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::focusable_noninteractive());
    let color = ui.visuals().weak_text_color();
    let painter = ui.painter();
    let stroke = Stroke::new(1.5_f32, color);
    let body = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 3.0, rect.center().y),
        egui::pos2(rect.right() - 3.0, rect.bottom() - 2.0),
    );
    painter.rect_filled(body, Rounding::same(2.0), color);
    let cx = rect.center().x;
    let top = rect.top() + 2.0;
    let base = rect.center().y;
    painter.line_segment(
        [egui::pos2(cx - 3.0, base), egui::pos2(cx - 3.0, top + 3.0)],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(cx + 3.0, base), egui::pos2(cx + 3.0, top + 3.0)],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(cx - 3.0, top + 3.0), egui::pos2(cx - 1.5, top)],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(cx - 1.5, top), egui::pos2(cx + 1.5, top)],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(cx + 1.5, top), egui::pos2(cx + 3.0, top + 3.0)],
        stroke,
    );
    response.on_hover_text(reason)
}
