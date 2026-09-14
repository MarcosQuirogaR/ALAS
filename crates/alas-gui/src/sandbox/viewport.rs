// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sandbox's central 3D viewport: orbit and zoom, the floating controls
//! over the design space (camera presets, geometry-category buttons and the
//! parameter search) and the parameter drag handles.
//!
//! Floating controls have no enclosing panel or toolbar background: each
//! draws only its own box, at [`REST_OPACITY`] while nobody points at it
//! and fully visible while hovered, pressed or keyboard-focused. Every
//! control registers its screen rectangle for one frame, and a pointer
//! gesture that starts on one of those rectangles never orbits the camera,
//! zooms, or picks a drag handle underneath it.

use alas_report::families::geometry::SceneFraming;
use alas_viz::SceneView;
use egui::{pos2, vec2, Color32, Context, Id, Pos2, Rect, Response, RichText, Stroke, Ui};

use crate::state::{AppState, PreviewCamera};
use crate::views::tr;

use super::drag::Handle;
use super::panel::show_parameter_access;
use super::scene::{SANDBOX_CAMERA_ID, SANDBOX_VIEW_KEY};

const HANDLE_RADIUS: f32 = 6.0;
const PICK_RADIUS: f32 = 11.0;
/// Opacity of a floating control nobody is pointing at.
pub const REST_OPACITY: f32 = 0.6;
/// Inset of the floating controls from the viewport edge, in points.
pub const OVERLAY_INSET: f32 = 8.0;
/// Height reserved for the camera row at the top of the viewport, in points.
pub const CAMERA_ROW_HEIGHT: f32 = 28.0;

