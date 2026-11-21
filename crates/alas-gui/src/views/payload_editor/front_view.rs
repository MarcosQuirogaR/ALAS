// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The front view of the payload canvas: the aircraft as the 3-D preview's
//! engine draws it (see [`super::front_render`]), with the cabin cutaway at a
//! chosen station drawn over it at the very same scale and origin: the
//! fuselage section, the floors, the seats and the declared point masses by
//! lateral position `y` and height `z`. Together with the side view it places
//! a point mass anywhere in the airframe envelope (see [`super::envelope`]).
//!
//! Axes: `y` (positive to starboard) is drawn to the right and `z` (up)
//! upward, as the numbers read; the picture is therefore the aircraft seen
//! from behind.

use alas_config::{PayloadItemsConfig, ZoneKind};
use alas_payload::layout::ItemKind;
use egui::{vec2, Align2, Color32, FontId, Pos2, Rect, Rounding, Sense, Shape, Stroke, Ui};

use super::camera::{self, CANVAS_RADIUS};
use super::canvas::{item_color, Action, Target};
use super::front_camera::{Camera, Frame};
#[cfg(test)]
use super::front_camera::{MAX_ZOOM, MIN_ZOOM, WING_TIP_ROOM};
use super::front_render;
use super::glyphs::{faded, fit_text};
use super::side_view::SideView;
use crate::views::tr_label;

const POINT_HIT: f32 = 9.0;
/// Shortest front-view canvas, points.
const FRONT_MIN_HEIGHT: f32 = 220.0;
/// Tallest front-view canvas, points.
const FRONT_MAX_HEIGHT: f32 = 900.0;
/// A point mass this near the section is drawn solid, m.
const NEAR_M: f64 = 1.5;

