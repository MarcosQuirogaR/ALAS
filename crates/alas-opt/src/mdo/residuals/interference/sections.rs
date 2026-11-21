// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Exact gap between two superellipse body sections.
//!
//! A section is `|y/a|^n + |z/b|^n = 1` about its centre (`a`, `b` the half
//! width and half height, `n` the shape exponent). The gap along the line
//! between the two centres is exact only when that line is the closest
//! approach; for offset or elongated sections (a wide, flat belly fairing
//! against a round nacelle, a nacelle beside the flank of the cabin) the
//! bodies can overlap off that line. This module measures the true minimum
//! separation of the two boundary curves, negative (the deepest penetration)
//! when they overlap.
//!
//! Frame: the section plane, `(y, z)` in metres.

use alas_geom::aircraft::fuselage::FuselageXSec;

/// Points on each boundary curve.
const BOUNDARY_POINTS: usize = 64;

type Point = (f64, f64);

/// Boundary of `section`, counter-clockwise, as a closed polyline.
fn boundary(section: &FuselageXSec) -> Vec<Point> {
    let a = 0.5 * section.width;
    let b = 0.5 * section.height;
    let n = if section.shape.is_finite() && section.shape > 0.0 {
        section.shape
    } else {
        2.0
    };
    let exponent = 2.0 / n;
    (0..BOUNDARY_POINTS)
        .map(|i| {
            let theta = std::f64::consts::TAU * i as f64 / BOUNDARY_POINTS as f64;
            let (s, c) = theta.sin_cos();
            (
                section.xyz_c[1] + a * c.signum() * c.abs().powf(exponent),
                section.xyz_c[2] + b * s.signum() * s.abs().powf(exponent),
            )
        })
        .collect()
}

/// Whether `point` lies inside `section`.
fn contains(section: &FuselageXSec, point: Point) -> bool {
    let a = 0.5 * section.width;
    let b = 0.5 * section.height;
    let n = if section.shape.is_finite() && section.shape > 0.0 {
        section.shape
    } else {
        2.0
    };
    ((point.0 - section.xyz_c[1]).abs() / a).powf(n)
        + ((point.1 - section.xyz_c[2]).abs() / b).powf(n)
        < 1.0
}

