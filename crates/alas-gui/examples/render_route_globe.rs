// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Produce a deterministic PNG used for first-hand route-globe visual review.

use std::error::Error;
use std::path::{Path, PathBuf};

use alas_report::families::mission::{figure_mission_route_3d, route_focused_camera};
use alas_report::scene::Camera3D;
use alas_route::route::{Route, RouteSource, Waypoint};

fn main() -> Result<(), Box<dyn Error>> {
    let output = std::env::args_os().nth(1).map_or_else(
        || PathBuf::from("outputs/route_globe_audit.png"),
        PathBuf::from,
    );
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let route = Route::new(
        vec![
            Waypoint::named(51.4700, -0.4543, "EGLL"),
            Waypoint::named(50.0, 8.0, "SULUS"),
            Waypoint::named(44.0, 22.0, "BALIK"),
            Waypoint::named(37.0, 38.0, "TUMAK"),
            Waypoint::named(25.2532, 55.3657, "OMDB"),
        ],
        RouteSource::GreatCircle,
    );
    // An optional second argument renders the same route at a maximized-view
    // camera zoom, which is how the globe's viewport confinement is reviewed.
    let camera = std::env::args()
        .nth(2)
        .and_then(|zoom| zoom.parse::<f64>().ok())
        .map(|zoom| Camera3D {
            zoom,
            ..route_focused_camera(&route)
        });
    let scene = figure_mission_route_3d(
        &route,
        Some(&[254_000.0, 248_000.0, 238_000.0, 224_000.0, 215_000.0]),
        Some(&[0.0, 10_000.0, 11_000.0, 8_000.0, 0.0]),
        camera,
        Some("dark"),
    );
    let png = alas_viz::raster::render_scene_png(&scene).map_err(std::io::Error::other)?;
    std::fs::write(Path::new(&output), png)?;
    Ok(())
}
