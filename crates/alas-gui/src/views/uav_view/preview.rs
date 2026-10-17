// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Developer-only preview of the UAV studio scene (decision D3).
//!
//! Compiled only with `debug_assertions`, so a release build contains no trace
//! of it, and drawn only below the unchanged release block of the UAV page. It
//! shows the reference aircraft built by `alas_uav::studio::reference` as two
//! orthographic views of the component-box scene (body axes, `x` aft, `y`
//! starboard, `z` up) with the CG and neutral-point markers and the L0
//! feasibility lights. No input is editable. The page itself is unlocked
//! (the preliminary banner), so this preview only adds a debug aid.

use std::sync::Arc;

use alas_uav::studio::feasibility::{l0, Light, Report};
use alas_uav::studio::markers::{mass_inputs_for, MarkerKind};
use alas_uav::studio::reference::xtra2upv;
use alas_uav::studio::scene::{build_scene, Scene};
use egui::{Align2, Color32, FontId, Pos2, Sense, Shape, Stroke, Ui, Vec2};

use super::super::{tr, tr_fields};
use super::presentation::card;

/// What the preview draws, computed once.
pub(super) struct PreviewData {
    pub(super) scene: Scene,
    pub(super) report: Report,
    pub(super) substitutions: Vec<String>,
    pub(super) name: String,
}

/// Build the preview data (the reference aircraft, its scene and L0 lights).
pub(super) fn preview_data() -> Result<PreviewData, String> {
    let r = xtra2upv().map_err(|e| e.to_string())?;
    let scene = build_scene(&r.spec, &r.db, &mass_inputs_for(&r.spec), &r.cases)
        .map_err(|e| e.to_string())?;
    let report = l0(&r.inputs()).map_err(|e| e.to_string())?;
    Ok(PreviewData {
        scene,
        report,
        substitutions: r
            .substitutions
            .iter()
            .map(|s| format!("{} -> {} ({})", s.published, s.used, s.note))
            .collect(),
        name: r.spec.name.clone(),
    })
}

#[derive(Clone, Copy)]
enum View {
    /// Looking from the port side: `x` right, `z` up.
    Side,
    /// Looking down: `x` right, `y` (starboard) up on the screen.
    Plan,
}

impl View {
    fn project(self, p: [f64; 3]) -> (f64, f64) {
        match self {
            Self::Side => (p[0], p[2]),
            Self::Plan => (p[0], p[1]),
        }
    }
}

fn category_color(category: &str) -> Color32 {
    match category {
        "motor" => Color32::from_rgb(200, 70, 60),
        "battery" => Color32::from_rgb(70, 160, 80),
        "esc" => Color32::from_rgb(230, 150, 40),
        "servo" => Color32::from_rgb(70, 110, 200),
        "receiver" | "electronics" | "flight_controller" => Color32::from_rgb(150, 90, 190),
        _ => Color32::GRAY,
    }
}

fn light_color(light: Light) -> Color32 {
    match light {
        Light::Green => Color32::from_rgb(60, 170, 80),
        Light::Amber => Color32::from_rgb(230, 170, 30),
        Light::Red => Color32::from_rgb(210, 60, 50),
        Light::Unknown => Color32::GRAY,
    }
}

fn bounds(scene: &Scene, view: View) -> ((f64, f64), (f64, f64)) {
    let mut lo = (f64::INFINITY, f64::INFINITY);
    let mut hi = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut put = |p: [f64; 3]| {
        let (a, b) = view.project(p);
        lo = (lo.0.min(a), lo.1.min(b));
        hi = (hi.0.max(a), hi.1.max(b));
    };
    for o in &scene.outlines {
        o.points_m.iter().for_each(|p| put(*p));
    }
    if let Some((a, b)) = scene.envelope {
        put(a);
        put(b);
    }
    if let Some((a, b)) = scene.boom {
        put(a);
        put(b);
    }
    for p in &scene.parts {
        put(p.aabb.min);
        put(p.aabb.max);
    }
    (lo, hi)
}

