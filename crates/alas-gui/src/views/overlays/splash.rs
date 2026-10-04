// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The boot splash.

use egui::{pos2, vec2, Context, Rect, Vec2};

use crate::state::AppState;

/// Consistent margin, in points, between splash content and the window edge.
pub(super) const SPLASH_MARGIN: f32 = 24.0;
/// Fixed height reserved at the bottom for the wordmark and the
/// author/license line beneath it, so the independently centred main symbol
/// above can never grow tall enough to overlap them.
pub(super) const SPLASH_FOOTER_HEIGHT: f32 = 96.0;
/// Keep the central mark visually subordinate to the footer at ordinary
/// desktop sizes. The bounds still shrink with the client area below these
/// caps, including when Windows reports a short high-DPI client rectangle.
pub(super) const SPLASH_SYMBOL_MAX_WIDTH: f32 = 460.0;
pub(super) const SPLASH_SYMBOL_MAX_HEIGHT: f32 = 220.0;
pub(super) const SPLASH_WORDMARK_MAX_WIDTH: f32 = 320.0;
pub(super) const SPLASH_WORDMARK_MAX_HEIGHT: f32 = 56.0;

/// Render the boot splash while `boot_frames_remaining` is still counting down.
///
/// The main three-stripe symbol and the smaller symbol/wordmark footer are
/// positioned independently (one centred in the full client area, the
/// other anchored to the bottom with a consistent margin) rather than as
/// one fused composite image, so each keeps sensible proportions as the
/// window is resized instead of both clustering toward the top.
pub fn show_splash(state: &mut AppState, ctx: &Context) {
    if state.boot_frames_remaining == 0 {
        return;
    }
    state.boot_frames_remaining -= 1;
    egui::CentralPanel::default().show(ctx, |ui| {
        let panel = ui.max_rect();

        let footer_top = (panel.max.y - SPLASH_FOOTER_HEIGHT).max(panel.min.y);
        let footer_rect = Rect::from_min_max(
            pos2(panel.min.x, footer_top),
            pos2(panel.max.x, panel.max.y - SPLASH_MARGIN),
        );
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(footer_rect), |ui| {
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.label(author_license_line());
                ui.add_space(6.0);
                if let Some(image) = crate::branding::text_logo_image(ctx) {
                    let width =
                        (panel.width() - 2.0 * SPLASH_MARGIN).clamp(1.0, SPLASH_WORDMARK_MAX_WIDTH);
                    ui.add(image.max_size(vec2(width, SPLASH_WORDMARK_MAX_HEIGHT)));
                }
            });
        });

        // Centred on the full client area, but capped to whichever of the
        // top or bottom clearance is tighter so it can never reach the
        // footer above, even in a short window.
        if let (Some(image), Some(natural)) = (
            crate::branding::logo_image(ctx),
            crate::branding::logo_natural_size(),
        ) {
            let size = splash_symbol_size(natural, panel, footer_top);
            // `Image::from_texture` uses `ImageFit::Exact(texture_size)` with
            // an unlimited `max_size`.  A surrounding `ui.put` rectangle is
            // therefore only a placement hint: egui lets the image keep its
            // native texture dimensions and it overflows that rectangle.  A
            // real image bound is required to make the computed splash size
            // reach the painter while retaining the source aspect ratio.
            ui.put(
                Rect::from_center_size(panel.center(), size),
                image.max_size(size),
            );
        }
    });
}

/// Choose a capped symbol size that remains centred while leaving the footer
/// clear on both sides of the client area.
pub(super) fn splash_symbol_size(natural: Vec2, panel: Rect, footer_top: f32) -> Vec2 {
    let top_half = (panel.center().y - panel.min.y - SPLASH_MARGIN).max(0.0);
    let bottom_half = (footer_top - SPLASH_MARGIN - panel.center().y).max(0.0);
    let bounds = vec2(
        (panel.width() - 2.0 * SPLASH_MARGIN).clamp(1.0, SPLASH_SYMBOL_MAX_WIDTH),
        (2.0 * top_half.min(bottom_half)).clamp(1.0, SPLASH_SYMBOL_MAX_HEIGHT),
    );
    fit_within(natural, bounds)
}

/// The largest size with `natural`'s aspect ratio that still fits within
/// `bounds` on both axes.
pub(super) fn fit_within(natural: Vec2, bounds: Vec2) -> Vec2 {
    if natural.x <= 0.0 || natural.y <= 0.0 || bounds.x <= 0.0 || bounds.y <= 0.0 {
        return Vec2::ZERO;
    }
    natural * (bounds.x / natural.x).min(bounds.y / natural.y)
}

/// The splash footer's author/license line, built from this crate's own
/// `Cargo.toml` metadata (workspace `authors`/`license`) rather than a second
/// hard-coded copy of it. Not routed through the translation catalog: a
/// personal name and an SPDX license identifier have no Spanish equivalent,
/// the same treatment the About window already gives this metadata below.
pub(super) fn author_license_line() -> String {
    let authors = env!("CARGO_PKG_AUTHORS");
    let author = authors
        .split_once('<')
        .map_or(authors, |(name, _)| name)
        .trim();
    format!("{author} \u{b7} {}", env!("CARGO_PKG_LICENSE"))
}
