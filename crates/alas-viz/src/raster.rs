// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Static PNG rasterization for desktop figure previews.
//!
//! Figures are authored once as backend-neutral scenes and exported as SVG.
//! Rasterizing that same SVG for the GUI keeps the preview visually identical
//! to the exported figure while moving geometry and text work out of every
//! egui paint pass.

use alas_report::scene::{Camera3D, SceneElement};
use alas_report::{render_svg, Scene};
use std::sync::{Arc, OnceLock};

/// Rasterize a scene to an RGBA PNG buffer.
pub fn render_scene_png(scene: &Scene) -> Result<Vec<u8>, String> {
    let (width, height, pixels) = render_scene_rgba(scene)?;
    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| "allocate figure PNG canvas".to_owned())?;
    pixmap.data_mut().copy_from_slice(&pixels);
    pixmap
        .encode_png()
        .map_err(|error| format!("encode figure PNG: {error}"))
}

/// Rasterize a scene to premultiplied RGBA pixels for an egui texture.
pub fn render_scene_rgba(scene: &Scene) -> Result<(u32, u32, Vec<u8>), String> {
    render_scene_rgba_scaled(scene, 1.0)
}

/// Rasterize a scene at a scaled pixel density for display surfaces.
///
/// Keeping the scene coordinates unchanged while increasing the texture
/// density makes text and thin plot lines survive resizing in an egui card.
pub fn render_scene_rgba_scaled(scene: &Scene, scale: f64) -> Result<(u32, u32, Vec<u8>), String> {
    // Paint bundled textures first, then rasterize the transparent vector
    // overlay. This keeps screenshots, exports generated through this path,
    // and the interactive direct-texture path visually consistent.
    let mut vector_scene = scene.clone();
    vector_scene.elements.retain(|element| {
        !matches!(
            element,
            alas_report::scene::SceneElement::Image { source, .. }
                | alas_report::scene::SceneElement::SphericalImage { source, .. }
                if source == "embedded://nasa-blue-marble"
        )
    });
    vector_scene.background = None;
    let svg = render_svg(&vector_scene);
    // usvg intentionally starts with an empty font database. Loading system
    // fonts on every globe orbit frame dominates raster time, so all figure
    // renders share one immutable desktop font database.
    let options = resvg::usvg::Options {
        fontdb: system_font_database(),
        ..resvg::usvg::Options::default()
    };
    let tree = resvg::usvg::Tree::from_str(&svg, &options)
        .map_err(|error| format!("parse figure SVG: {error}"))?;
    let scale = scale.max(1.0);
    let width = (scene.width.max(1.0) * scale).round() as u32;
    let height = (scene.height.max(1.0) * scale).round() as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| "allocate figure PNG canvas".to_owned())?;
    if let Some(background) = scene.background {
        pixmap.fill(tiny_skia::Color::from_rgba8(
            background.r,
            background.g,
            background.b,
            background.a,
        ));
    }
    draw_embedded_textures(scene, &mut pixmap, scale);
    draw_spherical_textures(scene, &mut pixmap, scale);
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale as f32, scale as f32),
        &mut pixmap.as_mut(),
    );
    Ok((width, height, pixmap.data().to_vec()))
}

fn draw_spherical_textures(scene: &Scene, destination: &mut tiny_skia::Pixmap, scale: f64) {
    let Some(texture) = blue_marble_texture() else {
        return;
    };
    for element in &scene.elements {
        let SceneElement::SphericalImage {
            source,
            center,
            radius,
            camera,
            mirror_longitude,
        } = element
        else {
            continue;
        };
        if source != "embedded://nasa-blue-marble" || !radius.is_finite() || *radius <= 0.0 {
            continue;
        }
        draw_equirectangular_sphere(
            destination,
            texture.as_ref(),
            *center,
            *radius,
            *camera,
            *mirror_longitude,
            scale,
        );
    }
}