/// The floating-control rectangles registered while rendering one frame.
#[derive(Clone, Default)]
struct OverlayRects(Vec<(&'static str, Rect)>);

fn overlay_rects_id() -> Id {
    Id::new("sandbox_viewport_overlay_rects")
}

fn camera_row_width_id() -> Id {
    Id::new("sandbox_viewport_camera_row_width")
}

/// The screen rectangles of the floating controls registered during the
/// last rendered frame.
pub fn overlay_rects(ctx: &Context) -> Vec<Rect> {
    ctx.data(|d| d.get_temp::<OverlayRects>(overlay_rects_id()))
        .map(|rects| rects.0.into_iter().map(|(_, rect)| rect).collect())
        .unwrap_or_default()
}

/// The rectangle registered under `tag` during the last rendered frame:
/// `camera`, `search`, `results` or `category:<discipline id>`.
pub fn overlay_rect_tagged(ctx: &Context, tag: &str) -> Option<Rect> {
    ctx.data(|d| d.get_temp::<OverlayRects>(overlay_rects_id()))
        .and_then(|rects| rects.0.into_iter().find(|(t, _)| *t == tag).map(|(_, r)| r))
}

fn reset_overlay_rects(ctx: &Context) {
    ctx.data_mut(|d| d.insert_temp(overlay_rects_id(), OverlayRects::default()));
}

/// Remember the rectangle of a floating control so the viewport ignores
/// pointer gestures that start on it.
pub(super) fn register_overlay_rect(ctx: &Context, tag: &'static str, rect: Rect) {
    ctx.data_mut(|d| {
        let mut rects = d
            .get_temp::<OverlayRects>(overlay_rects_id())
            .unwrap_or_default();
        rects.0.push((tag, rect));
        d.insert_temp(overlay_rects_id(), rects);
    });
}

/// Whether a pointer position lies on a floating control.
pub fn pointer_over_overlay(rects: &[Rect], pos: Option<Pos2>) -> bool {
    pos.is_some_and(|p| rects.iter().any(|r| r.contains(p)))
}

/// Whether a widget drawn at `id` last frame was hovered, pressed or
/// keyboard-focused, which is what lifts a floating control to full opacity.
pub(super) fn was_lit(ctx: &Context, id: Id) -> bool {
    ctx.read_response(id)
        .is_some_and(|r| r.hovered() || r.has_focus() || r.is_pointer_button_down_on())
}

/// Add a floating control: its own box only, [`REST_OPACITY`] at rest and
/// fully visible while hovered, pressed or keyboard-focused. The keyboard
/// focus cue is the same as anywhere else; only the resting opacity is
/// lowered.
pub(super) fn floating_button(
    ui: &mut Ui,
    tag: &'static str,
    button: egui::Button<'_>,
) -> Response {
    let id = ui.next_auto_id();
    let previous = ui.opacity();
    ui.set_opacity(if was_lit(ui.ctx(), id) {
        1.0
    } else {
        REST_OPACITY
    });
    let response = ui.add(button);
    ui.set_opacity(previous);
    debug_assert_eq!(response.id, id, "floating button id is predictable");
    register_overlay_rect(ui.ctx(), tag, response.rect);
    response
}

/// The fit-to-view mapping from scene canvas units to screen points, the
/// same centring rule the scene widget applies.
struct CanvasFit {
    scale: f32,
    offset: egui::Vec2,
}

impl CanvasFit {
    fn new(framing: &SceneFraming, rect: Rect) -> Self {
        let (w, h) = framing.canvas;
        let scale = (rect.width() / w as f32)
            .min(rect.height() / h as f32)
            .max(1e-4);
        let offset = vec2(
            rect.left() + (rect.width() - w as f32 * scale) * 0.5,
            rect.top() + (rect.height() - h as f32 * scale) * 0.5,
        );
        Self { scale, offset }
    }

    fn to_screen(&self, framing: &SceneFraming, point: [f64; 3]) -> egui::Pos2 {
        let [x, y] = framing.project(point);
        pos2(
            self.offset.x + x as f32 * self.scale,
            self.offset.y + y as f32 * self.scale,
        )
    }
}

/// Render the viewport and its overlays.
pub fn show_viewport(state: &mut AppState, ui: &mut Ui) {
    let Some((scene, framing)) = state.sandbox.scene.clone() else {
        ui.centered_and_justified(|ui| ui.label(tr("The aircraft could not be built.")));
        return;
    };
    // The control rectangles of the last frame decide who owns this
    // gesture; the controls of this frame register themselves again below.
    let overlays = overlay_rects(ui.ctx());
    reset_overlay_rects(ui.ctx());
    let size = vec2(
        ui.available_width().max(240.0),
        ui.available_height().max(180.0),
    );
    let revision = state.sandbox.scene_revision;
    let response = ui.add(
        SceneView::new(&scene, state.view_state_mut(SANDBOX_VIEW_KEY))
            .desired_size(size)
            .orbit_only()
            .vector_overlay(true)
            .show_toolbar(false)
            .cache_key(SANDBOX_VIEW_KEY)
            .cache_revision(revision),
    );
    let rect = response.rect;
    let fit = CanvasFit::new(&framing, rect);
    let handles = state.sandbox_handles();
    let press_origin = ui.input(|i| i.pointer.press_origin());
    let gesture_on_control = pointer_over_overlay(&overlays, press_origin);
    let pointer = response
        .hover_pos()
        .filter(|p| !pointer_over_overlay(&overlays, Some(*p)));
    let hovered_handle = pointer.and_then(|p| {
        handles
            .iter()
            .map(|h| (h, fit.to_screen(&framing, h.point).distance(p)))
            .filter(|(_, d)| *d <= PICK_RADIUS)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(h, _)| h.clone())
    });

    // A handle under the pointer takes the drag; the camera is the
    // fallback interaction. A gesture that began on a floating control
    // belongs to that control.
    let mut camera_changed = false;
    if state.sandbox.drag.is_some() {
        if response.dragged() {
            state.update_handle_drag(response.drag_motion());
        }
        if response.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
            state.end_handle_drag();
        }
    } else if gesture_on_control {
        // Owned by the control under the press origin.
    } else if response.drag_started() && hovered_handle.is_some() {
        if let Some(handle) = &hovered_handle {
            state.begin_handle_drag(handle, &framing, fit.scale);
        }
    } else if response.dragged_by(egui::PointerButton::Primary)
        || response.dragged_by(egui::PointerButton::Middle)
    {
        let delta = response.drag_motion();
        state
            .preview_camera_mut(SANDBOX_CAMERA_ID)
            .apply_orbit_motion(delta);
        camera_changed = delta.is_finite();
    }
    if response.hovered() && pointer.is_some() {
        let scroll_y = ui.input(|input| input.smooth_scroll_delta.y);
        if scroll_y.abs() > f32::EPSILON {
            let factor = f64::from((1.0 + scroll_y * 0.0015).clamp(0.5, 1.5));
            state
                .preview_camera_mut(SANDBOX_CAMERA_ID)
                .apply_zoom_factor(factor);
            camera_changed = true;
        }
    }

    let painter = ui.painter_at(rect);
    let accent = ui.visuals().hyperlink_color;
    for handle in &handles {
        let center = fit.to_screen(&framing, handle.point);
        let active = state
            .sandbox
            .drag
            .as_ref()
            .is_some_and(|d| d.handle.kind == handle.kind);
        let hovered = hovered_handle
            .as_ref()
            .is_some_and(|h| h.kind == handle.kind);
        let (fill, radius) = if active || hovered {
            (accent, HANDLE_RADIUS + 2.0)
        } else {
            (
                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 150),
                HANDLE_RADIUS,
            )
        };
        painter.circle(
            center,
            radius,
            fill,
            Stroke::new(1.0_f32, ui.visuals().strong_text_color()),
        );
        let tip = fit.to_screen(
            &framing,
            [
                handle.point[0] + handle.axis[0] * 1.5,
                handle.point[1] + handle.axis[1] * 1.5,
                handle.point[2] + handle.axis[2] * 1.5,
            ],
        );
        painter.line_segment([center, tip], Stroke::new(1.5_f32, fill));
    }
    match &hovered_handle {
        Some(handle) => {
            response.clone().on_hover_text(hover_text(handle));
        }
        None if pointer.is_some() => {
            response.clone().on_hover_text(tr(
                "Drag to orbit the camera; scroll to zoom; drag a handle to change its parameter",
            ));
        }
        None => {}
    }
    if let Some(drag) = &state.sandbox.drag {
        painter.text(
            rect.left_top() + vec2(12.0, 12.0),
            egui::Align2::LEFT_TOP,
            tr(drag.handle.label),
            egui::FontId::proportional(13.0),
            ui.visuals().strong_text_color(),
        );
    }

    show_viewport_controls(state, ui, rect, &mut camera_changed);
    show_parameter_access(state, ui, rect);
    if let Some(message) = &state.sandbox.rejected_edit {
        painter.text(
            rect.left_bottom() + vec2(12.0, -12.0),
            egui::Align2::LEFT_BOTTOM,
            message,
            egui::FontId::proportional(12.0),
            ui.visuals().warn_fg_color,
        );
    }
    if camera_changed {
        state.reproject_sandbox_scene();
        ui.ctx().request_repaint();
    }
}

