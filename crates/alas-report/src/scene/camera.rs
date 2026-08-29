// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Camera projection shared by three-dimensional report figures.

use super::{Point2D, Point3D};
use serde::{Deserialize, Serialize};

/// Orthographic camera used by three-dimensional report scenes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Camera3D {
    /// Elevation angle above ground plane in degrees.
    pub elev_deg: f64,
    /// Azimuth angle in degrees clockwise around vertical axis.
    pub azim_deg: f64,
    /// Overall zoom scale factor.
    pub zoom: f64,
}

impl Default for Camera3D {
    fn default() -> Self {
        Self {
            elev_deg: 22.0,
            azim_deg: -125.0,
            zoom: 1.0,
        }
    }
}

impl Camera3D {
    /// Camera looking down on the X-Y plane.
    pub const fn top() -> Self {
        Self {
            elev_deg: 90.0,
            azim_deg: 0.0,
            zoom: 1.0,
        }
    }

    /// Camera looking along the negative Y direction at the X-Z plane.
    pub const fn front() -> Self {
        Self {
            elev_deg: 0.0,
            azim_deg: 90.0,
            zoom: 1.0,
        }
    }

    /// Camera looking along the X direction at the X-Z side view.
    pub const fn side() -> Self {
        Self {
            elev_deg: 0.0,
            azim_deg: 0.0,
            zoom: 1.0,
        }
    }

    /// Nose-facing isometric camera used by the reference three-view.
    pub const fn isometric() -> Self {
        Self {
            elev_deg: 22.0,
            azim_deg: -125.0,
            zoom: 1.0,
        }
    }

    /// Projection span that fits a 3-D bounding box inside one viewport.
    ///
    /// Unlike a largest-model-axis heuristic, this accounts for azimuth,
    /// elevation, and the viewport aspect ratio, so a long fuselage does not
    /// become tiny merely because its projected length is oblique.
    pub fn fit_span_to_bbox(
        &self,
        bounds: [f64; 6],
        viewport: (f64, f64, f64, f64),
        padding_fraction: f64,
    ) -> f64 {
        let [x_min, x_max, y_min, y_max, z_min, z_max] = bounds;
        let corners = [
            [x_min, y_min, z_min],
            [x_min, y_min, z_max],
            [x_min, y_max, z_min],
            [x_min, y_max, z_max],
            [x_max, y_min, z_min],
            [x_max, y_min, z_max],
            [x_max, y_max, z_min],
            [x_max, y_max, z_max],
        ];
        self.fit_span_to_points(&corners, viewport, padding_fraction)
    }

    /// Projection span that fits the supplied model points inside one viewport.
    ///
    /// This is tighter than fitting all eight corners of an axis-aligned box
    /// for swept wings and oblique fuselages, whose empty box corners otherwise
    /// consume most of a compact preview card.
    pub fn fit_span_to_points(
        &self,
        points: &[Point3D],
        viewport: (f64, f64, f64, f64),
        padding_fraction: f64,
    ) -> f64 {
        let elev = self.elev_deg.to_radians();
        let azim = self.azim_deg.to_radians();
        let mut u_min = f64::INFINITY;
        let mut u_max = f64::NEG_INFINITY;
        let mut v_min = f64::INFINITY;
        let mut v_max = f64::NEG_INFINITY;
        for &[x, y, z] in points {
            let u = x * azim.cos() - y * azim.sin();
            let y_rot = x * azim.sin() + y * azim.cos();
            let v = -y_rot * elev.sin() + z * elev.cos();
            u_min = u_min.min(u);
            u_max = u_max.max(u);
            v_min = v_min.min(v);
            v_max = v_max.max(v);
        }
        let (_, _, width, height) = viewport;
        let padding = padding_fraction.clamp(0.0, 0.45);
        let usable_width = width * (1.0 - 2.0 * padding);
        let usable_height = height * (1.0 - 2.0 * padding);
        let projected_width = if u_min.is_finite() {
            (u_max - u_min).max(1e-6)
        } else {
            1.0
        };
        let projected_height = if v_min.is_finite() {
            (v_max - v_min).max(1e-6)
        } else {
            1.0
        };
        let target_scale = (usable_width / projected_width)
            .min(usable_height / projected_height)
            .max(1e-6);
        (width.min(height) * 0.45 / target_scale).max(1e-6)
    }