fn draw_equirectangular_sphere(
    destination: &mut tiny_skia::Pixmap,
    texture: tiny_skia::PixmapRef<'_>,
    center: [f64; 2],
    radius: f64,
    camera: Camera3D,
    mirror_longitude: bool,
    scale: f64,
) {
    let center_x = center[0] * scale;
    let center_y = center[1] * scale;
    let radius_px = radius * scale;
    let left = (center_x - radius_px).floor().max(0.0) as u32;
    let right = (center_x + radius_px)
        .ceil()
        .min(f64::from(destination.width())) as u32;
    let top = (center_y - radius_px).floor().max(0.0) as u32;
    let bottom = (center_y + radius_px)
        .ceil()
        .min(f64::from(destination.height())) as u32;
    for y in top..bottom {
        for x in left..right {
            let dx = (f64::from(x) + 0.5 - center_x) / radius_px;
            let dy = (f64::from(y) + 0.5 - center_y) / radius_px;
            let radial_sq = dx * dx + dy * dy;
            if radial_sq > 1.0 {
                continue;
            }
            let depth = (1.0 - radial_sq).sqrt();
            let [world_x, world_y, z] = sphere_sample_direction(dx, dy, depth, camera);
            let longitude = world_y.atan2(world_x);
            let source_longitude = source_longitude(longitude, mirror_longitude);
            let latitude = z.clamp(-1.0, 1.0).asin();
            let texture_x = ((source_longitude + std::f64::consts::PI)
                / (2.0 * std::f64::consts::PI)
                * f64::from(texture.width() - 1))
            .round() as u32;
            let texture_y = ((std::f64::consts::FRAC_PI_2 - latitude) / std::f64::consts::PI
                * f64::from(texture.height() - 1))
            .round() as u32;
            let source_index = ((texture_y * texture.width() + texture_x) * 4) as usize;
            let target_index = ((y * destination.width() + x) * 4) as usize;
            destination.data_mut()[target_index..target_index + 4]
                .copy_from_slice(&texture.data()[source_index..source_index + 4]);
        }
    }
}

/// Reconstruct the unit-length globe direction under one orthographic pixel.
/// This is the inverse of [`Camera3D::project`]; the caller may then reflect
/// the source longitude together with the globe's geographic geometry.
fn sphere_sample_direction(dx: f64, dy: f64, depth: f64, camera: Camera3D) -> [f64; 3] {
    let azimuth = camera.azim_deg.to_radians();
    let elevation = camera.elev_deg.to_radians();
    let y_rot = dy * elevation.sin() + depth * elevation.cos();
    let z = -dy * elevation.cos() + depth * elevation.sin();
    [
        dx * azimuth.cos() + y_rot * azimuth.sin(),
        -dx * azimuth.sin() + y_rot * azimuth.cos(),
        z,
    ]
}

fn source_longitude(world_longitude: f64, mirror_longitude: bool) -> f64 {
    if mirror_longitude {
        -world_longitude
    } else {
        world_longitude
    }
}

fn draw_embedded_textures(scene: &Scene, destination: &mut tiny_skia::Pixmap, scale: f64) {
    let Some(texture) = blue_marble_texture() else {
        return;
    };
    for element in &scene.elements {
        let SceneElement::Image {
            source,
            x,
            y,
            width,
            height,
            source_rect,
        } = element
        else {
            continue;
        };
        if source != "embedded://nasa-blue-marble" {
            continue;
        }
        let [left, top, crop_width, crop_height] = source_rect.unwrap_or([0.0, 0.0, 1.0, 1.0]);
        let left = left.clamp(0.0, 1.0);
        let top = top.clamp(0.0, 1.0);
        let crop_width = crop_width.clamp(0.0, 1.0 - left);
        let crop_height = crop_height.clamp(0.0, 1.0 - top);
        if crop_width <= f64::EPSILON || crop_height <= f64::EPSILON {
            continue;
        }
        let source_width = f64::from(texture.width());
        let source_height = f64::from(texture.height());
        let crop_x = (left * source_width).floor() as i32;
        let crop_y = (top * source_height).floor() as i32;
        let crop_w = ((crop_width * source_width).ceil() as u32).max(1);
        let crop_h = ((crop_height * source_height).ceil() as u32).max(1);
        let Some(crop_rect) = tiny_skia::IntRect::from_xywh(crop_x, crop_y, crop_w, crop_h) else {
            continue;
        };
        let Some(cropped) = texture.clone_rect(crop_rect) else {
            continue;
        };
        let x_scale = width * scale / f64::from(cropped.width());
        let y_scale = height * scale / f64::from(cropped.height());
        destination.draw_pixmap(
            0,
            0,
            cropped.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            tiny_skia::Transform::from_row(
                x_scale as f32,
                0.0,
                0.0,
                y_scale as f32,
                (x * scale) as f32,
                (y * scale) as f32,
            ),
            None,
        );
    }
}

