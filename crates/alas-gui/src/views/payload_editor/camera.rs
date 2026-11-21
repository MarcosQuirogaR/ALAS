// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the two canvases share of mouse navigation: the wheel zooms about the
//! cursor, a drag that starts on empty canvas pans, and a few small buttons
//! sit in a strip above it (Fit, Fuselage).

use egui::{Pos2, Response, Ui, Vec2};

/// Zoom per point of wheel travel (exponential, so a notch is a fixed ratio).
const WHEEL_RATE: f32 = 0.0035;

/// The zoom factor the wheel asks for over `response`, with the cursor
/// position it zooms about. The scroll is consumed so the page behind does not
/// scroll as well.
pub(super) fn wheel(ui: &Ui, response: &Response) -> Option<(Pos2, f64)> {
    if !response.hovered() {
        return None;
    }
    let scroll = ui.input(|input| input.raw_scroll_delta.y + input.smooth_scroll_delta.y);
    let pinch = ui.input(|input| input.zoom_delta());
    if scroll.abs() < 1.0e-3 && (pinch - 1.0).abs() < 1.0e-4 {
        return None;
    }
    let cursor = ui.input(|input| input.pointer.hover_pos())?;
    ui.input_mut(|input| {
        input.raw_scroll_delta = Vec2::ZERO;
        input.smooth_scroll_delta = Vec2::ZERO;
    });
    Some((cursor, f64::from((scroll * WHEEL_RATE).exp() * pinch)))
}

/// The pan the pointer asks for this frame: a drag that began on empty canvas
/// (not on an item, and not while an item is being dragged) moves the view.
pub(super) fn pan(
    ui: &Ui,
    response: &Response,
    started_on_item: bool,
    dragging_item: bool,
) -> Option<Vec2> {
    let id = response.id.with("pan");
    if response.drag_started() {
        ui.ctx()
            .data_mut(|data| data.insert_temp(id, !started_on_item));
    }
    if dragging_item || !response.dragged() {
        return None;
    }
    let panning = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(id))
        .unwrap_or(false);
    (panning && response.drag_delta() != Vec2::ZERO).then(|| response.drag_delta())
}

/// Corner radius of both canvases, points.
pub(super) const CANVAS_RADIUS: f32 = 6.0;

/// What a camera was fitted for: the canvas size (to 8 points, so a scrollbar
/// appearing does not count) and the extent of the aircraft. A camera the user
/// has not moved is fitted again when this changes.
pub(super) fn fit_key(rect: egui::Rect, extent: [f64; 5]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    ((rect.width() / 8.0).round() as i64).hash(&mut hasher);
    ((rect.height() / 8.0).round() as i64).hash(&mut hasher);
    for value in extent {
        value.to_bits().hash(&mut hasher);
    }
    hasher.finish() | 1
}

/// One small button in a canvas header strip.
pub(super) struct Overlay<'a> {
    pub label: String,
    pub tooltip: &'a str,
    pub selected: bool,
}

/// Height a canvas header strip takes (buttons plus the gap under them), points.
pub(super) const HEADER_HEIGHT: f32 = 32.0;

/// A strip above a canvas: `left` at the left, `buttons` right-aligned (last
/// button leftmost). It is a row of its own, so nothing is drawn over the
/// picture, and a button's label is never wrapped; on a very narrow canvas the
/// buttons move to a second line as whole buttons. Returns the index of the
/// button clicked.
pub(super) fn header(
    ui: &mut Ui,
    left: impl FnOnce(&mut Ui),
    buttons: &[Overlay],
) -> Option<usize> {
    let mut clicked = None;
    // `reverse` draws the last button first, for a right-to-left row.
    let mut draw = |ui: &mut Ui, reverse: bool| {
        let order: Vec<usize> = if reverse {
            (0..buttons.len()).rev().collect()
        } else {
            (0..buttons.len()).collect()
        };
        for index in order {
            let button = &buttons[index];
            let response = ui
                .add(crate::theme::selectable_button(
                    button.label.clone(),
                    button.selected,
                ))
                .on_hover_text(button.tooltip);
            if response.clicked() {
                clicked = Some(index);
            }
        }
    };
    ui.scope(|ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        ui.spacing_mut().interact_size.y = 24.0;
        if ui.available_width() < 460.0 {
            ui.horizontal_wrapped(|ui| {
                left(ui);
                draw(ui, false);
            });
        } else {
            ui.horizontal(|ui| {
                left(ui);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    draw(ui, true);
                });
            });
        }
    });
    ui.add_space(HEADER_HEIGHT - 24.0 - 2.0);
    clicked
}
