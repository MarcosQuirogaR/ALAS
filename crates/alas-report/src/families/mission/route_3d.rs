// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native three-dimensional globe view of the planned and flown route.
//!
//! The Python application used PyVista for this view. The Rust implementation
//! keeps the same physical coordinate model without adding a second renderer:
//! waypoints and altitude are converted to Earth-centered Cartesian
//! coordinates, great-circle legs are sampled on the sphere, and the shared
//! orthographic report camera projects the globe into the backend-neutral
//! scene graph used by the desktop, SVG, raster, and PDF paths.

use alas_route::route::Route;

use crate::chart_kit::draw_colorbar;
use crate::colormap::Colormap;
use crate::families::mission::earth::{draw_textured_earth, project_visible_path};
use crate::route_geometry::{route_to_xyz, EARTH_RADIUS_KM, PATH_VISIBILITY_OFFSET_KM};
use crate::scene::{
    Camera3D, Color, Fill, Point3D, Scene, SceneElement, Stroke, TextAlign, TextBaseline,
};
use crate::theme::get_palette;

const ARC_SAMPLES_PER_LEG: usize = 32;
const VIEWPORT: (f64, f64, f64, f64) = (30.0, 50.0, 720.0, 450.0);
const GLOBE_FRAMING_SPAN_KM: f64 = 1.12 * EARTH_RADIUS_KM;
const MIN_WAYPOINT_LABEL_SEPARATION_PX: f64 = 64.0;

fn normalize(point: Point3D) -> Point3D {
    let norm = (point[0] * point[0] + point[1] * point[1] + point[2] * point[2]).sqrt();
    if norm <= f64::EPSILON {
        [1.0, 0.0, 0.0]
    } else {
        [point[0] / norm, point[1] / norm, point[2] / norm]
    }
}

/// Use a conventional geographic presentation in the projected globe: east
/// advances to the visual right, as it does on the route's 2-D map.
fn mirror_longitude(point: Point3D) -> Point3D {
    [point[0], -point[1], point[2]]
}

fn route_to_globe_xyz(route: &Route, altitude_profile: &[f64]) -> Vec<Point3D> {
    route_to_xyz(route, altitude_profile)
        .into_iter()
        .map(mirror_longitude)
        .collect()
}

fn great_circle_direction(start: Point3D, end: Point3D, fraction: f64) -> Point3D {
    let a = normalize(start);
    let b = normalize(end);
    let dot = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]).clamp(-1.0, 1.0);
    let angle = dot.acos();
    if angle.abs() < 1e-10 || angle.sin().abs() < 1e-10 {
        return normalize([
            a[0] + fraction * (b[0] - a[0]),
            a[1] + fraction * (b[1] - a[1]),
            a[2] + fraction * (b[2] - a[2]),
        ]);
    }
    let start_weight = ((1.0 - fraction) * angle).sin() / angle.sin();
    let end_weight = (fraction * angle).sin() / angle.sin();
    normalize([
        start_weight * a[0] + end_weight * b[0],
        start_weight * a[1] + end_weight * b[1],
        start_weight * a[2] + end_weight * b[2],
    ])
}

