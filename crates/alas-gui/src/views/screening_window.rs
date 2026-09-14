// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native detached Airfoil Screening workspace.
//!
//! The page renderer remains shared with the navigation route. Keeping the
//! state in [`crate::screening::ScreeningState`] means a sweep, preview
//! selection, result cache, language change, and theme change have one owner
//! regardless of which surface displays them.

use egui::{vec2, ViewportBuilder};

use crate::native_viewport::show_native_viewport;
use crate::state::AppState;
use crate::views::{show_screening_view, tr};

/// Render the detached Airfoil Screening workspace when it is open.
pub fn show_screening_window(state: &mut AppState, ctx: &egui::Context) {
    if !state.screening.window_open {
        return;
    }

    let response = show_native_viewport(
        ctx,
        "airfoil_screening",
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
        state.screening.window_open = false;
    }
}
