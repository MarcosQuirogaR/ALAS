// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The flat, left-aligned row used by the navigation rail and the Files list.

use egui::{vec2, Sense, Ui};

/// One rail row: a full-width, left-aligned target; the selected row is filled
/// with the selection colour.
pub fn rail_row(ui: &mut Ui, label: &str, selected: bool, indent: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let (fill, text_color) = if selected {
            (visuals.selection.bg_fill, visuals.selection.stroke.color)
        } else if response.hovered() {
            (
                visuals.widgets.hovered.weak_bg_fill,
                visuals.widgets.hovered.fg_stroke.color,
            )
        } else {
            (egui::Color32::TRANSPARENT, visuals.text_color())
        };
        ui.painter()
            .rect_filled(rect, crate::theme::SELECTABLE_CONTROL_RADIUS, fill);
        ui.painter().text(
            egui::pos2(rect.left() + 10.0 + indent, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(14.0),
            text_color,
        );
    }
    response
}
