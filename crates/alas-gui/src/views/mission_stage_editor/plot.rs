// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The mission profile plot beside the stage list: altitude in ft against
//! mission progress, in the colours of the live mission profile (climb in the
//! accent colour, cruise in the success colour, descent in the warning
//! colour). The selected stage is thickened; mass events are diamonds on the
//! marker at the end of their stage.
//!
//! Every phase boundary carries a numbered marker (1 to N+1, see
//! `plot_markers`), so short phases stay apart; the plot names nothing under
//! the curve. Hovering a segment or a marker highlights the segment and says
//! what it is (`plot_hover`), and the numbered list below does the same for its
//! entries. The reserve program sits on a light band labelled once and is drawn
//! thin and grey; while the flown mission is computed the whole curve is an
//! estimate: dashed and muted, and said so under the axis.

use alas_config::EventTime;
use egui::{vec2, Align2, Color32, FontId, Pos2, Rect, Rounding, Sense, Shape, Stroke, Ui};

use super::model::{ProfileSegment, Trend};
use super::plot_axis::{draw_axis, XMap};
use super::plot_hover::{self, Facts};
use super::plot_markers::{self, Marker};
use super::plot_style::{small_font, trend_color};
use crate::views::tr_label;

const GRID_FT: f64 = 5_000.0;
/// Shortest and tallest curve area, pt; the plot follows the room the page has
/// between the two.
const MIN_HEIGHT: f32 = 220.0;
const MAX_HEIGHT: f32 = 640.0;
/// Space under the plot: x tick labels and the axis title.
const TICKS_H: f32 = 24.0;
const TITLE_H: f32 = 26.0;
/// Room left and right of the plot (altitude labels) and above it, pt.
pub(super) const LEFT: f32 = 62.0;
pub(super) const RIGHT: f32 = 16.0;
const TOP: f32 = 28.0;
/// Height under the curve area, pt.
pub(super) const BELOW: f32 = TICKS_H + TITLE_H;
/// How near the pointer must be to a segment to hover it, pt.
const HOVER_REACH: f32 = 8.0;
/// How near a click must be to a segment to select its stage, pt.
const CLICK_REACH: f32 = 14.0;

/// The least a whole plot (curve and axis) is tall, pt.
pub(super) fn min_total() -> f32 {
    MIN_HEIGHT + TOP + BELOW
}

/// The tallest a whole plot is, pt.
pub(super) fn max_total() -> f32 {
    MAX_HEIGHT + TOP + BELOW
}

/// The `(progress, altitude)` corners of a stage: its start and end, and for a
/// stage with a held level (a diversion) the climb to it and the descent from it.
pub(super) fn corners(segment: &ProfileSegment) -> Vec<(f64, f64)> {
    let [from, to] = segment.progress;
    let mut points = vec![(from, segment.altitude_ft[0])];
    if let Some(level) = segment.via_ft {
        let share = (to - from) * 0.2;
        points.push((from + share, level));
        points.push((to - share, level));
    }
    points.push((to, segment.altitude_ft[1]));
    points
}

/// Plot area inside the allocated `rect`, with the axis beneath it.
fn plot_area(rect: Rect) -> Rect {
    Rect::from_min_max(rect.min + vec2(LEFT, TOP), rect.max - vec2(RIGHT, BELOW))
}

/// The part of the plot area the curves and their markers are mapped onto: inset by a marker
/// radius (and a point) at both ends, so the first and the last marker lie wholly inside the plot.
fn data_area(frame: Rect) -> Rect {
    frame.shrink2(vec2(plot_markers::RADIUS + 1.0, 0.0))
}

fn altitude_range(segments: &[ProfileSegment]) -> (f64, f64) {
    let (low, high) = segments
        .iter()
        .flat_map(|segment| corners(segment).into_iter().map(|(_, ft)| ft))
        .fold((f64::MAX, f64::MIN), |(low, high), ft| {
            (low.min(ft), high.max(ft))
        });
    let low = (low.min(0.0) / GRID_FT).floor() * GRID_FT;
    let high = ((high.max(low + GRID_FT)) / GRID_FT).ceil() * GRID_FT;
    (low, high)
}

fn position(plot: Rect, range: (f64, f64), x: &XMap, progress: f64, altitude_ft: f64) -> Pos2 {
    let span = (range.1 - range.0).max(1.0);
    Pos2::new(
        plot.left() + plot.width() * x.fraction(progress),
        plot.bottom() - plot.height() * ((altitude_ft - range.0) / span) as f32,
    )
}

