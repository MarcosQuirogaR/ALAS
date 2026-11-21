// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The camera row of the sandbox viewport: Overview, the presets and Fit.

use egui::{pos2, Rect, Ui};

use crate::state::{AppState, PreviewCamera};
use crate::views::tr;

use super::scene::{SANDBOX_CAMERA_ID, SANDBOX_VIEW_KEY};
use super::viewport::{
    centered_row, floating_button, floating_control, CAMERA_ROW_HEIGHT, OVERLAY_INSET,
};

/// The camera row centred at the top of the viewport: Overview, the name of
/// the isolated component when one is focused, the presets and Fit.
/// Presets and orbit only turn the camera; Fit refits the framing to the
/// shown components and resets the zoom. Returns the row rectangle.
pub(super) fn show_viewport_controls(
    state: &mut AppState,
    ui: &mut Ui,
    viewport: Rect,
    _camera_changed: &mut bool,
) -> Rect {
    handle_escape(state, ui.ctx());
    let top = viewport.top() + OVERLAY_INSET;
    let row = centered_row(ui, "camera", viewport, top, CAMERA_ROW_HEIGHT, |ui| {
        // Overview is the no-component state of the same selection the
        // category stack renders, so it shows as selected exactly when no
        // component is focused.
        let overview = state.sandbox.focus().is_none();
        if floating_control(
            ui,
            "camera",
            true,
            overview,
            egui::Button::new(tr("Overview")).small(),
        )
        .on_hover_text(tr(
            "Show the whole aircraft again without changing any data.",
        ))
        .clicked()
        {
            state.show_whole_aircraft();
        }
        if !overview {
            // The isolation has an obvious way back: a labelled toggle here,
            // a second click on the component, or Escape.
            // An action, not a view toggle: tinted, bold, never in the
            // selected (filled) style the presets and Overview use.
            let tint = ui.visuals().hyperlink_color.gamma_multiply(0.22);
            if floating_control(
                ui,
                "camera",
                true,
                false,
                egui::Button::new(egui::RichText::new(tr("Show whole aircraft")).strong())
                    .fill(tint)
                    .small(),
            )
            .on_hover_text(tr("Show every component again (Esc)."))
            .clicked()
            {
                state.show_whole_aircraft();
            }
        }
        for (label, camera) in [
            ("Iso", PreviewCamera::isometric()),
            ("Top", PreviewCamera::top()),
            ("Front", PreviewCamera::front()),
            ("Rear", PreviewCamera::rear()),
            ("Side", PreviewCamera::side()),
        ] {
            if floating_button(ui, "camera", egui::Button::new(tr(label)).small()).clicked() {
                // A preset refits: the new orientation is framed for the free
                // part of the viewport at zoom 1, not at the old view's zoom.
                *state.preview_camera_mut(SANDBOX_CAMERA_ID) = camera;
                state.view_state_mut(SANDBOX_VIEW_KEY).reset();
                state.refit_sandbox_framing();
            }
        }
        if floating_button(ui, "camera", egui::Button::new(tr("Fit")).small())
            .on_hover_text(tr(
                "Refit the current view without changing its orientation.",
            ))
            .clicked()
        {
            state.preview_camera_mut(SANDBOX_CAMERA_ID).fit();
            state.view_state_mut(SANDBOX_VIEW_KEY).reset();
            state.refit_sandbox_framing();
        }
    });
    Rect::from_min_max(
        pos2(viewport.left(), top),
        pos2(viewport.right(), row.bottom().max(top + CAMERA_ROW_HEIGHT)),
    )
}

/// Escape returns from an isolated component to the whole aircraft, unless a
/// text box owns the key.
pub(super) fn handle_escape(state: &mut AppState, ctx: &egui::Context) {
    if state.sandbox.focus().is_some()
        && !ctx.wants_keyboard_input()
        && ctx.input(|input| input.key_pressed(egui::Key::Escape))
    {
        state.show_whole_aircraft();
    }
}
