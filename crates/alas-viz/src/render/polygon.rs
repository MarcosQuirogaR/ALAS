// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Polygon conditioning, convexity tests and ear-clipping triangulation.

use egui::epaint::Mesh;
use egui::{Color32, Pos2, Shape, Stroke as EguiStroke};

/// The largest miter factor the egui closed-path feathering may apply before
/// a polygon is tessellated without it. A corner whose adjacent edges turn by
/// `theta` gets its feathering vertices displaced by `1 / cos(theta / 2)`
/// times the feathering width; this bound keeps that under four pixels
/// (interior angles down to about 29 degrees).
pub(super) const MAX_MITER_FACTOR: f32 = 4.0;

/// Screen-space polygon vertices with consecutive duplicates removed and the
/// closing vertex dropped, so every edge has a defined direction.
pub(super) fn sanitized_polygon(points: impl IntoIterator<Item = Pos2>) -> Vec<Pos2> {
    const MIN_EDGE: f32 = 1e-3;
    let mut out: Vec<Pos2> = Vec::new();
    for p in points {
        if !p.x.is_finite() || !p.y.is_finite() {
            continue;
        }
        if out.last().is_some_and(|last| last.distance(p) < MIN_EDGE) {
            continue;
        }
        out.push(p);
    }
    while out.len() > 1 && out[0].distance(out[out.len() - 1]) < MIN_EDGE {
        out.pop();
    }
    out
}

/// Whether the egui miter feathering stays bounded on every corner.
///
/// egui places the feathering vertices of a closed path at
/// `normal / |normal|^2`, where `normal` is the mean of the two adjacent
/// edge normals. A lofted face seen edge-on projects to a sliver whose
/// consecutive edges nearly reverse, so `normal` tends to zero and the
/// vertices fly off the viewport (or become NaN for an exact reversal).
pub(super) fn polygon_corners_are_well_conditioned(points: &[Pos2]) -> bool {
    let n = points.len();
    if n < 3 {
        return false;
    }
    let min_length_sq = 1.0 / (MAX_MITER_FACTOR * MAX_MITER_FACTOR);
    (0..n).all(|i| {
        let prev = points[(i + n - 1) % n];
        let here = points[i];
        let next = points[(i + 1) % n];
        let n0 = (here - prev).normalized().rot90();
        let n1 = (next - here).normalized().rot90();
        let normal = (n0 + n1) * 0.5;
        normal.length_sq() >= min_length_sq
    })
}

/// Whether a polygon is convex in screen space.
///
/// `PathShape::convex_polygon` is deliberately used only for this subset of
/// faces. Native OpenVSP meshes can retain a concave boundary, and egui's
/// convex path fill would cover the indentation as if it were part of the
/// face. Collinear boundary points are allowed because tessellated CAD faces
/// commonly contain them.
pub(super) fn polygon_is_convex(points: &[Pos2]) -> bool {
    const EPSILON: f32 = 1.0e-5;
    let n = points.len();
    if n < 3 {
        return false;
    }
    let mut turn_sign = 0.0_f32;
    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        let c = points[(i + 2) % n];
        let cross = cross_2d(b - a, c - b);
        if cross.abs() <= EPSILON {
            continue;
        }
        if turn_sign == 0.0 {
            turn_sign = cross.signum();
        } else if cross.signum() != turn_sign {
            return false;
        }
    }
    turn_sign != 0.0
}

#[inline]
pub(super) fn cross_2d(left: egui::Vec2, right: egui::Vec2) -> f32 {
    left.x * right.y - left.y * right.x
}

pub(super) fn signed_polygon_area(points: &[Pos2]) -> f32 {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(left, right)| left.x * right.y - right.x * left.y)
        .sum::<f32>()
        * 0.5
}

pub(super) fn point_in_or_on_triangle(
    point: Pos2,
    a: Pos2,
    b: Pos2,
    c: Pos2,
    orientation: f32,
) -> bool {
    const EPSILON: f32 = 1.0e-5;
    let ab = cross_2d(b - a, point - a) * orientation;
    let bc = cross_2d(c - b, point - b) * orientation;
    let ca = cross_2d(a - c, point - c) * orientation;
    ab >= -EPSILON && bc >= -EPSILON && ca >= -EPSILON
}

/// Triangulate a simple screen-space polygon without changing its boundary.
///
/// This is used only by the egui backend, which accepts filled convex paths
/// but has no general concave path fill. The scene still carries the native
/// face as one polygon; the triangles are a renderer detail. Returning
/// `None` is safer than filling a self-intersecting or otherwise ambiguous
/// face with a fabricated fan.
pub(super) fn triangulate_polygon(points: &[Pos2]) -> Option<Vec<[usize; 3]>> {
    const EPSILON: f32 = 1.0e-5;
    if points.len() < 3 {
        return None;
    }
    let area = signed_polygon_area(points);
    if !area.is_finite() || area.abs() <= EPSILON {
        return None;
    }
    let orientation = area.signum();
    let mut remaining = (0..points.len()).collect::<Vec<_>>();
    let mut triangles = Vec::with_capacity(points.len().saturating_sub(2));
    let mut guard = 0usize;
    let max_iterations = points.len().saturating_mul(points.len()).max(1);

    while remaining.len() > 3 {
        let mut ear_found = false;
        let count = remaining.len();
        for offset in 0..count {
            let prev = remaining[(offset + count - 1) % count];
            let current = remaining[offset];
            let next = remaining[(offset + 1) % count];
            let turn = cross_2d(
                points[next] - points[current],
                points[prev] - points[current],
            );
            if turn * orientation <= EPSILON {
                continue;
            }
            if remaining.iter().any(|&candidate| {
                candidate != prev
                    && candidate != current
                    && candidate != next
                    && point_in_or_on_triangle(
                        points[candidate],
                        points[prev],
                        points[current],
                        points[next],
                        orientation,
                    )
            }) {
                continue;
            }
            triangles.push([prev, current, next]);
            remaining.remove(offset);
            ear_found = true;
            break;
        }
        if !ear_found {
            return None;
        }
        guard += 1;
        if guard > max_iterations {
            return None;
        }
    }

    if remaining.len() == 3 {
        triangles.push([remaining[0], remaining[1], remaining[2]]);
    }
    Some(triangles)
}

/// A polygon the feathered egui path cannot tessellate safely, or a concave
/// polygon for which that path's convex-only fill would be wrong: triangulate
/// the fill and draw the original boundary as independent segments.
pub(super) fn triangulated_polygon_shapes(
    points: &[Pos2],
    fill_color: Color32,
    outline: EguiStroke,
) -> Vec<Shape> {
    let mut shapes = Vec::new();
    if fill_color != Color32::TRANSPARENT {
        let triangles = triangulate_polygon(points).unwrap_or_default();
        let mut mesh = Mesh::default();
        for &p in points {
            mesh.colored_vertex(p, fill_color);
        }
        for [a, b, c] in triangles {
            mesh.add_triangle(a as u32, b as u32, c as u32);
        }
        if !mesh.indices.is_empty() {
            shapes.push(Shape::mesh(mesh));
        }
    }
    if outline != EguiStroke::NONE {
        let n = points.len();
        for i in 0..n {
            shapes.push(Shape::line_segment(
                [points[i], points[(i + 1) % n]],
                outline,
            ));
        }
    }
    shapes
}
