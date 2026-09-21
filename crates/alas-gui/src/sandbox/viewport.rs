// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sandbox's central 3D viewport: orbit and zoom, the floating controls
//! over the design space (camera presets, geometry-category buttons, the
//! parameter search, the action row and the Summary card) and the parameter
//! drag handles.
//!
//! Floating controls have no enclosing panel or toolbar background: each
//! draws only its own box, at [`REST_OPACITY`] while nobody points at it
//! and fully visible while hovered, pressed or keyboard-focused. Every
//! control registers its screen rectangle for one frame, and a pointer
//! gesture that starts on one of those rectangles never orbits the camera,
//! zooms, or picks a drag handle underneath it.
//!
//! Layout of one frame, from the top: the camera row centred at the top;
//! the category stack on the left, centred about the viewport's horizontal
//! centreline within the space the other overlays leave free; the action
//! row centred at the bottom ([`super::overlays`]). The derived geometry
//! metrics are behind the Summary button of the stack ([`super::panel`]).
//!
//! Framing: the scene canvas is the viewport itself, so a resize redraws
//! the kept framing on the new canvas (pixels per metre follow the smaller
//! viewport side); orbit, presets, zoom and edits never refit. Fit, a focus
//! change and a replaced design do (see
//! [`AppState::refit_sandbox_framing`]).

use alas_report::families::geometry::SceneFraming;
use alas_viz::SceneView;
use egui::{pos2, vec2, Color32, Context, Id, Pos2, Rect, Response, RichText, Stroke, Ui};

use crate::state::{AppState, PreviewCamera};
use crate::views::tr;

use super::drag::Handle;
use super::overlays::show_action_block;
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
/// Height of one line of the rejected-edit message, in points.
const MESSAGE_LINE_HEIGHT: f32 = 16.0;