fn sampled_route(
    route: &Route,
    mass_profile: Option<&[f64]>,
    altitude_profile: Option<&[f64]>,
) -> Vec<(Point3D, Option<f64>)> {
    let waypoint_count = route.waypoints.len();
    if waypoint_count == 0 {
        return Vec::new();
    }
    let masses = mass_profile.filter(|values| {
        values.len() == waypoint_count && values.iter().all(|value| value.is_finite())
    });
    let altitudes = altitude_profile.filter(|values| {
        values.len() == waypoint_count && values.iter().all(|value| value.is_finite())
    });
    let waypoint_xyz = route_to_globe_xyz(route, altitudes.unwrap_or(&[]));
    if waypoint_count == 1 {
        return vec![(waypoint_xyz[0], masses.map(|values| values[0]))];
    }

    let mut sampled = Vec::with_capacity((waypoint_count - 1) * ARC_SAMPLES_PER_LEG + 1);
    for leg in 0..waypoint_count - 1 {
        let start_altitude_km = altitudes.map_or(0.0, |values| values[leg] / 1000.0);
        let end_altitude_km = altitudes.map_or(0.0, |values| values[leg + 1] / 1000.0);
        for sample in 0..ARC_SAMPLES_PER_LEG {
            let fraction = sample as f64 / ARC_SAMPLES_PER_LEG as f64;
            let direction =
                great_circle_direction(waypoint_xyz[leg], waypoint_xyz[leg + 1], fraction);
            let altitude_km = start_altitude_km + fraction * (end_altitude_km - start_altitude_km);
            let radius = EARTH_RADIUS_KM + PATH_VISIBILITY_OFFSET_KM + altitude_km;
            let mass =
                masses.map(|values| values[leg] + fraction * (values[leg + 1] - values[leg]));
            sampled.push((
                [
                    direction[0] * radius,
                    direction[1] * radius,
                    direction[2] * radius,
                ],
                mass,
            ));
        }
    }
    sampled.push((
        waypoint_xyz[waypoint_count - 1],
        masses.map(|values| values[waypoint_count - 1]),
    ));
    sampled
}

/// Center an initial globe view on the route instead of an arbitrary global
/// longitude. Orbit and named camera controls remain relative to this view.
pub fn route_focused_camera(route: &Route) -> Camera3D {
    let mut direction = [0.0; 3];
    for waypoint in &route.waypoints {
        if !waypoint.lat.is_finite() || !waypoint.lon.is_finite() {
            continue;
        }
        let latitude = waypoint.lat.to_radians();
        let longitude = waypoint.lon.to_radians();
        direction[0] += latitude.cos() * longitude.cos();
        direction[1] -= latitude.cos() * longitude.sin();
        direction[2] += latitude.sin();
    }
    let norm =
        (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2])
            .sqrt();
    if norm <= f64::EPSILON {
        return Camera3D::default();
    }
    let longitude = direction[1].atan2(direction[0]);
    let latitude = (direction[2] / norm).clamp(-1.0, 1.0).asin();
    Camera3D {
        elev_deg: latitude.to_degrees(),
        azim_deg: 90.0 - longitude.to_degrees(),
        zoom: 1.0,
    }
}

