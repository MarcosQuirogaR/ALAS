// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Keyed camera state for the GUI's three-dimensional preview viewports.

/// Elevation bound for interactive orbiting.
///
/// The report camera keeps world north up and has no roll, so passing the
/// pole would mirror the scene instead of continuing the gesture.
pub const ORBIT_PITCH_LIMIT_DEG: f64 = 85.0;

/// Orbit state owned by one three-dimensional preview.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewCamera {
    /// Camera elevation in degrees.
    pub pitch_deg: f64,
    /// Camera azimuth in degrees.
    pub yaw_deg: f64,
    /// Camera magnification, independent from every 2-D viewport zoom.
    pub zoom: f64,
}

impl Default for PreviewCamera {
    fn default() -> Self {
        Self::from(alas_report::scene::Camera3D::isometric())
    }
}

impl PreviewCamera {
    /// Return the top camera from the shared report camera contract.
    pub fn top() -> Self {
        Self::from(alas_report::scene::Camera3D::top())
    }

    /// Return the front camera from the shared report camera contract.
    pub fn front() -> Self {
        Self::from(alas_report::scene::Camera3D::front())
    }

    /// Return the side camera from the shared report camera contract.
    pub fn side() -> Self {
        Self::from(alas_report::scene::Camera3D::side())
    }

    /// Return the isometric camera from the shared report camera contract.
    pub fn isometric() -> Self {
        Self::from(alas_report::scene::Camera3D::isometric())
    }

    /// Restore framing without changing the active camera's orientation.
    pub fn fit(&mut self) {
        self.zoom = 1.0;
    }

    /// Magnify the projected aircraft while keeping screen-space annotations fixed.
    pub fn apply_zoom_factor(&mut self, factor: f64) {
        if factor.is_finite() && factor > 0.0 {
            self.zoom = (self.zoom * factor).clamp(0.15, 8.0);
        }
    }

    /// Apply one frame of pointer motion without replaying the gesture total.
    pub fn apply_orbit_motion(&mut self, delta: egui::Vec2) {
        if delta.is_finite() {
            self.yaw_deg = (self.yaw_deg + f64::from(delta.x) * 0.45).rem_euclid(360.0);
            self.pitch_deg = (self.pitch_deg - f64::from(delta.y) * 0.45)
                .clamp(-ORBIT_PITCH_LIMIT_DEG, ORBIT_PITCH_LIMIT_DEG);
        }
    }

    /// Rotate a projected globe so the surface point under `from` follows the
    /// pointer to `to`, both in the scene coordinates of the figure that drew
    /// the disk `(center, radius)`.
    ///
    /// Unlike [`Self::apply_orbit_motion`], which sweeps a fixed number of
    /// degrees per pixel, this keeps the grabbed location under the cursor at
    /// any zoom. Returns whether the camera changed, so a gesture that has no
    /// solution can fall back to the angular sweep.
    pub fn apply_surface_anchored_drag(
        &mut self,
        from: [f64; 2],
        to: [f64; 2],
        center: [f64; 2],
        radius: f64,
    ) -> bool {
        let camera = alas_report::scene::Camera3D::from(*self);
        let Some(anchor) = camera.globe_direction_at(from, center, radius) else {
            return false;
        };
        let Some(rotated) = camera.anchored_to(anchor, to, center, radius, ORBIT_PITCH_LIMIT_DEG)
        else {
            return false;
        };
        let updated = Self::from(rotated);
        let changed = updated != *self;
        *self = updated;
        changed
    }
}

impl From<alas_report::scene::Camera3D> for PreviewCamera {
    fn from(camera: alas_report::scene::Camera3D) -> Self {
        Self {
            pitch_deg: camera.elev_deg,
            yaw_deg: camera.azim_deg,
            zoom: camera.zoom,
        }
    }
}

