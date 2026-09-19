// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native detached Airfoil Screening workspace.
//!
//! The page renderer remains shared with the navigation route. Keeping the
//! state in [`crate::screening::ScreeningState`] means a sweep, preview
//! selection, result cache, language change, and theme change have one owner
//! regardless of which surface displays them.

use egui::{vec2, RichText, Ui, ViewportBuilder, ViewportCommand};

use crate::native_viewport::{show_native_viewport, viewport_id};
use crate::state::{AppState, LogKind};
use crate::views::{show_screening_view, tr};

/// The key both the viewport and its focus command are built from.
const SCREENING_VIEWPORT_KEY: &str = "airfoil_screening";

/// Render the detached Airfoil Screening workspace when it is open.
pub fn show_screening_window(state: &mut AppState, ctx: &egui::Context) {
    if !state.screening.window_open {
        return;
    }

    let response = show_native_viewport(
        ctx,
        SCREENING_VIEWPORT_KEY,
        tr("Airfoil Screening"),
        ViewportBuilder::default()
            .with_title(tr("Airfoil Screening"))
            .with_inner_size(vec2(1120.0, 760.0))
            .with_min_inner_size(vec2(720.0, 480.0))
            .with_resizable(true),
        |_child_ctx, ui, _class| {
            show_screening_view(state, ui);
        },
    );

    if response.close_requested {
        close_window(state);
    }
}

/// Raise the detached workspace above the window the request came from.
pub(crate) fn focus_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd_to(viewport_id(SCREENING_VIEWPORT_KEY), ViewportCommand::Focus);
}

/// Close the detached workspace.
///
/// A running sweep is deliberately not cancelled here: closing a window is a
/// display action, and the sweep's state lives in [`AppState`], so the same
/// run, status, and Cancel action are still there on the Airfoil Screening
/// page. Silently killing minutes of work because a window was dismissed
/// would be the surprising behaviour. The log line keeps that explicit
/// instead of leaving a background sweep invisible.
fn close_window(state: &mut AppState) {
    state.screening.window_open = false;
    if state.screening.running {
        state.log(
            tr("Airfoil Screening window closed; the sweep is still running. Open the Airfoil Screening page or the Analysis menu to follow or cancel it."),
            LogKind::Info,
        );
    }
}

/// Render the Airfoil Screening navigation page.
///
/// The same renderer serves the detached window, so showing it in both places
/// at once would mean two live copies of one sweep's controls. While the
/// workspace is detached the page becomes a pointer back to it rather than
/// the blank panel an early return would leave.
pub fn show_screening_page(state: &mut AppState, ui: &mut Ui) {
    if !state.screening.window_open {
        show_screening_view(state, ui);
        return;
    }

    ui.heading(tr("Airfoil Screening"));
    ui.add_space(6.0);
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr(
            "The Airfoil Screening workspace is open in its own window. Its sweep, options, and results live there.",
        )));
        if state.screening.is_cancelling() {
            ui.label(
                RichText::new(tr(
                    "A cancellation was requested; the sweep stops at its next stage checkpoint.",
                ))
                .weak(),
            );
        } else if state.screening.running {
            ui.label(RichText::new(tr("A sweep is running in that window.")).weak());
        }
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr("Bring the window to the front")).clicked() {
                focus_window(ui.ctx());
            }
            if ui
                .button(tr("Show it on this page instead"))
                .on_hover_text(tr(
                    "Closes the separate window. A running sweep keeps running.",
                ))
                .clicked()
            {
                close_window(state);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;

    #[test]
    fn closing_the_window_never_cancels_the_sweep_it_was_showing() {
        let mut state = AppState::default();
        state.screening.window_open = true;
        state.screening.running = true;

        close_window(&mut state);

        assert!(!state.screening.window_open);
        assert!(state.screening.running);
        assert!(!state.screening.is_cancelling());
        assert!(state
            .logs
            .iter()
            .any(|line| line.text.contains("still running")));
    }

    #[test]
    fn closing_an_idle_window_does_not_log_a_running_sweep() {
        let mut state = AppState::default();
        state.screening.window_open = true;
        let lines_before = state.logs.len();

        close_window(&mut state);

        assert!(!state.screening.window_open);
        assert_eq!(state.logs.len(), lines_before);
    }
}
