// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Static PNG rasterization for desktop figure previews.
//!
//! Figures are authored once as backend-neutral scenes and exported as SVG.
//! Rasterizing that same SVG for the GUI keeps the preview visually identical
//! to the exported figure while moving geometry and text work out of every
//! egui paint pass.

use alas_fonts::{
    MATH_FONT_BYTES, MONOSPACE_FONT_BYTES, PROPORTIONAL_FAMILY, PROPORTIONAL_FONT_BYTES,
};
use alas_report::scene::{Camera3D, SceneElement};
use alas_report::{render_svg, Scene};
use rayon::prelude::*;
use std::sync::{Arc, OnceLock};

/// A raster as width, height and premultiplied RGBA bytes, or the reason it
/// could not be produced.
pub type RasterResult = Result<(u32, u32, Vec<u8>), String>;

/// Rasterize a scene to an RGBA PNG buffer.
pub fn render_scene_png(scene: &Scene) -> Result<Vec<u8>, String> {
    let (width, height, pixels) = render_scene_rgba(scene)?;
    encode_png_rgba(width, height, &pixels)
}

/// Encode premultiplied RGBA pixels (the layout every raster here returns)
/// as a PNG buffer.
pub fn encode_png_rgba(width: u32, height: u32, pixels: &[u8]) -> Result<Vec<u8>, String> {
    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| "allocate figure PNG canvas".to_owned())?;
    if pixmap.data().len() != pixels.len() {
        return Err(format!(
            "encode figure PNG: {} bytes for {width} x {height}",
            pixels.len()
        ));
    }
    pixmap.data_mut().copy_from_slice(pixels);
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
    // Keep `background` itself set to the true theme color so the automatic
    // title's contrast decision (`visual_title`, called from `render_svg`)
    // still sees it; only suppress painting the opaque rect that would
    // otherwise hide the texture layer drawn onto `pixmap` below.
    vector_scene.hide_background_paint();
    let svg = render_svg(&vector_scene);
    // usvg intentionally starts with an empty font database. All figure
    // renders share the same immutable bundled database, so glyph coverage
    // and metrics cannot depend on a user's installed fonts.
    let options = resvg::usvg::Options {
        fontdb: figure_font_database(),
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

/// Rasterize only the textured elements of `scene` (the embedded rasters and
/// the orthographic globe) onto a transparent canvas at `scale` pixels per
/// scene unit, leaving every vector element and the background undrawn.
///
/// This is the raster half of the interactive viewport's split rendering:
/// the vector elements are drawn as `egui` shapes directly, and only the
/// texture, which has no vector equivalent, still passes through a pixel
/// buffer. On the route globe the SVG round-trip of the vector overlay cost
/// about 80 ms per camera frame at 1.5x density while the sphere itself
/// cost about 4 ms (2026-09-11), so this split is what makes orbiting the
/// globe interactive. `None` when the scene has no textured element, so the
/// caller can skip the texture entirely.
pub fn render_scene_textures_rgba_scaled(scene: &Scene, scale: f64) -> Option<RasterResult> {
    if !scene.elements.iter().any(|element| {
        matches!(
            element,
            SceneElement::Image { .. } | SceneElement::SphericalImage { .. }
        )
    }) {
        return None;
    }
    let scale = scale.max(1.0);
    let width = (scene.width.max(1.0) * scale).round() as u32;
    let height = (scene.height.max(1.0) * scale).round() as u32;
    let Some(mut pixmap) = tiny_skia::Pixmap::new(width, height) else {
        return Some(Err("allocate texture canvas".to_owned()));
    };
    draw_embedded_textures(scene, &mut pixmap, scale);
    draw_spherical_textures(scene, &mut pixmap, scale);
    Some(Ok((width, height, pixmap.data().to_vec())))
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
            clip,
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
            SpherePlacement {
                center: *center,
                radius: *radius,
                clip: *clip,
            },
            *camera,
            *mirror_longitude,
            scale,
        );
    }
}

/// Where a projected sphere sits in scene coordinates, and the figure area it
/// may paint.
struct SpherePlacement {
    center: [f64; 2],
    radius: f64,
    clip: Option<[f64; 4]>,
}

