// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The right-side unified aircraft viewer, rendered live from the current
//! configuration with exterior/interior visibility, drag-to-orbit, and true
//! camera zoom. A port of the reference desktop app's `PreviewDock` +
//! `Preview3D`; closable from here or from View > 3D Live Preview.

use alas_report::scene::Scene;
use alas_viz::SceneView;
use egui::{vec2, Align, Color32, FontId, Frame, Id, Layout, RichText, TextStyle, Ui};

use crate::state::{AppState, PreviewCamera, PreviewTab};
use crate::views::tr;

#[path = "fullscreen_preview.rs"]
mod fullscreen_preview;
use fullscreen_preview::show_fullscreen_preview;

const AIRCRAFT_CAMERA_ID: &str = "aircraft_3d";
const AIRCRAFT_VIEW_KEY: &str = "preview_dock::aircraft_3d";

const PREVIEW_OVERLAY_MARGIN: f32 = 8.0;
const PREVIEW_CONTROLS_HEIGHT: f32 = 36.0;
const PREVIEW_LEGEND_HEIGHT: f32 = 80.0;
const PREVIEW_LEGEND_GAP: f32 = 5.0;
const LEGEND_INNER_MARGIN: f32 = 7.0;
const LEGEND_ROW_GAP: f32 = 3.0;
const CONTROLS_INNER_MARGIN: egui::Vec2 = vec2(5.0, 3.0);

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
    refit_on_subject_change(state, ui.ctx(), &view_key);
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

/// What the dock is currently drawing: the visibility mode and the figure.
fn preview_subject(state: &AppState) -> String {
    match state.preview_tab {
        PreviewTab::Cabin => "cabin".to_owned(),
        PreviewTab::Exterior => format!("exterior::{}", state.selected_preview_id),
    }
}

/// Fit the dock to its model again when the subject it draws changes.
///
/// One viewport state serves every figure the dock can show, so a pan or zoom
/// made on one subject was still in force when another took its place and the
/// new model was left off-centre and at the wrong scale, even though switching
/// subject is exactly when a fit is wanted. A camera orbit of the same subject
/// is untouched.
fn refit_on_subject_change(state: &mut AppState, ctx: &egui::Context, view_key: &str) {
    let subject = preview_subject(state);
    let id = Id::new(("preview_dock_subject", view_key));
    let previous = ctx.data(|data| data.get_temp::<String>(id));
    if previous.as_deref() == Some(subject.as_str()) {
        return;
    }
    ctx.data_mut(|data| data.insert_temp(id, subject));
    if previous.is_some() {
        state.view_state_mut(view_key.to_owned()).reset();
    }
}

