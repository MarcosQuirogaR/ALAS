// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The right-side vertical "3D Live Preview" dock: Exterior 3D / Cabin-Payload
//! sub-tabs, rendered live from the current configuration with drag-to-rotate
//! and scroll-to-zoom. A port of the reference desktop app's `PreviewDock` +
//! `Preview3D`; closable from here or from View > 3D Live Preview.

use alas_report::scene::Scene;
use alas_viz::SceneView;
use egui::{vec2, Align, Area, Color32, Frame, Id, Key, Layout, Order, RichText, Ui};

use crate::state::{AppState, PreviewCamera, PreviewTab};
use crate::views::tr;

/// Render the preview dock's contents into the given side panel.
pub fn show_preview_dock(state: &mut AppState, ui: &mut Ui) {
    #[cfg(debug_assertions)]
    crate::layout_debug::record_ui(
        ui.ctx(),
        "preview dock content",
        ui,
        crate::layout_debug::RegionKind::Preview,
    );
    ui.horizontal(|ui| {
        ui.heading(tr("3D Live Preview"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::theme::close_icon_button(ui, tr("Close")).clicked() {
                state.preview_open = false;
            }
        });
    });
    ui.label(RichText::new(&state.active_preset).weak().small());
    ui.add_space(2.0);

    ui.horizontal(|ui| {
        if ui
            .selectable_label(state.preview_tab == PreviewTab::Exterior, tr("Exterior 3D"))
            .clicked()
        {
            state.preview_tab = PreviewTab::Exterior;
            state.update_preview_scene();
        }
        if ui
            .selectable_label(
                state.preview_tab == PreviewTab::Cabin,
                tr("Cabin / Payload"),
            )
            .clicked()
        {
            state.preview_tab = PreviewTab::Cabin;
            state.update_preview_scene();
        }
    });
    ui.add_space(4.0);

    let active_camera_id = match state.preview_tab {
        PreviewTab::Exterior => state.selected_preview_id.clone(),
        PreviewTab::Cabin => "cabin_3d".to_owned(),
    };
    let view_key = match state.preview_tab {
        PreviewTab::Exterior => {
            format!("preview_dock::exterior::{}", state.selected_preview_id)
        }
        PreviewTab::Cabin => "preview_dock::cabin".to_owned(),
    };

    let mut camera_changed = show_camera_controls(state, ui, &active_camera_id, &view_key);
    if camera_changed {
        state.update_preview_scene();
    }
    ui.add_space(4.0);

    match &state.preview_scene {
        Some(scene) => {
            // Clone to release the state borrow before giving the view its
            // own mutable state slice.
            let scene = scene.clone();
            let width = ui.available_width().max(220.0);
            // The right-side dock owns the full remaining height, so the
            // viewport stays vertical instead of becoming a wide bottom row.
            let height = ui.available_height().max(180.0);
            let response = ui.add(
                SceneView::new(&scene, state.view_state_mut(&view_key))
                    .desired_size(vec2(width, height))
                    .orbit_only()
                    .wheel_zoom(true)
                    .show_toolbar(false),
            );
            let response = response.on_hover_text(tr("Drag to orbit the camera; scroll to zoom"));
            camera_changed |= handle_camera_response(state, &response, &active_camera_id);
            if response.double_clicked() {
                open_fullscreen_preview(state, ui.ctx(), &view_key, &active_camera_id);
            }
            if camera_changed {
                state.update_preview_scene();
                ui.ctx().request_repaint();
            }
            if fullscreen_open(ui.ctx(), &view_key) {
                let preset = state.active_preset.clone();
                show_fullscreen_preview(
                    state,
                    ui.ctx(),
                    &scene,
                    &active_camera_id,
                    &view_key,
                    &preset,
                );
            }
        }
        None => {
            ui.centered_and_justified(|ui| ui.label(tr("Loading configuration...")));
        }
    }
}

