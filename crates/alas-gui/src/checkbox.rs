// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The app's check box. egui draws the box glyph with the widget rounding,
//! which the theme sets for buttons and fields (6 pt): on a 16 pt glyph that
//! is a circle, the look of a radio button. A check box is drawn here with a
//! small corner radius so it reads as a square.

use egui::{Response, Ui, WidgetText};

/// Corner radius of the check box glyph, points.
pub const GLYPH_RADIUS: f32 = 3.0;

/// `ui.checkbox`, with a square glyph.
pub fn checkbox(ui: &mut Ui, checked: &mut bool, text: impl Into<WidgetText>) -> Response {
    let original = ui.style().clone();
    let widgets = &mut ui.visuals_mut().widgets;
    for state in [
        &mut widgets.noninteractive,
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
        &mut widgets.open,
    ] {
        state.rounding = egui::Rounding::same(GLYPH_RADIUS);
    }
    let text = crate::notation::widget(ui, text.into());
    let response = ui.checkbox(checked, text);
    ui.set_style(original);
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The glyph's corner radius is below half its side, so it is a square
    /// with soft corners, and the style is the same after the call.
    #[test]
    fn the_glyph_is_a_rounded_square_and_the_style_is_restored() {
        let ctx = egui::Context::default();
        let mut on = true;
        let mut rounding_after = None;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let before = ui.visuals().widgets.inactive.rounding;
                checkbox(ui, &mut on, "x");
                rounding_after = Some((before, ui.visuals().widgets.inactive.rounding));
            });
        });
        let (before, after) = rounding_after.expect("drawn");
        assert_eq!(before, after);
        assert!(GLYPH_RADIUS < ctx.style().spacing.icon_width / 2.0);
    }
}