/// Place visibility and recovery actions over the aircraft canvas.
///
/// The card is sized to its row before the frame is created: an egui child
/// ui seeds its minimum rect at its own top-left corner, so a frame that
/// simply hugs a centred row inside the wider controls region would stretch
/// from the region's left edge to the row's right edge and leave the buttons
/// in its right half. The row width is remembered from the previous frame
/// (see [`centered_row`]) so the card converges on the exact width even if
/// the estimate and the widgets disagree.
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
    let row_id = Id::new(("preview_controls_row", view_key));
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(controls), |ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        let row_width = remembered_row_width(ui, row_id, controls_row_width(ui));
        let row_height = ui.spacing().interact_size.y;
        let card_width = (row_width + 2.0 * CONTROLS_INNER_MARGIN.x).min(controls.width());
        let card =
            egui::Rect::from_center_size(controls.center(), vec2(card_width, controls.height()));
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(card), |ui| {
            Frame::group(ui.style())
                .fill(ui.visuals().panel_fill)
                .inner_margin(egui::Margin::symmetric(
                    CONTROLS_INNER_MARGIN.x,
                    CONTROLS_INNER_MARGIN.y,
                ))
                .show(ui, |ui| {
                    centered_row(ui, row_id, row_width, row_height, |ui| {
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
    let (rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, color);
    ui.label(RichText::new(tr(label)).size(11.0));
}

/// The width a [`centered_row`] with this id occupied on the previous frame,
/// or the caller's estimate when nothing has been measured yet.
fn remembered_row_width(ui: &Ui, row_id: Id, estimate: f32) -> f32 {
    ui.data(|data| data.get_temp::<f32>(row_id))
        .unwrap_or(estimate)
        .max(1.0)
}

/// Allocate a horizontal row of `row_width` centred in the available width so
/// the controls and their labels stay centred even when the overlay is wider
/// than the row.
///
/// After layout the width the contents actually occupied is stored under
/// `row_id` for [`remembered_row_width`], so a font, padding or translation
/// change can leave the row off-centre for at most one frame.
fn centered_row<R>(
    ui: &mut Ui,
    row_id: Id,
    row_width: f32,
    row_height: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    let row_width = row_width.max(1.0);
    let available = ui.available_rect_before_wrap();
    let rect = egui::Rect::from_min_size(
        egui::pos2(available.center().x - row_width * 0.5, available.top()),
        vec2(row_width, row_height.max(1.0)),
    );
    let layout = if ui.layout().prefer_right_to_left() {
        Layout::right_to_left(Align::Center)
    } else {
        Layout::left_to_right(Align::Center)
    };
    let response = ui.allocate_new_ui(
        egui::UiBuilder::new().max_rect(rect).layout(layout),
        add_contents,
    );
    let actual_width = response.response.rect.width();
    if actual_width.is_finite() && (actual_width - row_width).abs() > 0.25 {
        ui.data_mut(|data| data.insert_temp(row_id, actual_width));
        ui.ctx().request_repaint();
    }
    response
}

/// Size of a single line of text laid out the way the labels and buttons lay
/// it out, including the pixel rounding of the row height.
fn text_size(ui: &Ui, text: &str, font_id: FontId) -> egui::Vec2 {
    let text_color = ui.visuals().text_color();
    ui.fonts(|fonts| {
        fonts
            .layout_no_wrap(text.to_owned(), font_id, text_color)
            .size()
    })
}

fn text_width(ui: &Ui, text: &str, font_id: FontId) -> f32 {
    text_size(ui, text, font_id).x
}

fn controls_row_width(ui: &Ui) -> f32 {
    let button_padding = ui.spacing().button_padding.x;
    let reset_padding = if ui.visuals().button_frame {
        button_padding
    } else {
        0.0
    };
    let exterior = text_width(ui, &tr("Exterior"), TextStyle::Button.resolve(ui.style()))
        + 2.0 * button_padding;
    let interior = text_width(ui, &tr("Interior"), TextStyle::Button.resolve(ui.style()))
        + 2.0 * button_padding;
    let reset =
        text_width(ui, &tr("Reset"), TextStyle::Body.resolve(ui.style())) + 2.0 * reset_padding;
    exterior + interior + reset + 2.0 * ui.spacing().item_spacing.x
}

const LEGEND_LABELS: [&str; 6] = [
    "First class",
    "Business class",
    "Economy class",
    "Galley",
    "Lavatory",
    "Exit",
];

fn legend_item_width(ui: &Ui, label: &str) -> f32 {
    10.0 + ui.spacing().item_spacing.x + text_width(ui, &tr(label), FontId::proportional(11.0))
}

fn legend_row_width(ui: &Ui, labels: &[&str]) -> f32 {
    labels
        .iter()
        .map(|label| legend_item_width(ui, label))
        .sum::<f32>()
        + ui.spacing().item_spacing.x * labels.len().saturating_sub(1) as f32
}

/// Height of a legend row: the tallest translated label or the swatch.
fn legend_row_height(ui: &Ui) -> f32 {
    LEGEND_LABELS
        .iter()
        .map(|label| text_size(ui, &tr(label), FontId::proportional(11.0)).y)
        .fold(10.0_f32, f32::max)
}

/// Height of the legend card: title row, two entry rows, the vertical spacing
/// between them and the frame's inner margin.
fn legend_frame_height(ui: &Ui) -> f32 {
    let title_height = text_size(ui, &tr("Cabin legend"), FontId::proportional(12.0)).y;
    2.0 * LEGEND_INNER_MARGIN + title_height + 2.0 * legend_row_height(ui) + 2.0 * LEGEND_ROW_GAP
}

/// Draw a readable screen-space key that never follows the 3-D camera.
///
/// The legend is always present for the cabin view and sits along the bottom
/// edge of the viewport, away from the cabin geometry, which the camera can
/// otherwise place under a top-mounted card. Its six entries use two explicit
/// rows so the final entries remain inside the dock when translated labels or
/// a narrow dock need more room. The card is sized from its measured content
/// and anchored to the bottom of the legend region, so its lower edge keeps
/// the overlay margin from the viewport edge.
fn show_cabin_legend(ui: &mut Ui, viewport: egui::Rect) {
    let (_, region) = preview_overlay_rects(viewport, PreviewTab::Cabin);
    let frame_height = legend_frame_height(ui).min(region.height());
    let rect = egui::Rect::from_min_size(
        egui::pos2(region.left(), region.bottom() - frame_height),
        vec2(region.width(), frame_height),
    );
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
        Frame::group(ui.style())
            .fill(ui.visuals().panel_fill)
            .inner_margin(egui::Margin::same(LEGEND_INNER_MARGIN))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    // Keep the longest translated row inside the minimum
                    // 300-point dock while preserving a visible gap between
                    // each swatch/label pair.
                    ui.spacing_mut().item_spacing = vec2(4.0, LEGEND_ROW_GAP);
                    ui.label(RichText::new(tr("Cabin legend")).strong().size(12.0));
                    let row_height = legend_row_height(ui);
                    let first_row_width = legend_row_width(ui, &LEGEND_LABELS[..3]);
                    let first_row_id = Id::new(("cabin_legend_row", 0));
                    centered_row(ui, first_row_id, first_row_width, row_height, |ui| {
                        legend_item(ui, Color32::from_rgb(142, 68, 173), "First class");
                        legend_item(ui, Color32::from_rgb(41, 128, 185), "Business class");
                        legend_item(ui, Color32::from_rgb(39, 174, 96), "Economy class");
                    });
                    let second_row_width = legend_row_width(ui, &LEGEND_LABELS[3..]);
                    let second_row_id = Id::new(("cabin_legend_row", 1));
                    centered_row(ui, second_row_id, second_row_width, row_height, |ui| {
                        legend_item(ui, Color32::from_rgb(230, 126, 34), "Galley");
                        legend_item(ui, Color32::from_rgb(93, 173, 226), "Lavatory");
                        legend_item(ui, Color32::from_rgb(231, 76, 60), "Exit");
                    });
                    // Claim any sub-pixel remainder so the painted card ends
                    // exactly at the region's bottom edge.
                    let remainder = ui.available_size_before_wrap();
                    if remainder.y > 0.0 {
                        ui.allocate_space(vec2(0.0, remainder.y));
                    }
                });
            });
    });
}

