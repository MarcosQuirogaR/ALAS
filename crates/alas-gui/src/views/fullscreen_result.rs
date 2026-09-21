// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! In-window maximized view of one Results figure.
//!
//! Result figures maximize inside the main ALAS window: a foreground [`Area`]
//! covers the current viewport and is restored with Escape, the Close button,
//! or a double-click on the maximized figure.  Detached native viewports
//! (`crate::native_viewport`) are reserved for menus, editors and tool
//! windows; a figure is inspected in place and never spawns an OS window.

use super::{
    close_fullscreen_result, fullscreen_cache_key, fullscreen_camera_key, fullscreen_slot_key,
    fullscreen_view_key, images, result_3d, FullscreenFigure,
};
use egui::{vec2, Align2, Area, Color32, Frame, Id, Key, Layout, Order};

use crate::state::AppState;
use crate::views::tr;

/// Stable identity of the maximized overlay.  It shares the display-neutral
/// slot key so a theme or language change keeps the same overlay alive.
pub(super) fn fullscreen_area_id(view_key: &str) -> Id {
    Id::new(("alas_result_fullscreen_area", fullscreen_slot_key(view_key)))
}

/// Render one Results figure maximized over the main window.
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
    if ctx.input(|input| input.key_pressed(Key::Escape)) {
        close_fullscreen_result(state, ctx, view_key, camera_key);
        return;
    }

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
    let active_scene = fullscreen_scene.as_deref().unwrap_or(scene);
    let screen = ctx.screen_rect();
    let screen_size = screen.size();
    let mut restore = false;
    Area::new(fullscreen_area_id(view_key))
        .order(Order::Foreground)
        .default_size(screen_size)
        .constrain_to(screen)
        .pivot(Align2::LEFT_TOP)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            // Area defaults to the size its content requests.  A figure was
            // therefore able to create a short overlay on a tall monitor.
            // Establish the full window before the frame measures itself so
            // the overlay also covers, and shields, the gallery beneath it.
            ui.set_min_size(screen_size);
            Frame::default()
                .fill(Color32::from_black_alpha(220))
                .inner_margin(egui::Margin::same(18.0))
                .show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    ui.horizontal(|ui| {
                        ui.heading(tr(title)).on_hover_text(tr(description));
                        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                            if crate::theme::close_icon_button(ui, tr("Close")).clicked() {
                                restore = true;
                            }
                        });
                    });
                    ui.add_space(6.0);
                    // Keep the scene above the footer and inside the inset
                    // frame, including on the smallest supported window.
                    let available = ui.available_size();
                    let available = vec2(available.x.max(320.0), (available.y - 28.0).max(180.0));
                    let canvas_rect = egui::Rect::from_min_size(ui.cursor().min, available);
                    // A double-click maximized the card; the same gesture on
                    // the maximized figure restores it.  The click that
                    // opened the overlay cannot re-trigger here because egui
                    // resolves a click against the widget that took the press.
                    if images::scene_has_external_images(active_scene) {
                        restore |= images::show_external_images(
                            state,
                            ui,
                            active_scene,
                            available.x,
                            available.y,
                            true,
                        );
                    } else if orbitable {
                        let interaction = result_3d::show_orbit_view(
                            state,
                            ui,
                            active_scene,
                            &fullscreen_camera,
                            &fullscreen_view,
                            vec2(available.x, available.y.max(180.0)),
                            true,
                        );
                        restore |= interaction.double_clicked;
                        if interaction.camera_changed {
                            result_3d::rebuild_scene(
                                state,
                                &fullscreen_cache,
                                &fullscreen_camera,
                                config,
                                theme,
                            );
                            ctx.request_repaint();
                        }
                    } else {
                        let response = ui.add(
                            alas_viz::SceneView::new(
                                active_scene,
                                state.view_state_mut(fullscreen_view.clone()),
                            )
                            .wheel_zoom(true)
                            .show_toolbar(false)
                            .desired_size(available),
                        );
                        restore |= response.double_clicked();
                    }
                    if view_key.split(';').any(|part| part == "figure=openvsp_cad_preview") {
                        super::openvsp::show_launch_button(state, ui, canvas_rect);
                    }
                });
        });
    if restore {
        close_fullscreen_result(state, ctx, view_key, camera_key);
    }
}