/// What the plot draws.
pub(super) struct Inputs<'a> {
    pub segments: &'a [ProfileSegment],
    /// The stage-list row of each phase (its name for the tooltip).
    pub rows: &'a [String],
    /// What the flight data gives about each phase (empty when it gives nothing).
    pub facts: &'a [Facts],
    pub selected: Option<usize>,
    /// Whether the curve is the estimate shown while the flown mission is computed.
    pub estimate: bool,
    /// Height the page gives the whole plot, pt.
    pub room: f32,
}

/// How the curves are drawn this frame.
#[derive(Clone, Copy)]
struct Look {
    selected: Option<usize>,
    /// The segment (index) highlighted by the hover of the plot or of the list.
    lit: Option<usize>,
    estimate: bool,
}

/// The curve point numbered `k + 1`: the start of segment `k`, and the end of the last segment
/// for the last number.
fn boundary_points(
    plot: Rect,
    range: (f64, f64),
    x: &XMap,
    segments: &[ProfileSegment],
) -> Vec<Pos2> {
    let mut points: Vec<Pos2> = segments
        .iter()
        .map(|segment| {
            let (progress, ft) = corners(segment)[0];
            position(plot, range, x, progress, ft)
        })
        .collect();
    if let Some(last) = segments.last().map(corners) {
        let (progress, ft) = last[last.len() - 1];
        points.push(position(plot, range, x, progress, ft));
    }
    points
}

/// The light band behind the reserve program, labelled once.
fn reserve_band(
    ui: &Ui,
    painter: &egui::Painter,
    (frame, plot, x): (Rect, Rect, &XMap),
    segments: &[ProfileSegment],
    estimate: bool,
) {
    let Some(first) = segments.iter().position(|segment| segment.reserve) else {
        return;
    };
    let left = plot.left() + plot.width() * x.span(first).0;
    let band = Rect::from_min_max(Pos2::new(left, frame.top()), frame.right_bottom());
    painter.rect_filled(
        band,
        Rounding::ZERO,
        ui.visuals().text_color().gamma_multiply(0.07),
    );
    let label = if estimate {
        tr_label("Reserves")
    } else {
        tr_label("Reserves (flown)")
    };
    painter.with_clip_rect(band).text(
        band.left_top() + vec2(5.0, 3.0),
        Align2::LEFT_TOP,
        label,
        small_font(ui),
        ui.visuals().weak_text_color(),
    );
}

/// The segment under `pointer`: the one a marker under it bounds, else the nearest curve.
fn hit_segment(
    (plot, range, x): (Rect, (f64, f64), &XMap),
    segments: &[ProfileSegment],
    markers: &[Marker],
    pointer: Pos2,
) -> Option<usize> {
    let marker = markers
        .iter()
        .position(|marker| (marker.center - pointer).length() <= plot_markers::RADIUS + 1.0);
    match marker {
        Some(k) => Some(k.min(segments.len().saturating_sub(1))),
        None => nearest_segment(plot, range, x, segments, pointer, HOVER_REACH),
    }
}

/// Draw the profile and return the stage clicked, if any.
pub(super) fn show(ui: &mut Ui, inputs: &Inputs) -> Option<usize> {
    let Inputs {
        segments,
        rows,
        facts,
        selected,
        estimate,
        room,
    } = *inputs;
    let width = ui.available_width().max(1.0);
    let height = room.clamp(min_total(), max_total());
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let frame = plot_area(rect);
    let plot = data_area(frame);
    let x = XMap::new(segments, plot.width().max(1.0), &[]);
    let painter = ui.painter_at(rect);
    let weak = ui.visuals().weak_text_color();
    let grid = ui.visuals().widgets.noninteractive.bg_stroke.color;
    let font = small_font(ui);
    let range = altitude_range(segments);
    reserve_band(ui, &painter, (frame, plot, &x), segments, estimate);
    let mut tick = range.0;
    while tick <= range.1 + 1.0 {
        let y = position(plot, range, &x, 0.0, tick).y;
        painter.line_segment(
            [Pos2::new(frame.left(), y), Pos2::new(frame.right(), y)],
            Stroke::new(0.5_f32, grid),
        );
        painter.text(
            Pos2::new(frame.left() - 12.0, y),
            Align2::RIGHT_CENTER,
            alas_units::notation::group_thousands(&format!("{tick:.0}")),
            font.clone(),
            weak,
        );
        tick += GRID_FT;
    }
    let km = plot_hover::boundary_km(facts);
    painter.text(
        Pos2::new(frame.left(), rect.bottom() - 1.0),
        Align2::LEFT_BOTTOM,
        tr_label(if km.is_some() {
            "Distance [km]"
        } else {
            "Mission progress"
        }),
        font.clone(),
        weak,
    );
    if estimate {
        painter.text(
            Pos2::new(frame.right(), rect.bottom() - 1.0),
            Align2::RIGHT_BOTTOM,
            tr_label("Estimate"),
            font.clone(),
            ui.visuals().warn_fg_color,
        );
    }
    painter.text(
        Pos2::new(rect.left() + 2.0, rect.top()),
        Align2::LEFT_TOP,
        tr_label("Altitude [ft]"),
        font.clone(),
        weak,
    );
    draw_axis(ui, &painter, (frame, plot), &x, segments, km.as_deref());
    let points = boundary_points(plot, range, &x, segments);
    let markers = plot_markers::place(&points, (plot.top() - 4.0, plot.bottom()));
    let geo = (plot, range, &x);
    // The segment under the pointer (the plot's own hover) and the one the list hovers.
    let own = response
        .hover_pos()
        .and_then(|pointer| hit_segment(geo, segments, &markers, pointer));
    plot_hover::set_plot(ui.ctx(), own);
    let look = Look {
        selected,
        lit: plot_hover::hovered(ui.ctx()),
        estimate,
    };
    for (index, segment) in segments.iter().enumerate() {
        draw_stage(ui, &painter, geo, (index, segment), look);
    }
    draw_markers(ui, &painter, &markers, segments, look);
    if let Some(index) = own {
        let tip = plot_hover::tip(
            index,
            &segments[index],
            rows.get(index).map_or("", String::as_str),
            facts.get(index),
            estimate,
        );
        plot_hover::show_tip(ui, &tip);
    }
    if !response.clicked() {
        return None;
    }
    let pointer = response.interact_pointer_pos()?;
    hit_segment(geo, segments, &markers, pointer)
        .map(|index| segments[index].stage)
        .or_else(|| nearest_stage(plot, range, &x, segments, pointer))
}