impl From<PreviewCamera> for alas_report::scene::Camera3D {
    fn from(camera: PreviewCamera) -> Self {
        Self {
            elev_deg: camera.pitch_deg,
            azim_deg: camera.yaw_deg,
            zoom: camera.zoom,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PreviewCamera;

    #[test]
    fn orbit_motion_accumulates_each_frame_without_resetting_the_camera() {
        let mut camera = PreviewCamera::isometric();
        camera.apply_orbit_motion(egui::vec2(100.0, 20.0));
        camera.apply_orbit_motion(egui::vec2(50.0, -10.0));

        assert_eq!(camera.yaw_deg, 302.5);
        assert_eq!(camera.pitch_deg, 17.5);
    }

    #[test]
    fn gui_camera_converts_back_to_the_report_projection_contract() {
        let camera = PreviewCamera {
            pitch_deg: 31.0,
            yaw_deg: -72.0,
            zoom: 1.8,
        };

        let report_camera = alas_report::scene::Camera3D::from(camera);

        assert_eq!(report_camera.elev_deg, 31.0);
        assert_eq!(report_camera.azim_deg, -72.0);
        assert_eq!(report_camera.zoom, 1.8);
    }

    #[test]
    fn camera_zoom_is_bounded_and_ignores_invalid_input() {
        let mut camera = PreviewCamera::isometric();
        camera.apply_zoom_factor(100.0);
        assert_eq!(camera.zoom, 8.0);

        camera.apply_zoom_factor(0.0001);
        assert_eq!(camera.zoom, 0.15);

        camera.apply_zoom_factor(f64::NAN);
        assert_eq!(camera.zoom, 0.15);
    }

    #[test]
    fn a_surface_anchored_drag_moves_the_grabbed_location_to_the_pointer() {
        let mut camera = PreviewCamera {
            pitch_deg: 18.0,
            yaw_deg: 140.0,
            zoom: 2.5,
        };
        let center = [390.0, 275.0];
        let radius = 452.0;
        let grabbed = alas_report::scene::Camera3D::from(camera)
            .globe_direction_at([430.0, 250.0], center, radius)
            .expect("grabbed surface point");

        assert!(camera.apply_surface_anchored_drag([430.0, 250.0], [470.0, 300.0], center, radius));

        let released = alas_report::scene::Camera3D::from(camera)
            .globe_direction_at([470.0, 300.0], center, radius)
            .expect("surface point under the released pointer");
        for axis in 0..3 {
            assert!((grabbed[axis] - released[axis]).abs() < 1e-9);
        }
        assert_eq!(camera.zoom, 2.5);
    }

    #[test]
    fn an_anchored_drag_covers_less_angle_at_close_zoom_than_the_fixed_sweep() {
        let start = PreviewCamera {
            pitch_deg: 0.0,
            yaw_deg: 0.0,
            zoom: 4.0,
        };
        let center = [390.0, 275.0];
        // Globe radius at 4x zoom in the route figure's scene coordinates.
        let radius = 723.0;

        let mut anchored = start;
        assert!(anchored.apply_surface_anchored_drag(
            [390.0, 275.0],
            [450.0, 275.0],
            center,
            radius
        ));
        let mut swept = start;
        swept.apply_orbit_motion(egui::vec2(60.0, 0.0));

        let gap = |first: f64, second: f64| {
            let raw = (first - second).rem_euclid(360.0);
            raw.min(360.0 - raw)
        };
        let anchored_gap = gap(anchored.yaw_deg, start.yaw_deg);
        let swept_gap = gap(swept.yaw_deg, start.yaw_deg);
        assert!(anchored_gap < swept_gap);
        assert!(anchored_gap > 0.0);
    }

    #[test]
    fn a_degenerate_globe_disk_leaves_the_camera_untouched() {
        let mut camera = PreviewCamera::isometric();
        assert!(!camera.apply_surface_anchored_drag(
            [10.0, 10.0],
            [20.0, 20.0],
            [390.0, 275.0],
            0.0
        ));
        assert_eq!(camera, PreviewCamera::isometric());
    }
}
