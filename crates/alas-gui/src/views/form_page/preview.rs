// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The page-level live preview widget and its sizing.

use egui::{vec2, RichText, Ui};

use crate::state::AppState;
use crate::views::tr;

/// The smallest and largest height a page preview is drawn at, in points.
pub(super) const PREVIEW_MIN_HEIGHT: f32 = 240.0;
pub(super) const PREVIEW_MAX_HEIGHT: f32 = 480.0;

/// The size a page preview widget is given, in points.
///
/// A widget as wide as the container with a capped height would draw the
/// three-view schematic height-limited inside a box several times wider than
/// the drawing, leaving most of the container empty and the axis labels tiny.
/// Sizing the widget to the *scene's own aspect* makes the drawing as large as
/// the height budget allows and removes the empty band, because the widget is
/// no longer wider than what it draws.
pub(super) fn preview_size(
    available_width: f32,
    screen_height: f32,
    scene: (f64, f64),
) -> egui::Vec2 {
    let available_width = available_width.max(220.0);
    let aspect = if scene.1 > 0.0 && scene.0 > 0.0 {
        (scene.0 / scene.1) as f32
    } else {
        1.6
    };
    let ceiling = (screen_height * 0.6).clamp(PREVIEW_MIN_HEIGHT, PREVIEW_MAX_HEIGHT);
    let floor = PREVIEW_MIN_HEIGHT.min(ceiling);
    let height = (available_width / aspect).clamp(floor, ceiling);
    let width = (height * aspect).min(available_width);
    vec2(width, height)
}

pub(super) fn render_preview(
    state: &mut AppState,
    ui: &mut Ui,
    preview: Option<&str>,
    preview_title: Option<&str>,
) {
    let preview_id = if let Some(preview) = preview {
        preview
    } else if state.active_page == "cabin" {
        // Cabin changes deserve immediate visual confirmation even though this
        // advanced page has no report-only figure.
        "cabin_3d"
    } else {
        return;
    };
    ui.add_space(8.0);
    let title = preview_title.unwrap_or(if preview_id == "cabin_3d" {
        "Seat and payload preview"
    } else {
        "Preview"
    });
    ui.label(RichText::new(alas_i18n::t(Some(title), None)).strong());
    match state.cached_page_preview(preview_id) {
        Some((scene, revision)) => {
            let view_key = format!("page_preview::{preview_id}");
            let size = preview_size(
                ui.available_width(),
                ui.ctx().screen_rect().height(),
                (scene.width, scene.height),
            );
            let view = alas_viz::SceneView::new(&scene, state.view_state_mut(view_key.clone()))
                .static_view()
                .show_toolbar(false)
                .cache_key(&view_key)
                .cache_revision(revision)
                .desired_size(size);
            // Centred, and only as wide as the drawing, so the figure is not
            // letterboxed inside the container.
            ui.vertical_centered(|ui| {
                ui.add(view);
            });
        }
        None => {
            ui.label(RichText::new(tr("Preview needs a completed run.")).weak());
        }
    }
}
