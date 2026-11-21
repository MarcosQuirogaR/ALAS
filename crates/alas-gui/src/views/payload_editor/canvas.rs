// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The side view canvas: draws the generated view and turns the mouse into
//! placement and drag requests. It never edits the case; [`Action`] says what
//! the editor should do.

use alas_config::PayloadItemsConfig;
use alas_payload::layout::ItemKind;
use egui::{vec2, Align2, Color32, FontId, Pos2, Rect, Rounding, Sense, Shape, Stroke, Ui};

use super::camera;
#[cfg(test)]
use super::frame::MAX_ZOOM;
pub(super) use super::frame::{Frame, Window};
use super::frame::{MARGIN, VMARGIN};
use super::front_render;
use super::glyphs::{self, faded};
use super::seat_layer;
use super::side_view::{SideView, ViewItem};
use crate::theme::success_color;
use crate::views::tr_label;

/// Where the canvas was drawn last, in the egui context (tests drive the mouse
/// from it).
pub(super) fn canvas_rect_id() -> egui::Id {
    egui::Id::new("payload_editor_canvas_rect")
}

/// Smallest canvas height, points.
pub(super) const MIN_HEIGHT: f32 = 300.0;
/// Smallest side-view canvas, points: room to zoom into a thin body.
const SIDE_MIN_HEIGHT: f32 = 200.0;
/// How near a point mass is hit, in points.
const POINT_HIT: f32 = 9.0;

/// What the user picked on the canvas.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Target {
    Zone(usize),
    PointMass(usize),
}

/// What the mouse asked for this frame.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Action {
    None,
    /// A click at a station and height.
    Click {
        x: f64,
        z: f64,
    },
    /// The press that starts a drag landed on an item.
    DragStart {
        target: Target,
        x: f64,
    },
    /// A click that picked an item, or none.
    Select(Option<Target>),
    /// The pointer is at a station and height while dragging.
    DragTo {
        x: f64,
        z: f64,
    },
    DragEnd,
    /// A click at a lateral position and height in the front view.
    ClickFront {
        y: f64,
        z: f64,
    },
    /// The pointer is at a lateral position and height while dragging in the
    /// front view.
    DragToFront {
        y: f64,
        z: f64,
    },
}

pub(super) fn item_color(ui: &Ui, kind: ItemKind) -> Color32 {
    let visuals = ui.visuals();
    match kind {
        ItemKind::SeatRow => visuals.weak_text_color(),
        ItemKind::Uld | ItemKind::Bag => visuals.warn_fg_color,
        ItemKind::Zone => visuals.hyperlink_color,
        ItemKind::PointMass => success_color(visuals),
        ItemKind::Exit => visuals.error_fg_color,
        _ => visuals.text_color(),
    }
}

fn outline(frame: &Frame, view: &SideView) -> Vec<Pos2> {
    let mut points: Vec<Pos2> = view
        .stations
        .iter()
        .zip(&view.top)
        .map(|(&x, &z)| frame.to_screen(x, z))
        .collect();
    points.extend(
        view.stations
            .iter()
            .zip(&view.bottom)
            .rev()
            .map(|(&x, &z)| frame.to_screen(x, z)),
    );
    points
}

fn band(
    frame: &Frame,
    view: &SideView,
    from: f64,
    to: f64,
    upper: &[f64],
    lower: &[f64],
) -> Vec<Pos2> {
    let samples: Vec<(f64, f64, f64)> = view
        .stations
        .iter()
        .zip(upper.iter().zip(lower))
        .filter(|(&x, _)| x >= from && x <= to)
        .map(|(&x, (&u, &l))| (x, u, l))
        .collect();
    let mut points: Vec<Pos2> = samples
        .iter()
        .map(|&(x, u, _)| frame.to_screen(x, u))
        .collect();
    points.extend(samples.iter().rev().map(|&(x, _, l)| frame.to_screen(x, l)));
    points
}

fn built_rect(frame: &Frame, item: &ViewItem) -> Rect {
    Rect::from_two_pos(
        frame.to_screen(item.x - item.length / 2.0, item.z + item.height / 2.0),
        frame.to_screen(item.x + item.length / 2.0, item.z - item.height / 2.0),
    )
}

