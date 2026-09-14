// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native viewport renderer for a full-size Results figure.

use super::{
    close_fullscreen_result, fullscreen_cache_key, fullscreen_camera_key, fullscreen_view_key,
    images, result_3d, FullscreenFigure,
};
use egui::{vec2, Color32, Frame, Key, Layout, ViewportBuilder};

use crate::native_viewport::show_native_viewport;
use crate::state::AppState;
use crate::views::tr;

/// Render one Results figure in a detached native viewport.
pub(super) fn show_fullscreen_result(
    state: &mut AppState,
    ctx: &egui::Context,
    figure: FullscreenFigure<'_>,
) {
    let FullscreenFigure {
        scene,
        config,
        theme,
        camera_key,
        view_key,
        title,
        description,
        orbitable,
    } = figure;
    let fullscreen_view = fullscreen_view_key(view_key);
    let fullscreen_camera = fullscreen_camera_key(camera_key);
    let fullscreen_cache = fullscreen_cache_key(view_key);
    let fullscreen_scene = if orbitable {
        let camera = (*state.result_camera_mut(&fullscreen_camera)).into();
        state.cached_result_figure_with_camera(
            &fullscreen_cache,
            result_3d::MISSION_ROUTE_3D,
            config,
            theme,
            Some(camera),
        )
    } else {
        None
    };
    let active_scene = fullscreen_scene.as_deref().unwrap_or(scene).clone();
    let window_title = tr(title);
    let response = show_native_viewport(
        ctx,
        ("result_fullscreen", super::fullscreen_slot_key(view_key)),
        window_title.clone(),
        ViewportBuilder::default()
            .with_title(window_title)
            .with_inner_size(vec2(1100.0, 760.0))
            .with_min_inner_size(vec2(640.0, 420.0))
            .with_resizable(true),
        |child_ctx, ui, _class| {
            if child_ctx.input(|input| input.key_pressed(Key::Escape)) {
                close_fullscreen_result(state, child_ctx, view_key, camera_key);
                return;
            }

            Frame::default()
                .fill(Color32::from_black_alpha(220))
                .inner_margin(egui::Margin::same(18.0))
                .show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    ui.horizontal(|ui| {
                        ui.heading(tr(title)).on_hover_text(tr(description));
                        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                            if crate::theme::close_icon_button(ui, tr("Close")).clicked() {
                                close_fullscreen_result(state, child_ctx, view_key, camera_key);
                            }
                        });
                    });
                    ui.add_space(6.0);
                    // Keep the scene above the footer and inside the inset
                    // frame, including on the smallest supported window.
                    let available = ui.available_size();
                    let available = vec2(available.x.max(320.0), (available.y - 28.0).max(180.0));
                    if images::scene_has_external_images(&active_scene) {
                        let _ = images::show_external_images(
                            state,
                            ui,
                            &active_scene,
                            available.x,
                            available.y,
                            true,
                        );
                    } else if orbitable {
                        let interaction = result_3d::show_orbit_view(
                            state,
                            ui,
                            &active_scene,
                            &fullscreen_camera,
                            &fullscreen_view,
                            vec2(available.x, available.y.max(180.0)),
                            true,
                        );
                        if interaction.camera_changed {
                            result_3d::rebuild_scene(
                                state,
                                &fullscreen_cache,
                                &fullscreen_camera,
                                config,
                                theme,
                            );
                            child_ctx.request_repaint();
                        }
                    } else {
                        ui.add(
                            alas_viz::SceneView::new(
                                &active_scene,
                                state.view_state_mut(fullscreen_view.clone()),
                            )
                            .wheel_zoom(true)
                            .show_toolbar(false)
                            .desired_size(available),
                        );
                    }
                });
        },
    );
    if response.close_requested {
        close_fullscreen_result(state, ctx, view_key, camera_key);
    }
}
