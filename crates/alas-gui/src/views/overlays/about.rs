// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The About window.

use egui::{vec2, Context, RichText, ViewportBuilder};

use super::tr;
use crate::native_viewport::show_compact_native_viewport;
use crate::state::AppState;

/// Render the About window, if it is open.
pub fn show_about(state: &mut AppState, ctx: &Context) {
    if !state.show_about {
        return;
    }
    let title = tr("About ALAS");
    let response = show_compact_native_viewport(
        ctx,
        "about",
        title.clone(),
        ViewportBuilder::default()
            .with_title(title)
            .with_inner_size(vec2(440.0, 190.0))
            .with_resizable(false),
        |ctx, ui, _class| {
            if let Some(image) = crate::branding::text_logo_image(ctx) {
                ui.add(image.max_size(vec2(300.0, 40.0)));
            }
            ui.add_space(8.0);
            ui.label(RichText::new(format!("ALAS {}", env!("CARGO_PKG_VERSION"))).strong())
                .on_hover_text(tr(
                    "Conceptual transport aircraft sizing, optimization, and multi-disciplinary analysis.",
                ));
            ui.add_space(8.0);
            ui.label(tr("GNU AGPL v3.0 or later"));
            ui.label("Copyright (C) 2026 Marcos Quiroga Rodriguez");
        },
    );
    if response.close_requested {
        state.show_about = false;
    }
}