    /// Center the projected extent of `points` in a viewport without changing
    /// their depth coordinate. This matters for a swept or asymmetric model:
    /// the midpoint of its axis-aligned 3-D box is not generally the midpoint
    /// of the camera's projected `u`/`v` extent.
    pub fn fit_center_to_points(&self, points: &[Point3D], fallback: Point3D) -> Point3D {
        let elev = self.elev_deg.to_radians();
        let azim = self.azim_deg.to_radians();
        let mut u_min = f64::INFINITY;
        let mut u_max = f64::NEG_INFINITY;
        let mut v_min = f64::INFINITY;
        let mut v_max = f64::NEG_INFINITY;
        for &[x, y, z] in points {
            let u = x * azim.cos() - y * azim.sin();
            let y_rot = x * azim.sin() + y * azim.cos();
            let v = -y_rot * elev.sin() + z * elev.cos();
            u_min = u_min.min(u);
            u_max = u_max.max(u);
            v_min = v_min.min(v);
            v_max = v_max.max(v);
        }
        if !u_min.is_finite() || !v_min.is_finite() {
            return fallback;
        }

        let y_rot = fallback[0] * azim.sin() + fallback[1] * azim.cos();
        let depth = y_rot * elev.cos() + fallback[2] * elev.sin();
        let u = (u_min + u_max) * 0.5;
        let v = (v_min + v_max) * 0.5;
        let centered_y_rot = -v * elev.sin() + depth * elev.cos();
        let z = v * elev.cos() + depth * elev.sin();
        [
            u * azim.cos() + centered_y_rot * azim.sin(),
            -u * azim.sin() + centered_y_rot * azim.cos(),
            z,
        ]
    }

    /// Project a 3D model point `[x, y, z]` into 2D viewport coordinates.
    pub fn project(
        &self,
        pt: Point3D,
        center: Point3D,
        max_span: f64,
        viewport: (f64, f64, f64, f64),
    ) -> Point2D {
        let (vx, vy, vw, vh) = viewport;
        let elev = self.elev_deg.to_radians();
        let azim = self.azim_deg.to_radians();
        let dx = pt[0] - center[0];
        let dy = pt[1] - center[1];
        let dz = pt[2] - center[2];
        let x_rot = dx * azim.cos() - dy * azim.sin();
        let y_rot = dx * azim.sin() + dy * azim.cos();
        let y_proj = -y_rot * elev.sin() + dz * elev.cos();
        let scale = (vw.min(vh) * 0.45 * self.zoom) / max_span.max(1e-6);
        [
            vx + vw * 0.5 + x_rot * scale,
            vy + vh * 0.5 - y_proj * scale,
        ]
    }

    /// Signed depth of a point in the camera coordinate system.
    ///
    /// Positive values lie on the visible hemisphere for an Earth-centred
    /// globe. Report scenes remain painter's-algorithm renderers, so callers
    /// use this to omit the back side rather than draw false through-globe
    /// coastlines or graticules.
    pub fn view_depth(&self, pt: Point3D, center: Point3D) -> f64 {
        let elev = self.elev_deg.to_radians();
        let azim = self.azim_deg.to_radians();
        let dx = pt[0] - center[0];
        let dy = pt[1] - center[1];
        let dz = pt[2] - center[2];
        let y_rot = dx * azim.sin() + dy * azim.cos();
        y_rot * elev.cos() + dz * elev.sin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitted_bbox_uses_most_of_an_oblique_viewport_without_clipping() {
        let camera = Camera3D::default();
        let viewport = (20.0, 20.0, 560.0, 410.0);
        let bounds = [0.0, 70.0, -35.0, 35.0, -3.0, 12.0];
        let span = camera.fit_span_to_bbox(bounds, viewport, 0.08);
        let center = [35.0, 0.0, 4.5];
        let corners = [
            [0.0, -35.0, -3.0],
            [0.0, 35.0, 12.0],
            [70.0, -35.0, -3.0],
            [70.0, 35.0, 12.0],
        ];
        for corner in corners {
            let [x, y] = camera.project(corner, center, span, viewport);
            assert!((20.0..=580.0).contains(&x));
            assert!((20.0..=430.0).contains(&y));
        }
    }

    #[test]
    fn fitting_actual_points_does_not_reserve_empty_box_corners() {
        let camera = Camera3D::default();
        let viewport = (20.0, 20.0, 560.0, 410.0);
        let points = [[0.0, 0.0, 0.0], [70.0, 0.0, 0.0], [35.0, 35.0, 0.0]];
        let exact = camera.fit_span_to_points(&points, viewport, 0.08);
        let boxed = camera.fit_span_to_bbox([0.0, 70.0, 0.0, 35.0, 0.0, 0.0], viewport, 0.08);
        assert!(exact < boxed);
    }

    #[test]
    fn fitting_center_uses_the_projected_midpoint_for_an_asymmetric_model() {
        let camera = Camera3D::default();
        let viewport = (20.0, 20.0, 560.0, 410.0);
        let points = [[0.0, 0.0, 0.0], [70.0, 0.0, 0.0], [70.0, 35.0, 0.0]];
        let center = camera.fit_center_to_points(&points, [35.0, 17.5, 0.0]);
        let span = camera.fit_span_to_points(&points, viewport, 0.08);
        let projected: Vec<_> = points
            .iter()
            .map(|&point| camera.project(point, center, span, viewport))
            .collect();
        let x_min = projected
            .iter()
            .map(|point| point[0])
            .fold(f64::INFINITY, f64::min);
        let x_max = projected
            .iter()
            .map(|point| point[0])
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(((x_min + x_max) * 0.5 - 300.0).abs() < 1e-9);
    }
}