/// Draw the front view at station `section` and return the mouse action.
/// The section station is edited in the canvas corner.
#[allow(clippy::too_many_arguments)]
pub(super) fn show(
    ui: &mut Ui,
    view: &SideView,
    items: &PayloadItemsConfig,
    selected: Option<&Target>,
    section: &mut f64,
    camera: &mut Camera,
    dragging: bool,
    placing: bool,
    height: f32,
    theme: &'static str,
) -> Action {
    // The station field and the framing buttons are a strip above the canvas:
    // nothing is drawn over the picture.
    let framing = header(ui, view, section);
    // The canvas follows the page's room (clamped), like the mission plot; the
    // picture keeps its true aspect inside it.
    let width = ui.available_width().max(1.0);
    let room = (height - camera::HEADER_HEIGHT).max(FRONT_MIN_HEIGHT);
    let canvas_height = room.clamp(FRONT_MIN_HEIGHT, FRONT_MAX_HEIGHT);
    let (rect, response) =
        ui.allocate_exact_size(vec2(width, canvas_height), Sense::click_and_drag());
    let section_x = *section;
    let [x0, x1] = view.x_range();
    camera.sync(
        rect,
        view,
        section_x,
        camera::fit_key(
            rect,
            [
                x0,
                x1,
                view.render_z_range()[0],
                view.render_z_range()[1],
                view.semi_span(),
            ],
        ),
    );
    match framing {
        Some(Framing::Whole) => {
            *camera = Camera {
                key: camera.key,
                ..Camera::default()
            }
        }
        Some(Framing::Fuselage) => {
            *camera = Camera {
                key: camera.key,
                ..Camera::fuselage(rect, view, section_x)
            };
        }
        None => {}
    }
    let frame = Frame::new(rect, view, section_x, *camera);
    let on_item = response.drag_started()
        && response
            .ctx
            .input(|input| input.pointer.press_origin())
            .is_some_and(|origin| hit(&frame, items, origin).is_some());
    if let Some(delta) = camera::pan(ui, &response, on_item, dragging) {
        camera.pan_by(rect, view, section_x, delta);
    }
    if let Some((cursor, factor)) = camera::wheel(ui, &response) {
        camera.zoom_about(rect, view, section_x, cursor, factor);
    }
    let frame = Frame::new(rect, view, section_x, *camera);
    ui.ctx()
        .data_mut(|data| data.insert_temp(super::canvas::canvas_rect_id(), rect));
    let painter = ui.painter_at(rect);
    front_render::paint(
        ui,
        &painter,
        rect,
        frame.placement(),
        view,
        theme,
        front_render::Projection::Front,
    );
    paint_section(ui, &painter, &frame, view, section_x);
    paint_seats(&painter, &frame, view, section_x);
    paint_items(ui, &painter, &frame, view, items, selected, section_x);
    painter.rect_stroke(
        rect,
        Rounding::same(CANVAS_RADIUS),
        Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
    if placing {
        ui.output_mut(|output| output.cursor_icon = egui::CursorIcon::Crosshair);
    }
    interact(&response, &frame, items, dragging, placing)
}

/// Which framing button was clicked.
enum Framing {
    Whole,
    Fuselage,
}

/// The strip above the canvas: the section station field at the left and the
/// framing buttons at the right.
fn header(ui: &mut Ui, view: &SideView, section: &mut f64) -> Option<Framing> {
    let [x0, x1] = view.x_range();
    let buttons = [
        camera::Overlay {
            label: tr_label("Fit"),
            tooltip: &tr_label("Frame the whole front section"),
            selected: false,
        },
        camera::Overlay {
            label: tr_label("Fuselage"),
            tooltip: &tr_label("Mouse wheel zooms, drag pans"),
            selected: false,
        },
    ];
    camera::header(
        ui,
        |ui| {
            ui.add(
                egui::DragValue::new(section)
                    .speed(0.1)
                    .range(x0..=x1)
                    .prefix(format!("{} x ", tr_label("Section")))
                    .suffix(" m"),
            )
            .on_hover_text(tr_label("Station of the fuselage section"));
        },
        &buttons,
    )
    .map(|index| {
        if index == 0 {
            Framing::Whole
        } else {
            Framing::Fuselage
        }
    })
}

fn paint_section(ui: &Ui, painter: &egui::Painter, frame: &Frame, view: &SideView, x: f64) {
    let weak = ui.visuals().weak_text_color();
    let stroke = Stroke::new(1.5_f32, ui.visuals().widgets.noninteractive.fg_stroke.color);
    let outline: Vec<Pos2> = view
        .section_outline(x)
        .into_iter()
        .map(|(y, z)| frame.to_screen(y, z))
        .collect();
    painter.add(Shape::convex_polygon(
        outline,
        ui.visuals().extreme_bg_color,
        stroke,
    ));
    // The cabin floor across its usable width, and the hold floor.
    let floor = view.floor_z(x);
    // Clipped to the section contour at the floor height: the usable width
    // is a cabin figure, the contour is the skin.
    let usable = (view.at(&view.main_width, x) / 2.0).min(view.section_half_width_at(x, floor));
    painter.line_segment(
        [
            frame.to_screen(-usable, floor),
            frame.to_screen(usable, floor),
        ],
        Stroke::new(1.5_f32, ui.visuals().text_color()),
    );
    if view.has_hold {
        let hold = view.hold_floor_z(x);
        if hold < floor {
            let half =
                (view.at(&view.main_width, x) / 2.0).min(view.section_half_width_at(x, hold));
            painter.line_segment(
                [frame.to_screen(-half, hold), frame.to_screen(half, hold)],
                Stroke::new(0.8_f32, weak),
            );
        }
    }
}

/// The seat rows the section cuts, drawn as seats seen from behind: a
/// backrest and a cushion per seat, grouped in the row's lateral blocks.
fn paint_seats(painter: &egui::Painter, frame: &Frame, view: &SideView, x: f64) {
    for item in &view.items {
        let Some(seat) = item
            .seat
            .as_ref()
            .filter(|_| item.kind == ItemKind::SeatRow)
        else {
            continue;
        };
        if (item.x - x).abs() > item.length / 2.0 {
            continue;
        }
        let class = alas_report::families::geometry::seat_class_color(seat.cls);
        let color = Color32::from_rgb(class.r, class.g, class.b);
        let seats: i64 = seat.blocks.iter().sum();
        let aisles = seat.blocks.len().saturating_sub(1) as f64;
        let total = seats as f64 * seat.seat_w + aisles * seat.aisle_w;
        let mut y = item.y - total / 2.0;
        let top = item.z + item.height / 2.0;
        let pan = seat.floor_z + 0.42;
        for (block, count) in seat.blocks.iter().enumerate() {
            if block > 0 {
                y += seat.aisle_w;
            }
            for _ in 0..*count {
                let (y0, y1) = (y + seat.seat_w * 0.06, y + seat.seat_w * 0.94);
                let rect = |y0: f64, z0: f64, y1: f64, z1: f64| {
                    Rect::from_two_pos(frame.to_screen(y0, z1), frame.to_screen(y1, z0))
                };
                let edge = Stroke::new(0.6_f32, faded(color, 235));
                // Backrest with its headrest, armrests either side, then the
                // cushion: a seat seen from behind.
                let head = ((top - pan) * 0.26).clamp(0.08, 0.18);
                let inset = (y1 - y0) * 0.14;
                painter.rect(
                    rect(y0 + 0.03, pan + 0.06, y1 - 0.03, top - head),
                    Rounding::same(2.5),
                    faded(color, 215),
                    edge,
                );
                painter.rect(
                    rect(y0 + inset, top - head, y1 - inset, top),
                    Rounding::same(3.0),
                    lighten(color),
                    edge,
                );
                for (a, b) in [(y0, y0 + 0.05), (y1 - 0.05, y1)] {
                    painter.rect(
                        rect(a, pan + 0.06, b, pan + 0.22),
                        Rounding::same(1.0),
                        faded(color, 150),
                        edge,
                    );
                }
                painter.rect(
                    rect(y0, pan - 0.08, y1, pan + 0.06),
                    Rounding::same(1.5),
                    color,
                    edge,
                );
                painter.line_segment(
                    [
                        frame.to_screen(f64::midpoint(y0, y1), pan - 0.08),
                        frame.to_screen(f64::midpoint(y0, y1), seat.floor_z),
                    ],
                    Stroke::new(1.0_f32, faded(color, 160)),
                );
                y += seat.seat_w;
            }
        }
    }
}

/// `color` a step lighter, for a headrest.
fn lighten(color: Color32) -> Color32 {
    let up = |c: u8| (u16::from(c) * 9 / 8).min(255) as u8;
    Color32::from_rgb(up(color.r()), up(color.g()), up(color.b()))
}

fn paint_items(
    ui: &Ui,
    painter: &egui::Painter,
    frame: &Frame,
    view: &SideView,
    items: &PayloadItemsConfig,
    selected: Option<&Target>,
    x: f64,
) {
    let zone_color = item_color(ui, ItemKind::Zone);
    for (index, zone) in items.zones.iter().enumerate() {
        if zone.x_start_m > x || zone.x_end_m < x {
            continue;
        }
        // A zone box stays inside the skin at both its floor and its ceiling.
        let usable = view.at(&view.main_width, x).min(
            2.0 * view
                .section_half_width_at(x, view.floor_z(x))
                .min(view.section_half_width_at(x, view.ceiling_z(x))),
        );
        let width = zone.width_m.map_or(usable, |w| w.min(usable));
        let rect = Rect::from_two_pos(
            frame.to_screen(zone.y_center_m - width / 2.0, view.ceiling_z(x)),
            frame.to_screen(zone.y_center_m + width / 2.0, view.floor_z(x)),
        );
        let picked = selected == Some(&Target::Zone(index));
        painter.rect(
            rect,
            Rounding::same(2.0),
            faded(zone_color, if picked { 110 } else { 50 }),
            Stroke::new(if picked { 2.0_f32 } else { 1.0_f32 }, zone_color),
        );
        let label = if zone.kind == ZoneKind::Custom {
            zone.name.clone()
        } else {
            format!("{} ({:?})", zone.name, zone.kind)
        };
        let font = FontId::proportional(11.5);
        if let Some(label) = fit_text(painter, &label, &font, rect.width() - 6.0) {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                label,
                font,
                ui.visuals().text_color(),
            );
        }
    }
    let color = item_color(ui, ItemKind::PointMass);
    for (index, mass) in items.point_masses.iter().enumerate() {
        let near = (mass.x_m - x).abs() <= NEAR_M;
        let alpha = |full: u8| if near { full } else { full / 3 };
        let centre = frame.to_screen(mass.y_m, mass.z_m);
        let picked = selected == Some(&Target::PointMass(index));
        let r = if picked { 8.5 } else { 7.0 };
        painter.add(Shape::convex_polygon(
            vec![
                centre + vec2(0.0, -r),
                centre + vec2(r, 0.0),
                centre + vec2(0.0, r),
                centre + vec2(-r, 0.0),
            ],
            faded(color, alpha(if mass.releasable { 255 } else { 140 })),
            Stroke::new(1.0_f32, faded(color, alpha(255))),
        ));
        painter.text(
            centre + vec2(0.0, -r - 2.0),
            Align2::CENTER_BOTTOM,
            &mass.name,
            FontId::proportional(11.0),
            faded(ui.visuals().text_color(), alpha(255)),
        );
    }
}

