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

    /// Orthonormal camera basis in world coordinates: right, up, view.
    ///
    /// [`Self::project`] is the right/up pair of dot products and
    /// [`Self::view_depth`] is the view one, so an interactive viewport can
    /// invert the projection without duplicating the trigonometry.
    fn basis(&self) -> (Point3D, Point3D, Point3D) {
        let elev = self.elev_deg.to_radians();
        let azim = self.azim_deg.to_radians();
        (
            [azim.cos(), -azim.sin(), 0.0],
            [
                -azim.sin() * elev.sin(),
                -azim.cos() * elev.sin(),
                elev.cos(),
            ],
            [azim.sin() * elev.cos(), azim.cos() * elev.cos(), elev.sin()],
        )
    }

    /// Unit sphere direction under a viewport point of a projected globe.
    ///
    /// `center` and `radius` describe the disk the sphere occupies in scene
    /// coordinates. A pointer beyond the limb is clamped radially onto it, so
    /// a gesture that leaves the globe keeps a defined surface anchor instead
    /// of losing the grab; `None` only for a degenerate disk or non-finite
    /// input.
    pub fn globe_direction_at(
        &self,
        point: Point2D,
        center: Point2D,
        radius: f64,
    ) -> Option<Point3D> {
        if !radius.is_finite() || radius <= f64::EPSILON {
            return None;
        }
        let mut right = (point[0] - center[0]) / radius;
        // Viewport y grows downward while the projected up axis grows upward.
        let mut up = -(point[1] - center[1]) / radius;
        if !right.is_finite() || !up.is_finite() {
            return None;
        }
        let reach = (right * right + up * up).sqrt();
        if reach > 1.0 {
            right /= reach;
            up /= reach;
        }
        let depth = (1.0 - (right * right + up * up)).max(0.0).sqrt();
        let (right_axis, up_axis, view_axis) = self.basis();
        Some([
            right * right_axis[0] + up * up_axis[0] + depth * view_axis[0],
            right * right_axis[1] + up * up_axis[1] + depth * view_axis[1],
            right * right_axis[2] + up * up_axis[2] + depth * view_axis[2],
        ])
    }

    /// Rotate so `anchor` projects onto `target`, keeping zoom and north up.
    ///
    /// This is the surface-anchored globe drag: the picked location stays
    /// under the pointer instead of the centre-based angular sweep, which
    /// moves the surface by an amount that depends on where the gesture
    /// started. With two free angles and no roll the solution is exact
    /// wherever it exists; a target beyond the limb is clamped onto it, a
    /// solution past `pitch_limit_deg` saturates in elevation and still
    /// follows the pointer in azimuth, and hidden-hemisphere branches are
    /// rejected so the globe never flips through itself.
    pub fn anchored_to(
        &self,
        anchor: Point3D,
        target: Point2D,
        center: Point2D,
        radius: f64,
        pitch_limit_deg: f64,
    ) -> Option<Self> {
        if !radius.is_finite() || radius <= f64::EPSILON {
            return None;
        }
        let norm = (anchor[0] * anchor[0] + anchor[1] * anchor[1] + anchor[2] * anchor[2]).sqrt();
        if !norm.is_finite() || norm <= f64::EPSILON {
            return None;
        }
        let point = [anchor[0] / norm, anchor[1] / norm, anchor[2] / norm];
        let mut right = (target[0] - center[0]) / radius;
        let mut up = -(target[1] - center[1]) / radius;
        if !right.is_finite() || !up.is_finite() {
            return None;
        }
        let reach = (right * right + up * up).sqrt();
        if reach > 1.0 {
            right /= reach;
            up /= reach;
        }

        // right = |p_xy| cos(azim + phi) has a closed-form azimuth pair, and
        // up = sqrt(h^2 + p_z^2) cos(elev + psi) an elevation pair for each of
        // them. Four candidates, filtered by visibility and ranked by how
        // little they move the camera, keep the gesture continuous.
        let horizontal = (point[0] * point[0] + point[1] * point[1]).sqrt();
        let azimuths = if horizontal <= 1e-9 {
            vec![self.azim_deg.to_radians()]
        } else {
            let phase = point[1].atan2(point[0]);
            let offset = (right / horizontal).clamp(-1.0, 1.0).acos();
            vec![-phase + offset, -phase - offset]
        };
        let limit = pitch_limit_deg.abs().min(89.9);
        let mut best: Option<((u8, f64), Self)> = None;
        for azim in azimuths {
            let along = point[0] * azim.sin() + point[1] * azim.cos();
            let vertical = (along * along + point[2] * point[2]).sqrt();
            if vertical <= 1e-12 {
                continue;
            }
            let phase = along.atan2(point[2]);
            let offset = (up / vertical).clamp(-1.0, 1.0).acos();
            for elev in [-phase + offset, -phase - offset] {
                let elev_deg = wrap_signed_degrees(elev.to_degrees());
                let candidate = Self {
                    elev_deg: elev_deg.clamp(-limit, limit),
                    azim_deg: azim.to_degrees().rem_euclid(360.0),
                    zoom: self.zoom,
                };
                // A solution past the pole, or one that would place the anchor
                // on the hidden hemisphere, is kept only as the saturating
                // fallback: a drag that cannot be satisfied with north up
                // still follows the pointer instead of releasing the surface.
                let saturated =
                    elev_deg.abs() > 90.0 || candidate.view_depth(point, [0.0, 0.0, 0.0]) < 0.0;
                let cost = angular_gap_degrees(candidate.azim_deg, self.azim_deg).powi(2)
                    + angular_gap_degrees(candidate.elev_deg, self.elev_deg).powi(2);
                let rank = (u8::from(saturated), cost);
                if best
                    .as_ref()
                    .is_none_or(|(lowest, _): &((u8, f64), Self)| rank < *lowest)
                {
                    best = Some((rank, candidate));
                }
            }
        }
        best.map(|(_, camera)| camera)
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

/// Map degrees onto `(-180, 180]` so a wrapped solution can be compared with
/// the elevation range instead of its aliases.
fn wrap_signed_degrees(degrees: f64) -> f64 {
    let wrapped = degrees.rem_euclid(360.0);
    if wrapped > 180.0 {
        wrapped - 360.0
    } else {
        wrapped
    }
}

/// Shortest angular separation in degrees, so the 0/360 seam is not a jump.
fn angular_gap_degrees(first: f64, second: f64) -> f64 {
    let gap = (first - second).rem_euclid(360.0);
    gap.min(360.0 - gap)
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

    const DISK_CENTER: Point2D = [390.0, 275.0];
    const DISK_RADIUS: f64 = 253.0;

    fn globe_camera() -> Camera3D {
        Camera3D {
            elev_deg: 21.0,
            azim_deg: -43.0,
            zoom: 1.4,
        }
    }

    fn direction_gap(first: Point3D, second: Point3D) -> f64 {
        ((first[0] - second[0]).powi(2)
            + (first[1] - second[1]).powi(2)
            + (first[2] - second[2]).powi(2))
        .sqrt()
    }

    #[test]
    fn a_picked_surface_point_stays_under_the_pointer_after_an_anchored_drag() {
        let camera = globe_camera();
        let grab = [430.0, 232.0];
        let drop = [498.0, 311.0];
        let anchor = camera
            .globe_direction_at(grab, DISK_CENTER, DISK_RADIUS)
            .expect("surface anchor under the pointer");

        let rotated = camera
            .anchored_to(anchor, drop, DISK_CENTER, DISK_RADIUS, 85.0)
            .expect("anchored rotation");
        let under_pointer = rotated
            .globe_direction_at(drop, DISK_CENTER, DISK_RADIUS)
            .expect("surface point under the released pointer");

        assert!(direction_gap(anchor, under_pointer) < 1e-9);
        assert_eq!(rotated.zoom, camera.zoom);
    }

    #[test]
    fn a_pointer_beyond_the_limb_anchors_on_the_horizon_instead_of_jumping() {
        let camera = globe_camera();
        let outside = camera
            .globe_direction_at(
                [DISK_CENTER[0] + DISK_RADIUS * 4.0, DISK_CENTER[1]],
                DISK_CENTER,
                DISK_RADIUS,
            )
            .expect("clamped anchor");

        let norm =
            (outside[0] * outside[0] + outside[1] * outside[1] + outside[2] * outside[2]).sqrt();
        assert!((norm - 1.0).abs() < 1e-12);
        assert!(camera.view_depth(outside, [0.0, 0.0, 0.0]).abs() < 1e-9);

        let rotated = camera
            .anchored_to(
                outside,
                [DISK_CENTER[0] + DISK_RADIUS * 9.0, DISK_CENTER[1] + 20.0],
                DISK_CENTER,
                DISK_RADIUS,
                85.0,
            )
            .expect("clamped rotation");
        assert!(angular_gap_degrees(rotated.azim_deg, camera.azim_deg) < 90.0);
    }

    #[test]
    fn dragging_over_the_pole_saturates_elevation_without_flipping_the_globe() {
        let camera = Camera3D {
            elev_deg: 70.0,
            azim_deg: 12.0,
            zoom: 2.0,
        };
        let anchor = camera
            .globe_direction_at(
                [DISK_CENTER[0], DISK_CENTER[1] - 40.0],
                DISK_CENTER,
                DISK_RADIUS,
            )
            .expect("surface anchor");

        let rotated = camera
            .anchored_to(
                anchor,
                [DISK_CENTER[0], DISK_CENTER[1] + DISK_RADIUS * 0.9],
                DISK_CENTER,
                DISK_RADIUS,
                85.0,
            )
            .expect("anchored rotation near the pole");

        assert!(rotated.elev_deg <= 85.0 + 1e-9);
        assert!(rotated.elev_deg >= -85.0 - 1e-9);
    }

    #[test]
    fn anchored_azimuth_crosses_the_antimeridian_seam_in_small_steps() {
        let camera = Camera3D {
            elev_deg: 5.0,
            azim_deg: 0.4,
            zoom: 1.0,
        };
        let anchor = camera
            .globe_direction_at(DISK_CENTER, DISK_CENTER, DISK_RADIUS)
            .expect("surface anchor at the disk centre");

        let rotated = camera
            .anchored_to(
                anchor,
                [DISK_CENTER[0] - 6.0, DISK_CENTER[1]],
                DISK_CENTER,
                DISK_RADIUS,
                85.0,
            )
            .expect("anchored rotation across the seam");

        assert!((0.0..360.0).contains(&rotated.azim_deg));
        assert!(angular_gap_degrees(rotated.azim_deg, camera.azim_deg) < 5.0);
    }

    #[test]
    fn a_degenerate_globe_disk_has_no_anchor_and_no_rotation() {
        let camera = globe_camera();
        assert!(camera
            .globe_direction_at(DISK_CENTER, DISK_CENTER, 0.0)
            .is_none());
        assert!(camera
            .anchored_to([0.0, 0.0, 1.0], DISK_CENTER, DISK_CENTER, f64::NAN, 85.0)
            .is_none());
        assert!(camera
            .anchored_to([0.0, 0.0, 0.0], DISK_CENTER, DISK_CENTER, DISK_RADIUS, 85.0)
            .is_none());
    }
}