/// The screen rectangle that bounds a zone: its stations over the deck's
/// floor and ceiling along its whole length.
pub(super) fn zone_rect(frame: &Frame, view: &SideView, start: f64, end: f64) -> Rect {
    Rect::from_points(&glyphs::zone_points(frame, view, start, end))
}

/// Draw the view, the declared items and the centre of gravity; return the
/// mouse action.
#[allow(clippy::too_many_arguments)]
pub(super) fn show(
    ui: &mut Ui,
    view: &SideView,
    items: &PayloadItemsConfig,
    selected: Option<&Target>,
    dragging: bool,
    placing: bool,
    landing: Option<(f64, f64)>,
    window: &mut Window,
    height: f32,
    theme: &'static str,
) -> Action {
    // True proportions: the canvas is as tall as the fitted body needs (with
    // room for the scale), within the page's room.
    let [x0, x1] = view.frame_x_range();
    let [z0, z1] = view.render_z_range();
    let fit_x = (ui.available_width() - 2.0 * MARGIN).max(1.0) / (x1 - x0).max(1.0e-6) as f32;
    let needed = (z1 - z0) as f32 * fit_x + 2.0 * VMARGIN;
    let height = needed.clamp(SIDE_MIN_HEIGHT, height.max(SIDE_MIN_HEIGHT));
    // The framing button sits in a strip above the canvas, not over the body.
    let buttons = [camera::Overlay {
        label: tr_label("Fit"),
        tooltip: &tr_label("Mouse wheel zooms, drag pans"),
        selected: false,
    }];
    if camera::header(ui, |ui| seat_layer::legend(ui, view), &buttons).is_some() {
        *window = Window::default();
    }
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width().max(1.0), height),
        Sense::click_and_drag(),
    );
    window.sync(camera::fit_key(rect, [x0, x1, z0, z1, view.semi_span()]));
    navigate(ui, &response, rect, view, items, dragging, window);
    let frame = Frame::windowed(rect, view, *window);
    ui.ctx()
        .data_mut(|data| data.insert_temp(canvas_rect_id(), rect));
    let painter = ui.painter_at(rect);
    let weak = ui.visuals().weak_text_color();
    let stroke = Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.fg_stroke.color);
    // The aircraft as the engine draws it, at the very scale and origin of
    // the cabin cutaway drawn over it.
    front_render::paint(
        ui,
        &painter,
        rect,
        front_render::Placement {
            origin: frame.to_screen(0.0, 0.0),
            scale: frame.sx,
        },
        view,
        theme,
        front_render::Projection::Side,
    );
    paint_body(ui, &painter, &frame, view, stroke);
    // The seats are a render too (cached, registered with the airframe's);
    // only a view with no airframe draws them as glyphs.
    if view.airframe.is_some() {
        front_render::paint_seats(
            ui,
            &painter,
            rect,
            front_render::Placement {
                origin: frame.to_screen(0.0, 0.0),
                scale: frame.sx,
            },
            view,
            theme,
        );
        glyphs::paint_min_seats(&painter, &frame, view, item_color(ui, ItemKind::SeatRow));
    } else {
        glyphs::paint_seat_rows(&painter, &frame, view, item_color(ui, ItemKind::SeatRow));
    }
    for item in &view.items {
        if item.declared.is_empty() && item.mass > 0.0 && item.kind != ItemKind::SeatRow {
            let color = item_color(ui, item.kind);
            let drawn = built_rect(&frame, item);
            painter.rect(
                drawn,
                Rounding::same(1.0),
                faded(color, 80),
                Stroke::new(0.6_f32, faded(color, 190)),
            );
        }
    }
    paint_declared(ui, &painter, &frame, view, items, selected);
    if let Some((start, end)) = landing {
        let color = item_color(ui, ItemKind::Zone);
        glyphs::paint_landing(&painter, &frame, view, start, end, color);
    }
    glyphs::paint_cg(&painter, &frame, view, weak, ui.visuals().extreme_bg_color);
    glyphs::paint_scale(&painter, &frame, view, weak);
    painter.rect_stroke(
        rect,
        Rounding::same(camera::CANVAS_RADIUS),
        Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
    if placing {
        ui.output_mut(|output| output.cursor_icon = egui::CursorIcon::Crosshair);
    }
    interact(&response, &frame, view, items, dragging, placing)
}