fn interact(
    response: &egui::Response,
    frame: &Frame,
    items: &PayloadItemsConfig,
    dragging: bool,
    placing: bool,
) -> Action {
    if response.drag_started() {
        if let Some(origin) = response.ctx.input(|input| input.pointer.press_origin()) {
            if let Some(target) = hit(frame, items, origin) {
                return Action::DragStart { target, x: 0.0 };
            }
        }
        return Action::None;
    }
    if dragging {
        if response.drag_stopped() {
            return Action::DragEnd;
        }
        if let Some(pointer) = response.interact_pointer_pos() {
            let (y, z) = frame.to_world(pointer);
            return Action::DragToFront { y, z };
        }
        return Action::None;
    }
    if response.clicked() {
        if let Some(pointer) = response.interact_pointer_pos() {
            if placing {
                let (y, z) = frame.to_world(pointer);
                return Action::ClickFront { y, z };
            }
            return Action::Select(hit(frame, items, pointer));
        }
    }
    Action::None
}

/// The point mass under `point`, nearest first.
fn hit(frame: &Frame, items: &PayloadItemsConfig, point: Pos2) -> Option<Target> {
    items
        .point_masses
        .iter()
        .enumerate()
        .map(|(index, mass)| (index, frame.to_screen(mass.y_m, mass.z_m).distance(point)))
        .filter(|(_, distance)| *distance <= POINT_HIT)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| Target::PointMass(index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::payload_editor::side_view::test_body;
    use egui::pos2;

    fn body() -> SideView {
        let mut view = test_body();
        let n = view.stations.len();
        view.widths = vec![4.0; n];
        view.main_width = vec![3.6; n];
        view.wing_le = vec![[14.0, 0.0, -1.0], [18.0, 28.0, 0.5]];
        view.wing_chord = 6.0;
        view
    }

    fn canvas() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(700.0, 460.0))
    }

    #[test]
    fn the_default_framing_shows_the_whole_front_section_with_margin() {
        let view = body();
        let rect = canvas();
        let frame = Frame::new(rect, &view, 20.0, Camera::default());
        let inner = rect.shrink(8.0);
        for sign in [-1.0, 1.0] {
            let tip = frame.to_screen(sign * 28.0, 0.5);
            assert!(inner.contains(tip), "{tip:?} in {inner:?}");
        }
        let (low, high) = view.z_bounds(20.0).unwrap();
        assert!(frame.to_screen(0.0, low).y < inner.bottom());
        assert!(frame.to_screen(0.0, high.max(2.0)).y > inner.top());
        let fuselage = Camera::fuselage(rect, &view, 20.0);
        assert!(fuselage.zoom > 1.0, "the fuselage framing is closer in");
    }

    #[test]
    fn the_whole_aircraft_framing_includes_the_fin_seen_behind_the_section() {
        let mut view = body();
        let rect = canvas();
        let frame = Frame::new(rect, &view, 20.0, Camera::default());
        let without = frame.to_screen(0.0, 40.0).y;
        view.tail_top_z = Some(40.0);
        let frame = Frame::new(rect, &view, 20.0, Camera::default());
        assert!(
            without < rect.top(),
            "the fin is cut off without it: {without}"
        );
        let tip = frame.to_screen(0.0, 40.0);
        assert!(rect.shrink(8.0).contains(tip), "{tip:?} in {rect:?}");
    }

    #[test]
    fn whole_aircraft_brings_the_wing_tips_and_the_belly_reach_into_the_view() {
        let view = body();
        let rect = canvas();
        let camera = Camera::default();
        assert!((camera.zoom - 1.0).abs() < 1e-9, "{}", camera.zoom);
        let frame = Frame::new(rect, &view, 20.0, camera);
        for sign in [-1.0, 1.0] {
            let tip = frame.to_screen(sign * 28.0, 0.5);
            assert!(rect.contains(tip), "{tip:?} in {rect:?}");
        }
        let (low, _) = view.z_bounds(20.0).unwrap();
        assert!(frame.to_screen(0.0, low).y <= rect.bottom());
    }

    #[test]
    fn the_wheel_zooms_about_the_cursor_and_stays_within_limits() {
        let view = body();
        let rect = canvas();
        let mut camera = Camera::default();
        let cursor = pos2(520.0, 150.0);
        let (y, z) = Frame::new(rect, &view, 20.0, camera).to_world(cursor);
        camera.zoom_about(rect, &view, 20.0, cursor, 1.5);
        let frame = Frame::new(rect, &view, 20.0, camera);
        assert!((camera.zoom - 1.5).abs() < 1e-9);
        let (y2, z2) = frame.to_world(cursor);
        assert!(
            (y - y2).abs() < 1e-3 && (z - z2).abs() < 1e-3,
            "({y}, {z}) -> ({y2}, {z2})"
        );
        for _ in 0..40 {
            camera.zoom_about(rect, &view, 20.0, cursor, 2.0);
        }
        assert!((camera.zoom - MAX_ZOOM).abs() < 1e-9);
        for _ in 0..80 {
            camera.zoom_about(rect, &view, 20.0, cursor, 0.5);
        }
        assert!(camera.zoom >= MIN_ZOOM - 1e-9);
        assert!(camera.zoom < 1.0, "it can zoom out past the whole framing");
    }

    #[test]
    fn dragging_pans_the_view_by_the_pointer_travel() {
        let view = body();
        let rect = canvas();
        let mut camera = Camera::default();
        camera.zoom_about(rect, &view, 20.0, rect.center(), 3.0);
        let before = Frame::new(rect, &view, 20.0, camera);
        let anchor = before.to_screen(1.0, 0.5);
        camera.pan_by(rect, &view, 20.0, vec2(30.0, -20.0));
        let after = Frame::new(rect, &view, 20.0, camera);
        let moved = after.to_screen(1.0, 0.5) - anchor;
        assert!(
            (moved - vec2(30.0, -20.0)).length() < 0.5,
            "the picture follows the pointer: {moved:?}"
        );
        // It cannot be dragged away from the airframe altogether.
        for _ in 0..50 {
            camera.pan_by(rect, &view, 20.0, vec2(900.0, 0.0));
        }
        let (cy, _) = Frame::new(rect, &view, 20.0, camera).centre();
        assert!(cy.abs() <= view.semi_span() * WING_TIP_ROOM + 1e-6);
    }
}

