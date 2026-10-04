// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Orthographic hidden-line clipping against the shared loft faces.

use super::super::sandbox_scene::{SandboxFace, SceneFraming};
use crate::scene::{Point2D, Point3D};

struct ProjectedFace {
    boundary: Vec<Point3D>,
    triangles: Vec<[Point3D; 3]>,
    bounds: [f64; 4],
}

pub(super) struct ContourVisibility {
    faces: Vec<ProjectedFace>,
    framing: SceneFraming,
    depth_roundoff: f64,
}

fn cross(a: Point2D, b: Point2D) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn difference(a: Point2D, b: Point2D) -> Point2D {
    [a[0] - b[0], a[1] - b[1]]
}

fn weights(point: Point3D, triangle: &[Point3D; 3]) -> Option<[f64; 3]> {
    let xy = |point: Point3D| [point[0], point[1]];
    let a = xy(triangle[0]);
    let u = difference(xy(triangle[1]), a);
    let v = difference(xy(triangle[2]), a);
    let area = cross(u, v);
    if area.abs() <= f64::EPSILON * (u[0].hypot(u[1]) * v[0].hypot(v[1])) {
        return None;
    }
    let offset = difference(xy(point), a);
    let b = cross(offset, v) / area;
    let c = cross(u, offset) / area;
    Some([1.0 - b - c, b, c])
}

fn positive_interval(interval: &mut [f64; 2], start: f64, end: f64) -> bool {
    let delta = end - start;
    if delta == 0.0 {
        return start >= 0.0;
    }
    let crossing = -start / delta;
    if delta > 0.0 {
        interval[0] = interval[0].max(crossing);
    } else {
        interval[1] = interval[1].min(crossing);
    }
    interval[0] < interval[1]
}

fn hidden_interval(
    a: Point3D,
    b: Point3D,
    triangle: &[Point3D; 3],
    roundoff: f64,
) -> Option<[f64; 2]> {
    let wa = weights(a, triangle)?;
    let wb = weights(b, triangle)?;
    let mut interval = [0.0, 1.0];
    for index in 0..3 {
        if !positive_interval(&mut interval, wa[index], wb[index]) {
            return None;
        }
    }
    let depth = |weights: [f64; 3]| {
        weights
            .iter()
            .zip(triangle)
            .map(|(w, p)| w * p[2])
            .sum::<f64>()
    };
    if !positive_interval(
        &mut interval,
        depth(wa) - a[2] - roundoff,
        depth(wb) - b[2] - roundoff,
    ) {
        return None;
    }
    Some(interval)
}

fn on_boundary(point: Point3D, boundary: &[Point3D], roundoff: f64) -> bool {
    boundary.iter().enumerate().any(|(index, a)| {
        let b = boundary[(index + 1) % boundary.len()];
        let direction: Point3D = std::array::from_fn(|axis| b[axis] - a[axis]);
        let squared = direction.iter().map(|value| value * value).sum::<f64>();
        let fraction = if squared == 0.0 {
            0.0
        } else {
            (0..3)
                .map(|axis| (point[axis] - a[axis]) * direction[axis])
                .sum::<f64>()
                / squared
        };
        let nearest = fraction.clamp(0.0, 1.0);
        (0..3)
            .map(|axis| (point[axis] - a[axis] - nearest * direction[axis]).powi(2))
            .sum::<f64>()
            <= roundoff * roundoff
    })
}