fn draw_equirectangular_sphere(
    destination: &mut tiny_skia::Pixmap,
    texture: tiny_skia::PixmapRef<'_>,
    placement: SpherePlacement,
    camera: Camera3D,
    mirror_longitude: bool,
    scale: f64,
) {
    let SpherePlacement {
        center,
        radius,
        clip,
    } = placement;
    let center_x = center[0] * scale;
    let center_y = center[1] * scale;
    let radius_px = radius * scale;
    // A zoomed globe is larger than the figure area that frames it. The clip
    // keeps the sphere inside that area instead of over the figure's title,
    // footnote and colorbar.
    let (clip_left, clip_top, clip_right, clip_bottom) = match clip {
        Some([x, y, width, height]) if [x, y, width, height].iter().all(|v| v.is_finite()) => (
            x * scale,
            y * scale,
            (x + width) * scale,
            (y + height) * scale,
        ),
        _ => (
            0.0,
            0.0,
            f64::from(destination.width()),
            f64::from(destination.height()),
        ),
    };
    let left = (center_x - radius_px).floor().max(clip_left).max(0.0) as u32;
    let right = (center_x + radius_px)
        .ceil()
        .min(clip_right)
        .min(f64::from(destination.width()))
        .max(0.0) as u32;
    let top = (center_y - radius_px).floor().max(clip_top).max(0.0) as u32;
    let bottom = (center_y + radius_px)
        .ceil()
        .min(clip_bottom)
        .min(f64::from(destination.height()))
        .max(0.0) as u32;
    if bottom <= top || right <= left {
        return;
    }
    // Every row is projected independently and written into its own slice
    // of the canvas, so the rows run on the rayon pool; the per-pixel
    // arithmetic is unchanged and the result is identical to a serial pass.
    let rotation = CameraRotation::new(camera);
    let stride = destination.width() as usize * 4;
    let rows = &mut destination.data_mut()[top as usize * stride..bottom as usize * stride];
    rows.par_chunks_mut(stride)
        .enumerate()
        .for_each(|(offset, row)| {
            let y = top + offset as u32;
            let dy = (f64::from(y) + 0.5 - center_y) / radius_px;
            for x in left..right {
                let dx = (f64::from(x) + 0.5 - center_x) / radius_px;
                let radial_sq = dx * dx + dy * dy;
                if radial_sq > 1.0 {
                    continue;
                }
                let depth = (1.0 - radial_sq).sqrt();
                let [world_x, world_y, z] = rotation.sample_direction(dx, dy, depth);
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
                let target_index = x as usize * 4;
                row[target_index..target_index + 4]
                    .copy_from_slice(&texture.data()[source_index..source_index + 4]);
            }
        });
}

/// Reconstruct the unit-length globe direction under one orthographic pixel.
/// This is the inverse of [`Camera3D::project`]; the caller may then reflect
/// the source longitude together with the globe's geographic geometry.
#[cfg(test)]
fn sphere_sample_direction(dx: f64, dy: f64, depth: f64, camera: Camera3D) -> [f64; 3] {
    CameraRotation::new(camera).sample_direction(dx, dy, depth)
}

/// Sines and cosines of a camera's azimuth and elevation, computed once per
/// globe rather than once per pixel.
#[derive(Clone, Copy)]
struct CameraRotation {
    sin_azimuth: f64,
    cos_azimuth: f64,
    sin_elevation: f64,
    cos_elevation: f64,
}

impl CameraRotation {
    fn new(camera: Camera3D) -> Self {
        let (sin_azimuth, cos_azimuth) = camera.azim_deg.to_radians().sin_cos();
        let (sin_elevation, cos_elevation) = camera.elev_deg.to_radians().sin_cos();
        Self {
            sin_azimuth,
            cos_azimuth,
            sin_elevation,
            cos_elevation,
        }
    }