/// Where `(y, z)` is drawn in a canvas of `rect` at section `x` under
/// `camera`, for the tests that drive the mouse.
#[cfg(test)]
pub(super) fn screen_of(
    rect: Rect,
    view: &SideView,
    x: f64,
    camera: Camera,
    y: f64,
    z: f64,
) -> Pos2 {
    Frame::new(rect, view, x, camera).to_screen(y, z)
}

#[cfg(test)]
mod scale_tests {
    use super::*;
    use crate::views::payload_editor::side_view::test_body;
    use egui::pos2;

    #[test]
    fn lateral_and_vertical_points_per_metre_are_equal_at_every_zoom() {
        let mut view = test_body();
        view.widths = vec![4.0; view.stations.len()];
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(900.0, 380.0));
        for zoom in [0.05, 0.3, 1.0, 2.5, MAX_ZOOM] {
            let camera = Camera {
                zoom,
                ..Camera::default()
            };
            let frame = Frame::new(rect, &view, 20.0, camera);
            let per_y = frame.to_screen(1.0, 0.0).x - frame.to_screen(0.0, 0.0).x;
            let per_z = frame.to_screen(0.0, 0.0).y - frame.to_screen(0.0, 1.0).y;
            assert!(
                (per_y - per_z).abs() < 1e-3,
                "zoom {zoom}: {per_y} vs {per_z}"
            );
        }
    }
}