impl ContourVisibility {
    pub(super) fn new(faces: &[SandboxFace], framing: SceneFraming) -> Self {
        let coordinate_scale = faces
            .iter()
            .flat_map(|face| &face.points)
            .flatten()
            .map(|value| value.abs())
            .fold(framing.max_span.max(1.0), f64::max);
        let project = |point| {
            let xy = framing.project(point);
            [
                xy[0],
                xy[1],
                framing.camera.view_depth(point, framing.center),
            ]
        };
        let faces = faces
            .iter()
            .filter(|face| face.points.len() >= 3)
            .map(|face| {
                let points = face.points.iter().copied().map(project).collect::<Vec<_>>();
                let triangles = (1..points.len() - 1)
                    .map(|index| [points[0], points[index], points[index + 1]])
                    .collect();
                let bounds = points.iter().fold(
                    [
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                    ],
                    |mut bounds, p| {
                        bounds[0] = bounds[0].min(p[0]);
                        bounds[1] = bounds[1].max(p[0]);
                        bounds[2] = bounds[2].min(p[1]);
                        bounds[3] = bounds[3].max(p[1]);
                        bounds
                    },
                );
                ProjectedFace {
                    boundary: face.points.clone(),
                    triangles,
                    bounds,
                }
            })
            .collect();
        Self {
            faces,
            framing,
            depth_roundoff: 128.0 * f64::EPSILON * coordinate_scale,
        }
    }

