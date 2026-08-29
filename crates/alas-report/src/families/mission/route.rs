// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/sidecar/figures.py: _fig_route_2d.
// Reference: alas @ rust-port-baseline.

//! Geographic route map with optional flown mass and altitude profiles.

const BLUE_MARBLE_SOURCE: &str = "embedded://nasa-blue-marble";
const MIN_WAYPOINT_LABEL_SEPARATION_PX: f64 = 64.0;

use alas_route::route::Route;

use crate::chart_kit::draw_colorbar;
use crate::colormap::Colormap;
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

/// Draw route waypoints in longitude/latitude coordinates.
pub fn figure_mission_route_2d(
    route: &Route,
    mass_profile: Option<&[f64]>,
    altitude_profile: Option<&[f64]>,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    if route.waypoints.is_empty() {
        return status_scene(
            pal,
            "Mission route",
            "Route data is not available for this run.",
        );
    }
    // Blue Marble is an equirectangular, public-domain NASA image. Both axes
    // use its longitude/latitude transform, with the regional panel taking a
    // clipped source crop rather than scaling the entire world into its box.
    let x_range = (-180.0, 180.0);
    let y_range = (-90.0, 90.0);
    let mut scene = Scene::new(900.0, 590.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Mission route".to_owned());
    scene.suppress_derived_title();
    scene.add(SceneElement::Text {
        text: "Mission route".to_owned(),
        pos: [450.0, 42.0],
        font_size: 15.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: true,
    });
    let overview = Axes2D::new((56.0, 104.0, 270.0, 135.0), x_range, y_range);
    add_full_panel_texture(&mut scene, &overview, None);
    overview.draw_frame(&mut scene, pal);
    let points = unwrapped_route_points(route);
    let (lon_range, lat_range) = route_detail_ranges(&points);
    let detail = Axes2D::new((372.0, 104.0, 380.0, 306.0), lon_range, lat_range);
    draw_detail_earth_texture(&mut scene, &detail);
    detail.draw_frame(&mut scene, pal);
    scene.add(SceneElement::Text {
        text: "Route detail (plate carree)".to_owned(),
        pos: [detail.left, detail.top - 9.0],
        font_size: 10.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
    draw_global_route(&mut scene, &overview, route);
    let mass_coloring = mass_profile
        .filter(|profile| profile.len() == points.len() && !profile.is_empty())
        .filter(|profile| profile.iter().all(|mass| mass.is_finite()));
    if let Some(mass) = mass_coloring {
        let mass_min = mass.iter().copied().fold(f64::INFINITY, f64::min);
        let mass_max = mass.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let mass_span = (mass_max - mass_min).max(f64::EPSILON);
        for (index, pair) in points.windows(2).enumerate() {
            let t = (mass[index] - mass_min) / mass_span;
            scene.add(SceneElement::Line {
                p1: detail.map_point(pair[0].0, pair[0].1),
                p2: detail.map_point(pair[1].0, pair[1].1),
                stroke: Stroke::new(Colormap::Jet.sample(t), 3.0),
            });
        }
        draw_colorbar(
            &mut scene,
            (790.0, 96.0, 14.0, 330.0),
            Colormap::Jet,
            mass_min / 1_000.0,
            mass_max / 1_000.0,
            "Total Mass (t)",
            pal,
        );
    } else {
        detail.add_line_series(
            &mut scene,
            &points,
            Stroke::new(Color::from_hex("#00d8ff"), 2.0),
        );
    }
    let mut last_label_position: Option<[f64; 2]> = None;
    for (index, waypoint) in route.waypoints.iter().enumerate() {
        let center = detail.map_point(points[index].0, waypoint.lat);
        scene.add(SceneElement::Circle {
            center,
            radius: 3.0,
            fill: Some(Fill::new(Color::from_hex(if index == 0 {
                "#2ecc71"
            } else if index + 1 == route.waypoints.len() {
                "#e74c3c"
            } else {
                "#aaaaaa"
            }))),
            stroke: Some(Stroke::new(Color::from_hex(pal.spine), 0.7)),
        });
        let endpoint = index == 0 || index + 1 == route.waypoints.len();
        let separated = last_label_position.is_none_or(|previous| {
            let dx = center[0] - previous[0];
            let dy = center[1] - previous[1];
            dx * dx + dy * dy >= MIN_WAYPOINT_LABEL_SEPARATION_PX.powi(2)
        });
        if !waypoint.ident.is_empty() && (endpoint || separated) {
            scene.add(SceneElement::Text {
                text: waypoint.ident.clone(),
                pos: [center[0] + 5.0, center[1] - 5.0],
                font_size: 9.0,
                color: Color::from_hex(pal.tick),
                align: TextAlign::Left,
                baseline: TextBaseline::Bottom,
                angle_deg: 0.0,
                bold: false,
            });
            last_label_position = Some(center);
        }
    }
    draw_endpoint_legend(&mut scene, [650.0, 448.0], pal);
    let total_km = route.total_distance_m() / 1000.0;
    let profile_note = match (mass_profile, altitude_profile) {
        (Some(mass), Some(alt)) => format!(
            "{} waypoints | {:.0} km | flown profile: {:.0} kg to {:.0} kg, {:.0} to {:.0} m",
            route.waypoints.len(),
            total_km,
            mass.first().copied().unwrap_or(0.0),
            mass.last().copied().unwrap_or(0.0),
            alt.first().copied().unwrap_or(0.0),
            alt.last().copied().unwrap_or(0.0),
        ),
        _ => format!(
            "{} waypoints | {:.0} km | lateral route only (mission telemetry unavailable)",
            route.waypoints.len(),
            total_km
        ),
    };
    scene.add(SceneElement::Text {
        text: profile_note,
        pos: [450.0, 550.0],
        font_size: 9.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: false,
    });
    scene.add(SceneElement::Text {
        text: "Longitude [deg]".to_owned(),
        pos: [562.0, 448.0],
        font_size: 9.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
    scene.add(SceneElement::Text {
        text: "Latitude [deg]".to_owned(),
        pos: [340.0, 257.0],
        font_size: 9.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: -90.0,
        bold: false,
    });
    scene
}

fn draw_detail_earth_texture(scene: &mut Scene, axes: &Axes2D) {
    let lat_lo = axes.y_min.max(-90.0);
    let lat_hi = axes.y_max.min(90.0);
    if lat_lo >= lat_hi {
        return;
    }
    let first_tile = ((axes.x_min + 180.0) / 360.0).floor() as i32;
    let last_tile = ((axes.x_max + 180.0) / 360.0).floor() as i32;
    for tile in first_tile..=last_tile {
        let tile_left = -180.0 + 360.0 * f64::from(tile);
        let lon_lo = axes.x_min.max(tile_left);
        let lon_hi = axes.x_max.min(tile_left + 360.0);
        if lon_lo >= lon_hi {
            continue;
        }
        let x0 = axes.left + (lon_lo - axes.x_min) / (axes.x_max - axes.x_min) * axes.width;
        let x1 = axes.left + (lon_hi - axes.x_min) / (axes.x_max - axes.x_min) * axes.width;
        let y0 = axes.top + (axes.y_max - lat_hi) / (axes.y_max - axes.y_min) * axes.height;
        let y1 = axes.top + (axes.y_max - lat_lo) / (axes.y_max - axes.y_min) * axes.height;
        scene.add(SceneElement::Image {
            source: BLUE_MARBLE_SOURCE.to_owned(),
            x: x0.clamp(axes.left, axes.left + axes.width),
            y: y0.clamp(axes.top, axes.top + axes.height),
            width: (x1 - x0).max(0.0).min(axes.width),
            height: (y1 - y0).max(0.0).min(axes.height),
            source_rect: Some([
                ((lon_lo - tile_left) / 360.0).clamp(0.0, 1.0),
                ((90.0 - lat_hi) / 180.0).clamp(0.0, 1.0),
                ((lon_hi - lon_lo) / 360.0).clamp(0.0, 1.0),
                ((lat_hi - lat_lo) / 180.0).clamp(0.0, 1.0),
            ]),
        });
    }
}

fn add_full_panel_texture(scene: &mut Scene, axes: &Axes2D, source_rect: Option<[f64; 4]>) {
    scene.add(SceneElement::Image {
        source: BLUE_MARBLE_SOURCE.to_owned(),
        x: axes.left,
        y: axes.top,
        width: axes.width,
        height: axes.height,
        source_rect,
    });
}

fn draw_endpoint_legend(scene: &mut Scene, pos: [f64; 2], pal: &crate::theme::Palette) {
    for (offset, label, color) in [(0.0, "Origin", "#2ecc71"), (98.0, "Destination", "#e74c3c")] {
        scene.add(SceneElement::Circle {
            center: [pos[0] + offset, pos[1]],
            radius: 4.0,
            fill: Some(Fill::new(Color::from_hex(color))),
            stroke: None,
        });
        scene.add(SceneElement::Text {
            text: label.to_owned(),
            pos: [pos[0] + offset + 9.0, pos[1]],
            font_size: 9.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }
}

fn unwrapped_route_points(route: &Route) -> Vec<(f64, f64)> {
    let mut points = Vec::with_capacity(route.waypoints.len());
    let mut previous_lon = None;
    for waypoint in &route.waypoints {
        let mut lon = waypoint.lon;
        if let Some(previous) = previous_lon {
            while lon - previous > 180.0 {
                lon -= 360.0;
            }
            while lon - previous < -180.0 {
                lon += 360.0;
            }
        }
        previous_lon = Some(lon);
        points.push((lon, waypoint.lat));
    }
    points
}

fn route_detail_ranges(points: &[(f64, f64)]) -> ((f64, f64), (f64, f64)) {
    let (mut lon_lo, mut lon_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut lat_lo, mut lat_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for &(lon, lat) in points {
        lon_lo = lon_lo.min(lon);
        lon_hi = lon_hi.max(lon);
        lat_lo = lat_lo.min(lat);
        lat_hi = lat_hi.max(lat);
    }
    let lon_pad = ((lon_hi - lon_lo).abs() * 0.12).max(2.0);
    let lat_pad = ((lat_hi - lat_lo).abs() * 0.12).max(2.0);
    (
        (lon_lo - lon_pad, lon_hi + lon_pad),
        (lat_lo - lat_pad, lat_hi + lat_pad),
    )
}

fn draw_global_route(scene: &mut Scene, axes: &Axes2D, route: &Route) {
    for pair in route.waypoints.windows(2) {
        let (start_lon, end_lon) = (pair[0].lon, pair[1].lon);
        if (end_lon - start_lon).abs() <= 180.0 {
            scene.add(SceneElement::Line {
                p1: axes.map_point(start_lon, pair[0].lat),
                p2: axes.map_point(end_lon, pair[1].lat),
                stroke: Stroke::new(Color::from_hex("#00d8ff"), 1.2),
            });
            continue;
        }
        let seam_a = if start_lon > 0.0 { 180.0 } else { -180.0 };
        let seam_b = -seam_a;
        let unwrapped_end = if end_lon > 0.0 {
            end_lon - 360.0
        } else {
            end_lon + 360.0
        };
        let fraction = ((seam_a - start_lon) / (unwrapped_end - start_lon)).clamp(0.0, 1.0);
        let seam_lat = pair[0].lat + fraction * (pair[1].lat - pair[0].lat);
        for (a_lon, a_lat, b_lon, b_lat) in [
            (start_lon, pair[0].lat, seam_a, seam_lat),
            (seam_b, seam_lat, end_lon, pair[1].lat),
        ] {
            scene.add(SceneElement::Line {
                p1: axes.map_point(a_lon, a_lat),
                p2: axes.map_point(b_lon, b_lat),
                stroke: Stroke::new(Color::from_hex("#00d8ff"), 1.2),
            });
        }
    }
}

fn status_scene(pal: &crate::theme::Palette, title: &str, message: &str) -> Scene {
    let mut scene = Scene::new(800.0, 180.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some(title.to_owned());
    scene.add(SceneElement::Text {
        text: message.to_owned(),
        pos: [400.0, 90.0],
        font_size: 13.0,
        color: Color::from_hex("#c0392b"),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: true,
    });
    scene
}

#[cfg(test)]
// Fixture parsing failures are test-authoring failures, not runtime paths.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::families::mission::test_support::sample_mission;
    use crate::svg::render_svg;
    use alas_route::route::{RouteSource, Waypoint};

    #[test]
    fn w33_route_render_uses_an_uncluttered_tonnes_legend_in_both_themes() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../golden/report/reference_render_w33.json"
        ))
        .expect("W3.3 reference fixture is valid JSON");
        assert_eq!(fixture["route_acceptance"]["legend"][2], "Total Mass (kg)");

        let route = Route::new(
            vec![
                Waypoint::named(40.47, -3.56, "LEMD"),
                Waypoint::named(45.0, -2.0, "W33_FIX"),
                Waypoint::named(51.15, -0.19, "EGKK"),
            ],
            RouteSource::SimbriefApi,
        );
        let mission = sample_mission();
        let (mass, altitude) = crate::route_geometry::sync_mass_to_route(&route, &mission);
        for theme in ["light", "dark"] {
            let contract = &fixture["figures"][&format!("mission_route_2d:{theme}")];
            assert_eq!(contract["available"], true);
            assert_eq!(contract["panel_count"], 2);
            let svg = render_svg(&figure_mission_route_2d(
                &route,
                Some(&mass),
                Some(&altitude),
                Some(theme),
            ));
            for label in [
                "Mission route",
                "Longitude [deg]",
                "Latitude [deg]",
                "LEMD",
                "EGKK",
            ] {
                assert!(svg.contains(label), "route scene is missing {label}");
            }
            assert!(svg.contains("Origin"));
            assert!(svg.contains("Destination"));
            assert!(svg.contains("Total Mass (t)"));
            assert!(!svg.contains("Total Mass (kg)"));
            assert_eq!(svg.matches("Mission route").count(), 2);
        }
    }

    #[test]
    fn route_detail_uses_a_clipped_equirectangular_texture() {
        let route = Route::new(
            vec![
                Waypoint::named(51.47, -0.45, "EGLL"),
                Waypoint::named(25.25, 55.36, "OMDB"),
            ],
            RouteSource::GreatCircle,
        );
        let scene = figure_mission_route_2d(&route, None, None, Some("dark"));
        let images = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Image {
                    source,
                    x,
                    y,
                    width,
                    height,
                    source_rect,
                } if source == BLUE_MARBLE_SOURCE => Some((*x, *y, *width, *height, *source_rect)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(images.len() >= 2);
        assert_eq!(images[0].4, None);
        let detail = images
            .iter()
            .find(|(x, _, _, _, crop)| *x >= 372.0 && crop.is_some())
            .expect("route detail texture");
        assert!(detail.2 > 0.0 && detail.3 > 0.0);
        let [left, top, width, height] = detail.4.expect("detail crop");
        assert!((0.0..=1.0).contains(&left));
        assert!((0.0..=1.0).contains(&top));
        assert!(width > 0.0 && height > 0.0);
        let svg = render_svg(&scene);
        assert!(svg.matches("data:image/png;base64,").count() >= 2);
        assert!(svg.contains("viewBox="));
    }

    #[test]
    fn dense_route_labels_leave_endpoints_and_skip_nearby_waypoints() {
        let waypoints = (0..12)
            .map(|index| Waypoint::named(40.0 + f64::from(index) * 0.01, -3.0, format!("P{index}")))
            .collect();
        let route = Route::new(waypoints, RouteSource::SimbriefApi);
        let scene = figure_mission_route_2d(&route, None, None, Some("dark"));
        let waypoint_labels = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, .. } if text.starts_with('P') => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(waypoint_labels.contains(&"P0"));
        assert!(waypoint_labels.contains(&"P11"));
        assert!(waypoint_labels.len() < route.waypoints.len());
    }

    #[test]
    fn endpoint_legend_is_horizontal_below_the_route_detail() {
        let route = Route::new(
            vec![
                Waypoint::named(40.47, -3.56, "LEMD"),
                Waypoint::named(51.15, -0.19, "EGKK"),
            ],
            RouteSource::GreatCircle,
        );
        let scene = figure_mission_route_2d(&route, Some(&[254_000.0, 215_000.0]), None, None);
        let mut origin = None;
        let mut destination = None;
        for element in &scene.elements {
            if let SceneElement::Text { text, pos, .. } = element {
                match text.as_str() {
                    "Origin" => origin = Some(*pos),
                    "Destination" => destination = Some(*pos),
                    _ => {}
                }
            }
        }
        let (origin, destination) = origin.zip(destination).expect("endpoint legend labels");
        assert_eq!(origin[1], destination[1]);
        assert!(destination[0] > origin[0]);
        assert!(origin[1] > 410.0);
    }

    #[test]
    fn route_segments_use_starting_waypoint_mass_for_their_colors() {
        let route = Route::new(
            vec![
                Waypoint::named(40.47, -3.56, "LEMD"),
                Waypoint::named(45.0, -2.0, "W33_FIX"),
                Waypoint::named(51.15, -0.19, "EGKK"),
            ],
            RouteSource::SimbriefApi,
        );
        let scene =
            figure_mission_route_2d(&route, Some(&[100.0, 200.0, 300.0]), None, Some("dark"));
        let strokes: Vec<_> = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Line { stroke, .. } if stroke.width == 3.0 => Some(stroke),
                _ => None,
            })
            .collect();
        assert!(strokes.len() >= 2);
        assert_ne!(strokes[0].color, strokes[1].color);
    }

    #[test]
    fn route_axis_label_and_profile_footer_have_separate_rows() {
        let route = Route::new(
            vec![
                Waypoint::named(40.47, -3.56, "LEMD"),
                Waypoint::named(51.15, -0.19, "EGKK"),
            ],
            RouteSource::SimbriefApi,
        );
        let scene = figure_mission_route_2d(
            &route,
            Some(&[300_000.0, 280_000.0]),
            Some(&[25.0, 19.0]),
            Some("dark"),
        );
        let mut axis_y = None;
        let mut footer_y = None;
        for element in &scene.elements {
            if let SceneElement::Text { text, pos, .. } = element {
                if text == "Longitude [deg]" {
                    axis_y = Some(pos[1]);
                } else if text.contains("waypoints |") {
                    footer_y = Some(pos[1]);
                }
            }
        }
        assert!(axis_y
            .zip(footer_y)
            .is_some_and(|(axis, footer)| { footer - axis >= 50.0 && footer < scene.height }));
    }

    #[test]
    fn route_detail_unwraps_dateline_legs_instead_of_spanning_the_world() {
        let route = Route::new(
            vec![
                Waypoint::named(35.0, 170.0, "WEST"),
                Waypoint::named(40.0, -170.0, "EAST"),
            ],
            RouteSource::GreatCircle,
        );
        let points = unwrapped_route_points(&route);
        assert!((points[1].0 - points[0].0).abs() < 30.0);
        let (longitude, latitude) = route_detail_ranges(&points);
        assert!(longitude.1 - longitude.0 < 40.0);
        assert!(latitude.1 - latitude.0 < 20.0);
        let svg = render_svg(&figure_mission_route_2d(&route, None, None, Some("dark")));
        assert!(svg.contains("Route detail (plate carree)"));
    }

    #[test]
    fn w33_unavailable_reasons_remain_explicit_in_the_pinned_contract() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../golden/report/reference_render_w33.json"
        ))
        .expect("W3.3 reference fixture is valid JSON");
        assert!(fixture["unavailable_reasons"]["mission"]
            .as_str()
            .unwrap_or("")
            .contains("mission_result=None"));
        assert!(fixture["unavailable_reasons"]["route"]
            .as_str()
            .unwrap_or("")
            .contains("route=None"));
    }
}
