// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The right-side unified aircraft viewer, rendered live from the current
//! configuration with exterior/interior visibility, drag-to-orbit, and true
//! camera zoom. A port of the reference desktop app's `PreviewDock` +
//! `Preview3D`; closable from here or from View > 3D Live Preview.

use alas_report::scene::Scene;
use alas_viz::SceneView;
use egui::{vec2, Align, Area, Color32, Frame, Id, Key, Layout, Order, RichText, Ui};

use crate::state::{AppState, PreviewCamera, PreviewTab};
use crate::views::tr;

const AIRCRAFT_CAMERA_ID: &str = "aircraft_3d";
const AIRCRAFT_VIEW_KEY: &str = "preview_dock::aircraft_3d";

const PREVIEW_OVERLAY_MARGIN: f32 = 8.0;
const PREVIEW_CONTROLS_HEIGHT: f32 = 36.0;
const PREVIEW_LEGEND_HEIGHT: f32 = 80.0;
const PREVIEW_LEGEND_GAP: f32 = 5.0;

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
    ui.add_space(4.0);

    let active_camera_id = AIRCRAFT_CAMERA_ID.to_owned();
    let view_key = AIRCRAFT_VIEW_KEY.to_owned();
    let mut camera_changed = false;

    match &state.preview_scene {
        Some(scene) => {
            // Clone to release the state borrow before giving the view its
            // own mutable state slice.
            let scene = preview_scene_for_tab(scene.clone(), state.preview_tab);
            let width = ui.available_width().max(220.0);
            // The right-side dock owns the full remaining height, so the
            // viewport stays vertical instead of becoming a wide bottom row.
            let height = ui.available_height().max(180.0);
            let scene_revision = state.preview_scene_revision;
            let response = ui.add(
                SceneView::new(&scene, state.view_state_mut(&view_key))
                    .desired_size(vec2(width, height))
                    .orbit_only()
                    .raster_scale(1.0)
                    .show_toolbar(false)
                    .cache_key(&view_key)
                    .cache_revision(scene_revision),
            );
            let response = response.on_hover_text(tr("Drag to orbit the camera; scroll to zoom"));
            camera_changed |= handle_camera_response(state, &response, &active_camera_id);
            camera_changed |= show_aircraft_viewer_controls(
                state,
                ui,
                response.rect,
                &active_camera_id,
                &view_key,
            );
            if state.preview_tab == PreviewTab::Cabin {
                show_cabin_legend(ui, response.rect);
            }
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

/// Place visibility and recovery actions over the aircraft canvas.
fn show_aircraft_viewer_controls(
    state: &mut AppState,
    ui: &mut Ui,
    viewport: egui::Rect,
    camera_id: &str,
    view_key: &str,
) -> bool {
    let current = state.preview_tab;
    let mut requested = current;
    let mut reset = false;
    let (controls, _) = preview_overlay_rects(viewport, current);
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(controls), |ui| {
        Frame::group(ui.style())
            .fill(ui.visuals().panel_fill)
            .inner_margin(egui::Margin::symmetric(5.0, 3.0))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    if ui
                        .selectable_label(current == PreviewTab::Exterior, tr("Exterior"))
                        .on_hover_text(tr("Show the complete aircraft exterior"))
                        .clicked()
                    {
                        requested = PreviewTab::Exterior;
                    }
                    if ui
                        .selectable_label(current == PreviewTab::Cabin, tr("Interior"))
                        .on_hover_text(tr("Reveal the cabin and payload layout"))
                        .clicked()
                    {
                        requested = PreviewTab::Cabin;
                    }
                    if ui
                        .add(egui::Button::new(tr("Reset")).small())
                        .on_hover_text(tr("Restore the default isometric camera and framing"))
                        .clicked()
                    {
                        reset = true;
                    }
                });
            });
    });

    if requested != current {
        state.preview_tab = requested;
    }
    if reset {
        reset_camera(state, camera_id, view_key);
    }
    requested != current || reset
}

fn legend_item(ui: &mut Ui, color: Color32, label: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 2.0, color);
        ui.label(RichText::new(tr(label)).size(11.0));
    });
}

/// Draw a readable screen-space key that never follows the 3-D camera.
///
/// The legend is always present for the cabin view. Its six entries use two
/// explicit rows so the final entries remain inside the dock when translated
/// labels or a narrow dock need more room.
fn show_cabin_legend(ui: &mut Ui, viewport: egui::Rect) {
    let (_, rect) = preview_overlay_rects(viewport, PreviewTab::Cabin);
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
        Frame::group(ui.style())
            .fill(ui.visuals().panel_fill)
            .inner_margin(egui::Margin::same(7.0))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(tr("Cabin legend")).strong().size(12.0));
                    // Keep the longest translated row inside the minimum
                    // 300-point dock while preserving a visible gap between
                    // each swatch/label pair.
                    ui.spacing_mut().item_spacing = vec2(4.0, 3.0);
                    ui.horizontal_centered(|ui| {
                        legend_item(ui, Color32::from_rgb(142, 68, 173), "First class");
                        legend_item(ui, Color32::from_rgb(41, 128, 185), "Business class");
                        legend_item(ui, Color32::from_rgb(39, 174, 96), "Economy class");
                    });
                    ui.horizontal_centered(|ui| {
                        legend_item(ui, Color32::from_rgb(230, 126, 34), "Galley");
                        legend_item(ui, Color32::from_rgb(93, 173, 226), "Lavatory");
                        legend_item(ui, Color32::from_rgb(231, 76, 60), "Exit");
                    });
                });
            });
    });
}