    pub(super) fn visible_segments(&self, a: Point3D, b: Point3D) -> Vec<[Point2D; 2]> {
        let project = |point| {
            let xy = self.framing.project(point);
            [
                xy[0],
                xy[1],
                self.framing.camera.view_depth(point, self.framing.center),
            ]
        };
        let (pa, pb) = (project(a), project(b));
        let mut hidden = Vec::new();
        for face in &self.faces {
            if pa[0].max(pb[0]) < face.bounds[0]
                || pa[0].min(pb[0]) > face.bounds[1]
                || pa[1].max(pb[1]) < face.bounds[2]
                || pa[1].min(pb[1]) > face.bounds[3]
            {
                continue;
            }
            // A section belongs to its supporting loft strip even when the quad is twisted.
            if on_boundary(a, &face.boundary, self.depth_roundoff)
                && on_boundary(b, &face.boundary, self.depth_roundoff)
            {
                continue;
            }
            hidden.extend(
                face.triangles
                    .iter()
                    .filter_map(|triangle| hidden_interval(pa, pb, triangle, self.depth_roundoff)),
            );
        }
        hidden.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let point = |fraction: f64| {
            if fraction == 0.0 {
                return [pa[0], pa[1]];
            }
            if fraction == 1.0 {
                return [pb[0], pb[1]];
            }
            [
                pa[0] + fraction * (pb[0] - pa[0]),
                pa[1] + fraction * (pb[1] - pa[1]),
            ]
        };
        let mut visible = Vec::new();
        let mut start = 0.0_f64;
        for interval in hidden {
            if interval[0] > start {
                visible.push([point(start), point(interval[0])]);
            }
            start = start.max(interval[1]);
        }
        if start < 1.0 {
            visible.push([point(start), point(1.0)]);
        }
        visible
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::sandbox_scene::SceneComponent;
    use super::*;
    use crate::scene::Camera3D;

    fn framing() -> SceneFraming {
        SceneFraming {
            camera: Camera3D::top(),
            center: [0.0; 3],
            max_span: 3.0,
            viewport: (0.0, 0.0, 600.0, 600.0),
            canvas: (600.0, 600.0),
        }
    }

    fn square(depth: f64) -> SandboxFace {
        SandboxFace {
            component: SceneComponent::Fuselage,
            points: vec![
                [-1.0, -1.0, depth],
                [1.0, -1.0, depth],
                [1.0, 1.0, depth],
                [-1.0, 1.0, depth],
            ],
            normal: [0.0, 0.0, 1.0],
        }
    }

    fn assert_point(actual: Point2D, expected: Point2D) {
        for axis in 0..2 {
            assert!(
                (actual[axis] - expected[axis]).abs()
                    <= 256.0 * f64::EPSILON * expected[axis].abs().max(1.0)
            );
        }
    }

    #[test]
    fn foreground_square_removes_exactly_the_covered_part_of_a_far_contour() {
        let framing = framing();
        let visibility = ContourVisibility::new(&[square(1.0)], framing);
        let visible = visibility.visible_segments([-2.0, 0.0, 0.0], [2.0, 0.0, 0.0]);
        assert_eq!(visible.len(), 2);
        for (actual, expected) in visible.iter().flatten().zip([
            [-2.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
        ]) {
            assert_point(*actual, framing.project(expected));
        }
    }

    #[test]
    fn foreground_and_coplanar_contours_remain_visible() {
        let framing = framing();
        let visibility = ContourVisibility::new(&[square(1.0)], framing);
        for depth in [1.0, 2.0] {
            let (a, b) = ([-0.5, 0.0, depth], [0.5, 0.0, depth]);
            assert_eq!(
                visibility.visible_segments(a, b),
                vec![[framing.project(a), framing.project(b)]]
            );
        }
    }

    #[test]
    fn depth_crossing_clips_at_the_surface_intersection() {
        let framing = framing();
        let visibility = ContourVisibility::new(&[square(1.0)], framing);
        let visible = visibility.visible_segments([-0.5, 0.0, 0.0], [0.5, 0.0, 2.0]);
        assert_eq!(visible.len(), 1);
        assert_point(visible[0][0], framing.project([0.0, 0.0, 1.0]));
        assert_point(visible[0][1], framing.project([0.5, 0.0, 2.0]));
    }

    #[test]
    fn far_component_ring_is_hidden_but_the_foreground_ring_survives() {
        let framing = framing();
        let visibility = ContourVisibility::new(&[square(1.0)], framing);
        for depth in [0.0, 2.0] {
            let ring = [
                [-0.5, -0.5, depth],
                [0.5, -0.5, depth],
                [0.5, 0.5, depth],
                [-0.5, 0.5, depth],
                [-0.5, -0.5, depth],
            ];
            for pair in ring.windows(2) {
                let visible = visibility.visible_segments(pair[0], pair[1]);
                if depth < 1.0 {
                    assert!(visible.is_empty());
                } else {
                    assert_eq!(
                        visible,
                        vec![[framing.project(pair[0]), framing.project(pair[1])]]
                    );
                }
            }
        }
    }

    #[test]
    fn overlapping_occluders_do_not_reintroduce_hidden_intervals() {
        let framing = framing();
        let mut shifted = square(1.5);
        for point in &mut shifted.points {
            point[0] += 1.0;
        }
        let visibility = ContourVisibility::new(&[square(1.0), shifted], framing);
        let visible = visibility.visible_segments([-2.0, 0.0, 0.0], [3.0, 0.0, 0.0]);
        assert_eq!(visible.len(), 2);
        assert_point(visible[0][1], framing.project([-1.0, 0.0, 0.0]));
        assert_point(visible[1][0], framing.project([2.0, 0.0, 0.0]));
    }

    #[test]
    fn sloped_occluder_hides_only_the_positive_depth_half() {
        let framing = framing();
        let mut slope = square(0.0);
        for point in &mut slope.points {
            point[2] = point[0];
        }
        let visibility = ContourVisibility::new(&[slope], framing);
        let visible = visibility.visible_segments([-0.5, 0.0, 0.0], [0.5, 0.0, 0.0]);
        // The plane z = x lies ahead of the z = 0 contour exactly when x > 0.
        assert_eq!(visible.len(), 1);
        assert_point(visible[0][0], framing.project([-0.5, 0.0, 0.0]));
        assert_point(visible[0][1], framing.project([0.0, 0.0, 0.0]));
    }

    #[test]
    fn twisted_support_preserves_its_contour_but_does_not_disable_other_occluders() {
        let framing = framing();
        let support = SandboxFace {
            component: SceneComponent::Wing,
            points: vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 1.0],
                [-1.0, 1.0, -1.0],
            ],
            normal: [0.0, 0.0, 1.0],
        };
        // The y = 0 contour joins the midpoints of the two spanwise edges.
        let (a, b) = ([-1.0, 0.0, -0.5], [1.0, 0.0, 0.5]);
        let visibility = ContourVisibility::new(std::slice::from_ref(&support), framing);
        assert_eq!(
            visibility.visible_segments(a, b),
            vec![[framing.project(a), framing.project(b)]]
        );
        let visibility = ContourVisibility::new(&[support, square(2.0)], framing);
        assert!(visibility.visible_segments(a, b).is_empty());
    }
}