/// Wheel zoom about the cursor and drag-pan of the empty canvas.
fn navigate(
    ui: &Ui,
    response: &egui::Response,
    rect: Rect,
    view: &SideView,
    items: &PayloadItemsConfig,
    dragging: bool,
    window: &mut Window,
) {
    let frame = Frame::windowed(rect, view, *window);
    let on_item = response.drag_started()
        && response
            .ctx
            .input(|input| input.pointer.press_origin())
            .is_some_and(|origin| hit(&frame, view, items, origin).is_some());
    if let Some(delta) = camera::pan(ui, response, on_item, dragging) {
        window.pan_by(view, rect, delta);
    }
    if let Some((cursor, factor)) = camera::wheel(ui, response) {
        window.zoom_about(view, rect, cursor, factor);
    }
}

fn paint_body(ui: &Ui, painter: &egui::Painter, frame: &Frame, view: &SideView, stroke: Stroke) {
    // With the airframe behind it the body is a cutaway: dark enough to read
    // the cabin on, light enough to show the aircraft through.
    let alpha = if view.airframe.is_some() { 205 } else { 255 };
    painter.add(Shape::convex_polygon(
        outline(frame, view),
        faded(ui.visuals().extreme_bg_color, alpha),
        stroke,
    ));
    let line = |series: &[f64], from: f64, to: f64| -> Vec<Pos2> {
        view.stations
            .iter()
            .zip(series)
            .filter(|(&x, _)| x >= from && x <= to)
            .map(|(&x, &z)| frame.to_screen(x, z))
            .collect()
    };
    let [cabin_from, cabin_to] = view.cabin_x;
    painter.add(Shape::line(
        line(&view.main_floor, cabin_from, cabin_to),
        Stroke::new(1.5_f32, ui.visuals().text_color()),
    ));
    for [from, to] in &view.reserved {
        let mid = f64::midpoint(*from, *to);
        painter.rect_filled(
            Rect::from_two_pos(
                frame.to_screen(*from, view.ceiling_z(mid)),
                frame.to_screen(*to, view.floor_z(mid)),
            ),
            Rounding::ZERO,
            faded(ui.visuals().weak_text_color(), 25),
        );
    }
    for door in &view.doors {
        let at = frame.to_screen(*door, view.floor_z(*door));
        painter.line_segment(
            [at - vec2(0.0, 7.0), at + vec2(0.0, 2.0)],
            Stroke::new(2.0_f32, ui.visuals().error_fg_color),
        );
    }
    if view.has_hold {
        let hold = Stroke::new(0.8_f32, ui.visuals().weak_text_color());
        let gap = view.hold_gap_x.unwrap_or([cabin_to, cabin_to]);
        let spans: Vec<(f64, f64)> = if view.declared_holds {
            view.hold_spans
                .iter()
                .map(|span| (span[0], span[1]))
                .collect()
        } else {
            vec![(cabin_from, gap[0]), (gap[1], cabin_to)]
        };
        for (from, to) in spans {
            if to > from {
                painter.add(Shape::closed_line(
                    band(frame, view, from, to, &view.hold_ceiling, &view.hold_floor),
                    hold,
                ));
            }
        }
    }
}

