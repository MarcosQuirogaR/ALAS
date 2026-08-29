// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Render self-contained 2-D and 3-D Earth route figures for visual review.

use std::error::Error;
use std::path::{Path, PathBuf};

use alas_report::families::mission::{figure_mission_route_2d, figure_mission_route_3d};
use alas_report::render_svg;
use alas_report::scene::Camera3D;
use alas_route::route::{Route, RouteSource, Waypoint};

fn main() -> Result<(), Box<dyn Error>> {
    let directory = std::env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("outputs/route_earth_audit"), PathBuf::from);
    std::fs::create_dir_all(&directory)?;
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
    let mass = [254_000.0, 248_000.0, 238_000.0, 224_000.0, 215_000.0];
    let altitude = [0.0, 10_000.0, 11_000.0, 8_000.0, 0.0];
    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    write_png(
        &directory.join("route_2d_dark.png"),
        alas_gui::scene::localize_scene_for_display(figure_mission_route_2d(
            &route,
            Some(&mass),
            Some(&altitude),
            Some("dark"),
        )),
    )?;
    write_png(
        &directory.join("route_3d_dark.png"),
        alas_gui::scene::localize_scene_for_display(figure_mission_route_3d(
            &route,
            Some(&mass),
            Some(&altitude),
            None,
            Some("dark"),
        )),
    )?;
    write_png(
        &directory.join("route_3d_greenwich_uv_dark.png"),
        alas_gui::scene::localize_scene_for_display(figure_mission_route_3d(
            &route,
            Some(&mass),
            Some(&altitude),
            Some(Camera3D::front()),
            Some("dark"),
        )),
    )?;
    write_svg(
        &directory.join("route_2d_dark.svg"),
        alas_gui::scene::localize_scene_for_display(figure_mission_route_2d(
            &route,
            Some(&mass),
            Some(&altitude),
            Some("dark"),
        )),
    )?;
    write_svg(
        &directory.join("route_3d_dark.svg"),
        alas_gui::scene::localize_scene_for_display(figure_mission_route_3d(
            &route,
            Some(&mass),
            Some(&altitude),
            None,
            Some("dark"),
        )),
    )?;
    Ok(())
}

fn write_png(path: &Path, scene: alas_report::Scene) -> Result<(), Box<dyn Error>> {
    let png = alas_viz::raster::render_scene_png(&scene).map_err(std::io::Error::other)?;
    std::fs::write(path, png)?;
    Ok(())
}

fn write_svg(path: &Path, scene: alas_report::Scene) -> Result<(), Box<dyn Error>> {
    std::fs::write(path, render_svg(&scene))?;
    Ok(())
}
