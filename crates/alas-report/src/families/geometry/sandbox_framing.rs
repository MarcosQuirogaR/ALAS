// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! How a sandbox scene is framed: the reference a caller keeps across edits,
//! the projection a scene was drawn with, and the tight fit of a model to the
//! part of a canvas that its floating controls leave free.

use crate::scene::{Camera3D, Point2D, Point3D};

use super::sandbox_scene::bounds;

/// The model point at the viewport centre and the model extent (a radius
/// about it) that maps to 45 % of the viewport's smaller side at zoom 1,
/// independent of the camera orientation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FramingReference {
    /// The model point mapped to the viewport centre.
    pub center: Point3D,
    /// The model radius about `center` that maps to 45 % of the smaller
    /// viewport side, the same convention as [`Camera3D::project`]'s
    /// `max_span`.
    pub extent: f64,
}

impl FramingReference {
    /// The reference enclosing `points` from every orientation: the centre
    /// of their bounding box and the radius of the sphere about it that
    /// contains them all, so the projected model never leaves the 90 %
    /// central disc of the viewport at zoom 1. `None` for an empty set.
    pub fn enclosing(points: &[Point3D]) -> Option<Self> {
        if points.is_empty() {
            return None;
        }
        let b = bounds(points);
        let center = [
            (b[0] + b[1]) * 0.5,
            (b[2] + b[3]) * 0.5,
            (b[4] + b[5]) * 0.5,
        ];
        let radius = points
            .iter()
            .map(|p| {
                let d = [p[0] - center[0], p[1] - center[1], p[2] - center[2]];
                (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
            })
            .fold(0.0_f64, f64::max);
        Some(Self {
            center,
            extent: radius.max(1e-6),
        })
    }
}

/// Margin of the scene viewport inside its canvas, in canvas units per side.
const VIEWPORT_MARGIN: f64 = 20.0;

impl FramingReference {
    /// Canvas units per model metre for `camera` (its zoom included) on a
    /// `canvas`: the same scale [`SceneFraming::scale`] reports for a scene
    /// drawn with this reference.
    pub fn scale_for(&self, camera: &Camera3D, canvas: (f64, f64)) -> f64 {
        let side = (canvas.0.min(canvas.1) - 2.0 * VIEWPORT_MARGIN).max(1e-6);
        side * 0.45 * camera.zoom / self.extent.max(1e-6)
    }

    /// Where `point` lands relative to the canvas centre, in canvas units
    /// (x right, y down), under this reference.
    pub fn offset_of(&self, camera: &Camera3D, canvas: (f64, f64), point: Point3D) -> (f64, f64) {
        let scale = self.scale_for(camera, canvas);
        let (right, up) = camera.screen_axes();
        let d = [
            point[0] - self.center[0],
            point[1] - self.center[1],
            point[2] - self.center[2],
        ];
        let dot = |axis: Point3D| d[0] * axis[0] + d[1] * axis[1] + d[2] * axis[2];
        (dot(right) * scale, -dot(up) * scale)
    }

    /// The reference with the same extent whose centre puts `pivot` at
    /// `pan` canvas units from the canvas centre for `camera`. Orbiting the
    /// camera about a fixed `pivot` and `pan` therefore keeps the pivot at
    /// one place on screen; a pan moves it, an orbit never does.
    pub fn pivoted(
        &self,
        camera: &Camera3D,
        canvas: (f64, f64),
        pivot: Point3D,
        pan: (f64, f64),
    ) -> Self {
        let scale = self.scale_for(camera, canvas);
        let (right, up) = camera.screen_axes();
        let center =
            std::array::from_fn(|k| pivot[k] - pan.0 / scale * right[k] + pan.1 / scale * up[k]);
        Self {
            center,
            extent: self.extent,
        }
    }
}

/// The projection a sandbox scene was drawn with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneFraming {
    /// The camera used for every face.
    pub camera: Camera3D,
    /// The model point mapped to the viewport centre.
    pub center: Point3D,
    /// The model span that fills the viewport.
    pub max_span: f64,
    /// The viewport rectangle inside the scene canvas.
    pub viewport: (f64, f64, f64, f64),
    /// The scene canvas size.
    pub canvas: (f64, f64),
}

impl SceneFraming {
    /// Project a model point into scene canvas coordinates.
    pub fn project(&self, point: Point3D) -> Point2D {
        self.camera
            .project(point, self.center, self.max_span, self.viewport)
    }

