// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sandbox's central 3D viewport: orbit and zoom, component overview,
//! camera presets, and the parameter drag handles.

use alas_report::families::geometry::SceneFraming;
use alas_viz::SceneView;
use egui::{pos2, vec2, Color32, Rect, RichText, Stroke, Ui};

use crate::state::{AppState, PreviewCamera};
use crate::views::tr;

use super::drag::Handle;
use super::scene::{SANDBOX_CAMERA_ID, SANDBOX_VIEW_KEY};

const HANDLE_RADIUS: f32 = 6.0;
const PICK_RADIUS: f32 = 11.0;

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
    let pointer = response.hover_pos();
    let hovered_handle = pointer.and_then(|p| {
        handles
            .iter()
            .map(|h| (h, fit.to_screen(&framing, h.point).distance(p)))
            .filter(|(_, d)| *d <= PICK_RADIUS)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(h, _)| h.clone())
    });

    // A handle under the pointer takes the drag; the camera is the
    // fallback interaction.
    let mut camera_changed = false;
    if state.sandbox.drag.is_some() {
        if response.dragged() {
            state.update_handle_drag(response.drag_motion());
        }
        if response.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
            state.end_handle_drag();
        }
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
    if response.hovered() {
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
        None => {
            response.clone().on_hover_text(tr(
                "Drag to orbit the camera; scroll to zoom; drag a handle to change its parameter",
            ));
        }
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

fn show_viewport_controls(
    state: &mut AppState,
    ui: &mut Ui,
    viewport: Rect,
    camera_changed: &mut bool,
) {
    let width = 440.0f32.min((viewport.width() - 16.0).max(1.0));
    let controls = Rect::from_min_size(
        pos2(viewport.center().x - width * 0.5, viewport.top() + 8.0),
        vec2(width, 34.0),
    );
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(controls), |ui| {
        egui::Frame::group(ui.style())
            .fill(ui.visuals().panel_fill)
            .inner_margin(egui::Margin::symmetric(5.0, 3.0))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    let overview = state.sandbox.focus().is_none();
                    if ui
                        .add(crate::theme::selectable_button(tr("Overview"), overview))
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
                    ui.separator();
                    for (label, camera) in [
                        ("Iso", PreviewCamera::isometric()),
                        ("Top", PreviewCamera::top()),
                        ("Front", PreviewCamera::front()),
                        ("Side", PreviewCamera::side()),
                    ] {
                        if ui.add(egui::Button::new(tr(label)).small()).clicked() {
                            *state.preview_camera_mut(SANDBOX_CAMERA_ID) = camera;
                            state.view_state_mut(SANDBOX_VIEW_KEY).reset();
                            *camera_changed = true;
                        }
                    }
                    if ui
                        .add(egui::Button::new(tr("Fit")).small())
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
    });
}