/// Compute the screen-space rectangles shared by the controls and cabin key.
///
/// Both overlays are centred on the canvas rather than on the dock's content
/// cursor. Keeping the rectangles together also makes the five-point gap
/// explicit and prevents the legend frame from extending beyond a narrow
/// viewport.
fn preview_overlay_rects(viewport: egui::Rect, tab: PreviewTab) -> (egui::Rect, egui::Rect) {
    let preferred_controls_width: f32 = if tab == PreviewTab::Cabin {
        304.0
    } else {
        226.0
    };
    let usable_width = (viewport.width() - 16.0).max(1.0);
    let controls_width = preferred_controls_width.min(usable_width);
    let controls = egui::Rect::from_min_size(
        egui::pos2(
            viewport.center().x - controls_width * 0.5,
            viewport.top() + PREVIEW_OVERLAY_MARGIN,
        ),
        vec2(controls_width, PREVIEW_CONTROLS_HEIGHT),
    );

    let legend_width = usable_width.min(310.0);
    let legend_top = controls.bottom() + PREVIEW_LEGEND_GAP;
    let available_height = (viewport.bottom() - legend_top - PREVIEW_OVERLAY_MARGIN).max(1.0);
    let legend_height = PREVIEW_LEGEND_HEIGHT.min(available_height);
    let legend = egui::Rect::from_min_size(
        egui::pos2(viewport.center().x - legend_width * 0.5, legend_top),
        vec2(legend_width, legend_height),
    );
    (controls, legend)
}

/// Hide the generated cabin heading in the live viewer while leaving the
/// report scene builders and exported scene metadata unchanged.
fn preview_scene_for_tab(mut scene: Scene, tab: PreviewTab) -> Scene {
    if tab == PreviewTab::Cabin {
        scene.title = None;
        scene.render_title = false;
    }
    scene
}

fn reset_camera(state: &mut AppState, camera_id: &str, view_key: &str) {
    *state.preview_camera_mut(camera_id) = PreviewCamera::isometric();
    state.view_state_mut(view_key).reset();
}

/// Apply orbit and zoom input to the 3-D camera only.
///
/// Keeping the SceneView at fit-to-view means the raster and its annotations
/// never pan or scale independently from the physical projection.
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
    if response.hovered() {
        let scroll_y = response.ctx.input(|input| input.smooth_scroll_delta.y);
        if scroll_y.abs() > f32::EPSILON {
            let factor = f64::from((1.0 + scroll_y * 0.0015).clamp(0.5, 1.5));
            state
                .preview_camera_mut(camera_id)
                .apply_zoom_factor(factor);
            changed = true;
        }
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
    let figure_id = match state.preview_tab {
        PreviewTab::Exterior => state.selected_preview_id.as_str(),
        PreviewTab::Cabin => "cabin_3d",
    };
    let fullscreen_scene =
        crate::scene::build_page_preview_with_camera(state, figure_id, Some(camera));
    let active_scene = fullscreen_scene.as_ref().unwrap_or(scene).clone();
    let active_scene = preview_scene_for_tab(active_scene, state.preview_tab);
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
        fullscreen_camera_key, fullscreen_open, fullscreen_view_key, open_fullscreen_preview,
        preview_overlay_rects, preview_scene_for_tab, reset_camera, set_fullscreen,
        AIRCRAFT_CAMERA_ID, AIRCRAFT_VIEW_KEY,
    };
    use crate::state::{AppState, PreviewCamera};
    use alas_report::scene::Scene;
    use egui::{pos2, vec2, Context};

    #[test]
    fn exterior_and_interior_share_one_fullscreen_slot() {
        let ctx = Context::default();
        set_fullscreen(&ctx, AIRCRAFT_VIEW_KEY, true);

        assert!(fullscreen_open(&ctx, AIRCRAFT_VIEW_KEY));
        assert!(!fullscreen_open(&ctx, "preview_dock::cabin"));
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
        let view_key = AIRCRAFT_VIEW_KEY;
        let camera_id = AIRCRAFT_CAMERA_ID;
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
        let camera_id = AIRCRAFT_CAMERA_ID;
        let view_key = AIRCRAFT_VIEW_KEY;
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

    #[test]
    fn cabin_overlay_rects_are_centered_inside_the_viewport_with_five_point_gap() {
        let viewport = egui::Rect::from_min_size(pos2(40.0, 20.0), vec2(300.0, 420.0));
        let (controls, legend) = preview_overlay_rects(viewport, crate::state::PreviewTab::Cabin);

        assert!((controls.center().x - viewport.center().x).abs() < f32::EPSILON);
        assert!((legend.center().x - viewport.center().x).abs() < f32::EPSILON);
        assert!((legend.top() - controls.bottom() - 5.0).abs() < f32::EPSILON);
        assert!(controls.left() >= viewport.left());
        assert!(controls.right() <= viewport.right());
        assert!(legend.left() >= viewport.left());
        assert!(legend.right() <= viewport.right());
    }

    #[test]
    fn cabin_preview_hides_the_generated_scene_heading() {
        let mut scene = Scene::new(800.0, 520.0, None);
        scene.title = Some("Cabin / Payload - 3D".to_owned());

        let scene = preview_scene_for_tab(scene, crate::state::PreviewTab::Cabin);

        assert!(scene.title.is_none());
        assert!(!scene.render_title);
    }

    #[test]
    fn exterior_preview_keeps_its_scene_heading() {
        let mut scene = Scene::new(800.0, 520.0, None);
        scene.title = Some("Exterior".to_owned());

        let scene = preview_scene_for_tab(scene, crate::state::PreviewTab::Exterior);

        assert_eq!(scene.title.as_deref(), Some("Exterior"));
        assert!(scene.render_title);
    }
}
