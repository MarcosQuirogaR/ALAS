// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The fuselage section editor: a side profile with draggable station
//! points and a cross-section with draggable width and height.
//!
//! Every point maps to one configuration field, so a drag is one field edit
//! with the same validity domain and undo transaction as a typed value. The
//! profile follows the builder's station laws: the nose radius grows as
//! `sqrt(1 - (1 - xi)^2)` and the tailcone radius shrinks as `1 - xi^1.5`.

use egui::{pos2, vec2, Color32, Rect, Sense, Stroke, Ui};
use serde_json::Value;

use crate::state::AppState;
use crate::views::tr;

use super::editors::{commit_field, Gesture};
use super::fields::{self, SandboxField};

struct Profile {
    length: f64,
    diameter: f64,
    height: f64,
    nose_z: f64,
    cabin_start_x: f64,
    cabin_z: f64,
    tailcone_length: f64,
    tail_z: f64,
}

fn profile(state: &AppState) -> Option<Profile> {
    let config = state.typed_config()?;
    let f = &config.geometry.fuselage;
    Some(Profile {
        length: state.design_values.get("fuselage_length_m").copied()?,
        diameter: f.diameter_m,
        height: f.effective_height_m(),
        nose_z: f.nose_z_m,
        cabin_start_x: f.cabin_start_x_m,
        cabin_z: f.cabin_z_m,
        tailcone_length: f.tailcone_length_m,
        tail_z: f.tail_z_m,
    })
}

fn field<'a>(state: &'a AppState, id: &str) -> Option<&'a SandboxField> {
    state.sandbox.fields.iter().find(|f| f.id == id)
}

fn drag_point(
    state: &mut AppState,
    ui: &mut Ui,
    id: &str,
    center: egui::Pos2,
    value_of: impl Fn(egui::Pos2) -> f64,
    label: &str,
) {
    let Some(field) = field(state, id).cloned() else {
        return;
    };
    let rect = Rect::from_center_size(center, vec2(14.0, 14.0));
    let response = ui.interact(rect, ui.id().with(("fuselage_point", id)), Sense::drag());
    let color = if response.hovered() || response.dragged() {
        ui.visuals().hyperlink_color
    } else {
        ui.visuals().strong_text_color()
    };
    ui.painter().circle(
        center,
        5.0,
        color,
        Stroke::new(1.0_f32, ui.visuals().panel_fill),
    );
    response.clone().on_hover_text(tr(label));
    let gesture = if response.drag_started() {
        Some(Gesture::DragStarted)
    } else if response.drag_stopped() {
        Some(Gesture::DragStopped)
    } else if response.dragged() {
        Some(Gesture::Dragging)
    } else {
        None
    };
    if let (Some(gesture), Some(pointer)) = (gesture, response.interact_pointer_pos()) {
        let raw = value_of(pointer);
        let scale = 10f64.powi(field.decimals as i32);
        let rounded = ((raw.clamp(field.min, field.max)) * scale).round() / scale;
        let value = match field.kind {
            fields::FieldKind::OptionalFloat => Value::from(rounded),
            _ => Value::from(rounded),
        };
        commit_field(state, &field, value, gesture);
    }
}

fn nose_radius(xi: f64) -> f64 {
    (1.0 - (1.0 - xi).powi(2)).max(0.0).sqrt()
}

fn tail_radius(xi: f64) -> f64 {
    1.0 - xi.powf(1.5)
}

