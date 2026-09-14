// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native viewport renderer for the guided 3-D preview.

use super::{
    close_fullscreen_preview, fullscreen_camera_key, fullscreen_view_key, handle_camera_response,
    preview_scene_for_tab, show_aircraft_viewer_controls, show_cabin_legend,
};
use alas_report::scene::Scene;
use alas_viz::SceneView;
use egui::{vec2, Align, Color32, Frame, Key, Layout, RichText, ViewportBuilder};

use crate::native_viewport::show_native_viewport;
use crate::state::{AppState, PreviewTab};
use crate::views::tr;

/// Render the detached guided 3-D preview in a native viewport.
pub(super) fn show_fullscreen_preview(
    state: &mut AppState,
    ctx: &egui::Context,
    scene: &Scene,
    camera_id: &str,
    view_key: &str,
    preset: &str,
) {
    let fullscreen_view = fullscreen_view_key(view_key);
    let fullscreen_camera = fullscreen_camera_key(camera_id);
    let camera = (*state.preview_camera_mut(&fullscreen_camera)).into();
    let figure_id = match state.preview_tab {
        PreviewTab::Exterior => state.selected_preview_id.as_str(),
        PreviewTab::Cabin => "cabin_3d",
    };
    let fullscreen_scene =
        crate::scene::build_page_preview_with_camera(state, figure_id, Some(camera));
    let active_scene = fullscreen_scene.as_ref().unwrap_or(scene).clone();
    let active_scene = preview_scene_for_tab(active_scene, state.preview_tab);
    let window_title = tr("3D Live Preview");
    let response = show_native_viewport(
        ctx,
        ("preview_fullscreen", view_key),
        window_title.clone(),
        ViewportBuilder::default()
            .with_title(window_title)
            .with_inner_size(vec2(1100.0, 760.0))
            .with_min_inner_size(vec2(640.0, 420.0))
            .with_resizable(true),
        |child_ctx, ui, _class| {
            if child_ctx.input(|input| input.key_pressed(Key::Escape)) {
                close_fullscreen_preview(state, child_ctx, view_key, camera_id);
                return;
            }

            Frame::default()
                .fill(Color32::from_black_alpha(220))
                .inner_margin(egui::Margin::same(18.0))
                .show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    ui.horizontal(|ui| {
                        ui.heading(tr("3D Live Preview"));
                        ui.label(RichText::new(preset).weak().small());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if crate::theme::close_icon_button(ui, tr("Close")).clicked() {
                                close_fullscreen_preview(state, child_ctx, view_key, camera_id);
                            }
                        });
                    });
                    let available = ui.available_size();
                    let available = vec2(available.x.max(320.0), available.y.max(180.0));
                    let scene_revision = state.preview_scene_revision;
                    let response = ui.add(
                        SceneView::new(
                            &active_scene,
                            state.view_state_mut(fullscreen_view.clone()),
                        )
                        .desired_size(available)
                        .orbit_only()
                        .raster_scale(1.5)
                        .show_toolbar(false)
                        .cache_key(&fullscreen_view)
                        .cache_revision(scene_revision),
                    );
                    let response =
                        response.on_hover_text(tr("Drag to orbit the camera; scroll to zoom"));
                    let changed = handle_camera_response(state, &response, &fullscreen_camera)
                        || show_aircraft_viewer_controls(
                            state,
                            ui,
                            response.rect,
                            &fullscreen_camera,
                            &fullscreen_view,
                        );
                    if state.preview_tab == PreviewTab::Cabin {
                        show_cabin_legend(ui, response.rect);
                    }
                    if changed {
                        state.update_preview_scene();
                        child_ctx.request_repaint();
                    }
                });
        },
    );
    if response.close_requested {
        close_fullscreen_preview(state, ctx, view_key, camera_id);
    }
}
