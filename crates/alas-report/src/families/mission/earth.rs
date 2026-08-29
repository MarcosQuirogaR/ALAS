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

/// Draw an opaque, geographically registered orthographic Earth texture.
pub(super) fn draw_textured_earth(
    scene: &mut Scene,
    camera: &Camera3D,
    viewport: (f64, f64, f64, f64),
    span_km: f64,
    palette: &Palette,
) {
    let center = camera.project(CENTER, CENTER, span_km, viewport);
    let radius = viewport.2.min(viewport.3) * 0.45 * camera.zoom * EARTH_RADIUS_KM / span_km;
    scene.add(SceneElement::SphericalImage {
        source: BLUE_MARBLE_SOURCE.to_owned(),
        center,
        radius,
        camera: *camera,
        mirror_longitude: true,
    });
    scene.add(SceneElement::Circle {
        center,
        radius,
        fill: None,
        stroke: Some(Stroke::new(Color::from_hex(palette.spine), 1.0)),
    });
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

/// Project visible fragments of a spherical polyline, splitting at the horizon.
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
}