/// Show the one explicit recovery action for both preview tabs.
///
/// Camera movement is deliberately gesture-driven: dragging orbits the
/// projection and the viewport's wheel zooms it. Keeping reset here also
/// resets the 2-D viewport state used to rasterize the projected scene.
fn show_camera_controls(
    state: &mut AppState,
    ui: &mut Ui,
    camera_id: &str,
    view_key: &str,
) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        if ui
            .button(tr("Reset"))
            .on_hover_text(tr("Restore the default isometric camera and framing"))
            .clicked()
        {
            reset_camera(state, camera_id, view_key);
            changed = true;
        }
        ui.label(
            RichText::new(tr("Drag to orbit; scroll to zoom"))
                .weak()
                .small(),
        );
    });
    changed
}

fn reset_camera(state: &mut AppState, camera_id: &str, view_key: &str) {
    *state.preview_camera_mut(camera_id) = PreviewCamera::isometric();
    state.view_state_mut(view_key).reset();
}

/// Apply orbit input to the 3-D camera only. The SceneView's 2-D pan state is
/// deliberately not touched, which keeps fullscreen and dock gestures on the
/// same independent camera state.
fn handle_camera_response(
    state: &mut AppState,
    response: &egui::Response,
    camera_id: &str,
) -> bool {
    let mut changed = false;
    if !response.double_clicked()
        && (response.dragged_by(egui::PointerButton::Primary)
            || response.dragged_by(egui::PointerButton::Middle))
    {
        // The response is backed by a scene that is rebuilt after every
        // camera change.  Applying the gesture total against an origin tied
        // to that response lets the next frame replay stale input and makes
        // the projection appear to snap back.  Consume only this frame's
        // motion so the keyed camera remains the single source of truth.
        let delta = response.drag_motion();
        state
            .preview_camera_mut(camera_id)
            .apply_orbit_motion(delta);
        changed = delta.is_finite();
    }
    changed
}

fn fullscreen_id(view_key: &str) -> Id {
    Id::new(("alas_preview_fullscreen", view_key))
}

fn fullscreen_open(ctx: &egui::Context, view_key: &str) -> bool {
    ctx.data(|data| data.get_temp::<bool>(fullscreen_id(view_key)))
        .unwrap_or(false)
}

fn set_fullscreen(ctx: &egui::Context, view_key: &str, open: bool) {
    ctx.data_mut(|data| data.insert_temp(fullscreen_id(view_key), open));
}

fn fullscreen_view_key(view_key: &str) -> String {
    format!("fullscreen_view::{view_key}")
}

fn fullscreen_camera_key(camera_id: &str) -> String {
    format!("fullscreen_camera::{camera_id}")
}

/// Copy the dock state once as the overlay opens. The dock and overlay must
/// never share a camera or a 2-D viewport because either can remain visible
/// after the other is closed.
fn open_fullscreen_preview(
    state: &mut AppState,
    ctx: &egui::Context,
    view_key: &str,
    camera_id: &str,
) {
    let view_state = state.view_states.get(view_key).cloned().unwrap_or_default();
    state
        .view_states
        .insert(fullscreen_view_key(view_key), view_state);
    let camera = *state.preview_camera_mut(camera_id);
    state
        .preview_cameras
        .insert(fullscreen_camera_key(camera_id), camera);
    set_fullscreen(ctx, view_key, true);
}

fn show_fullscreen_preview(
    state: &mut AppState,
    ctx: &egui::Context,
    scene: &Scene,
    camera_id: &str,
    view_key: &str,
    preset: &str,
) {
    if ctx.input(|input| input.key_pressed(Key::Escape)) {
        close_fullscreen_preview(state, ctx, view_key, camera_id);
        return;
    }

    let fullscreen_view = fullscreen_view_key(view_key);
    let fullscreen_camera = fullscreen_camera_key(camera_id);
    let camera = (*state.preview_camera_mut(&fullscreen_camera)).into();
    let fullscreen_scene =
        crate::scene::build_page_preview_with_camera(state, camera_id, Some(camera));
    let active_scene = fullscreen_scene.as_ref().unwrap_or(scene);
    let screen = ctx.screen_rect();
    let screen_size = screen.size();
    Area::new(Id::new(("alas_preview_fullscreen_area", view_key)))
        .order(Order::Foreground)
        .default_size(screen.size())
        .constrain_to(screen)
        .pivot(egui::Align2::LEFT_TOP)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            // A foreground Area otherwise takes its child content's height,
            // which makes a so-called maximized preview stop mid-window.
            ui.set_min_size(screen_size);
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
                                close_fullscreen_preview(state, ctx, view_key, camera_id);
                            }
                        });
                    });
                    let changed =
                        show_camera_controls(state, ui, &fullscreen_camera, &fullscreen_view);
                    // Leave room for the footer so the scene stays within
                    // the inset frame instead of reaching the lower edge.
                    let available = ui.available_size();
                    let available = vec2(available.x.max(320.0), (available.y - 28.0).max(180.0));
                    let response = ui.add(
                        SceneView::new(active_scene, state.view_state_mut(fullscreen_view.clone()))
                            .desired_size(available)
                            .orbit_only()
                            .wheel_zoom(true)
                            .show_toolbar(false),
                    );
                    let response =
                        response.on_hover_text(tr("Drag to orbit the camera; scroll to zoom"));
                    let changed =
                        changed || handle_camera_response(state, &response, &fullscreen_camera);
                    if changed {
                        ctx.request_repaint();
                    }
                });
        });
}