/// Render the side profile and the cross-section.
pub fn show_fuselage_editor(state: &mut AppState, ui: &mut Ui) {
    let Some(p) = profile(state) else {
        return;
    };
    ui.label(egui::RichText::new(tr("Section editor")).strong());
    ui.label(
        egui::RichText::new(tr("Drag the station points to place the nose, cabin and tail; drag the section edges to size it."))
            .weak()
            .small(),
    );
    let width = ui.available_width().max(260.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 150.0), Sense::hover());
    let inner = rect.shrink2(vec2(12.0, 14.0));
    let z_extent = (p.height * 0.5 + p.nose_z.abs().max(p.tail_z.abs()) + 1.0).max(3.0);
    let scale =
        (inner.width() / p.length.max(1.0) as f32).min(inner.height() / (2.0 * z_extent) as f32);
    let origin = pos2(inner.left(), inner.center().y);
    let to_screen = |x: f64, z: f64| pos2(origin.x + x as f32 * scale, origin.y - z as f32 * scale);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);

    let radius = p.height * 0.5;
    let cabin_end = (p.length - p.tailcone_length).max(p.cabin_start_x);
    let mut top = Vec::new();
    let mut bottom = Vec::new();
    for k in 0..=10 {
        let xi = k as f64 / 10.0;
        let x = xi * p.cabin_start_x;
        let z = p.cabin_z + (p.nose_z - p.cabin_z) * (1.0 - xi).powi(2);
        let r = radius * nose_radius(xi);
        top.push(to_screen(x, z + r));
        bottom.push(to_screen(x, z - r));
    }
    top.push(to_screen(cabin_end, p.cabin_z + radius));
    bottom.push(to_screen(cabin_end, p.cabin_z - radius));
    for k in 1..=10 {
        let xi = k as f64 / 10.0;
        let x = cabin_end + xi * (p.length - cabin_end);
        let z = p.cabin_z + (p.tail_z - p.cabin_z) * xi.powf(1.5);
        let r = radius * tail_radius(xi);
        top.push(to_screen(x, z + r));
        bottom.push(to_screen(x, z - r));
    }
    let stroke = Stroke::new(1.5_f32, ui.visuals().hyperlink_color);
    painter.add(egui::Shape::line(top, stroke));
    painter.add(egui::Shape::line(bottom, stroke));
    painter.line_segment(
        [to_screen(0.0, 0.0), to_screen(p.length, 0.0)],
        Stroke::new(0.5_f32, ui.visuals().weak_text_color()),
    );

    let from_x = move |pos: egui::Pos2| f64::from((pos.x - origin.x) / scale);
    let from_z = move |pos: egui::Pos2| f64::from((origin.y - pos.y) / scale);
    drag_point(
        state,
        ui,
        "geometry.fuselage.nose_z_m",
        to_screen(0.0, p.nose_z),
        from_z,
        "Nose height",
    );
    drag_point(
        state,
        ui,
        "geometry.fuselage.cabin_start_x_m",
        to_screen(p.cabin_start_x, p.cabin_z + radius),
        from_x,
        "Cabin start",
    );
    drag_point(
        state,
        ui,
        "geometry.fuselage.cabin_z_m",
        to_screen(p.cabin_start_x + 1.0, p.cabin_z),
        from_z,
        "Cabin height",
    );
    let length = p.length;
    drag_point(
        state,
        ui,
        "geometry.fuselage.tailcone_length_m",
        to_screen(cabin_end, p.cabin_z - radius),
        move |pos| length - from_x(pos),
        "Tailcone start",
    );
    drag_point(
        state,
        ui,
        "design.fuselage_length_m",
        to_screen(p.length, p.tail_z),
        from_x,
        "Fuselage length",
    );
    drag_point(
        state,
        ui,
        "geometry.fuselage.tail_z_m",
        to_screen(p.length - 0.15 * p.tailcone_length, p.tail_z + 0.6),
        from_z,
        "Tail height",
    );

    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 130.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    let inner = rect.shrink(12.0);
    let extent = p.diameter.max(p.height).max(1.0);
    let scale = (inner.width().min(inner.height()) / extent as f32) * 0.9;
    let center = inner.center();
    let ellipse: Vec<egui::Pos2> = (0..=48)
        .map(|k| {
            let theta = std::f64::consts::TAU * k as f64 / 48.0;
            pos2(
                center.x + (p.diameter * 0.5 * theta.cos()) as f32 * scale,
                center.y - (p.height * 0.5 * theta.sin()) as f32 * scale,
            )
        })
        .collect();
    painter.add(egui::Shape::line(
        ellipse,
        Stroke::new(1.5_f32, ui.visuals().hyperlink_color),
    ));
    let half_width = (p.diameter * 0.5) as f32 * scale;
    let half_height = (p.height * 0.5) as f32 * scale;
    drag_point(
        state,
        ui,
        "geometry.fuselage.diameter_m",
        pos2(center.x + half_width, center.y),
        move |pos| f64::from((pos.x - center.x).abs() / scale) * 2.0,
        "Width",
    );
    drag_point(
        state,
        ui,
        "geometry.fuselage.height_m",
        pos2(center.x, center.y - half_height),
        move |pos| f64::from((center.y - pos.y).abs() / scale) * 2.0,
        "Height",
    );
    let _ = Color32::TRANSPARENT;
}
