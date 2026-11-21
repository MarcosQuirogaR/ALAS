// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Small layout helpers of the Cabin & Cargo page: the sub-heading style and
//! the equal-height rule of a row of cards.

use egui::{Id, Ui};

/// A sub-heading inside a card: the section header style of its depth, with a
/// thin rule under it, so a group title is not mistaken for the first field's
/// label.
pub(super) fn sub_heading(ui: &mut Ui, title: &str) {
    let title = crate::theme::section_title(ui, title);
    crate::theme::header_label(ui, title);
    crate::theme::header_row(ui, |ui| {
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 2.0), egui::Sense::hover());
        let y = rect.center().y;
        ui.painter().line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            ui.visuals().widgets.noninteractive.bg_stroke,
        );
    });
}

/// What one row of cards remembers between frames.
#[derive(Debug, Clone, Copy, Default)]
struct RowHeights {
    frame: u64,
    /// The tallest card of the previous frame.
    target: f32,
    /// The tallest card of this frame so far.
    building: f32,
}

/// Pad the card being drawn to the height of the tallest card of its row.
///
/// Call at the end of each card's content with the content's natural height
/// (`ui.min_rect().height()` before any padding). Cards of one row pass the
/// same `row` id; the heights are matched from the previous frame, so the row
/// settles after one extra frame.
pub(super) fn match_row_height(ui: &mut Ui, row: Id, natural: f32) {
    let frame = ui.ctx().cumulative_pass_nr();
    let mut heights: RowHeights = ui.data(|data| data.get_temp(row)).unwrap_or_default();
    if heights.frame != frame {
        heights.target = heights.building;
        heights.building = 0.0;
        heights.frame = frame;
    }
    heights.building = heights.building.max(natural);
    let extra = (heights.target - natural).max(0.0);
    ui.data_mut(|data| data.insert_temp(row, heights));
    if heights.building > heights.target + 0.5 {
        ui.ctx().request_repaint();
    }
    if extra > 0.0 {
        ui.allocate_space(egui::vec2(0.0, extra));
    }
}
