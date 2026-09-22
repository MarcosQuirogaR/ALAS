// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Earth background for geographic route figures.
//!
//! The 2-D map uses the separately bundled NASA Blue Marble raster. The
//! globe maps that equirectangular source onto an orthographic sphere in the
//! raster renderer. Keeping the Earth surface to one raster prevents a
//! second, differently projected coastline layer from obscuring the route.

use crate::route_geometry::EARTH_RADIUS_KM;
use crate::scene::{Camera3D, Color, Point2D, Point3D, Scene, SceneElement, Stroke};
use crate::theme::Palette;

const BLUE_MARBLE_SOURCE: &str = "embedded://nasa-blue-marble";
const CENTER: Point3D = [0.0, 0.0, 0.0];

/// Segments used to approximate a clipped globe contour.
const CONTOUR_SEGMENTS: usize = 256;

/// Screen-space disk the projected globe occupies: centre and radius in scene
/// coordinates.
///
/// Pointer picking and texture clipping both need this, so the zoom-dependent
/// radius is defined once next to the element that draws it.
pub(super) fn globe_disk(
    camera: &Camera3D,
    viewport: (f64, f64, f64, f64),
    span_km: f64,
) -> (Point2D, f64) {
    (
        camera.project(CENTER, CENTER, span_km, viewport),
        viewport.2.min(viewport.3) * 0.45 * camera.zoom * EARTH_RADIUS_KM / span_km.max(1e-6),
    )
}

/// Draw an opaque, geographically registered orthographic Earth texture.
pub(super) fn draw_textured_earth(
    scene: &mut Scene,
    camera: &Camera3D,
    viewport: (f64, f64, f64, f64),
    span_km: f64,
    palette: &Palette,
) {
    let (center, radius) = globe_disk(camera, viewport, span_km);
    scene.add(SceneElement::SphericalImage {
        source: BLUE_MARBLE_SOURCE.to_owned(),
        center,
        radius,
        camera: *camera,
        mirror_longitude: true,
        clip: Some([viewport.0, viewport.1, viewport.2, viewport.3]),
    });
    let stroke = Stroke::new(Color::from_hex(palette.spine), 1.0);
    if disk_fits_viewport(center, radius, viewport) {
        scene.add(SceneElement::Circle {
            center,
            radius,
            fill: None,
            stroke: Some(stroke),
        });
        return;
    }
    // A zoomed globe reaches past its viewport. Drawing the remaining contour
    // as clipped arcs keeps the horizon visible where it exists instead of
    // ringing the figure's title and colorbar with a circle.
    for arc in clip_polyline(&contour_points(center, radius), viewport) {
        scene.add(SceneElement::Polyline {
            points: arc,
            stroke: stroke.clone(),
        });
    }
}

fn disk_fits_viewport(center: Point2D, radius: f64, viewport: (f64, f64, f64, f64)) -> bool {
    let (x, y, width, height) = viewport;
    center[0] - radius >= x
        && center[0] + radius <= x + width
        && center[1] - radius >= y
        && center[1] + radius <= y + height
}

fn contour_points(center: Point2D, radius: f64) -> Vec<Point2D> {
    (0..=CONTOUR_SEGMENTS)
        .map(|step| {
            let angle = std::f64::consts::TAU * step as f64 / CONTOUR_SEGMENTS as f64;
            [
                center[0] + radius * angle.cos(),
                center[1] + radius * angle.sin(),
            ]
        })
        .collect()
}

/// Whether a scene point lies inside a viewport rectangle.
pub(super) fn viewport_contains(point: Point2D, viewport: (f64, f64, f64, f64)) -> bool {
    let (x, y, width, height) = viewport;
    (x..=x + width).contains(&point[0]) && (y..=y + height).contains(&point[1])
}

