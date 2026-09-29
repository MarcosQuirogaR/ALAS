// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The About window.

use egui::{vec2, Context, RichText, Window};

use super::tr;
use crate::state::AppState;

/// Render the About window, if it is open.
pub fn show_about(state: &mut AppState, ctx: &Context) {
    if !state.show_about {
        return;
    }
    let mut open = true;
    Window::new(tr("About ALAS"))
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .show(ctx, |ui| {
            if let Some(image) = crate::branding::text_logo_image(ctx) {
                ui.add(image.max_size(vec2(300.0, 40.0)));
            }
            ui.add_space(8.0);
            ui.label(RichText::new(tr("About ALAS")).strong())
                .on_hover_text(tr(
                    "Conceptual transport aircraft sizing, optimization, and multi-disciplinary analysis.",
                ));
            ui.add_space(8.0);
            ui.label("SPDX-License-Identifier: AGPL-3.0-or-later");
            ui.label("Copyright (C) 2026 Marcos Quiroga Rodriguez");
        });
    state.show_about = open;
}