#[cfg(test)]
mod framing_tests {
    use super::*;
    use crate::views::payload_editor::front_camera;
    use crate::views::payload_editor::side_view::test_body;
    use egui::pos2;

    fn rect(width: f32, height: f32) -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(width, height))
    }

    /// An unmoved camera is framed again for a new canvas size (and keeps the
    /// framing it was last given); one the user zoomed or panned keeps its
    /// view; the Fit button's framing starts a fresh, unmoved camera.
    #[test]
    fn the_camera_refits_with_the_canvas_unless_the_user_moved_it() {
        let view = test_body();
        let (small, large) = (rect(700.0, 360.0), rect(1600.0, 700.0));
        let key = |r: Rect| camera::fit_key(r, [0.0, 1.0, 2.0, 3.0, 4.0]);
        // Fuselage framing from a small canvas is repeated on a large one.
        let mut camera = Camera::fuselage(small, &view, 20.0);
        camera.key = key(small);
        camera.sync(large, &view, 20.0, key(large));
        let expected = Camera::fuselage(large, &view, 20.0);
        assert!((camera.zoom - expected.zoom).abs() < 1e-9);
        assert!(!camera.touched && camera.fuselage);
        // Zoomed by hand: the canvas changing does not move the view.
        let mut moved = Camera {
            key: key(small),
            ..Camera::default()
        };
        moved.zoom_about(small, &view, 20.0, small.center(), 2.0);
        assert!(moved.touched);
        let before = moved;
        moved.sync(large, &view, 20.0, key(large));
        assert!((moved.zoom - before.zoom).abs() < 1e-12 && moved.centre == before.centre);
        // Same canvas, same aircraft: nothing happens either way.
        let mut idle = Camera {
            key: key(small),
            ..Camera::default()
        };
        idle.sync(small, &view, 20.0, key(small));
        assert_eq!(idle.zoom, 1.0);
    }

    /// The canvas follows the aircraft's own aspect: tall enough to frame it
    /// at the card's width, never an empty band, and the same width gives
    /// the same height whatever station the section is at.
    #[test]
    fn the_canvas_height_follows_the_aircraft_aspect() {
        let view = test_body();
        let narrow = front_camera::fit_height(&view, 700.0);
        let wide = front_camera::fit_height(&view, 1500.0);
        assert!(wide > narrow, "{narrow} {wide}");
        let [z0, z1] = view.render_z_range();
        let y_half = view.semi_span().max(3.0) * front_camera::WING_TIP_ROOM;
        // The aircraft fills the canvas width: its aspect is width : height.
        let aspect = f64::from(1500.0_f32 - 60.0) / f64::from(wide - 60.0);
        let wanted = 2.0 * y_half / ((z1 - z0) * 1.10);
        assert!(
            (aspect - wanted).abs() / wanted < 0.01,
            "{aspect} vs {wanted}"
        );
    }
}