/// The numbered markers, the event diamonds on them and the release marks.
fn draw_markers(
    ui: &Ui,
    painter: &egui::Painter,
    markers: &[Marker],
    segments: &[ProfileSegment],
    look: Look,
) {
    for (k, marker) in markers.iter().enumerate() {
        let segment = &segments[k.min(segments.len() - 1)];
        let lit = look
            .lit
            .is_some_and(|lit| lit == k || k.checked_sub(1) == Some(lit));
        plot_markers::draw(
            ui,
            painter,
            marker,
            k + 1,
            plot_markers::Look {
                color: trend_color(ui, segment.trend),
                lit,
            },
        );
    }
    let badge = vec2(plot_markers::RADIUS * 0.8, -plot_markers::RADIUS * 0.8);
    for (index, segment) in segments.iter().enumerate() {
        if segment.has_events {
            diamond(
                painter,
                markers[index + 1].center + badge,
                ui.visuals().text_color(),
            );
        }
        if let Some(time) = segment.release {
            let at = match time {
                EventTime::Start => markers[index].point,
                EventTime::End => markers[index + 1].point,
            };
            release_marker(painter, at, ui.visuals().warn_fg_color);
        }
    }
}

/// One stage's curve: dashed and muted while it is an estimate, thin and grey for a reserve
/// phase, thick (the rest dimmed) while it is the highlighted one.
fn draw_stage(
    ui: &Ui,
    painter: &egui::Painter,
    (plot, range, x): (Rect, (f64, f64), &XMap),
    (index, segment): (usize, &ProfileSegment),
    look: Look,
) {
    let chosen = look.selected == Some(segment.stage);
    let lit = look.lit == Some(index);
    let dimmed = look.lit.is_some() && !lit && !chosen;
    let width = if chosen {
        5.0_f32
    } else if lit {
        4.5_f32
    } else if segment.reserve {
        1.5_f32
    } else {
        2.5_f32
    };
    // A reserve phase is secondary to the trip: muted grey and thin (solid even when the trip
    // is an estimate, which is dashed); an estimate is fainter.
    let fade = |color: Color32| {
        if dimmed {
            color.gamma_multiply(0.3)
        } else if look.estimate && !chosen && !lit {
            color.gamma_multiply(0.5)
        } else {
            color
        }
    };
    let corners = corners(segment);
    if chosen || lit {
        // A wide translucent band under the selected or hovered segment.
        let halo = if chosen {
            ui.visuals().selection.bg_fill.gamma_multiply(0.8)
        } else {
            ui.visuals().text_color().gamma_multiply(0.18)
        };
        for pair in corners.windows(2) {
            let from = position(plot, range, x, pair[0].0, pair[0].1);
            let to = position(plot, range, x, pair[1].0, pair[1].1);
            painter.line_segment([from, to], Stroke::new(14.0_f32, halo));
        }
    }
    for pair in corners.windows(2) {
        let from = position(plot, range, x, pair[0].0, pair[0].1);
        let to = position(plot, range, x, pair[1].0, pair[1].1);
        let trend = if pair[1].1 > pair[0].1 + 50.0 {
            Trend::Climb
        } else if pair[1].1 < pair[0].1 - 50.0 {
            Trend::Descent
        } else {
            Trend::Level
        };
        let base = if segment.reserve && !chosen && !lit {
            ui.visuals().weak_text_color()
        } else {
            trend_color(ui, trend)
        };
        let stroke = Stroke::new(width, fade(base));
        if look.estimate {
            painter.extend(Shape::dashed_line(&[from, to], stroke, 7.0, 5.0));
        } else {
            painter.line_segment([from, to], stroke);
        }
    }
}

