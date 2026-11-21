// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The section editors of the Discipline Windows: true-aspect views of the
//! built geometry with the same drag handles as the 3-D viewport.
//!
//! Every drawing reads the built aircraft ([`super::built_sections`]) and the
//! lofted faces the 3-D model draws, never a parametric stand-in. Each view
//! is an orthographic projection scaled uniformly (one metre is the same
//! number of points along both screen axes). A handle edits the same field,
//! through the same undo transaction and domain check, as its 3-D twin
//! ([`super::drag`]); the editor builds a [`SceneFraming`] of its own view so
//! that machinery applies unchanged.
//!
//! Views read like a standard three-view: the plan has the nose at the left
//! and the starboard side up, the side view has the nose at the left, and
//! the front view looks at the nose with starboard on the viewer's left.

use alas_geom::aircraft::airplane::Airplane;
use alas_report::families::geometry::{SceneComponent, SceneFraming};
use alas_report::scene::Camera3D;
use egui::{pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui};
use serde_json::Value;

use crate::state::AppState;
use crate::views::tr;

use super::built_sections::{
    body_stations, is_symmetric, largest_station, lifting_sections, station_ring, BodyStation,
    LiftingSection,
};
use super::drag::{handles, Handle};
use super::editors::{commit_field, Gesture};
use super::fields::{Discipline, FieldKind, SandboxField};
use super::scene::discipline_component;

/// Margin inside a view, points.
const MARGIN: f32 = 10.0;
/// Pick radius of a handle, points.
const PICK: f32 = 8.0;
/// Segments of a drawn cross-section ring (the 3-D model's own count).
const RING_SEGMENTS: usize = 48;

/// A drawing direction of a section editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlanView {
    /// Plan: x aft to the right, y (starboard) up.
    Top,
    /// Side: x aft to the right, z up.
    Side,
    /// From the nose: y (starboard) to the left, z up.
    Front,
}

impl PlanView {
    /// The projection camera of this drawing (see the module note).
    pub fn camera(self) -> Camera3D {
        let (elev_deg, azim_deg) = match self {
            Self::Top => (-90.0, 0.0),
            Self::Side => (0.0, 0.0),
            Self::Front => (0.0, 90.0),
        };
        Camera3D {
            elev_deg,
            azim_deg,
            zoom: 1.0,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Top => "Plan view",
            Self::Side => "Side view",
            Self::Front => "Front view",
        }
    }
}

/// The drawings a discipline's section editor shows.
pub fn views_for(discipline: Discipline) -> &'static [PlanView] {
    match discipline {
        Discipline::Wing | Discipline::HorizontalTail => &[PlanView::Top, PlanView::Side],
        Discipline::VerticalTail => &[PlanView::Side],
        Discipline::Fuselage => &[PlanView::Side, PlanView::Top],
        Discipline::Propulsion => &[PlanView::Side, PlanView::Top, PlanView::Front],
    }
}

/// One drawing's mapping from aircraft axes to screen points.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    /// The drawing area.
    pub rect: Rect,
    /// The projection, in the rectangle's own point units.
    pub framing: SceneFraming,
    /// Points per metre, the same along both axes.
    pub scale: f64,
    centre: [f64; 3],
}

/// The extent in metres of `points` along the screen axes of `view`.
fn extent_of(view: PlanView, points: &[[f64; 3]]) -> Option<((f64, f64), (f64, f64))> {
    let (right, up) = view.camera().screen_axes();
    let dot = |a: [f64; 3], p: &[f64; 3]| a[0] * p[0] + a[1] * p[1] + a[2] * p[2];
    let mut u = (f64::INFINITY, f64::NEG_INFINITY);
    let mut v = (f64::INFINITY, f64::NEG_INFINITY);
    for p in points {
        u = (u.0.min(dot(right, p)), u.1.max(dot(right, p)));
        v = (v.0.min(dot(up, p)), v.1.max(dot(up, p)));
    }
    (u.0.is_finite() && v.0.is_finite()).then_some((u, v))
}