fn close_fullscreen_preview(
    state: &mut AppState,
    ctx: &egui::Context,
    view_key: &str,
    camera_id: &str,
) {
    state.view_states.remove(&fullscreen_view_key(view_key));
    state
        .preview_cameras
        .remove(&fullscreen_camera_key(camera_id));
    set_fullscreen(ctx, view_key, false);
}

#[cfg(test)]
mod tests {
    use super::{
        fullscreen_camera_key, fullscreen_id, fullscreen_open, fullscreen_view_key,
        open_fullscreen_preview, reset_camera, set_fullscreen,
    };
    use crate::state::{AppState, PreviewCamera};
    use egui::Context;

    #[test]
    fn fullscreen_state_is_independent_for_each_preview_figure() {
        let ctx = Context::default();
        set_fullscreen(&ctx, "preview_dock::exterior::exterior_3d", true);

        assert!(fullscreen_open(&ctx, "preview_dock::exterior::exterior_3d"));
        assert!(!fullscreen_open(&ctx, "preview_dock::cabin"));
        assert_ne!(
            fullscreen_id("preview_dock::exterior::exterior_3d"),
            fullscreen_id("preview_dock::cabin")
        );
    }

    #[test]
    fn closing_fullscreen_keeps_the_per_figure_slot_available() {
        let ctx = Context::default();
        set_fullscreen(&ctx, "preview_dock::cabin", true);
        set_fullscreen(&ctx, "preview_dock::cabin", false);

        assert!(!fullscreen_open(&ctx, "preview_dock::cabin"));
    }

    #[test]
    fn fullscreen_preview_keeps_the_dock_camera_and_viewport_unchanged() {
        let ctx = Context::default();
        let mut state = AppState::default();
        let view_key = "preview_dock::exterior::exterior_3d";
        let camera_id = "exterior_3d";
        state.view_state_mut(view_key).pan = egui::vec2(-6.0, 9.0);
        *state.preview_camera_mut(camera_id) = PreviewCamera::side();

        open_fullscreen_preview(&mut state, &ctx, view_key, camera_id);
        state.view_state_mut(fullscreen_view_key(view_key)).pan = egui::vec2(18.0, 3.0);
        state
            .preview_camera_mut(fullscreen_camera_key(camera_id))
            .zoom = 4.0;

        assert_eq!(state.view_state_mut(view_key).pan, egui::vec2(-6.0, 9.0));
        assert_eq!(
            state.preview_camera_mut(camera_id).zoom,
            PreviewCamera::side().zoom
        );
    }

    #[test]
    fn reset_returns_camera_and_raster_view_to_default_isometric() {
        let mut state = AppState::default();
        let camera_id = "exterior_3d";
        let view_key = "preview_dock::exterior::exterior_3d";
        *state.preview_camera_mut(camera_id) = PreviewCamera::top();
        let view = state.view_state_mut(view_key);
        view.pan = egui::vec2(24.0, -12.0);
        view.zoom = 3.0;
        view.auto_fit = false;

        reset_camera(&mut state, camera_id, view_key);

        assert_eq!(
            *state.preview_camera_mut(camera_id),
            PreviewCamera::isometric()
        );
        assert_eq!(
            *state.view_state_mut(view_key),
            alas_viz::SceneViewState::default()
        );
    }
}