fn paint_declared(
    ui: &Ui,
    painter: &egui::Painter,
    frame: &Frame,
    view: &SideView,
    items: &PayloadItemsConfig,
    selected: Option<&Target>,
) {
    for (index, zone) in items.zones.iter().enumerate() {
        let color = item_color(ui, ItemKind::Zone);
        let picked = selected == Some(&Target::Zone(index));
        glyphs::paint_zone(
            painter,
            frame,
            view,
            zone,
            color,
            picked,
            ui.visuals().text_color(),
        );
    }
    paint_ghosts(ui, painter, frame, view, items);
    for (index, mass) in items.point_masses.iter().enumerate() {
        let center = frame.to_screen(mass.x_m, mass.z_m);
        let color = item_color(ui, ItemKind::PointMass);
        let picked = selected == Some(&Target::PointMass(index));
        let r = if picked { 8.5 } else { 7.0 };
        painter.add(Shape::convex_polygon(
            vec![
                center + vec2(0.0, -r),
                center + vec2(r, 0.0),
                center + vec2(0.0, r),
                center + vec2(-r, 0.0),
            ],
            if mass.releasable {
                color
            } else {
                faded(color, 140)
            },
            Stroke::new(1.0_f32, color),
        ));
        painter.text(
            center + vec2(0.0, -r - 2.0),
            Align2::CENTER_BOTTOM,
            format!("{} \u{b7} {:.0} kg", mass.name, mass.mass_kg),
            FontId::proportional(11.0),
            ui.visuals().text_color(),
        );
    }
}

/// Where the payload auto-balance puts the movable items: a dashed outline at
/// the balanced station, drawn where it differs from the declared one.
fn paint_ghosts(
    ui: &Ui,
    painter: &egui::Painter,
    frame: &Frame,
    view: &SideView,
    items: &PayloadItemsConfig,
) {
    let color = ui.visuals().weak_text_color();
    let stroke = Stroke::new(1.0_f32, color);
    for mass in items.point_masses.iter().filter(|mass| mass.movable) {
        let Some(balanced) = view.items.iter().find(|item| item.declared == mass.id) else {
            continue;
        };
        if (balanced.x - mass.x_m).abs() > 0.05 {
            let at = frame.to_screen(balanced.x, mass.z_m);
            painter.circle_stroke(at, 6.0, stroke);
        }
    }
    for zone in items.zones.iter().filter(|zone| zone.movable) {
        let Some(balanced) = view.items.iter().find(|item| item.declared == zone.name) else {
            continue;
        };
        let centre = f64::midpoint(zone.x_start_m, zone.x_end_m);
        if (balanced.x - centre).abs() > 0.05 {
            let half = zone.length_m() / 2.0;
            let rect = zone_rect(frame, view, balanced.x - half, balanced.x + half);
            painter.rect_stroke(rect, Rounding::same(2.0), stroke);
        }
    }
}

fn interact(
    response: &egui::Response,
    frame: &Frame,
    view: &SideView,
    items: &PayloadItemsConfig,
    dragging: bool,
    placing: bool,
) -> Action {
    if response.drag_started() {
        if let Some(origin) = response.ctx.input(|input| input.pointer.press_origin()) {
            if let Some(target) = hit(frame, view, items, origin) {
                let (x, _) = frame.to_world(origin);
                return Action::DragStart { target, x };
            }
        }
        return Action::None;
    }
    if dragging {
        if response.drag_stopped() {
            return Action::DragEnd;
        }
        if let Some(pointer) = response.interact_pointer_pos() {
            let (x, z) = frame.to_world(pointer);
            return Action::DragTo { x, z };
        }
        return Action::None;
    }
    if response.clicked() {
        if let Some(pointer) = response.interact_pointer_pos() {
            if placing {
                let (x, z) = frame.to_world(pointer);
                return Action::Click { x, z };
            }
            return Action::Select(hit(frame, view, items, pointer));
        }
    }
    Action::None
}