/// Distance from `p` to the closed polyline.
fn distance_to_polyline(p: Point, line: &[Point]) -> f64 {
    let mut best = f64::INFINITY;
    for (i, &start) in line.iter().enumerate() {
        let end = line[(i + 1) % line.len()];
        let (dx, dy) = (end.0 - start.0, end.1 - start.1);
        let length2 = dx * dx + dy * dy;
        let t = if length2 > 0.0 {
            (((p.0 - start.0) * dx + (p.1 - start.1) * dy) / length2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        best = best.min((p.0 - start.0 - t * dx).hypot(p.1 - start.1 - t * dy));
    }
    best
}

/// Minimum separation of the boundaries of two sections, m: positive when
/// apart, negative (the deepest penetration of one boundary into the other)
/// when they overlap; `None` for a degenerate section.
pub(super) fn section_gap_m(first: &FuselageXSec, second: &FuselageXSec) -> Option<f64> {
    let valid = |s: &FuselageXSec| {
        s.width > 0.0
            && s.height > 0.0
            && [s.width, s.height, s.xyz_c[1], s.xyz_c[2]]
                .iter()
                .all(|v| v.is_finite())
    };
    if !valid(first) || !valid(second) {
        return None;
    }
    let (p, q) = (boundary(first), boundary(second));
    let mut penetration: f64 = 0.0;
    for &point in &q {
        if contains(first, point) {
            penetration = penetration.max(distance_to_polyline(point, &p));
        }
    }
    for &point in &p {
        if contains(second, point) {
            penetration = penetration.max(distance_to_polyline(point, &q));
        }
    }
    if penetration > 0.0 {
        return Some(-penetration);
    }
    let apart = p
        .iter()
        .map(|&point| distance_to_polyline(point, &q))
        .chain(q.iter().map(|&point| distance_to_polyline(point, &p)))
        .fold(f64::INFINITY, f64::min);
    Some(apart)
}

/// An axis-aligned rectangle `[y0, y1] x [z0, z1]` in the section plane.
#[derive(Debug, Clone, Copy)]
pub(super) struct Rect {
    pub y0: f64,
    pub y1: f64,
    pub z0: f64,
    pub z1: f64,
}

impl Rect {
    fn contains(&self, p: Point) -> bool {
        p.0 > self.y0 && p.0 < self.y1 && p.1 > self.z0 && p.1 < self.z1
    }

    fn outline(&self) -> Vec<Point> {
        let edge = |from: Point, to: Point| {
            (0..16).map(move |i| {
                let t = i as f64 / 16.0;
                (from.0 + t * (to.0 - from.0), from.1 + t * (to.1 - from.1))
            })
        };
        let corners = [
            (self.y0, self.z0),
            (self.y1, self.z0),
            (self.y1, self.z1),
            (self.y0, self.z1),
        ];
        (0..4)
            .flat_map(|i| edge(corners[i], corners[(i + 1) % 4]))
            .collect()
    }
}

/// Minimum separation of a section and a rectangle, m: positive when apart,
/// negative (the deepest penetration) when they overlap.
pub(super) fn section_rect_gap_m(section: &FuselageXSec, rect: &Rect) -> Option<f64> {
    if !(section.width > 0.0 && section.height > 0.0)
        || ![
            rect.y0,
            rect.y1,
            rect.z0,
            rect.z1,
            section.xyz_c[1],
            section.xyz_c[2],
        ]
        .iter()
        .all(|v| v.is_finite())
        || rect.y1 <= rect.y0
        || rect.z1 <= rect.z0
    {
        return None;
    }
    let (round, frame) = (boundary(section), rect.outline());
    let mut penetration: f64 = 0.0;
    for &point in &frame {
        if contains(section, point) {
            penetration = penetration.max(distance_to_polyline(point, &round));
        }
    }
    for &point in &round {
        if rect.contains(point) {
            penetration = penetration.max(distance_to_polyline(point, &frame));
        }
    }
    if penetration > 0.0 {
        return Some(-penetration);
    }
    let apart = frame
        .iter()
        .map(|&point| distance_to_polyline(point, &round))
        .chain(
            round
                .iter()
                .map(|&point| distance_to_polyline(point, &frame)),
        )
        .fold(f64::INFINITY, f64::min);
    Some(apart)
}

/// A lower bound of the gap from the sections' circumscribed circles, m.
pub(super) fn gap_lower_bound_m(first: &FuselageXSec, second: &FuselageXSec) -> f64 {
    let radius = |s: &FuselageXSec| 0.5 * s.width.max(s.height);
    (second.xyz_c[1] - first.xyz_c[1]).hypot(second.xyz_c[2] - first.xyz_c[2])
        - radius(first)
        - radius(second)
}

// A test asserts on values it built here, so a failed unwrap is the assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn section(y: f64, z: f64, width: f64, height: f64, shape: f64) -> FuselageXSec {
        FuselageXSec {
            xyz_c: [0.0, y, z],
            width,
            height,
            shape,
        }
    }

    #[test]
    fn two_circles_have_the_centre_distance_less_the_radii() {
        let a = section(0.0, 0.0, 2.0, 2.0, 2.0);
        let b = section(5.0, 0.0, 2.0, 2.0, 2.0);
        let gap = section_gap_m(&a, &b).unwrap();
        assert!((gap - 3.0).abs() < 1.0e-2, "{gap}");
    }

    #[test]
    fn overlapping_circles_report_the_penetration() {
        let a = section(0.0, 0.0, 2.0, 2.0, 2.0);
        let b = section(1.5, 0.0, 2.0, 2.0, 2.0);
        let gap = section_gap_m(&a, &b).unwrap();
        assert!((gap + 0.5).abs() < 1.0e-2, "{gap}");
    }

    #[test]
    fn a_wide_flat_section_is_hit_off_the_centre_line() {
        // A flat 8 m wide, 1 m high belly section and a 1.6 m round nacelle
        // 1.4 m below its centre but 3.2 m to the side: along the line
        // between the centres the gap is positive; the nacelle overlaps the
        // wide section's lower flank.
        let wide = section(0.0, 0.0, 8.0, 1.0, 2.0);
        let pod = section(3.2, -1.4, 1.6, 1.6, 2.0);
        let line_gap = (3.2f64).hypot(1.4) - 0.0 - 0.8;
        assert!(line_gap > 0.0);
        let gap = section_gap_m(&wide, &pod).unwrap();
        assert!(gap < line_gap);
    }

    #[test]
    fn a_rectangle_below_a_round_section_is_apart_by_the_free_distance() {
        let pod = section(0.0, 1.0, 1.0, 1.0, 2.0);
        let tyre = Rect {
            y0: -0.3,
            y1: 0.3,
            z0: -0.5,
            z1: 0.2,
        };
        let gap = section_rect_gap_m(&pod, &tyre).unwrap();
        assert!((gap - 0.3).abs() < 1.0e-2, "{gap}");
        let touching = Rect { z1: 0.8, ..tyre };
        assert!(section_rect_gap_m(&pod, &touching).unwrap() < 0.0);
    }

    #[test]
    fn the_circle_bound_never_exceeds_the_exact_gap() {
        let a = section(0.0, 0.0, 4.0, 1.0, 2.0);
        let b = section(3.0, -2.0, 1.5, 1.5, 2.0);
        assert!(gap_lower_bound_m(&a, &b) <= section_gap_m(&a, &b).unwrap() + 1.0e-9);
    }
}
