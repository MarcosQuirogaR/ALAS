// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! CATIA-style navigation of the sandbox viewport.
//!
//! The view is a camera orientation plus a pivot (a model point) that sits at
//! a screen offset `pan` (canvas units from the canvas centre). Orbiting turns
//! the camera about the pivot and never moves it on screen; panning moves the
//! pivot's screen position; the wheel zooms towards the cursor; a pick sets
//! the pivot to the surface point under the cursor without moving the picture.
//! By default the pivot is the centre of the shown components' bounding box.

use alas_report::families::geometry::{SandboxFace, SceneComponent, SceneFraming};

use crate::viewport::PreviewCamera;

/// The camera zoom after multiplying by `factor`, and the pan that keeps the
/// model point under the cursor where it is. `cursor` is the cursor's offset
/// from the canvas centre in canvas units.
pub fn zoom_towards_cursor(
    camera: &mut PreviewCamera,
    pan: &mut (f64, f64),
    factor: f64,
    cursor: (f64, f64),
) {
    let before = camera.zoom;
    camera.apply_zoom_factor(factor);
    let k = if before > 0.0 {
        camera.zoom / before
    } else {
        1.0
    };
    if k.is_finite() {
        pan.0 = cursor.0 - (cursor.0 - pan.0) * k;
        pan.1 = cursor.1 - (cursor.1 - pan.1) * k;
    }
}

/// The nearest surface point of the shown faces under a canvas point, or
/// `None` over empty space. Orthographic: the ray runs along the view
/// direction, and the face with the greatest depth towards the camera wins.
pub fn pick_surface_point(
    faces: &[SandboxFace],
    isolate: Option<SceneComponent>,
    framing: &SceneFraming,
    canvas_point: [f64; 2],
) -> Option<[f64; 3]> {
    let scale = framing.scale();
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let (right, up) = framing.camera.screen_axes();
    let view = framing.view_direction();
    let a = (canvas_point[0] - framing.canvas.0 * 0.5) / scale;
    let b = -(canvas_point[1] - framing.canvas.1 * 0.5) / scale;
    let origin: [f64; 3] = std::array::from_fn(|k| framing.center[k] + a * right[k] + b * up[k]);
    let dot = |x: [f64; 3], y: [f64; 3]| x[0] * y[0] + x[1] * y[1] + x[2] * y[2];
    let mut best: Option<(f64, [f64; 3])> = None;
    for face in faces {
        if isolate.is_some_and(|c| c != face.component) || face.points.len() < 3 {
            continue;
        }
        let facing = dot(face.normal, view);
        if facing.abs() < 1e-9 {
            continue;
        }
        let t = (dot(face.normal, face.points[0]) - dot(face.normal, origin)) / facing;
        let hit: [f64; 3] = std::array::from_fn(|k| origin[k] + t * view[k]);
        if best.is_some_and(|(depth, _)| t <= depth) {
            continue;
        }
        if inside_face(&face.points, face.normal, hit) {
            best = Some((t, hit));
        }
    }
    best.map(|(_, hit)| hit)
}

/// Whether `p`, on the plane of the polygon, lies inside it (same side of
/// every edge as the polygon's normal).
fn inside_face(points: &[[f64; 3]], normal: [f64; 3], p: [f64; 3]) -> bool {
    let mut sign = 0.0_f64;
    for i in 0..points.len() {
        let (p0, p1) = (points[i], points[(i + 1) % points.len()]);
        let edge = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let rel = [p[0] - p0[0], p[1] - p0[1], p[2] - p0[2]];
        let cross = [
            edge[1] * rel[2] - edge[2] * rel[1],
            edge[2] * rel[0] - edge[0] * rel[2],
            edge[0] * rel[1] - edge[1] * rel[0],
        ];
        let side = cross[0] * normal[0] + cross[1] * normal[1] + cross[2] * normal[2];
        if side.abs() < 1e-12 {
            continue;
        }
        if sign == 0.0 {
            sign = side.signum();
        } else if side.signum() != sign {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_report::scene::Camera3D;

    fn quad_facing_x() -> SandboxFace {
        SandboxFace {
            component: SceneComponent::Fuselage,
            points: vec![
                [5.0, -1.0, -1.0],
                [5.0, 1.0, -1.0],
                [5.0, 1.0, 1.0],
                [5.0, -1.0, 1.0],
            ],
            normal: [1.0, 0.0, 0.0],
        }
    }

    fn framing(camera: Camera3D) -> SceneFraming {
        SceneFraming {
            camera,
            center: [0.0, 0.0, 0.0],
            max_span: 10.0,
            viewport: (20.0, 20.0, 760.0, 560.0),
            canvas: (800.0, 600.0),
        }
    }

    #[test]
    fn zoom_keeps_the_model_point_under_the_cursor() {
        let camera = Camera3D {
            elev_deg: 22.0,
            azim_deg: -125.0,
            zoom: 1.0,
        };
        let mut preview = PreviewCamera::isometric();
        let mut pan = (10.0, -5.0);
        let reference = alas_report::families::geometry::FramingReference {
            center: [3.0, 1.0, 0.5],
            extent: 12.0,
        };
        let pivot = [4.0, 0.0, 0.0];
        let cursor = (140.0, 60.0);
        let canvas = (1000.0, 700.0);
        let place = |camera: &PreviewCamera, pan: (f64, f64)| {
            let cam: Camera3D = (*camera).into();
            reference.pivoted(&cam, canvas, pivot, pan)
        };
        // The model point under the cursor before the zoom.
        let before = place(&preview, pan);
        let cam: Camera3D = preview.into();
        let scale = before.scale_for(&cam, canvas);
        let (right, up) = cam.screen_axes();
        let point: [f64; 3] = std::array::from_fn(|k| {
            before.center[k] + cursor.0 / scale * right[k] - cursor.1 / scale * up[k]
        });
        let _ = camera;
        zoom_towards_cursor(&mut preview, &mut pan, 1.5, cursor);
        assert!((preview.zoom - 1.5).abs() < 1e-12);
        let after = place(&preview, pan);
        let cam: Camera3D = preview.into();
        let (x, y) = after.offset_of(&cam, canvas, point);
        assert!(
            (x - cursor.0).abs() < 1e-9 && (y - cursor.1).abs() < 1e-9,
            "{x} {y}"
        );
    }

    #[test]
    fn a_pick_returns_the_nearest_surface_under_the_cursor() {
        let near = quad_facing_x();
        let mut far = quad_facing_x();
        for p in &mut far.points {
            p[0] = -5.0;
        }
        // Camera at +x (rear) sees x = 5 first.
        let rear = framing(PreviewCamera::rear().into());
        let hit = pick_surface_point(&[far.clone(), near.clone()], None, &rear, [400.0, 300.0])
            .expect("hit");
        assert!((hit[0] - 5.0).abs() < 1e-9);
        // Camera ahead (front) sees x = -5 first.
        let front = framing(PreviewCamera::front().into());
        let hit = pick_surface_point(&[far, near], None, &front, [400.0, 300.0]).expect("hit");
        assert!((hit[0] + 5.0).abs() < 1e-9);
        // Empty space far from the quad.
        assert!(pick_surface_point(&[quad_facing_x()], None, &rear, [10.0, 10.0]).is_none());
        // Isolation hides other components.
        assert!(pick_surface_point(
            &[quad_facing_x()],
            Some(SceneComponent::Wing),
            &rear,
            [400.0, 300.0]
        )
        .is_none());
    }
}

#[cfg(test)]
#[path = "navigation_state_tests.rs"]
mod state_tests;