/// The declared item under `point`: point masses first (they are small and
/// sit on top of zones).
pub(super) fn hit(
    frame: &Frame,
    view: &SideView,
    items: &PayloadItemsConfig,
    point: Pos2,
) -> Option<Target> {
    let mass = items
        .point_masses
        .iter()
        .enumerate()
        .map(|(index, mass)| (index, frame.to_screen(mass.x_m, mass.z_m).distance(point)))
        .filter(|(_, distance)| *distance <= POINT_HIT)
        .min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((index, _)) = mass {
        return Some(Target::PointMass(index));
    }
    items
        .zones
        .iter()
        .position(|zone| zone_rect(frame, view, zone.x_start_m, zone.x_end_m).contains(point))
        .map(Target::Zone)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::payload_editor::{placement, side_view::test_body};
    use egui::pos2;

    fn frame_for(view: &SideView) -> Frame {
        Frame::new(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(900.0, MIN_HEIGHT)),
            view,
        )
    }

    #[test]
    fn the_screen_mapping_round_trips() {
        let view = test_body();
        let frame = frame_for(&view);
        let point = frame.to_screen(21.5, 0.7);
        let (x, z) = frame.to_world(point);
        assert!((x - 21.5).abs() < 1e-4 && (z - 0.7).abs() < 1e-4);
    }

    #[test]
    fn point_masses_win_over_the_zone_they_sit_in() {
        let view = test_body();
        let frame = frame_for(&view);
        let mut items = PayloadItemsConfig::default();
        placement::place_zone(&mut items, &view, 10.0, 1.0).unwrap();
        placement::place_point_mass(&mut items, &view, 11.0, 0.8).unwrap();
        let on_mass = frame.to_screen(11.0, 0.8);
        assert_eq!(
            hit(&frame, &view, &items, on_mass),
            Some(Target::PointMass(0))
        );
        let on_zone = frame.to_screen(12.5, 1.5);
        assert_eq!(hit(&frame, &view, &items, on_zone), Some(Target::Zone(0)));
        let nowhere = frame.to_screen(30.0, 1.0);
        assert_eq!(hit(&frame, &view, &items, nowhere), None);
    }

    #[test]
    fn a_zoomed_window_maps_back_and_returns_to_true_proportions() {
        let view = test_body();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(900.0, 300.0));
        let whole = Frame::windowed(rect, &view, Window::default());
        let zoom = Window {
            zoom: 4.0,
            pan: 0.5,
            centre: None,
            ..Window::default()
        };
        let zoomed = Frame::windowed(rect, &view, zoom);
        let point = zoomed.to_screen(30.0, 0.4);
        let (x, z) = zoomed.to_world(point);
        assert!((x - 30.0).abs() < 1e-4 && (z - 0.4).abs() < 1e-4);
        // One scale on both axes, whole body or zoomed.
        let tall = |frame: Frame| {
            (frame.to_screen(10.0, 0.0).y - frame.to_screen(10.0, 1.0).y).abs()
                / (frame.to_screen(11.0, 0.0).x - frame.to_screen(10.0, 0.0).x).abs()
        };
        assert!((tall(whole) - 1.0).abs() < 0.05, "{}", tall(whole));
        assert!((tall(zoomed) - 1.0).abs() < 0.05, "{}", tall(zoomed));
        let end = Window {
            zoom: 4.0,
            pan: 1.0,
            centre: None,
            ..Window::default()
        };
        let [from, to] = end.span(&view);
        assert!((to - view.x_range()[1]).abs() < 1e-9 && from > view.x_range()[0]);
    }
}

#[cfg(test)]
mod nav_tests {
    use super::*;
    use crate::views::payload_editor::side_view::test_body;
    use egui::pos2;