impl Frame {
    /// Fit `points` into `rect` at one uniform scale, centred.
    pub fn fit(rect: Rect, view: PlanView, points: &[[f64; 3]]) -> Option<Self> {
        let ((u0, u1), (v0, v1)) = extent_of(view, points)?;
        let (right, up) = view.camera().screen_axes();
        let usable = (
            f64::from(rect.width() - 2.0 * MARGIN).max(1.0),
            f64::from(rect.height() - 2.0 * MARGIN).max(1.0),
        );
        let scale = (usable.0 / (u1 - u0).max(1e-3)).min(usable.1 / (v1 - v0).max(1e-3));
        let (uc, vc) = ((u0 + u1) * 0.5, (v0 + v1) * 0.5);
        let centre: [f64; 3] = std::array::from_fn(|k| right[k] * uc + up[k] * vc);
        let side = f64::from(rect.width().min(rect.height()));
        let framing = SceneFraming {
            camera: view.camera(),
            center: centre,
            max_span: side * 0.45 / scale,
            viewport: (0.0, 0.0, f64::from(rect.width()), f64::from(rect.height())),
            canvas: (f64::from(rect.width()), f64::from(rect.height())),
        };
        Some(Self {
            rect,
            framing,
            scale,
            centre,
        })
    }

    /// The screen point of an aircraft-axes point.
    pub fn to_screen(&self, point: [f64; 3]) -> Pos2 {
        let [x, y] = self.framing.project(point);
        self.rect.min + vec2(x as f32, y as f32)
    }

    /// The aircraft-axes point under a screen point, on the plane through the
    /// view's centre (the coordinate along the view direction is that of the
    /// centre).
    pub fn model_at(&self, pos: Pos2) -> [f64; 3] {
        let (right, up) = self.framing.camera.screen_axes();
        let mid = self.rect.center();
        let (dx, dy) = (
            f64::from(pos.x - mid.x) / self.scale,
            f64::from(mid.y - pos.y) / self.scale,
        );
        std::array::from_fn(|k| self.centre[k] + right[k] * dx + up[k] * dy)
    }
}

fn component_colour(component: SceneComponent) -> Color32 {
    let c = component.color();
    Color32::from_rgb(c.r, c.g, c.b)
}