/// Compute the screen-space rectangles shared by the controls and cabin key.
///
/// Both overlays are centred on the canvas rather than on the dock's content
/// cursor. The controls hang from the top edge and the legend region hugs
/// the bottom edge, each keeping the overlay margin. When the viewport is too
/// short for both, the legend region slides up to the five-point gap under
/// the controls and shrinks rather than overlapping them or leaving the
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
    let legend_bottom = viewport.bottom() - PREVIEW_OVERLAY_MARGIN;
    let earliest_top = controls.bottom() + PREVIEW_LEGEND_GAP;
    let available_height = (legend_bottom - earliest_top).max(1.0);
    let legend_height = PREVIEW_LEGEND_HEIGHT.min(available_height);
    let legend_top = (legend_bottom - legend_height).max(earliest_top);
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
        AIRCRAFT_CAMERA_ID, AIRCRAFT_VIEW_KEY, PREVIEW_LEGEND_GAP, PREVIEW_LEGEND_HEIGHT,
        PREVIEW_OVERLAY_MARGIN,
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
    fn cabin_overlay_rects_hang_from_the_top_and_bottom_edges_and_never_overlap() {
        let viewport = egui::Rect::from_min_size(pos2(40.0, 20.0), vec2(300.0, 420.0));
        let (controls, legend) = preview_overlay_rects(viewport, crate::state::PreviewTab::Cabin);

        assert!((controls.center().x - viewport.center().x).abs() < f32::EPSILON);
        assert!((legend.center().x - viewport.center().x).abs() < f32::EPSILON);
        assert!((controls.top() - viewport.top() - PREVIEW_OVERLAY_MARGIN).abs() < f32::EPSILON);
        assert!(
            (viewport.bottom() - legend.bottom() - PREVIEW_OVERLAY_MARGIN).abs() < f32::EPSILON
        );
        assert!((legend.height() - PREVIEW_LEGEND_HEIGHT).abs() < f32::EPSILON);
        assert!(legend.top() >= controls.bottom() + PREVIEW_LEGEND_GAP);
        assert!(controls.left() >= viewport.left());
        assert!(controls.right() <= viewport.right());
        assert!(legend.left() >= viewport.left());
        assert!(legend.right() <= viewport.right());

        // A viewport too short for both overlays keeps the five-point gap and
        // shrinks the legend instead of overlapping the controls.
        let short = egui::Rect::from_min_size(pos2(40.0, 20.0), vec2(300.0, 100.0));
        let (controls, legend) = preview_overlay_rects(short, crate::state::PreviewTab::Cabin);
        assert!((legend.top() - controls.bottom() - PREVIEW_LEGEND_GAP).abs() < f32::EPSILON);
        assert!(legend.bottom() <= short.bottom() - PREVIEW_OVERLAY_MARGIN + f32::EPSILON);
        assert!(legend.height() < PREVIEW_LEGEND_HEIGHT);
        assert!(legend.height() >= 1.0);
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

#[cfg(test)]
#[path = "preview_dock_cabin_tests.rs"]
mod cabin_layout_tests;