    fn rect() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 420.0))
    }

    #[test]
    fn the_default_view_fills_the_width_and_both_axes_share_one_scale_at_every_zoom() {
        let view = test_body();
        let frame = Frame::windowed(rect(), &view, Window::default());
        let [x0, x1] = view.x_range();
        let width = frame.to_screen(x1, 0.0).x - frame.to_screen(x0, 0.0).x;
        assert!(width > 800.0 - 2.0 * MARGIN - 1.0, "{width}");
        for zoom in [1.0, 1.7, 4.0, 11.0, MAX_ZOOM] {
            let mut window = Window::default();
            window.zoom_about(&view, rect(), rect().center(), zoom);
            let frame = Frame::windowed(rect(), &view, window);
            let per_x = frame.to_screen(11.0, 0.0).x - frame.to_screen(10.0, 0.0).x;
            let per_z = frame.to_screen(10.0, 0.0).y - frame.to_screen(10.0, 1.0).y;
            assert!(
                (per_x - per_z).abs() < 1e-3,
                "zoom {zoom}: {per_x} vs {per_z}"
            );
        }
    }

    #[test]
    fn zooming_at_the_corners_never_takes_the_body_out_of_the_view() {
        let view = test_body();
        let canvas = rect();
        let corners = [
            canvas.left_top(),
            canvas.right_top(),
            canvas.left_bottom(),
            canvas.right_bottom(),
            canvas.center(),
        ];
        for cursor in corners {
            let mut window = Window::default();
            for factor in [2.0, 2.0, 2.0, 2.0, 2.0] {
                window.zoom_about(&view, canvas, cursor, factor);
                let frame = Frame::windowed(canvas, &view, window);
                let [from, to] = window.span(&view);
                let station = f64::midpoint(from, to);
                let (low, high) = (view.at(&view.bottom, station), view.at(&view.top, station));
                let (top, bottom) = (
                    frame.to_screen(station, high).y,
                    frame.to_screen(station, low).y,
                );
                assert!(
                    bottom >= canvas.top() && top <= canvas.bottom(),
                    "cursor {cursor:?} zoom {}: body at y {top}..{bottom}",
                    window.zoom
                );
            }
        }
    }

    #[test]
    fn the_wheel_zooms_about_the_cursor_keeping_the_point_under_it() {
        let view = test_body();
        let cursor = pos2(300.0, 160.0);
        let mut window = Window::default();
        let before = Frame::windowed(rect(), &view, window).to_world(cursor);
        window.zoom_about(&view, rect(), cursor, 2.0);
        assert!((window.zoom - 2.0).abs() < 1e-9);
        let after = Frame::windowed(rect(), &view, window).to_world(cursor);
        // A cursor over empty sky anchors on the body at that station
        // (`zoom_about`), so the height kept is the cursor's clamped to it.
        let (low, high) = (
            view.at(&view.bottom, before.0),
            view.at(&view.top, before.0),
        );
        let anchored = before.1.clamp(low, high);
        assert!(
            (before.0 - after.0).abs() < 1e-3 && (anchored - after.1).abs() < 1e-3,
            "{before:?} -> {after:?} (anchored height {anchored})"
        );
        window.zoom_about(&view, rect(), cursor, 1.0e6);
        assert!((window.zoom - MAX_ZOOM).abs() < 1e-9);
        window.zoom_about(&view, rect(), cursor, 1.0e-9);
        assert!(
            (window.zoom - 1.0).abs() < 1e-9,
            "never below the whole body"
        );
        let [from, to] = window.span(&view);
        assert_eq!([from, to], view.x_range());
    }

    #[test]
    fn a_drag_pans_by_the_pointer_travel_and_stays_on_the_body() {
        let view = test_body();
        let mut window = Window::default();
        window.zoom_about(&view, rect(), rect().center(), 4.0);
        let anchor = Frame::windowed(rect(), &view, window).to_screen(30.0, 0.5);
        window.pan_by(&view, rect(), vec2(-40.0, 6.0));
        let moved = Frame::windowed(rect(), &view, window).to_screen(30.0, 0.5) - anchor;
        assert!((moved.x + 40.0).abs() < 0.5, "{moved:?}");
        for _ in 0..200 {
            window.pan_by(&view, rect(), vec2(500.0, 0.0));
        }
        let [from, _] = window.span(&view);
        assert!(
            (from - view.x_range()[0]).abs() < 1e-9,
            "stopped at the nose"
        );
    }
}

#[cfg(test)]
mod refit_tests {
    use super::*;

    /// A window nobody moved is fitted again when the canvas or the aircraft
    /// changes; one the user zoomed keeps its zoom.
    #[test]
    fn an_unmoved_window_refits_and_a_zoomed_one_stays() {
        let (a, b) = (1_u64, 2_u64);
        let mut window = Window::default();
        window.sync(a);
        window.zoom = 3.0; // as a wheel would, but without the touch
        window.sync(b);
        assert_eq!(window.zoom, 1.0, "refitted");
        assert_eq!(window.key, b);
        window.touched = true;
        window.zoom = 3.0;
        window.sync(a);
        assert_eq!(window.zoom, 3.0, "kept");
        window.sync(a);
        assert_eq!(window.zoom, 3.0);
    }
}
