// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the side view draws that is more than a rectangle: seats as seat
//! glyphs at their real pitch, and zones that follow the floor and the
//! ceiling along their whole length.

use alas_config::FloorZoneConfig;
use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Rounding, Shape, Stroke};

use super::canvas::Frame;
use super::side_view::{SideView, ViewItem};
use crate::views::tr_fields;

pub(super) fn faded(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

/// `text` if it fits `max_width` points in `font`, else its short form, or `None`
/// when that does not fit either: a label never overflows its box.
pub(super) fn fit_text(
    painter: &Painter,
    text: &str,
    font: &FontId,
    max_width: f32,
) -> Option<String> {
    let width = |candidate: &str| {
        painter
            .layout_no_wrap(candidate.to_owned(), font.clone(), Color32::WHITE)
            .size()
            .x
    };
    if width(text) <= max_width {
        return Some(text.to_owned());
    }
    // Never an ellipsised word: the short form (the initials of the words and
    // any number, "Zone 2" -> "Z2"), else no label.
    let short: String = text
        .split_whitespace()
        .map(|word| {
            if word.chars().all(|c| c.is_ascii_digit()) {
                word.to_owned()
            } else {
                word.chars().take(1).collect()
            }
        })
        .collect();
    (!short.is_empty() && width(&short) <= max_width).then_some(short)
}

/// The outline of the stretch `[start, end]` of the main deck: the ceiling
/// along the whole stretch out, the floor back, so a zone never leaves the
/// fuselage where the section narrows or the crown falls away.
pub(super) fn zone_points(frame: &Frame, view: &SideView, start: f64, end: f64) -> Vec<Pos2> {
    let mut stations = vec![start];
    stations.extend(
        view.stations
            .iter()
            .copied()
            .filter(|x| *x > start && *x < end),
    );
    stations.push(end);
    let mut points: Vec<Pos2> = stations
        .iter()
        .map(|&x| frame.to_screen(x, view.ceiling_z(x)))
        .collect();
    points.extend(
        stations
            .iter()
            .rev()
            .map(|&x| frame.to_screen(x, view.floor_z(x))),
    );
    points
}

/// Draw a declared zone over its floor-to-ceiling outline.
pub(super) fn paint_zone(
    painter: &Painter,
    frame: &Frame,
    view: &SideView,
    zone: &FloorZoneConfig,
    color: Color32,
    picked: bool,
    text: Color32,
) {
    let points = zone_points(frame, view, zone.x_start_m, zone.x_end_m);
    let centre = Rect::from_points(&points).center();
    painter.add(Shape::convex_polygon(
        points,
        faded(color, if picked { 110 } else { 60 }),
        Stroke::new(if picked { 2.0_f32 } else { 1.0_f32 }, color),
    ));
    let width = Rect::from_points(&zone_points(frame, view, zone.x_start_m, zone.x_end_m)).width();
    let font = FontId::proportional(11.5);
    if let Some(label) = fit_text(painter, &zone.name, &font, width - 6.0) {
        painter.text(centre, Align2::CENTER_CENTER, label, font, text);
    }
}

/// A dashed outline of the stretch `[start, end]`: where a dragged zone would
/// land if released.
pub(super) fn paint_landing(
    painter: &Painter,
    frame: &Frame,
    view: &SideView,
    start: f64,
    end: f64,
    color: Color32,
) {
    let mut points = zone_points(frame, view, start, end);
    if let Some(first) = points.first().copied() {
        points.push(first);
    }
    painter.extend(Shape::dashed_line(
        &points,
        Stroke::new(1.5_f32, color),
        5.0,
        3.0,
    ));
}

/// Draw one seat row as a seat in profile: cushion, backrest at the aft end
/// and a pedestal, with the proportions the 3-D cabin figure uses, centred on
/// the row's station. Rows sit at the layout's own stations, so the spacing on
/// screen is the real seat pitch.
pub(super) fn paint_seat(painter: &Painter, frame: &Frame, item: &ViewItem, color: Color32) {
    let floor = item.z - item.height * 0.5;
    let cushion_length = (item.length * 0.54).max(0.22);
    let cushion_height = item.height.clamp(0.18, 0.48) * 0.30;
    let cushion_top = floor + item.height.min(0.48);
    let back_height = (item.height * 0.68).max(alas_report::families::geometry::SEAT_BACK_HEIGHT_M);
    let back_length = (item.length * 0.13).clamp(0.08, 0.18);
    let back_x = item.x + cushion_length * 0.42;
    let between = |x0: f64, z0: f64, x1: f64, z1: f64| {
        Rect::from_two_pos(frame.to_screen(x0, z1), frame.to_screen(x1, z0))
    };
    let outline = Stroke::new(0.6_f32, faded(color, 220));
    painter.rect(
        between(
            item.x - cushion_length * 0.5,
            cushion_top - cushion_height,
            item.x + cushion_length * 0.5,
            cushion_top,
        ),
        Rounding::same(1.0),
        faded(color, 150),
        outline,
    );
    painter.rect(
        between(
            back_x - back_length * 0.5,
            floor,
            back_x + back_length * 0.5,
            floor + back_height,
        ),
        Rounding::same(1.0),
        faded(color, 190),
        outline,
    );
    let foot = frame.to_screen(item.x, floor);
    let seat = frame.to_screen(item.x, cushion_top - cushion_height);
    painter.line_segment([seat, foot], Stroke::new(1.0_f32, faded(color, 160)));
    painter.line_segment(
        [foot - vec2(2.0, 0.0), foot + vec2(2.0, 0.0)],
        Stroke::new(1.0_f32, faded(color, 160)),
    );
}

/// Pixels of row pitch from which a seat is drawn as a seat in profile; closer
/// rows are drawn as one block per run, so they stay readable.
const GLYPH_MIN_PITCH_PX: f32 = 12.0;

/// Draw the built seat rows: seat glyphs when a row is wide enough on screen,
/// otherwise one block per run of rows on the same deck.
pub(super) fn paint_seat_rows(painter: &Painter, frame: &Frame, view: &SideView, color: Color32) {
    let mut rows: Vec<&ViewItem> = view
        .items
        .iter()
        .filter(|item| {
            item.kind == alas_payload::layout::ItemKind::SeatRow
                && item.declared.is_empty()
                && item.mass > 0.0
        })
        .collect();
    rows.sort_by(|a, b| a.x.total_cmp(&b.x));
    let mut run: Vec<&ViewItem> = Vec::new();
    for item in rows {
        if f64::from(frame.pixels_per_metre()) * item.length >= f64::from(GLYPH_MIN_PITCH_PX) {
            paint_seat(painter, frame, item, color);
            continue;
        }
        let continues = run.last().is_some_and(|last| {
            (item.x - last.x) <= 1.6 * last.length.max(item.length) && (item.z - last.z).abs() < 0.2
        });
        if !continues {
            paint_block(painter, frame, &run, color);
            run.clear();
        }
        run.push(item);
    }
    paint_block(painter, frame, &run, color);
}

/// One block over a run of seat rows: the seats' footprint from the floor to
/// the top of the backrests.
fn paint_block(painter: &Painter, frame: &Frame, run: &[&ViewItem], color: Color32) {
    let (Some(first), Some(last)) = (run.first(), run.last()) else {
        return;
    };
    let floor = first.z - first.height * 0.5;
    let top =
        floor + (first.height * 0.68).max(alas_report::families::geometry::SEAT_BACK_HEIGHT_M);
    let mut rect = Rect::from_two_pos(
        frame.to_screen(first.x - first.length * 0.5, top),
        frame.to_screen(last.x + last.length * 0.5, floor),
    );
    // Never thinner than a visible bar.
    if rect.height() < 4.0 {
        rect = Rect::from_center_size(rect.center(), vec2(rect.width(), 4.0));
    }
    painter.rect(
        rect,
        Rounding::same(1.5),
        faded(color, 170),
        Stroke::new(0.8_f32, faded(color, 230)),
    );
}

/// `text` on a small backing so it reads over the picture; kept inside the
/// painter's clip rectangle (it flips to the other side of `anchor` when it
/// would leave it).
pub(super) fn chip(painter: &Painter, anchor: Pos2, text: String, color: Color32, bg: Color32) {
    let galley = painter.layout_no_wrap(text, FontId::proportional(11.5), color);
    let clip = painter.clip_rect();
    let size = galley.size() + vec2(8.0, 4.0);
    let mut left = anchor.x;
    if left + size.x > clip.right() - 2.0 {
        left = anchor.x - size.x;
    }
    left = left.max(clip.left() + 2.0);
    let top = (anchor.y - size.y).max(clip.top() + 2.0);
    let rect = Rect::from_min_size(pos2(left, top), size);
    painter.rect_filled(rect, Rounding::same(3.0), faded(bg, 225));
    painter.galley(rect.min + vec2(4.0, 2.0), galley, color);
}

/// Length of the leader from a centre-of-gravity marker to its label, points.
const CG_LEADER: f32 = 12.0;

/// The centre-of-gravity markers and the balance target.
pub(super) fn paint_cg(
    painter: &egui::Painter,
    frame: &Frame,
    view: &SideView,
    weak: Color32,
    bg: Color32,
) {
    let [z0, z1] = view.z_range();
    let marker = |x: f64, color: Color32, inset: f64, label: String| {
        let top = frame.to_screen(x, z1 - inset);
        let bottom = frame.to_screen(x, z0 + inset);
        painter.line_segment([top, bottom], Stroke::new(1.5_f32, color));
        // The label floats above the airframe on a leader, in the canvas, clear of the body and
        // of the fin; only where the canvas has no such place does it sit on the old tip.
        let galley = painter.layout_no_wrap(label.clone(), FontId::proportional(11.5), weak);
        let size = galley.size() + vec2(8.0, 4.0);
        match super::cg_label::place(frame, view, painter.clip_rect(), x, size) {
            Some(placed) => {
                painter.line_segment(placed.leader, Stroke::new(1.0_f32, faded(color, 200)));
                painter.rect_filled(placed.rect, Rounding::same(3.0), faded(bg, 225));
                painter.galley(placed.rect.min + vec2(4.0, 2.0), galley, weak);
            }
            None => {
                let tip = top - vec2(0.0, CG_LEADER);
                painter.line_segment([top, tip], Stroke::new(1.0_f32, faded(color, 200)));
                chip(painter, tip + vec2(-4.0, -1.0), label, weak, bg);
            }
        }
    };
    let pct = |x: f64| format!("{:.1}", view.pct_mac(x));
    if let Some(zero_fuel) = view.zero_fuel_cg {
        marker(
            zero_fuel,
            Color32::from_rgb(220, 90, 90),
            0.0,
            tr_fields(
                if view.estimate {
                    "Zero-fuel CG (estimate): {pct} % MAC"
                } else {
                    "Zero-fuel CG: {pct} % MAC"
                },
                &[("pct", pct(zero_fuel))],
            ),
        );
        let payload = frame.to_screen(view.cg_x, z0);
        painter.line_segment(
            [payload, payload - vec2(0.0, 10.0)],
            Stroke::new(2.0_f32, Color32::from_rgb(90, 150, 230)),
        );
    } else {
        marker(
            view.cg_x,
            Color32::from_rgb(90, 150, 230),
            0.0,
            tr_fields("Payload CG: {pct} % MAC", &[("pct", pct(view.cg_x))]),
        );
    }
    if let Some(balance) = view.balance {
        let target = frame.to_screen(balance.target_x, z1);
        painter.circle_stroke(target, 4.0, Stroke::new(1.0_f32, weak));
    }
}

/// Room kept between a scale label and the plot's edge, points.
const SCALE_PAD: f32 = 6.0;

/// The station scale along the bottom edge of the canvas, clear of the aircraft. A label is
/// centred on its station but never closer than [`SCALE_PAD`] to the plot's edges.
pub(super) fn paint_scale(painter: &egui::Painter, frame: &Frame, view: &SideView, weak: Color32) {
    let [x0, x1] = view.frame_x_range();
    let clip = painter.clip_rect();
    let bottom = clip.bottom() - 4.0;
    let font = FontId::proportional(11.0);
    // The unit label owns the left end of the scale; tick labels keep clear of it.
    let unit_right = clip.left()
        + SCALE_PAD
        + painter
            .layout_no_wrap("x [m]".to_owned(), font.clone(), weak)
            .size()
            .x
        + SCALE_PAD;
    let mut tick = (x0 / 5.0).ceil() * 5.0;
    while tick <= x1 {
        let x = frame.to_screen(tick, 0.0).x;
        if clip.x_range().contains(x) {
            let label = format!("{tick:.0}");
            let half = painter
                .layout_no_wrap(label.clone(), font.clone(), weak)
                .size()
                .x
                / 2.0;
            let x = x.clamp(
                clip.left() + SCALE_PAD + half,
                (clip.right() - SCALE_PAD - half).max(clip.left() + SCALE_PAD + half),
            );
            if x - half >= unit_right {
                painter.text(
                    pos2(x, bottom),
                    Align2::CENTER_BOTTOM,
                    label,
                    font.clone(),
                    weak,
                );
            }
        }
        tick += 5.0;
    }
    // The unit of the ticks, at the left end of the scale.
    painter.text(
        pos2(clip.left() + SCALE_PAD, bottom),
        Align2::LEFT_BOTTOM,
        "x [m]",
        font,
        weak,
    );
}

/// Smallest a seat is drawn, points: a seat that the scale would shrink below this stays a
/// readable glyph (width x height).
const MIN_SEAT_PX: (f32, f32) = (4.0, 8.0);

/// Over the seats' render: a glyph of at least [`MIN_SEAT_PX`] for each seat row whose true-size
/// seat is drawn smaller than that, so the seats stay readable at every scale without zooming.
pub(super) fn paint_min_seats(painter: &Painter, frame: &Frame, view: &SideView, color: Color32) {
    let px = f64::from(frame.pixels_per_metre());
    for item in view.items.iter().filter(|item| {
        item.kind == alas_payload::layout::ItemKind::SeatRow
            && item.declared.is_empty()
            && item.mass > 0.0
    }) {
        let depth = (item.length * 0.54).max(0.22) * px;
        let back =
            (item.height * 0.68).max(alas_report::families::geometry::SEAT_BACK_HEIGHT_M) * px;
        if depth >= f64::from(MIN_SEAT_PX.0) && back >= f64::from(MIN_SEAT_PX.1) {
            continue;
        }
        let color = item.seat.as_ref().map_or(color, |seat| {
            let class = alas_report::families::geometry::seat_class_color(seat.cls);
            Color32::from_rgb(class.r, class.g, class.b)
        });
        let floor = frame.to_screen(item.x, item.z - item.height * 0.5);
        let rect = Rect::from_min_max(
            pos2(
                floor.x - MIN_SEAT_PX.0 / 2.0,
                floor.y - MIN_SEAT_PX.1.max(back as f32),
            ),
            pos2(floor.x + MIN_SEAT_PX.0 / 2.0, floor.y),
        );
        painter.rect(
            rect,
            Rounding::same(1.0),
            faded(color, 170),
            Stroke::new(0.6_f32, faded(color, 230)),
        );
    }
}