fn hover_text(handle: &Handle) -> String {
    format!("{} ({})", tr(handle.label), tr("drag to edit"))
}

/// The camera row centred at the top of the viewport: Overview, the name of
/// the isolated component when one is focused, the presets and Fit. The row
/// is centred on the width it measured last frame, since the buttons have no
/// enclosing frame to size against.
fn show_viewport_controls(
    state: &mut AppState,
    ui: &mut Ui,
    viewport: Rect,
    camera_changed: &mut bool,
) {
    let ctx = ui.ctx().clone();
    let measured = ctx
        .data(|d| d.get_temp::<f32>(camera_row_width_id()))
        .unwrap_or(330.0);
    let width = measured.min((viewport.width() - 2.0 * OVERLAY_INSET).max(1.0));
    let controls = Rect::from_min_size(
        pos2(
            viewport.center().x - width * 0.5,
            viewport.top() + OVERLAY_INSET,
        ),
        vec2(width, CAMERA_ROW_HEIGHT),
    );
    let row = ui.allocate_new_ui(egui::UiBuilder::new().max_rect(controls), |ui| {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let overview = state.sandbox.focus().is_none();
            if floating_button(ui, "camera", egui::Button::new(tr("Overview")).small())
                .on_hover_text(tr(
                    "Show the whole aircraft again without changing any data.",
                ))
                .clicked()
                && !overview
            {
                state.sandbox.set_focus(None);
                *camera_changed = true;
            }
            if let Some(focus) = state.sandbox.focus() {
                ui.label(RichText::new(tr(focus.title())).weak().small());
            }
            for (label, camera) in [
                ("Iso", PreviewCamera::isometric()),
                ("Top", PreviewCamera::top()),
                ("Front", PreviewCamera::front()),
                ("Side", PreviewCamera::side()),
            ] {
                if floating_button(ui, "camera", egui::Button::new(tr(label)).small()).clicked() {
                    *state.preview_camera_mut(SANDBOX_CAMERA_ID) = camera;
                    state.view_state_mut(SANDBOX_VIEW_KEY).reset();
                    *camera_changed = true;
                }
            }
            if floating_button(ui, "camera", egui::Button::new(tr("Fit")).small())
                .on_hover_text(tr(
                    "Refit the current view without changing its orientation.",
                ))
                .clicked()
            {
                state.preview_camera_mut(SANDBOX_CAMERA_ID).fit();
                state.view_state_mut(SANDBOX_VIEW_KEY).reset();
                *camera_changed = true;
            }
        });
    });
    let content_width = row.response.rect.width().max(1.0);
    if (content_width - measured).abs() > 0.5 {
        ctx.data_mut(|d| d.insert_temp(camera_row_width_id(), content_width));
        ctx.request_repaint();
    }
}