fn draw_view(ui: &mut Ui, scene: &Scene, view: View, height: f32) {
    let (lo, hi) = bounds(scene, view);
    let (w, h) = ((hi.0 - lo.0).max(1e-3), (hi.1 - lo.1).max(1e-3));
    let desired = Vec2::new(ui.available_width().max(320.0), height);
    let (response, painter) = ui.allocate_painter(desired, Sense::hover());
    let rect = response.rect.shrink(14.0);
    let scale = (f64::from(rect.width()) / w).min(f64::from(rect.height()) / h) as f32;
    let origin = rect.center();
    let mid = (0.5 * (lo.0 + hi.0), 0.5 * (lo.1 + hi.1));
    let pos = |p: [f64; 3]| -> Pos2 {
        let (a, b) = view.project(p);
        Pos2::new(
            origin.x + (a - mid.0) as f32 * scale,
            origin.y - (b - mid.1) as f32 * scale,
        )
    };
    let weak = ui.visuals().weak_text_color();
    let line = ui.visuals().text_color();
    if let Some((a, b)) = scene.envelope {
        painter.rect_stroke(
            egui::Rect::from_two_pos(pos(a), pos(b)),
            2.0,
            Stroke::new(1.0_f32, weak),
        );
    }
    for (_, a, b) in &scene.bays {
        painter.rect_stroke(
            egui::Rect::from_two_pos(pos(*a), pos(*b)),
            0.0,
            Stroke::new(0.5_f32, weak),
        );
    }
    if let Some((a, b)) = scene.boom {
        painter.line_segment([pos(a), pos(b)], Stroke::new(3.0_f32, weak));
    }
    for o in &scene.outlines {
        let pts: Vec<Pos2> = o.points_m.iter().map(|p| pos(*p)).collect();
        painter.add(Shape::closed_line(pts, Stroke::new(1.5_f32, line)));
    }
    for p in &scene.parts {
        let c = category_color(p.category);
        let r = egui::Rect::from_two_pos(pos(p.aabb.min), pos(p.aabb.max));
        painter.rect_filled(
            r,
            1.0,
            Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 110),
        );
        painter.rect_stroke(r, 1.0, Stroke::new(1.0_f32, c));
    }
    for d in &scene.discs {
        let (a, b) = match view {
            View::Side => (
                [d.centre_m[0], d.centre_m[1], d.centre_m[2] - d.radius_m],
                [d.centre_m[0], d.centre_m[1], d.centre_m[2] + d.radius_m],
            ),
            View::Plan => (
                [d.centre_m[0], d.centre_m[1] - d.radius_m, d.centre_m[2]],
                [d.centre_m[0], d.centre_m[1] + d.radius_m, d.centre_m[2]],
            ),
        };
        painter.line_segment(
            [pos(a), pos(b)],
            Stroke::new(1.5_f32, Color32::from_rgb(200, 70, 60)),
        );
    }
    for l in &scene.lines {
        painter.line_segment([pos(l.a_m), pos(l.b_m)], Stroke::new(0.8_f32, weak));
    }
    for m in &scene.markers {
        let at = pos(m.position_m);
        let (color, radius) = match m.kind {
            MarkerKind::Cg => (Color32::from_rgb(210, 60, 50), 5.0),
            MarkerKind::NeutralPoint => (Color32::from_rgb(40, 120, 220), 5.0),
            MarkerKind::WingAc | MarkerKind::TailAc => (weak, 3.0),
        };
        painter.circle_filled(at, radius, color);
        painter.text(
            at + Vec2::new(6.0, -6.0),
            Align2::LEFT_BOTTOM,
            &m.label,
            FontId::proportional(10.0),
            color,
        );
    }
}

/// Draw the developer preview at the foot of the UAV page.
pub(super) fn dev_preview(ui: &mut Ui) {
    let id = egui::Id::new("uav_dev_preview_data");
    let data: Arc<Result<PreviewData, String>> = ui.data_mut(|d| {
        d.get_temp_mut_or_insert_with(id, || Arc::new(preview_data()))
            .clone()
    });
    ui.add_space(8.0);
    card(ui, "UAV preview (developer build only)", None, |ui| {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            tr("Preview: models not yet validated for this aircraft class"),
        );
        match &*data {
            Err(e) => {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            Ok(d) => {
                ui.label(tr_fields(
                    "Reference aircraft: {name} (published, built from a spec)",
                    &[("name", d.name.clone())],
                ));
                ui.label(tr("Side view (x aft, z up)"));
                draw_view(ui, &d.scene, View::Side, 160.0);
                ui.label(tr("Plan view (x aft, y starboard up)"));
                draw_view(ui, &d.scene, View::Plan, 260.0);
                ui.label(tr_fields(
                    "Static margin, first load case: {value}",
                    &[("value", format!("{:.3}", d.scene.static_margin))],
                ));
                ui.label(tr("Feasibility (closed form)"));
                for item in &d.report.items {
                    ui.horizontal(|ui| {
                        let (r, p) = ui.allocate_painter(Vec2::splat(10.0), Sense::hover());
                        p.circle_filled(r.rect.center(), 4.0, light_color(item.light));
                        ui.label(&item.label).on_hover_text(&item.note);
                    });
                }
                ui.label(tr("Substituted parts"));
                for s in &d.substitutions {
                    ui.weak(s);
                }
            }
        }
    });
}

#[cfg(test)]
// A test unwraps values it built itself, so a failed unwrap is the failing assertion.
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn preview_data_holds_boxes_markers_and_lights() {
        let d = preview_data().unwrap();
        assert!(!d.scene.parts.is_empty() && !d.scene.markers.is_empty());
        assert!(!d.report.items.is_empty());
        assert!(d.scene.static_margin.is_finite());
    }

    #[test]
    fn preview_renders_headless() {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, dev_preview);
        });
    }
}