fn system_font_database() -> Arc<resvg::usvg::fontdb::Database> {
    static FONT_DATABASE: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    FONT_DATABASE
        .get_or_init(|| {
            let mut database = resvg::usvg::fontdb::Database::new();
            database.load_system_fonts();
            Arc::new(database)
        })
        .clone()
}

fn blue_marble_texture() -> Option<&'static tiny_skia::Pixmap> {
    static TEXTURE: OnceLock<Option<tiny_skia::Pixmap>> = OnceLock::new();
    TEXTURE
        .get_or_init(|| {
            tiny_skia::Pixmap::decode_png(include_bytes!(
                "../../../assets/textures/earth_blue_marble.png"
            ))
            .ok()
        })
        .as_ref()
}

#[cfg(test)]
mod tests {
    use super::{render_scene_png, render_scene_rgba, source_longitude, sphere_sample_direction};
    use alas_report::scene::{Camera3D, Color, Scene, SceneElement, TextAlign, TextBaseline};

    #[test]
    fn scene_rasterization_returns_a_decodable_png() {
        let scene = Scene::new(64.0, 48.0, Some(Color::rgb(12, 34, 56)));
        let png = render_scene_png(&scene).expect("PNG rasterization");

        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert!(png.len() > 100);
    }

    #[test]
    fn scene_rasterization_keeps_text_in_the_static_png() {
        let mut scene = Scene::new(160.0, 48.0, Some(Color::rgb(255, 255, 255)));
        scene.add(SceneElement::Text {
            text: "Required vs Available".to_owned(),
            pos: [80.0, 24.0],
            font_size: 14.0,
            color: Color::rgb(0, 0, 0),
            align: TextAlign::Center,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: true,
        });

        let (_, _, pixels) = render_scene_rgba(&scene).expect("text PNG rasterization");
        assert!(pixels
            .chunks_exact(4)
            .any(|pixel| pixel[3] > 0 && pixel[0] < 200));
    }

    #[test]
    fn embedded_earth_texture_survives_static_rasterization() {
        let mut scene = Scene::new(64.0, 32.0, Some(Color::rgb(0, 0, 0)));
        scene.add(SceneElement::Image {
            source: "embedded://nasa-blue-marble".to_owned(),
            x: 0.0,
            y: 0.0,
            width: 64.0,
            height: 32.0,
            source_rect: None,
        });
        let (_, _, pixels) = render_scene_rgba(&scene).expect("earth texture rasterization");
        assert!(pixels
            .chunks_exact(4)
            .any(|pixel| pixel[0] > 40 && pixel[1] > 70 && pixel[2] > 35));
    }

    #[test]
    fn bundled_earth_texture_is_the_high_detail_asset() {
        let texture = tiny_skia::Pixmap::decode_png(include_bytes!(
            "../../../assets/textures/earth_blue_marble.png"
        ))
        .expect("bundled Earth texture");

        assert_eq!((texture.width(), texture.height()), (4096, 2048));
    }