/// Draw an orbitable globe route with optional mass and altitude profiles.
pub fn figure_mission_route_3d(
    route: &Route,
    mass_profile: Option<&[f64]>,
    altitude_profile: Option<&[f64]>,
    camera: Option<Camera3D>,
    theme: Option<&str>,
) -> Scene {
    let palette = get_palette(theme);
    let mut scene = Scene::new(860.0, 540.0, Some(Color::from_hex(palette.bg)));
    scene.title = Some("Mission route globe".to_owned());
    scene.suppress_derived_title();
    scene.add(SceneElement::Text {
        text: "Mission route globe".to_owned(),
        pos: [24.0, 20.0],
        font_size: 13.0,
        color: Color::from_hex(palette.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: true,
    });
    let camera = camera.unwrap_or_else(|| route_focused_camera(route));
    draw_textured_earth(
        &mut scene,
        &camera,
        VIEWPORT,
        GLOBE_FRAMING_SPAN_KM,
        palette,
    );
    let sampled = sampled_route(route, mass_profile, altitude_profile);
    let center = [0.0, 0.0, 0.0];
    let span = GLOBE_FRAMING_SPAN_KM;
    let finite_masses: Vec<f64> = sampled
        .iter()
        .filter_map(|(_, mass)| mass.filter(|value| value.is_finite()))
        .collect();
    let mass_range: Option<(f64, f64)> =
        finite_masses.iter().copied().fold(None, |range, value| {
            Some(match range {
                None => (value, value),
                Some((minimum, maximum)) => (minimum.min(value), maximum.max(value)),
            })
        });
    for pair in sampled.windows(2) {
        let color = match (pair[0].1, mass_range) {
            (Some(mass), Some((minimum, maximum))) => {
                Colormap::Jet.sample((mass - minimum) / (maximum - minimum).max(f64::EPSILON))
            }
            _ => Color::from_hex("#00d8ff"),
        };
        for points in project_visible_path([pair[0].0, pair[1].0], &camera, VIEWPORT, span) {
            if points.len() >= 2 {
                scene.add(SceneElement::Polyline {
                    points,
                    stroke: Stroke::new(color, 2.6),
                });
            }
        }
    }
    if let Some((minimum, maximum)) = mass_range {
        draw_colorbar(
            &mut scene,
            (770.0, 90.0, 12.0, 330.0),
            Colormap::Jet,
            minimum / 1_000.0,
            maximum / 1_000.0,
            "Total Mass (t)",
            palette,
        );
    }

    let waypoint_xyz = route_to_globe_xyz(route, altitude_profile.unwrap_or(&[]));
    let mut last_label_position: Option<[f64; 2]> = None;
    for (index, (waypoint, xyz)) in route.waypoints.iter().zip(waypoint_xyz).enumerate() {
        if camera.view_depth(xyz, center) < 0.0 {
            continue;
        }
        let projected = camera.project(xyz, center, span, VIEWPORT);
        let marker = if index == 0 {
            "#2ecc71"
        } else if index + 1 == route.waypoints.len() {
            "#e74c3c"
        } else {
            "#d0d0d0"
        };
        scene.add(SceneElement::Circle {
            center: projected,
            radius: 3.4,
            fill: Some(Fill::new(Color::from_hex(marker))),
            stroke: Some(Stroke::new(Color::from_hex(palette.spine), 0.8)),
        });
        let is_endpoint = index == 0 || index + 1 == route.waypoints.len();
        let separated = last_label_position.is_none_or(|previous| {
            let dx = projected[0] - previous[0];
            let dy = projected[1] - previous[1];
            dx * dx + dy * dy >= MIN_WAYPOINT_LABEL_SEPARATION_PX.powi(2)
        });
        if !waypoint.ident.is_empty() && (is_endpoint || separated) {
            let vertical_offset = if index % 2 == 0 { -7.0 } else { 8.0 };
            scene.add(SceneElement::Text {
                text: waypoint.ident.clone(),
                pos: [projected[0] + 5.0, projected[1] + vertical_offset],
                font_size: 9.0,
                color: Color::from_hex(palette.tick),
                align: TextAlign::Left,
                baseline: TextBaseline::Bottom,
                angle_deg: 0.0,
                bold: false,
            });
            last_label_position = Some(projected);
        }
    }

    let profile_note = if mass_profile.is_some_and(|values| {
        values.len() == route.waypoints.len() && values.iter().all(|value| value.is_finite())
    }) && altitude_profile.is_some_and(|values| {
        values.len() == route.waypoints.len() && values.iter().all(|value| value.is_finite())
    }) {
        "flown altitude and mass profile"
    } else {
        "planned lateral route; flown profile incomplete or unavailable"
    };
    scene.add(SceneElement::Text {
        text: format!(
            "{} waypoints | {:.0} km | {profile_note} | orthographic globe; drag to orbit; scroll to zoom",
            route.waypoints.len(),
            route.total_distance_m() / 1000.0
        ),
        pos: [430.0, 520.0],
        font_size: 9.0,
        color: Color::from_hex(palette.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: false,
    });
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::svg::render_svg;
    use alas_route::route::{RouteSource, Waypoint};

    fn dateline_route() -> Route {
        Route::new(
            vec![
                Waypoint::named(35.0, 170.0, "WEST"),
                Waypoint::named(40.0, -170.0, "EAST"),
            ],
            RouteSource::GreatCircle,
        )
    }

    #[test]
    fn sampled_legs_follow_the_sphere_instead_of_cutting_a_cartesian_chord() {
        let route = dateline_route();
        let points = sampled_route(&route, None, Some(&[0.0, 10_000.0]));
        assert_eq!(points.len(), ARC_SAMPLES_PER_LEG + 1);
        for (index, (point, _)) in points.iter().enumerate() {
            let radius = (point[0] * point[0] + point[1] * point[1] + point[2] * point[2]).sqrt();
            let expected = EARTH_RADIUS_KM
                + PATH_VISIBILITY_OFFSET_KM
                + 10.0 * index as f64 / ARC_SAMPLES_PER_LEG as f64;
            assert!((radius - expected).abs() < 1e-9);
        }
    }

    #[test]
    fn the_scene_contains_a_globe_route_and_both_endpoint_labels() {
        let scene = figure_mission_route_3d(
            &dateline_route(),
            Some(&[70_000.0, 65_000.0]),
            Some(&[0.0, 10_000.0]),
            None,
            Some("dark"),
        );
        assert!(scene.elements.len() > ARC_SAMPLES_PER_LEG);
        let text: Vec<&str> = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(text.contains(&"WEST"));
        assert!(text.contains(&"EAST"));
    }

    #[test]
    fn hidden_side_waypoints_are_not_drawn_through_the_globe() {
        let route = Route::new(
            vec![
                Waypoint::named(0.0, 0.0, "HIDDEN_ORIGIN"),
                Waypoint::named(0.0, 10.0, "HIDDEN_DESTINATION"),
            ],
            RouteSource::GreatCircle,
        );
        let scene = figure_mission_route_3d(
            &route,
            Some(&[70_000.0, 65_000.0]),
            Some(&[0.0, 10_000.0]),
            Some(Camera3D::default()),
            Some("dark"),
        );
        let text: Vec<&str> = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(!text.contains(&"HIDDEN_ORIGIN"));
        assert!(!text.contains(&"HIDDEN_DESTINATION"));
        assert!(!scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Circle { radius, .. } if (*radius - 3.4).abs() < 1e-9)
        }));
    }

    #[test]
    fn globe_has_one_visible_title_and_a_tonnes_mass_scale() {
        let scene = figure_mission_route_3d(
            &dateline_route(),
            Some(&[70_000.0, 65_000.0]),
            Some(&[0.0, 10_000.0]),
            None,
            Some("dark"),
        );
        let svg = render_svg(&scene);
        assert_eq!(svg.matches("Mission route globe").count(), 2);
        assert!(svg.contains("Total Mass (t)"));
        assert!(!svg.contains("Total Mass (kg)"));
    }

    #[test]
    fn initial_camera_centers_the_visible_hemisphere_on_the_route() {
        let route = Route::new(
            vec![
                Waypoint::named(51.4700, -0.4543, "EGLL"),
                Waypoint::named(25.2532, 55.3657, "OMDB"),
            ],
            RouteSource::GreatCircle,
        );
        let camera = route_focused_camera(&route);
        for point in route_to_globe_xyz(&route, &[]) {
            assert!(camera.view_depth(point, [0.0, 0.0, 0.0]) > 0.0);
        }
    }

    #[test]
    fn globe_surface_contains_one_blue_marble_and_no_vector_land_layer() {
        let scene = figure_mission_route_3d(&dateline_route(), None, None, None, Some("dark"));
        assert_eq!(
            scene
                .elements
                .iter()
                .filter(|element| matches!(element, SceneElement::SphericalImage { .. }))
                .count(),
            1
        );
        assert!(scene.elements.iter().any(|element| {
            matches!(
                element,
                SceneElement::SphericalImage {
                    mirror_longitude: true,
                    ..
                }
            )
        }));
        assert!(!scene
            .elements
            .iter()
            .any(|element| matches!(element, SceneElement::Polygon { .. })));
    }

    #[test]
    fn route_opens_with_london_to_the_left_of_dubai() {
        let route = Route::new(
            vec![
                Waypoint::named(51.4700, -0.4543, "EGLL"),
                Waypoint::named(25.2532, 55.3657, "OMDB"),
            ],
            RouteSource::GreatCircle,
        );
        let camera = route_focused_camera(&route);
        let points = route_to_globe_xyz(&route, &[])
            .into_iter()
            .map(|point| camera.project(point, [0.0, 0.0, 0.0], GLOBE_FRAMING_SPAN_KM, VIEWPORT))
            .collect::<Vec<_>>();

        assert!(points[0][0] < points[1][0]);
    }
}