/// Split a polyline into the fragments that lie inside a viewport rectangle.
///
/// Liang-Barsky per segment: the figure's vector elements are painted without
/// a renderer-side clip, so a zoomed route is bounded here rather than left to
/// bleed across the surrounding labels.
pub(super) fn clip_polyline(
    points: &[Point2D],
    viewport: (f64, f64, f64, f64),
) -> Vec<Vec<Point2D>> {
    let mut fragments = Vec::new();
    let mut current: Vec<Point2D> = Vec::new();
    for pair in points.windows(2) {
        let Some((entry, exit)) = clip_segment(pair[0], pair[1], viewport) else {
            if current.len() >= 2 {
                fragments.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
            continue;
        };
        if current.last().is_none_or(|last| {
            (last[0] - entry[0]).abs() > 1e-9 || (last[1] - entry[1]).abs() > 1e-9
        }) {
            if current.len() >= 2 {
                fragments.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
            current.push(entry);
        }
        current.push(exit);
    }
    if current.len() >= 2 {
        fragments.push(current);
    }
    fragments
}

fn clip_segment(
    start: Point2D,
    end: Point2D,
    viewport: (f64, f64, f64, f64),
) -> Option<(Point2D, Point2D)> {
    if !start
        .iter()
        .chain(end.iter())
        .all(|value| value.is_finite())
    {
        return None;
    }
    let (x, y, width, height) = viewport;
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let mut enter = 0.0_f64;
    let mut leave = 1.0_f64;
    for (direction, distance) in [
        (-dx, start[0] - x),
        (dx, x + width - start[0]),
        (-dy, start[1] - y),
        (dy, y + height - start[1]),
    ] {
        if direction.abs() <= f64::EPSILON {
            if distance < 0.0 {
                return None;
            }
            continue;
        }
        let fraction = distance / direction;
        if direction < 0.0 {
            enter = enter.max(fraction);
        } else {
            leave = leave.min(fraction);
        }
    }
    if enter > leave {
        return None;
    }
    Some((
        [start[0] + enter * dx, start[1] + enter * dy],
        [start[0] + leave * dx, start[1] + leave * dy],
    ))
}

/// Convert geographic degrees to the image coordinates of an equirectangular texture.
///
/// U advances east from the antimeridian and V advances south from the north
/// pole. This is the same latitude/longitude convention `route_to_xyz` uses.
#[cfg(test)]
fn equirectangular_uv(latitude_deg: f64, longitude_deg: f64) -> Point2D {
    [
        (longitude_deg + 180.0).rem_euclid(360.0) / 360.0,
        (90.0 - latitude_deg) / 180.0,
    ]
}

fn normalize(point: Point3D) -> Point3D {
    let norm = (point[0] * point[0] + point[1] * point[1] + point[2] * point[2]).sqrt();
    if norm <= f64::EPSILON {
        [1.0, 0.0, 0.0]
    } else {
        [point[0] / norm, point[1] / norm, point[2] / norm]
    }
}

fn horizon_intersection(a: Point3D, b: Point3D, da: f64, db: f64) -> Point3D {
    let fraction = (da / (da - db)).clamp(0.0, 1.0);
    let direction = normalize([
        a[0] / EARTH_RADIUS_KM + fraction * (b[0] - a[0]) / EARTH_RADIUS_KM,
        a[1] / EARTH_RADIUS_KM + fraction * (b[1] - a[1]) / EARTH_RADIUS_KM,
        a[2] / EARTH_RADIUS_KM + fraction * (b[2] - a[2]) / EARTH_RADIUS_KM,
    ]);
    [
        direction[0] * EARTH_RADIUS_KM,
        direction[1] * EARTH_RADIUS_KM,
        direction[2] * EARTH_RADIUS_KM,
    ]
}

/// Project visible fragments of a spherical polyline, splitting at the horizon
/// and at the viewport boundary.
pub(super) fn project_visible_path(
    points: impl IntoIterator<Item = Point3D>,
    camera: &Camera3D,
    viewport: (f64, f64, f64, f64),
    span_km: f64,
) -> Vec<Vec<Point2D>> {
    let mut output = Vec::new();
    let mut current = Vec::new();
    let mut previous: Option<(Point3D, f64)> = None;
    for point in points {
        let depth = camera.view_depth(point, CENTER);
        if let Some((previous_point, previous_depth)) = previous {
            if (previous_depth >= 0.0) != (depth >= 0.0) {
                let horizon = camera.project(
                    horizon_intersection(previous_point, point, previous_depth, depth),
                    CENTER,
                    span_km,
                    viewport,
                );
                if previous_depth >= 0.0 {
                    current.push(horizon);
                    if current.len() >= 2 {
                        output.push(std::mem::take(&mut current));
                    }
                } else {
                    current.push(horizon);
                }
            }
        }
        if depth >= 0.0 {
            current.push(camera.project(point, CENTER, span_km, viewport));
        }
        previous = Some((point, depth));
    }
    if current.len() >= 2 {
        output.push(current);
    }
    output
        .iter()
        .flat_map(|fragment| clip_polyline(fragment, viewport))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blue_marble_uv_places_greenwich_and_iberia_at_their_geographic_coordinates() {
        let greenwich = equirectangular_uv(0.0, 0.0);
        assert!((greenwich[0] - 0.5).abs() < 1e-12);
        assert!((greenwich[1] - 0.5).abs() < 1e-12);
        let iberia = equirectangular_uv(40.4168, -3.7038);
        assert!((iberia[0] - 0.489_711_666_666_666_7).abs() < 1e-12);
        assert!((iberia[1] - 0.275_462_222_222_222_2).abs() < 1e-12);
    }

    const VIEWPORT: (f64, f64, f64, f64) = (30.0, 50.0, 720.0, 450.0);

    #[test]
    fn a_polyline_leaving_the_viewport_is_cut_at_the_boundary() {
        let fragments = clip_polyline(&[[400.0, 300.0], [400.0, 900.0]], VIEWPORT);
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0][0], [400.0, 300.0]);
        assert!((fragments[0][1][1] - 500.0).abs() < 1e-9);

        assert!(clip_polyline(&[[900.0, 700.0], [1000.0, 800.0]], VIEWPORT).is_empty());
    }

    #[test]
    fn a_polyline_that_re_enters_the_viewport_keeps_separate_fragments() {
        let fragments = clip_polyline(
            &[
                [400.0, 300.0],
                [400.0, 900.0],
                [500.0, 900.0],
                [500.0, 300.0],
            ],
            VIEWPORT,
        );
        assert_eq!(fragments.len(), 2);
        assert!(fragments
            .iter()
            .flatten()
            .all(|point| viewport_contains(*point, VIEWPORT)));
    }

    #[test]
    fn a_zoomed_globe_contour_is_drawn_as_arcs_inside_the_viewport_only() {
        let mut scene = Scene::new(860.0, 540.0, None);
        let camera = Camera3D {
            zoom: 3.0,
            ..Camera3D::default()
        };
        draw_textured_earth(
            &mut scene,
            &camera,
            VIEWPORT,
            1.12 * EARTH_RADIUS_KM,
            crate::theme::get_palette(Some("dark")),
        );

        assert!(!scene
            .elements
            .iter()
            .any(|element| matches!(element, SceneElement::Circle { .. })));
        for element in &scene.elements {
            if let SceneElement::Polyline { points, .. } = element {
                assert!(points
                    .iter()
                    .all(|point| viewport_contains(*point, VIEWPORT)));
            }
            if let SceneElement::SphericalImage { clip, radius, .. } = element {
                assert_eq!(*clip, Some([30.0, 50.0, 720.0, 450.0]));
                assert!(*radius > VIEWPORT.3 * 0.5);
            }
        }
    }

    #[test]
    fn an_unzoomed_globe_keeps_its_exact_circular_contour() {
        let mut scene = Scene::new(860.0, 540.0, None);
        draw_textured_earth(
            &mut scene,
            &Camera3D::default(),
            VIEWPORT,
            1.12 * EARTH_RADIUS_KM,
            crate::theme::get_palette(Some("dark")),
        );

        assert_eq!(
            scene
                .elements
                .iter()
                .filter(|element| matches!(element, SceneElement::Circle { .. }))
                .count(),
            1
        );
    }
}
