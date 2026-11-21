// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The closed-combo arrow. egui flips the default arrow upward when the popup
//! would open above the box (a combo near the window bottom); every combo
//! here shows the same downward arrow instead.

use egui::epaint::{Shape, Stroke};
use egui::{vec2, Rect, Ui};

/// Icon function for `ComboBox::icon`: always a downward triangle.
pub(crate) fn down_arrow(
    ui: &Ui,
    rect: Rect,
    visuals: &egui::style::WidgetVisuals,
    _is_open: bool,
    _above_or_below: egui::AboveOrBelow,
) {
    let rect = Rect::from_center_size(
        rect.center(),
        vec2(rect.width() * 0.7, rect.height() * 0.45),
    );
    ui.painter().add(Shape::convex_polygon(
        vec![rect.left_top(), rect.right_top(), rect.center_bottom()],
        visuals.fg_stroke.color,
        Stroke::NONE,
    ));
}