    #[test]
    fn equirectangular_earth_sphere_places_greenwich_at_the_front_view_center() {
        let mut scene = Scene::new(65.0, 65.0, Some(Color::rgb(0, 0, 0)));
        scene.add(SceneElement::SphericalImage {
            source: "embedded://nasa-blue-marble".to_owned(),
            center: [32.5, 32.5],
            radius: 30.0,
            camera: Camera3D::front(),
            mirror_longitude: false,
        });

        let (_, _, pixels) = render_scene_rgba(&scene).expect("Earth sphere rasterization");
        let texture = tiny_skia::Pixmap::decode_png(include_bytes!(
            "../../../assets/textures/earth_blue_marble.png"
        ))
        .expect("bundled Earth texture");
        let x = (texture.width() - 1) / 2 + 1;
        let y = (texture.height() - 1) / 2 + 1;
        let texture_pixel = &texture.data()[((y * texture.width() + x) * 4) as usize..][..4];
        let globe_center = &pixels[((32 * 65 + 32) * 4) as usize..][..4];

        assert_eq!(globe_center, texture_pixel);
    }

    #[test]
    fn globe_texture_and_route_geometry_share_the_east_west_projection() {
        let camera = Camera3D::front();
        let east = sphere_sample_direction(-0.25, 0.0, (1.0_f64 - 0.25_f64.powi(2)).sqrt(), camera);
        let west = sphere_sample_direction(0.25, 0.0, (1.0_f64 - 0.25_f64.powi(2)).sqrt(), camera);

        assert!(east[1] > 0.0, "east must occupy the camera's left side");
        assert!(west[1] < 0.0, "west must occupy the camera's right side");
    }

    #[test]
    fn mirrored_globe_samples_the_opposite_source_longitude() {
        let east = 55.0_f64.to_radians();
        assert!((source_longitude(east, true).to_degrees() + 55.0).abs() < 1e-12);
        assert!((source_longitude(east, false).to_degrees() - 55.0).abs() < 1e-12);
    }

    #[test]
    fn embedded_earth_crop_stays_inside_its_declared_panel() {
        let mut scene = Scene::new(80.0, 48.0, Some(Color::rgb(0, 0, 0)));
        scene.add(SceneElement::Image {
            source: "embedded://nasa-blue-marble".to_owned(),
            x: 20.0,
            y: 10.0,
            width: 40.0,
            height: 20.0,
            source_rect: Some([0.25, 0.25, 0.25, 0.25]),
        });
        let (_, _, pixels) = render_scene_rgba(&scene).expect("earth crop rasterization");
        let pixel = |x: usize, y: usize| &pixels[(y * 80 + x) * 4..(y * 80 + x + 1) * 4];
        assert_eq!(pixel(10, 10), &[0, 0, 0, 255]);
        assert_ne!(pixel(30, 20), &[0, 0, 0, 255]);
        assert_eq!(pixel(70, 35), &[0, 0, 0, 255]);
    }

    #[test]
    fn exported_svg_embeds_and_rasterizes_the_earth_crop() {
        let mut scene = Scene::new(80.0, 48.0, Some(Color::rgb(0, 0, 0)));
        scene.add(SceneElement::Image {
            source: "embedded://nasa-blue-marble".to_owned(),
            x: 20.0,
            y: 10.0,
            width: 40.0,
            height: 20.0,
            source_rect: Some([0.25, 0.25, 0.25, 0.25]),
        });
        let svg = alas_report::render_svg(&scene);
        assert!(svg.contains("data:image/png;base64,"));
        let options = resvg::usvg::Options::default();
        let tree = resvg::usvg::Tree::from_str(&svg, &options).expect("embedded Earth SVG");
        let mut pixmap = tiny_skia::Pixmap::new(80, 48).expect("SVG test canvas");
        resvg::render(
            &tree,
            tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
        assert_ne!(
            pixmap.pixel(30, 20),
            Some(tiny_skia::ColorU8::from_rgba(0, 0, 0, 255).premultiply())
        );
    }
}
