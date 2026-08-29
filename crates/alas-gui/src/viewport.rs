// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Keyed camera state for the GUI's three-dimensional preview viewports.

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
            self.pitch_deg = (self.pitch_deg - f64::from(delta.y) * 0.45).clamp(-85.0, 85.0);
        }
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
}