    /// The unit-length globe direction under one orthographic pixel: the
    /// inverse of [`Camera3D::project`].
    fn sample_direction(self, dx: f64, dy: f64, depth: f64) -> [f64; 3] {
        let y_rot = dy * self.sin_elevation + depth * self.cos_elevation;
        let z = -dy * self.cos_elevation + depth * self.sin_elevation;
        [
            dx * self.cos_azimuth + y_rot * self.sin_azimuth,
            -dx * self.sin_azimuth + y_rot * self.cos_azimuth,
            z,
        ]
    }
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

fn figure_font_database() -> Arc<resvg::usvg::fontdb::Database> {
    static FONT_DATABASE: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    FONT_DATABASE
        .get_or_init(|| Arc::new(bundled_font_database()))
        .clone()
}

/// Build the deterministic portion of the figure font database.
///
/// The load order is intentional: usvg walks its database when a primary face
/// lacks a glyph, so the mathematical face precedes the monospaced face.
fn bundled_font_database() -> resvg::usvg::fontdb::Database {
    let mut database = resvg::usvg::fontdb::Database::new();
    database.load_font_data(PROPORTIONAL_FONT_BYTES.to_vec());
    database.load_font_data(MATH_FONT_BYTES.to_vec());
    database.load_font_data(MONOSPACE_FONT_BYTES.to_vec());
    database.set_sans_serif_family(PROPORTIONAL_FAMILY);
    database.set_serif_family(PROPORTIONAL_FAMILY);
    database
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
    use super::{
        bundled_font_database, render_scene_png, render_scene_rgba, render_scene_rgba_scaled,
        render_scene_textures_rgba_scaled, source_longitude, sphere_sample_direction,
    };
    use alas_fonts::{MATH_FAMILY, MONOSPACE_FAMILY, PROPORTIONAL_FAMILY};
    use alas_report::scene::{Camera3D, Color, Scene, SceneElement, TextAlign, TextBaseline};

    const ENGINEERING_GLYPH_CORPUS: &str = concat!(
        "\u{03B1}\u{03B7}\u{03C1}\u{03C3}\u{03C9}\u{1E41}",
        "\u{2070}\u{00B2}\u{00B3}\u{2080}\u{2092}\u{2095}\u{209A}\u{209C}",
        "\u{00B0}\u{00B1}\u{00B7}\u{00D7}\u{2212}\u{221A}\u{2264}\u{2265}\u{2260}\u{2192}\u{2202}\u{2207}\u{2211}\u{222B}\u{1D6FC}"
    );

    #[test]
    fn bundled_figure_fonts_render_engineering_text_without_host_fonts() {
        let database = bundled_font_database();
        assert_eq!(database.len(), 3, "no host fonts belong in a GUI figure");
        for family in [PROPORTIONAL_FAMILY, MATH_FAMILY, MONOSPACE_FAMILY] {
            assert!(
                database
                    .faces()
                    .any(|face| face.families.iter().any(|entry| entry.0 == family)),
                "bundled figure database is missing {family}"
            );
        }

        let mut scene = Scene::new(800.0, 160.0, Some(Color::rgb(255, 255, 255)));
        scene.add(SceneElement::Text {
            text: ENGINEERING_GLYPH_CORPUS.to_owned(),
            pos: [20.0, 50.0],
            font_size: 15.0,
            color: Color::rgb(0, 0, 0),
            align: TextAlign::Left,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: false,
        });
        let (_width, _height, rgba) = render_scene_rgba(&scene).expect("figure rasterizes");
        assert!(
            rgba.chunks_exact(4)
                .any(|pixel| pixel != [255, 255, 255, 255]),
            "engineering text must paint into the bundled-font raster"
        );
    }

    /// Regression for the actual GUI figure-card bug: this function used to
    /// clear `scene.background` before calling `render_svg` so the vector
    /// layer stayed transparent over pre-painted texture content. That also
    /// blinded `visual_title`'s contrast decision (it reads the same field),
    /// so every automatic figure title rendered in its near-black
    /// light-theme color on Grey and Dark, on top of a still-correctly-dark
    /// background, reproducing the reported "black global titles on Grey
    /// background despite white panel titles". Panel headings were
    /// unaffected because they are colored explicitly from the palette, not
    /// through `visual_title`.
    #[test]
    fn automatic_title_stays_legible_against_dark_and_grey_backgrounds() {
        for (theme_name, bg_hex) in [("grey", "#3a3a3a"), ("dark", "#1e1e1e")] {
            let bg = Color::from_hex(bg_hex);
            let mut scene = Scene::new(300.0, 120.0, Some(bg));
            scene.title = Some("Themed Figure Title".to_owned());

            let (width, _height, rgba) =
                render_scene_rgba_scaled(&scene, 1.0).expect("raster succeeds");

            // The automatic title sits at [width * 0.5, 18.0] in scene units;
            // scan a small neighborhood for its brightest opaque pixel, since
            // anti-aliased glyph edges vary in exact intensity.
            let cx = (width as f64 * 0.5) as i64;
            let mut brightest = 0u8;
            for dy in -8i64..=8 {
                for dx in -40i64..=40 {
                    let (x, y) = ((cx + dx) as u32, (18 + dy) as u32);
                    let idx = ((y * width + x) * 4) as usize;
                    let Some(pixel) = rgba.get(idx..idx + 4) else {
                        continue;
                    };
                    if pixel[3] > 10 {
                        let luma =
                            (u32::from(pixel[0]) + u32::from(pixel[1]) + u32::from(pixel[2])) / 3;
                        brightest = brightest.max(luma as u8);
                    }
                }
            }
            assert!(
                brightest > 200,
                "theme={theme_name}: automatic title should paint near-white glyphs against \
                 background {bg_hex}, brightest sampled luma was {brightest}"
            );
        }
    }

    #[test]
    fn scene_rasterization_returns_a_decodable_png() {
        let scene = Scene::new(64.0, 48.0, Some(Color::rgb(12, 34, 56)));
        let png = render_scene_png(&scene).expect("PNG rasterization");

        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert!(png.len() > 100);
    }

    #[test]
    fn a_clipped_sphere_paints_inside_its_figure_area_only() {
        let mut scene = Scene::new(80.0, 80.0, None);
        scene.add(SceneElement::SphericalImage {
            source: "embedded://nasa-blue-marble".to_owned(),
            center: [40.0, 40.0],
            // A globe zoomed past its own figure area, as the maximized route
            // view allows.
            radius: 70.0,
            camera: Camera3D::front(),
            mirror_longitude: false,
            clip: Some([20.0, 25.0, 40.0, 30.0]),
        });

        let (width, _, pixels) = render_scene_textures_rgba_scaled(&scene, 1.0)
            .expect("textured scene")
            .expect("texture raster");
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let x = (index as u32 % width) as f64;
            let y = (index as u32 / width) as f64;
            let inside = (20.0..60.0).contains(&x) && (25.0..55.0).contains(&y);
            if !inside {
                assert_eq!(pixel[3], 0, "sphere painted outside its clip at {x},{y}");
            }
        }
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] > 0));
    }

    #[test]
    fn the_texture_layer_alone_matches_the_full_raster_of_a_texture_only_scene() {
        let mut scene = Scene::new(65.0, 65.0, None);
        scene.add(SceneElement::SphericalImage {
            source: "embedded://nasa-blue-marble".to_owned(),
            center: [32.5, 32.5],
            radius: 30.0,
            camera: Camera3D::front(),
            mirror_longitude: false,
            clip: None,
        });
        let full = super::render_scene_rgba_scaled(&scene, 1.5).expect("full raster");
        let textures = super::render_scene_textures_rgba_scaled(&scene, 1.5)
            .expect("the scene has a texture")
            .expect("texture raster");
        assert_eq!(textures, full);
        assert!(textures.2.chunks_exact(4).any(|pixel| pixel[3] > 0));
    }

    #[test]
    fn a_scene_without_textures_has_no_texture_layer() {
        let mut scene = Scene::new(40.0, 20.0, Some(Color::rgb(0, 0, 0)));
        scene.add(SceneElement::Line {
            p1: [0.0, 0.0],
            p2: [40.0, 20.0],
            stroke: alas_report::scene::Stroke::new(Color::rgb(255, 255, 255), 1.0),
        });
        assert!(super::render_scene_textures_rgba_scaled(&scene, 1.0).is_none());
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
            clip: None,
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