    /// The unit view direction (from the model toward the viewer) in model
    /// axes, used for headlight shading and painter ordering.
    pub fn view_direction(&self) -> Point3D {
        let elev = self.camera.elev_deg.to_radians();
        let azim = self.camera.azim_deg.to_radians();
        [azim.sin() * elev.cos(), azim.cos() * elev.cos(), elev.sin()]
    }

    /// The framing reference this scene was drawn with.
    pub fn reference(&self) -> FramingReference {
        FramingReference {
            center: self.center,
            extent: self.max_span,
        }
    }

    /// Canvas units per model metre, the same in every direction of an
    /// orthographic view.
    pub fn scale(&self) -> f64 {
        let (_, _, vw, vh) = self.viewport;
        vw.min(vh) * 0.45 * self.camera.zoom / self.max_span.max(1e-6)
    }
}

/// The tight framing of `points` for `camera` on a `canvas` whose edges
/// `inset` (left, top, right, bottom) are covered by controls: the model
/// fills `fill` of the free rectangle and is centred in it.
pub(super) fn tight_reference(
    points: &[Point3D],
    camera: Camera3D,
    canvas: (f64, f64),
    fill: f64,
    inset: (f64, f64, f64, f64),
) -> FramingReference {
    let camera = Camera3D {
        zoom: 1.0,
        ..camera
    };
    let (width, height) = canvas;
    let (left, top, right, bottom) = inset;
    let free = (
        (width - left - right).max(40.0),
        (height - top - bottom).max(40.0),
    );
    let target = (0.0, 0.0, free.0 * fill, free.1 * fill);
    let span = camera.fit_span_to_points(points, target, 0.0);
    // `fit_span_to_points` scales by the smaller side of the viewport it is
    // given; the scene is drawn in its own, smaller, viewport (20 units of
    // margin on each side), whose smaller side sets the pixels per metre.
    let viewport_min = (width.min(height) - 40.0).max(1.0);
    let target_min = (free.0.min(free.1) * fill).max(1.0);
    let extent = (span * viewport_min / target_min).max(1e-6);
    let fitted = camera.fit_center_to_points(points, [0.0; 3]);
    // The fitted centre appears at the middle of the canvas; the middle of the
    // free rectangle is `(dx, dy)` canvas units away from it. Moving the
    // reference centre by the opposite of that, along the camera's screen
    // axes, puts the model there.
    let (dx, dy) = (0.5 * (left - right), 0.5 * (top - bottom));
    let scale = viewport_min * 0.45 / extent;
    let (right_axis, up_axis) = camera.screen_axes();
    let center =
        std::array::from_fn(|k| fitted[k] - dx / scale * right_axis[k] + dy / scale * up_axis[k]);
    FramingReference { center, extent }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pivot placed at a pan offset stays at that screen place for every
    /// camera orientation: an orbit never moves it.
    #[test]
    fn a_pivot_keeps_its_screen_offset_while_the_camera_orbits() {
        let canvas = (1200.0, 800.0);
        let reference = FramingReference {
            center: [20.0, 0.0, 0.0],
            extent: 40.0,
        };
        let pivot = [31.0, 0.0, 1.5];
        let pan = (37.0, -12.0);
        for (elev, azim, zoom) in [(22.0, -125.0, 1.0), (0.0, -90.0, 2.0), (90.0, 0.0, 0.5)] {
            let camera = Camera3D {
                elev_deg: elev,
                azim_deg: azim,
                zoom,
            };
            let placed = reference.pivoted(&camera, canvas, pivot, pan);
            let (x, y) = placed.offset_of(&camera, canvas, pivot);
            assert!((x - pan.0).abs() < 1e-9 && (y - pan.1).abs() < 1e-9);
            // The same offset through the projection the scene is drawn with.
            let framing = SceneFraming {
                camera,
                center: placed.center,
                max_span: placed.extent,
                viewport: (20.0, 20.0, canvas.0 - 40.0, canvas.1 - 40.0),
                canvas,
            };
            let [px, py] = framing.project(pivot);
            assert!((px - (canvas.0 * 0.5 + pan.0)).abs() < 1e-9);
            assert!((py - (canvas.1 * 0.5 + pan.1)).abs() < 1e-9);
            assert!((framing.scale() - placed.scale_for(&camera, canvas)).abs() < 1e-9);
        }
    }
}
