// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Detached view of one Results figure, in its own native OS window.
//!
//! The figure opens in a real viewport (`crate::native_viewport`, which falls
//! back to an egui window where the platform cannot create one) and is closed
//! with Escape, the window's close button, the Close button, or a double-click
//! on the figure.

use super::{
    close_fullscreen_result, fullscreen_cache_key, fullscreen_camera_key, fullscreen_slot_key,
    fullscreen_view_key, images, result_3d, FullscreenFigure,
};
use egui::{vec2, Color32, Frame, Id, Key, Layout, ViewportBuilder};

use crate::native_viewport::show_native_viewport;
use crate::state::AppState;
use crate::views::tr;

/// Stable identity of the detached window. It shares the display-neutral slot
/// key so a theme or language change keeps the same window alive.
pub(super) fn fullscreen_area_id(view_key: &str) -> Id {
    Id::new(("alas_result_fullscreen_area", fullscreen_slot_key(view_key)))
}

/// Render one Results figure in its own window.
pub(super) fn show_fullscreen_result(
    state: &mut AppState,
    ctx: &egui::Context,
    figure: FullscreenFigure<'_>,
) {
    let FullscreenFigure {
        scene,
        revision,
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
    let active_scene = fullscreen_scene.as_deref().unwrap_or(scene);
    let mut restore = false;
    let window_title = tr(title);
    let response = show_native_viewport(
        ctx,
        fullscreen_area_id(view_key),
        window_title.clone(),
        ViewportBuilder::default()
            .with_title(window_title)
            .with_inner_size(vec2(1100.0, 760.0))
            .with_maximized(true)
            .with_min_inner_size(vec2(640.0, 420.0))
            .with_resizable(true),
        |child_ctx, ui, _class| {
            if child_ctx.input(|input| input.key_pressed(Key::Escape)) {
                restore = true;
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
                                restore = true;
                            }
                        });
                    });
                    ui.add_space(6.0);
                    // Keep the scene above the footer and inside the inset
                    // frame, including on the smallest supported window.
                    let openvsp = view_key
                        .split(';')
                        .any(|part| part == "figure=openvsp_cad_preview");
                    let launch_row = if openvsp {
                        super::openvsp::LAUNCH_ROW_HEIGHT
                    } else {
                        0.0
                    };
                    let available = ui.available_size();
                    let available = vec2(
                        available.x.max(320.0),
                        (available.y - 28.0 - launch_row).max(180.0),
                    );
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
                            child_ctx.request_repaint();
                        }
                    } else {
                        let response = ui.add(
                            alas_viz::SceneView::new(
                                active_scene,
                                state.view_state_mut(fullscreen_view.clone()),
                            )
                            .wheel_zoom(true)
                            .show_toolbar(false)
                            .cache_key(&fullscreen_view)
                            .cache_revision(revision)
                            .desired_size(available),
                        );
                        if let Some(figure_id) =
                            crate::views::plot_data_menu::figure_id_from_view_key(view_key)
                        {
                            crate::views::plot_data_menu::attach_scene(
                                &response,
                                active_scene,
                                figure_id,
                                Some(crate::export::run_label(state)),
                            );
                        }
                        restore |= response.double_clicked();
                    }
                    if openvsp {
                        super::openvsp::show_launch_button(state, ui);
                    }
                });
        },
    );
    if restore || response.close_requested {
        close_fullscreen_result(state, ctx, view_key, camera_key);
    }
}