fn diamond(painter: &egui::Painter, center: Pos2, color: Color32) {
    let r = 4.0;
    painter.add(Shape::convex_polygon(
        vec![
            center + vec2(0.0, -r),
            center + vec2(r, 0.0),
            center + vec2(0.0, r),
            center + vec2(-r, 0.0),
        ],
        color,
        Stroke::NONE,
    ));
}

/// The payload-release point: a downward triangle under the profile with a
/// "Release" label.
fn release_marker(painter: &egui::Painter, at: Pos2, color: Color32) {
    let tip = at + vec2(0.0, 26.0);
    painter.add(Shape::convex_polygon(
        vec![tip, tip + vec2(-5.0, -9.0), tip + vec2(5.0, -9.0)],
        color,
        Stroke::NONE,
    ));
    painter.text(
        tip + vec2(0.0, 2.0),
        Align2::CENTER_TOP,
        tr_label("Release"),
        FontId::proportional(12.0),
        color,
    );
}

/// The index (in `segments`) of the segment whose drawn curve is closest to `pointer`, within
/// `reach` points.
pub(super) fn nearest_segment(
    plot: Rect,
    range: (f64, f64),
    x: &XMap,
    segments: &[ProfileSegment],
    pointer: Pos2,
    reach: f32,
) -> Option<usize> {
    segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            let nearest = corners(segment)
                .windows(2)
                .map(|pair| {
                    let from = position(plot, range, x, pair[0].0, pair[0].1);
                    let to = position(plot, range, x, pair[1].0, pair[1].1);
                    distance_to_segment(pointer, from, to)
                })
                .fold(f32::MAX, f32::min);
            (index, nearest)
        })
        .filter(|(_, distance)| *distance <= reach)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| index)
}

/// The stage whose drawn segment is closest to `pointer`, within 14 points.
pub(super) fn nearest_stage(
    plot: Rect,
    range: (f64, f64),
    x: &XMap,
    segments: &[ProfileSegment],
    pointer: Pos2,
) -> Option<usize> {
    nearest_segment(plot, range, x, segments, pointer, CLICK_REACH)
        .map(|index| segments[index].stage)
}

fn distance_to_segment(point: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let length_sq = ab.length_sq();
    let t = if length_sq <= f32::EPSILON {
        0.0
    } else {
        ((point - a).dot(ab) / length_sq).clamp(0.0, 1.0)
    };
    (point - (a + ab * t)).length()
}

/// Where the middle of segment `index` is drawn in a plot `width` wide with `room` for it, at the
/// origin of the screen (what the tests hover).
#[cfg(test)]
pub(super) fn anchor(width: f32, room: f32, segments: &[ProfileSegment], index: usize) -> Pos2 {
    let height = room.clamp(min_total(), max_total());
    let plot = data_area(plot_area(Rect::from_min_size(
        Pos2::ZERO,
        vec2(width, height),
    )));
    let x = XMap::new(segments, plot.width(), &[]);
    let range = altitude_range(segments);
    let corners = corners(&segments[index]);
    let (from, to) = (corners[0], corners[corners.len() - 1]);
    position(
        plot,
        range,
        &x,
        f64::midpoint(from.0, to.0),
        f64::midpoint(from.1, to.1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::mission_stage_editor::model::profile;

    #[test]
    fn a_click_on_a_segment_selects_its_stage_and_empty_space_selects_none() {
        let plan = alas_config::MissionPlan::from_profile(
            &alas_config::MissionProfileConfig::default(),
            35_000.0,
        );
        let segments = profile(&plan, 0.0, 0.0, 3_000.0);
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(600.0, min_total()));
        let plot = data_area(plot_area(rect));
        let range = altitude_range(&segments);
        let x = XMap::new(&segments, plot.width(), &[]);
        let cruise = &segments[2];
        let middle = position(
            plot,
            range,
            &x,
            f64::midpoint(cruise.progress[0], cruise.progress[1]),
            cruise.altitude_ft[0],
        );
        assert_eq!(nearest_stage(plot, range, &x, &segments, middle), Some(2));
        let far = Pos2::new(middle.x, plot.bottom() - 1.0);
        assert_eq!(nearest_stage(plot, range, &x, &segments, far), None);
    }
}