/// The floating-control rectangles registered while rendering one frame.
#[derive(Clone, Default)]
struct OverlayRects(Vec<(&'static str, Rect)>);

fn overlay_rects_id() -> Id {
    Id::new("sandbox_viewport_overlay_rects")
}

/// The screen rectangles of the floating controls registered during the
/// last rendered frame.
pub fn overlay_rects(ctx: &Context) -> Vec<Rect> {
    ctx.data(|d| d.get_temp::<OverlayRects>(overlay_rects_id()))
        .map(|rects| rects.0.into_iter().map(|(_, rect)| rect).collect())
        .unwrap_or_default()
}

/// The rectangle registered under `tag` during the last rendered frame:
/// `camera`, `search`, `results`, `category:<discipline id>`, `action`,
/// `metric` or `stack` (the whole category column).
pub fn overlay_rect_tagged(ctx: &Context, tag: &str) -> Option<Rect> {
    ctx.data(|d| d.get_temp::<OverlayRects>(overlay_rects_id()))
        .and_then(|rects| rects.0.into_iter().find(|(t, _)| *t == tag).map(|(_, r)| r))
}

/// Every rectangle registered under `tag` during the last rendered frame.
pub fn overlay_rects_tagged(ctx: &Context, tag: &str) -> Vec<Rect> {
    ctx.data(|d| d.get_temp::<OverlayRects>(overlay_rects_id()))
        .map(|rects| {
            rects
                .0
                .into_iter()
                .filter(|(t, _)| *t == tag)
                .map(|(_, r)| r)
                .collect()
        })
        .unwrap_or_default()
}

fn viewport_rect_id() -> Id {
    Id::new("sandbox_viewport_rect")
}

/// The screen rectangle the scene was drawn in during the last rendered
/// frame (not an overlay: gestures on it orbit the camera).
pub fn viewport_rect(ctx: &Context) -> Option<Rect> {
    ctx.data(|d| d.get_temp::<Rect>(viewport_rect_id()))
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
/// lowered. A disabled control stays at rest and draws egui's disabled look.
///
/// `selected` is a persistent on/off state, not a hover cue: it keeps its
/// accent fill and full opacity once the pointer moves away, which is what
/// lets the category stack, the camera row's context label and the isolated
/// component agree, all three rendering from one value.
pub(super) fn floating_control(
    ui: &mut Ui,
    tag: &'static str,
    enabled: bool,
    selected: bool,
    button: egui::Button<'_>,
) -> Response {
    let id = ui.next_auto_id();
    let previous = ui.opacity();
    ui.set_opacity(if enabled && (selected || was_lit(ui.ctx(), id)) {
        1.0
    } else {
        REST_OPACITY
    });
    let response = ui.add_enabled(enabled, button.selected(selected));
    ui.set_opacity(previous);
    debug_assert_eq!(response.id, id, "floating control id is predictable");
    register_overlay_rect(ui.ctx(), tag, response.rect);
    response
}

/// An enabled, momentary [`floating_control`].
pub(super) fn floating_button(
    ui: &mut Ui,
    tag: &'static str,
    button: egui::Button<'_>,
) -> Response {
    floating_control(ui, tag, true, false, button)
}

/// The size a centred row measured last frame, or `default_height` tall
/// and as wide as the viewport allows before it has been drawn.
pub(super) fn measured_row_size(
    ctx: &Context,
    key: &str,
    viewport: Rect,
    default_height: f32,
) -> egui::Vec2 {
    let available = (viewport.width() - 2.0 * OVERLAY_INSET).max(1.0);
    let measured = ctx
        .data(|d| d.get_temp::<egui::Vec2>(Id::new(("sandbox_centered_row_size", key))))
        .unwrap_or(vec2(available.min(320.0), default_height));
    vec2(measured.x.min(available), measured.y.max(default_height))
}

/// Lay out one row of floating controls centred horizontally in `viewport`
/// at `top`, at least `min_height` points tall. The buttons have no
/// enclosing frame to size against, so the row is centred on the width it
/// measured last frame (a repaint is requested when that changes); when
/// the contents are wider than the viewport they wrap onto further lines
/// and the measured height grows, so every control stays reachable on a
/// narrow preview. Returns the rectangle the row's contents occupied.
pub(super) fn centered_row(
    ui: &mut Ui,
    key: &'static str,
    viewport: Rect,
    top: f32,
    min_height: f32,
    add_contents: impl FnOnce(&mut Ui),
) -> Rect {
    let ctx = ui.ctx().clone();
    let id = Id::new(("sandbox_centered_row_size", key));
    let available = (viewport.width() - 2.0 * OVERLAY_INSET).max(1.0);
    let raw = ctx.data(|d| d.get_temp::<egui::Vec2>(id));
    let wraps = raw.is_some_and(|r| r.x > available + 0.5);
    let size = measured_row_size(&ctx, key, viewport, min_height);
    let row = Rect::from_min_size(
        pos2(viewport.center().x - size.x * 0.5, top),
        vec2(
            size.x,
            if wraps {
                (viewport.bottom() - top).max(size.y)
            } else {
                size.y
            },
        ),
    );
    let inner = ui.allocate_new_ui(egui::UiBuilder::new().max_rect(row), |ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
        if wraps {
            ui.horizontal_wrapped(add_contents);
        } else {
            // One line: every control centred on the row's own height.
            ui.horizontal_centered(add_contents);
        }
    });
    let content = inner.response.rect;
    let measured = vec2(content.width().max(1.0), content.height().max(1.0));
    let previous = ctx.data(|d| d.get_temp::<egui::Vec2>(id));
    if previous.is_none_or(|p| (p - measured).abs().max_elem() > 0.5) {
        ctx.data_mut(|d| d.insert_temp(id, measured));
        ctx.request_repaint();
    }
    content
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
    // The control rectangles of the last frame decide who owns this
    // gesture; the controls of this frame register themselves again below.
    let overlays = overlay_rects(ui.ctx());
    reset_overlay_rects(ui.ctx());
    let size = vec2(
        ui.available_width().max(240.0),
        ui.available_height().max(180.0),
    );
    // The canvas is the viewport: a resize redraws the kept framing.
    state.set_sandbox_viewport_size((size.x, size.y));
    let Some((scene, framing)) = state.sandbox.scene.clone() else {
        ui.centered_and_justified(|ui| ui.label(tr("The aircraft could not be built.")));
        return;
    };
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
    ui.ctx()
        .data_mut(|d| d.insert_temp(viewport_rect_id(), rect));
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
            .is_some_and(|d| same_handle_identity(&d.handle, handle));
        let hovered = hovered_handle
            .as_ref()
            .is_some_and(|h| same_handle_identity(h, handle));
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
    // Only a handle explains itself; the empty design space has no tooltip.
    if let Some(handle) = &hovered_handle {
        response.clone().on_hover_text(hover_text(handle));
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

    let block = show_action_block(state, ui, rect);
    let camera_row = show_viewport_controls(state, ui, rect, &mut camera_changed);
    // A rejected-edit message sits above the action block, left-aligned;
    // the category stack keeps clear of it. Run status is not painted here:
    // the run log window and the estimates strip carry it.
    let mut message_bottom = block.top() - 6.0;
    let messages: Vec<(String, Color32)> = state
        .sandbox
        .rejected_edit
        .clone()
        .map(|m| (m, ui.visuals().warn_fg_color))
        .into_iter()
        .collect();
    for (message, color) in &messages {
        painter.text(
            pos2(rect.left() + 12.0, message_bottom),
            egui::Align2::LEFT_BOTTOM,
            message,
            egui::FontId::proportional(12.0),
            *color,
        );
        message_bottom -= MESSAGE_LINE_HEIGHT;
    }
    show_parameter_access(
        state,
        ui,
        rect,
        camera_row.bottom() + OVERLAY_INSET,
        message_bottom - OVERLAY_INSET,
    );
    if camera_changed {
        state.reproject_sandbox_scene();
        ui.ctx().request_repaint();
    }
}

/// Compare handles by the model target they edit, rather than by their
/// current screen geometry.  A dragged handle's point and axis are rebuilt
/// after every model refresh, so those values cannot identify the same
/// gesture across frames.  Dynamic section handles share a `HandleKind`; the
/// section index and component are what distinguish their individual cells.
fn same_handle_identity(left: &Handle, right: &Handle) -> bool {
    left.kind == right.kind
        && left.discipline == right.discipline
        && left.field_id == right.field_id
        && left.component == right.component
        && left.section_index == right.section_index
}

fn hover_text(handle: &Handle) -> String {
    format!("{} ({})", tr(handle.label), tr("drag to edit"))
}

/// The camera row centred at the top of the viewport: Overview, the name of
/// the isolated component when one is focused, the presets and Fit.
/// Presets and orbit only turn the camera; Fit refits the framing to the
/// shown components and resets the zoom. Returns the row rectangle.
fn show_viewport_controls(
    state: &mut AppState,
    ui: &mut Ui,
    viewport: Rect,
    camera_changed: &mut bool,
) -> Rect {
    let top = viewport.top() + OVERLAY_INSET;
    let row = centered_row(ui, "camera", viewport, top, CAMERA_ROW_HEIGHT, |ui| {
        // Overview is the no-component state of the same selection the
        // category stack renders, so it shows as selected exactly when no
        // component is focused.
        let overview = state.sandbox.focus().is_none();
        if floating_control(
            ui,
            "camera",
            true,
            overview,
            egui::Button::new(tr("Overview")).small(),
        )
        .on_hover_text(tr(
            "Show the whole aircraft again without changing any data.",
        ))
        .clicked()
            && !overview
        {
            state.set_sandbox_focus(None);
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
                let zoom = state.preview_camera_mut(SANDBOX_CAMERA_ID).zoom;
                *state.preview_camera_mut(SANDBOX_CAMERA_ID) = PreviewCamera { zoom, ..camera };
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
            state.refit_sandbox_framing();
        }
    });
    Rect::from_min_max(
        pos2(viewport.left(), top),
        pos2(viewport.right(), row.bottom().max(top + CAMERA_ROW_HEIGHT)),
    )
}

#[cfg(test)]
mod tests {
    use super::super::drag::{Handle, HandleKind};
    use super::super::fields::Discipline;
    use super::same_handle_identity;

    fn custom_wing_handle(section_index: usize, component: usize) -> Handle {
        Handle {
            kind: HandleKind::CustomWingSectionChord,
            discipline: Discipline::Wing,
            field_id: "geometry.custom_sections",
            label: "Custom wing section chord",
            point: [section_index as f64, component as f64, 0.0],
            axis: [1.0, 0.0, 0.0],
            per_metre: 1.0,
            component: Some(component),
            section_index: Some(section_index),
        }
    }

    #[test]
    fn handle_identity_survives_reprojection_and_distinguishes_sections() {
        let original = custom_wing_handle(0, 2);
        let reprojected = Handle {
            point: [11.0, 12.0, 13.0],
            axis: [0.0, 1.0, 0.0],
            per_metre: 0.25,
            ..original.clone()
        };
        let other_section = custom_wing_handle(1, 2);
        let other_component = custom_wing_handle(0, 3);

        assert!(same_handle_identity(&original, &reprojected));
        assert!(!same_handle_identity(&original, &other_section));
        assert!(!same_handle_identity(&original, &other_component));
    }
}