/// The face polygons of `component` as the 3-D model draws them.
fn component_faces(state: &AppState, component: SceneComponent) -> Vec<Vec<[f64; 3]>> {
    state
        .sandbox
        .model
        .as_ref()
        .map(|model| {
            model
                .faces()
                .iter()
                .filter(|face| face.component == component)
                .map(|face| face.points.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// What the section editor draws, gathered once per frame from the built
/// aircraft.
struct Built {
    faces: Vec<Vec<[f64; 3]>>,
    lifting: Vec<LiftingSection>,
    bodies: Vec<Vec<BodyStation>>,
    symmetric: bool,
}

fn gather(state: &AppState, plane: &Airplane, component: SceneComponent) -> Built {
    Built {
        faces: component_faces(state, component),
        lifting: lifting_sections(plane, component),
        bodies: body_stations(plane, component),
        symmetric: is_symmetric(plane, component),
    }
}

/// Render the section editor of `discipline` for the built aircraft.
pub fn show_section_editor(state: &mut AppState, ui: &mut Ui, discipline: Discipline) {
    let component = discipline_component(discipline);
    let Some(plane) = state.sandbox.airplane.as_ref() else {
        return;
    };
    let built = gather(state, plane, component);
    let points: Vec<[f64; 3]> = built.faces.iter().flatten().copied().collect();
    if points.is_empty() {
        return;
    }
    ui.label(egui::RichText::new(tr("Section editor")).strong())
        .on_hover_text(tr(
            "Drawn from the built geometry. Drag a handle to edit the same parameter as in the 3-D view.",
        ));
    let handle_list = match (state.typed_config(), state.current_design()) {
        (Some(config), Some(design)) => handles(&config, &design, Some(discipline)),
        _ => Vec::new(),
    };
    // A window wider than tall puts the drawings and the section cells side
    // by side, so both are visible without scrolling below them.
    let has_cells = !built.lifting.is_empty() || !built.bodies.is_empty();
    let wide = has_cells && ui.available_width() > ui.ctx().screen_rect().height();
    if wide {
        ui.columns(2, |columns| {
            for &view in views_for(discipline) {
                show_view(
                    state,
                    &mut columns[0],
                    discipline,
                    component,
                    view,
                    &built,
                    &points,
                    &handle_list,
                );
            }
            if !built.lifting.is_empty() {
                show_airfoil_sections(&mut columns[1], &built.lifting);
            }
            if !built.bodies.is_empty() {
                show_cross_section(&mut columns[1], discipline, &built.bodies);
            }
        });
        return;
    }
    for &view in views_for(discipline) {
        show_view(
            state,
            ui,
            discipline,
            component,
            view,
            &built,
            &points,
            &handle_list,
        );
    }
    if !built.lifting.is_empty() {
        show_airfoil_sections(ui, &built.lifting);
    }
    if !built.bodies.is_empty() {
        show_cross_section(ui, discipline, &built.bodies);
    }
}

#[allow(clippy::too_many_arguments)]
fn show_view(
    state: &mut AppState,
    ui: &mut Ui,
    discipline: Discipline,
    component: SceneComponent,
    view: PlanView,
    built: &Built,
    points: &[[f64; 3]],
    handle_list: &[Handle],
) {
    let Some(((u0, u1), (v0, v1))) = extent_of(view, points) else {
        return;
    };
    ui.add_space(4.0);
    ui.label(egui::RichText::new(tr(view.title())).weak().small());
    let width = ui.available_width().max(260.0);
    // The height follows the drawing's own aspect, so neither axis is
    // stretched and no band of the card is left empty.
    let aspect = ((v1 - v0) / (u1 - u0).max(1e-3)) as f32;
    // With an airfoil strip below, the drawings stay short enough for both
    // to be visible together in a default-size window.
    let tallest = if built.lifting.is_empty() {
        260.0
    } else {
        180.0
    };
    let height = ((width - 2.0 * MARGIN) * aspect + 2.0 * MARGIN).clamp(110.0, tallest);
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let Some(frame) = Frame::fit(rect, view, points) else {
        return;
    };
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);

    let base = component_colour(component);
    let fill = base.gamma_multiply(0.45);
    let edge = Stroke::new(0.6_f32, base.gamma_multiply(0.9));
    for face in &built.faces {
        let polygon: Vec<Pos2> = face.iter().map(|p| frame.to_screen(*p)).collect();
        painter.add(Shape::convex_polygon(polygon, fill, edge));
    }
    // The built sections, where they sit in this view.
    let section_stroke = Stroke::new(1.0_f32, ui.visuals().strong_text_color());
    for section in &built.lifting {
        let line: Vec<Pos2> = section
            .outline
            .iter()
            .map(|p| frame.to_screen(*p))
            .collect();
        painter.add(Shape::line(line, section_stroke));
        if built.symmetric && matches!(view, PlanView::Top | PlanView::Front) {
            // Mirror half of a symmetric surface is part of the faces; the
            // section line of the far side is its mirror image.
            let mirrored: Vec<Pos2> = section
                .outline
                .iter()
                .map(|p| frame.to_screen([p[0], -p[1], p[2]]))
                .collect();
            painter.add(Shape::line(mirrored, section_stroke.into_faint()));
        }
    }
    for body in &built.bodies {
        for station in body {
            let r = station.height * 0.5;
            let (a, b) = (
                frame.to_screen([station.centre[0], station.centre[1], station.centre[2] + r]),
                frame.to_screen([station.centre[0], station.centre[1], station.centre[2] - r]),
            );
            if view == PlanView::Side {
                painter.line_segment([a, b], section_stroke.into_faint());
            } else if view == PlanView::Top {
                let w = station.width * 0.5;
                painter.line_segment(
                    [
                        frame.to_screen([station.centre[0], station.centre[1] + w, 0.0]),
                        frame.to_screen([station.centre[0], station.centre[1] - w, 0.0]),
                    ],
                    section_stroke.into_faint(),
                );
            }
        }
    }

    for handle in handle_list {
        drive_handle(state, ui, &frame, view, handle);
    }
    if discipline == Discipline::Fuselage && view == PlanView::Side {
        show_parametric_points(state, ui, &frame);
    }
}

trait Faint {
    fn into_faint(self) -> Self;
}

impl Faint for Stroke {
    fn into_faint(self) -> Self {
        Stroke::new(self.width * 0.6, self.color.gamma_multiply(0.45))
    }
}

/// Draw one handle in `frame` and run its drag, when its axis is visible in
/// this view (an edge-on axis cannot be dragged here).
fn drive_handle(state: &mut AppState, ui: &mut Ui, frame: &Frame, view: PlanView, handle: &Handle) {
    let origin = frame.to_screen(handle.point);
    let tip = frame.to_screen([
        handle.point[0] + handle.axis[0],
        handle.point[1] + handle.axis[1],
        handle.point[2] + handle.axis[2],
    ]);
    if (tip - origin).length() < 0.25 * frame.scale as f32 || !frame.rect.contains(origin) {
        return;
    }
    let id = ui.id().with((
        "section_handle",
        handle.field_id,
        handle.kind as u8,
        handle.component,
        handle.section_index,
        view,
    ));
    let response = ui.interact(
        Rect::from_center_size(origin, vec2(2.0 * PICK, 2.0 * PICK)),
        id,
        Sense::drag(),
    );
    let active = response.dragged() || response.hovered();
    let accent = ui.visuals().hyperlink_color;
    ui.painter_at(frame.rect).circle(
        origin,
        if active { 5.5 } else { 4.0 },
        if active {
            accent
        } else {
            accent.gamma_multiply(0.7)
        },
        Stroke::new(1.0_f32, ui.visuals().strong_text_color()),
    );
    response.clone().on_hover_text(tr(handle.label));
    if response.drag_started() {
        state.begin_handle_drag(handle, &frame.framing, 1.0);
    } else if response.dragged() {
        state.update_handle_drag(response.drag_delta());
    }
    if response.drag_stopped() {
        state.end_handle_drag();
    }
}

fn field(state: &AppState, id: &str) -> Option<SandboxField> {
    state.sandbox.fields.iter().find(|f| f.id == id).cloned()
}

/// A point that writes one configuration field from the pointer.
fn drag_point(
    state: &mut AppState,
    ui: &mut Ui,
    frame: &Frame,
    id: &str,
    at: [f64; 3],
    value_of: impl Fn([f64; 3]) -> f64,
    label: &str,
) {
    let Some(field) = field(state, id) else {
        return;
    };
    let centre = frame.to_screen(at);
    if !frame.rect.contains(centre) {
        return;
    }
    let response = ui.interact(
        Rect::from_center_size(centre, vec2(14.0, 14.0)),
        ui.id().with(("section_point", id)),
        Sense::drag(),
    );
    let colour = if response.hovered() || response.dragged() {
        ui.visuals().hyperlink_color
    } else {
        ui.visuals().strong_text_color()
    };
    ui.painter_at(frame.rect).circle(
        centre,
        4.5,
        colour,
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
        let raw = value_of(frame.model_at(pointer));
        let scale = 10f64.powi(field.decimals as i32);
        let value = ((raw.clamp(field.min, field.max)) * scale).round() / scale;
        let value = match field.kind {
            FieldKind::OptionalFloat => Value::from(value),
            _ => Value::from(value),
        };
        commit_field(state, &field, value, gesture);
    }
}

/// The parametric body's own station points (nose, cabin, tail cone, tail):
/// only a body generated from those parameters answers to them, so a lofted
/// fuselage shows none.
fn show_parametric_points(state: &mut AppState, ui: &mut Ui, frame: &Frame) {
    let Some(config) = state.typed_config() else {
        return;
    };
    let f = &config.geometry.fuselage;
    if f.loft.is_some() {
        return;
    }
    let Some(length) = state.design_values.get("fuselage_length_m").copied() else {
        return;
    };
    let radius = f.effective_height_m() * 0.5;
    let cabin_end = (length - f.tailcone_length_m).max(f.cabin_start_x_m);
    let x_of = |p: [f64; 3]| p[0];
    let z_of = |p: [f64; 3]| p[2];
    drag_point(
        state,
        ui,
        frame,
        "geometry.fuselage.nose_z_m",
        [0.0, 0.0, f.nose_z_m],
        z_of,
        "Nose height",
    );
    drag_point(
        state,
        ui,
        frame,
        "geometry.fuselage.cabin_start_x_m",
        [f.cabin_start_x_m, 0.0, f.cabin_z_m + radius],
        x_of,
        "Cabin start",
    );
    drag_point(
        state,
        ui,
        frame,
        "geometry.fuselage.cabin_z_m",
        [f.cabin_start_x_m + 1.0, 0.0, f.cabin_z_m],
        z_of,
        "Cabin height",
    );
    drag_point(
        state,
        ui,
        frame,
        "geometry.fuselage.tailcone_length_m",
        [cabin_end, 0.0, f.cabin_z_m - radius],
        move |p| length - p[0],
        "Tailcone start",
    );
    drag_point(
        state,
        ui,
        frame,
        "geometry.fuselage.tail_z_m",
        [length - 0.15 * f.tailcone_length_m, 0.0, f.tail_z_m + 0.6],
        z_of,
        "Tail height",
    );
}

/// The in-section axis perpendicular to the chord (x) of section `index`: the
/// normal of the surface's own plane, so a vertical fin shows its thickness
/// along y and a wing along z. Oriented up (or to starboard for a fin).
pub fn thickness_axis(sections: &[LiftingSection], index: usize) -> [f64; 3] {
    let (a, b) = if index + 1 < sections.len() {
        (sections[index].le, sections[index + 1].le)
    } else if index > 0 {
        (sections[index - 1].le, sections[index].le)
    } else {
        return [0.0, 0.0, 1.0];
    };
    let span = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    // span x (1, 0, 0) = (0, span_z, -span_y)
    let mut axis = [0.0, span[2], -span[1]];
    let length = (axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    if length < 1e-9 {
        return [0.0, 0.0, 1.0];
    }
    axis = [0.0, axis[1] / length, axis[2] / length];
    let flip = if axis[2].abs() >= axis[1].abs() {
        axis[2] < 0.0
    } else {
        axis[1] < 0.0
    };
    if flip {
        axis = [0.0, -axis[1], -axis[2]];
    }
    axis
}

/// The narrowest an airfoil card is drawn, in points: room for its widest caption.
const CAPTION_CELL_MIN_W: f32 = 96.0;

/// The airfoil of every built section, each at its own chord: the shape the
/// 3-D model lofts (camber, thickness, twist), at true aspect.
fn show_airfoil_sections(ui: &mut Ui, sections: &[LiftingSection]) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(tr("Airfoil sections")).weak().small());
    // Cards wrap within the column: never wider than the room there is.
    let width = ui.available_width().max(1.0);
    let columns = (((width + 8.0) / (CAPTION_CELL_MIN_W + 8.0)).floor() as usize)
        .clamp(1, sections.len().max(1));
    let cell_w = (width - 8.0 * (columns as f32 - 1.0)) / columns as f32;
    let cell_h = cell_w * 0.34 + 32.0;
    // A vertical surface spans z, not y.
    let span_axis = if sections
        .windows(2)
        .any(|pair| (pair[1].le[2] - pair[0].le[2]).abs() > (pair[1].le[1] - pair[0].le[1]).abs())
    {
        2
    } else {
        1
    };
    let span_name = if span_axis == 2 { "z" } else { "y" };
    let rows = sections.len().div_ceil(columns);
    let (rect, _) =
        ui.allocate_exact_size(vec2(width, rows as f32 * (cell_h + 6.0)), Sense::hover());
    let painter = ui.painter_at(rect);
    let ink = ui.visuals().strong_text_color();
    for (index, section) in sections.iter().enumerate() {
        let (row, col) = (index / columns, index % columns);
        let cell = Rect::from_min_size(
            rect.min + vec2(col as f32 * (cell_w + 8.0), row as f32 * (cell_h + 6.0)),
            vec2(cell_w, cell_h),
        );
        painter.rect_filled(cell, 4.0, ui.visuals().extreme_bg_color);
        let scale = f64::from(cell_w - 2.0 * MARGIN) / section.chord.max(1e-6);
        let origin = pos2(cell.left() + MARGIN, cell.top() + (cell_h - 32.0) * 0.5);
        let axis = thickness_axis(sections, index);
        let line: Vec<Pos2> = section
            .outline
            .iter()
            .map(|p| {
                pos2(
                    origin.x + ((p[0] - section.le[0]) * scale) as f32,
                    origin.y
                        - (((p[1] - section.le[1]) * axis[1] + (p[2] - section.le[2]) * axis[2])
                            * scale) as f32,
                )
            })
            .collect();
        painter.add(Shape::closed_line(line, Stroke::new(1.4_f32, ink)));
        // Two short lines, so a caption fits the narrowest card.
        let caption = painter.with_clip_rect(cell);
        let font = FontId::proportional(10.5);
        let weak = ui.visuals().weak_text_color();
        caption.text(
            pos2(cell.center().x, cell.bottom() - 15.0),
            Align2::CENTER_BOTTOM,
            format!("{span_name} {:.1} m", section.le[span_axis]),
            font.clone(),
            weak,
        );
        caption.text(
            pos2(cell.center().x, cell.bottom() - 3.0),
            Align2::CENTER_BOTTOM,
            format!(
                "c {:.2} m \u{b7} {:+.1}\u{b0}",
                section.chord, section.twist_deg
            ),
            font,
            weak,
        );
    }
}

/// The cross-section of a body at a chosen station, with the other stations
/// behind it at their true offsets, at true aspect.
fn show_cross_section(ui: &mut Ui, discipline: Discipline, bodies: &[Vec<BodyStation>]) {
    let Some(stations) = bodies.first().filter(|s| !s.is_empty()) else {
        return;
    };
    ui.add_space(4.0);
    ui.label(egui::RichText::new(tr("Cross-section")).weak().small());
    let id = ui.id().with(("section_station", discipline.id()));
    let mut index = ui
        .data(|d| d.get_temp::<usize>(id))
        .unwrap_or_else(|| largest_station(stations))
        .min(stations.len() - 1);
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut index, 0..=stations.len() - 1).show_value(false));
        ui.label(format!(
            "x = {:.2} m, {:.2} x {:.2} m",
            stations[index].centre[0], stations[index].width, stations[index].height
        ));
    });
    ui.data_mut(|d| d.insert_temp(id, index));
    let width = ui.available_width().max(1.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 150.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    let selected = &stations[index];
    // Extent of every ring about the selected centre.
    let mut half = (0.5_f64, 0.5_f64);
    for s in stations {
        half.0 = half
            .0
            .max((s.centre[1] - selected.centre[1]).abs() + s.width * 0.5);
        half.1 = half
            .1
            .max((s.centre[2] - selected.centre[2]).abs() + s.height * 0.5);
    }
    let scale = (f64::from(rect.width() - 2.0 * MARGIN) / 2.0 / half.0)
        .min(f64::from(rect.height() - 2.0 * MARGIN) / 2.0 / half.1);
    let centre = rect.center();
    // Front view: starboard (+y) to the viewer's left.
    let to_screen = |p: [f64; 3]| {
        pos2(
            centre.x - ((p[1] - selected.centre[1]) * scale) as f32,
            centre.y - ((p[2] - selected.centre[2]) * scale) as f32,
        )
    };
    let colour = ui.visuals().hyperlink_color;
    for (k, station) in stations.iter().enumerate() {
        if k == index {
            continue;
        }
        let ring: Vec<Pos2> = station_ring(station, RING_SEGMENTS)
            .iter()
            .map(|p| to_screen(*p))
            .collect();
        painter.add(Shape::closed_line(
            ring,
            Stroke::new(0.7_f32, colour.gamma_multiply(0.25)),
        ));
    }
    let ring: Vec<Pos2> = station_ring(selected, RING_SEGMENTS)
        .iter()
        .map(|p| to_screen(*p))
        .collect();
    painter.add(Shape::closed_line(ring, Stroke::new(2.0_f32, colour)));
}

#[cfg(test)]
#[path = "section_views_tests.rs"]
mod tests;
