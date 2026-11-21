// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The numbered markers of the mission plot: one small circle per phase
//! boundary, filled with the panel colour, carrying the boundary's number.
//!
//! A marker sits on its point. When two would overlap, the later one's label is
//! nudged up (or down) in steps of one marker with a short leader back to its
//! point; the point itself never moves.

use egui::{vec2, Align2, Color32, Painter, Pos2, Stroke, Ui};

use super::plot_style::small_font;

/// Radius of a marker circle, pt: room for a two-digit number in the small font.
pub(super) const RADIUS: f32 = 9.0;
/// Clear space kept between two marker circles, pt.
pub(super) const CLEAR: f32 = 6.0;

/// A marker: the curve point it numbers and where its circle is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Marker {
    pub point: Pos2,
    pub center: Pos2,
}

impl Marker {
    /// Whether the label was moved off its point.
    pub(super) fn nudged(&self) -> bool {
        (self.center - self.point).length() > 0.5
    }
}

/// Where the marker of each of `points` is drawn. `limits` are the least and
/// greatest y a circle's centre may take.
pub(super) fn place(points: &[Pos2], limits: (f32, f32)) -> Vec<Marker> {
    let step = 2.0 * RADIUS + CLEAR;
    let mut placed: Vec<Marker> = Vec::with_capacity(points.len());
    for &point in points {
        let free = |center: Pos2| {
            placed
                .iter()
                .all(|other| (other.center - center).length() >= step - 1e-3)
        };
        // Up first (the ground points sit on the axis), then down, ever farther.
        let center = std::iter::once(0_i32)
            .chain((1..12_i32).flat_map(|k| [-k, k]))
            .map(|k| Pos2::new(point.x, point.y + k as f32 * step))
            .filter(|center| center.y >= limits.0 && center.y <= limits.1 || *center == point)
            .find(|center| free(*center))
            .unwrap_or(point);
        placed.push(Marker { point, center });
    }
    placed
}

/// How a marker is drawn.
#[derive(Debug, Clone, Copy)]
pub(super) struct Look {
    /// The curve colour at the marker's point (the leader's dot).
    pub color: Color32,
    /// Whether the marker bounds the highlighted segment.
    pub lit: bool,
}

/// Draw the marker of boundary `number` (1-based).
pub(super) fn draw(ui: &Ui, painter: &Painter, marker: &Marker, number: usize, look: Look) {
    let text = if look.lit {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().text_color()
    };
    if marker.nudged() {
        let direction = (marker.center - marker.point).normalized();
        painter.line_segment(
            [marker.point, marker.center - direction * RADIUS],
            Stroke::new(1.0_f32, ui.visuals().weak_text_color()),
        );
        painter.circle_filled(marker.point, 2.0, look.color);
    }
    let border = if look.lit {
        Stroke::new(1.6_f32, ui.visuals().strong_text_color())
    } else {
        Stroke::new(1.0_f32, ui.visuals().weak_text_color())
    };
    painter.circle(marker.center, RADIUS, ui.visuals().panel_fill, border);
    painter.text(
        marker.center + vec2(0.0, 0.5),
        Align2::CENTER_CENTER,
        number.to_string(),
        small_font(ui),
        text,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clear_of_each_other(markers: &[Marker]) -> bool {
        markers.iter().enumerate().all(|(i, a)| {
            markers[i + 1..]
                .iter()
                .all(|b| (a.center - b.center).length() >= 2.0 * RADIUS + CLEAR - 1e-3)
        })
    }

    #[test]
    fn markers_that_would_collide_are_nudged_apart_and_the_points_stay() {
        let points = [
            Pos2::new(100.0, 300.0),
            Pos2::new(104.0, 300.0),
            Pos2::new(104.0, 300.0),
            Pos2::new(110.0, 296.0),
            Pos2::new(300.0, 300.0),
        ];
        let markers = place(&points, (10.0, 300.0));
        assert!(clear_of_each_other(&markers), "{markers:?}");
        for (marker, point) in markers.iter().zip(&points) {
            assert_eq!(marker.point, *point);
            assert_eq!(marker.center.x, point.x);
        }
        // The first and the lone far marker stay on their points.
        assert!(!markers[0].nudged() && !markers[4].nudged());
        // Nudged labels stay inside the limits on the ground side.
        assert!(markers.iter().all(|m| m.center.y <= 300.0));
    }

    #[test]
    fn apart_markers_are_not_moved() {
        let points: Vec<Pos2> = (0..10)
            .map(|k| Pos2::new(20.0 + 32.0 * k as f32, 100.0))
            .collect();
        let markers = place(&points, (0.0, 200.0));
        assert!(markers.iter().all(|m| !m.nudged()));
    }
}
