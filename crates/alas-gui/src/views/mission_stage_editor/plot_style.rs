// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Text sizes and colours the mission plot, its axis and its markers share.

use egui::{Color32, FontId, TextStyle, Ui};

use super::model::Trend;
use crate::theme::success_color;

/// The text size of the plot's markers and axis: the app's small text.
pub(super) fn small_font(ui: &Ui) -> FontId {
    TextStyle::Small.resolve(ui.style())
}

/// Width of `text` in `font`, pt.
pub(super) fn text_width(ui: &Ui, text: &str, font: &FontId) -> f32 {
    ui.fonts(|fonts| {
        fonts
            .layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE)
            .size()
            .x
    })
}

pub(super) fn trend_color(ui: &Ui, trend: Trend) -> Color32 {
    match trend {
        Trend::Climb => ui.visuals().hyperlink_color,
        Trend::Level => success_color(ui.visuals()),
        Trend::Descent => ui.visuals().warn_fg_color,
    }
}
