// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Textured triangle rasterizer for the CFD evidence renders.
//!
//! egui hands every shape to the backend as a mesh whose vertices carry a UV
//! into the font atlas: solid geometry points at the atlas's white texel, and
//! a glyph points at its own coverage bitmap.  Filling each triangle with the
//! *average* coverage over its UV box -- the earlier approach -- therefore turns
//! every glyph into a flat grey rectangle, which is legible as layout but not
//! as text.
//!
//! Sampling the atlas per pixel, at the barycentric-interpolated UV, is what
//! makes the letters come out. That is all this module does; it is the same
//! alpha-texture blend a real backend performs, on the CPU, for a test.

use egui::{Color32, Context, FullOutput};

/// One premultiplied RGBA canvas in 0..=1 floats.
struct Canvas {
    width: usize,
    height: usize,
    pixels: Vec<[f32; 4]>,
}

impl Canvas {
    fn new(width: usize, height: usize, background: Color32) -> Self {
        let fill = [
            f32::from(background.r()) / 255.0,
            f32::from(background.g()) / 255.0,
            f32::from(background.b()) / 255.0,
            1.0,
        ];
        Self {
            width,
            height,
            pixels: vec![fill; width * height],
        }
    }

    /// Source-over blend of one premultiplied sample.
    fn blend(&mut self, x: usize, y: usize, source: [f32; 4]) {
        let destination = &mut self.pixels[y * self.width + x];
        let inverse = 1.0 - source[3];
        for channel in 0..4 {
            destination[channel] = source[channel] + destination[channel] * inverse;
        }
    }

    fn into_rgba8(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.pixels.len() * 4);
        for pixel in self.pixels {
            for channel in pixel {
                bytes.push((channel.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
            }
        }
        bytes
    }
}

/// One decoded user texture: an artifact the UI loaded from disk, such as a
/// rendered contour PNG.
///
/// egui uploads these premultiplied; the bytes decoded from a PNG are straight
/// alpha, so [`UserTexture::sample`] premultiplies to match what a real backend
/// would hold.
pub(super) struct UserTexture {
    pub width: usize,
    pub height: usize,
    /// Straight (unmultiplied) RGBA8, exactly as decoded.
    pub rgba: Vec<u8>,
}

/// Decoded artifacts by the texture id the UI registered them under.
pub(super) type UserTextures = std::collections::HashMap<egui::TextureId, UserTexture>;

impl UserTexture {
    /// Bilinear sample returning premultiplied RGBA in 0..=1.
    fn sample(&self, u: f32, v: f32) -> [f32; 4] {
        let x = (u * self.width as f32 - 0.5).clamp(0.0, self.width as f32 - 1.0);
        let y = (v * self.height as f32 - 0.5).clamp(0.0, self.height as f32 - 1.0);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let texel = |cx: usize, cy: usize, channel: usize| {
            f32::from(self.rgba[(cy * self.width + cx) * 4 + channel]) / 255.0
        };
        let mut sampled = [0.0_f32; 4];
        for (channel, value) in sampled.iter_mut().enumerate() {
            let top = texel(x0, y0, channel) * (1.0 - fx) + texel(x1, y0, channel) * fx;
            let bottom = texel(x0, y1, channel) * (1.0 - fx) + texel(x1, y1, channel) * fx;
            *value = top * (1.0 - fy) + bottom * fy;
        }
        let alpha = sampled[3];
        for value in sampled.iter_mut().take(3) {
            *value *= alpha;
        }
        sampled
    }
}

/// What a triangle reads per pixel: font coverage, or a decoded artifact.
enum Sampler<'a> {
    Font(&'a Atlas),
    Image(&'a UserTexture),
}

/// The font atlas as a coverage texture, sampled bilinearly.
struct Atlas {
    width: usize,
    height: usize,
    coverage: Vec<f32>,
}

impl Atlas {
    fn sample(&self, u: f32, v: f32) -> f32 {
        let x = (u * self.width as f32 - 0.5).clamp(0.0, self.width as f32 - 1.0);
        let y = (v * self.height as f32 - 0.5).clamp(0.0, self.height as f32 - 1.0);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let texel = |cx: usize, cy: usize| self.coverage[cy * self.width + cx];
        let top = texel(x0, y0) * (1.0 - fx) + texel(x1, y0) * fx;
        let bottom = texel(x0, y1) * (1.0 - fx) + texel(x1, y1) * fx;
        top * (1.0 - fy) + bottom * fy
    }
}

/// Rasterize one finished egui frame to a premultiplied RGBA PNG.
///
/// Pixel-centre sampling with no multisampling: thin strokes and small glyphs
/// come out slightly harder than on a GPU backend, but every letter is formed
/// from its own coverage bitmap and is readable at 1:1.
pub(super) fn render_frame_png(
    ctx: &Context,
    output: FullOutput,
    size: egui::Vec2,
    background: Color32,
    textures: &UserTextures,
) -> Vec<u8> {
    let width = size.x.round().max(1.0) as usize;
    let height = size.y.round().max(1.0) as usize;
    let atlas = ctx.fonts(|fonts| {
        let image = fonts.image();
        Atlas {
            width: image.size[0],
            height: image.size[1],
            coverage: image.pixels.clone(),
        }
    });
    let mut canvas = Canvas::new(width, height, background);
    for primitive in ctx.tessellate(output.shapes, output.pixels_per_point) {
        let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else {
            continue;
        };
        // The font atlas is the default id; anything else is an artifact the UI
        // loaded from disk. It is drawn only when the caller supplied the same
        // decoded bytes -- never invented, and never filled flat, so a texture
        // that failed to decode stays visibly empty instead of becoming a
        // fabricated picture.
        let sampler = if mesh.texture_id == egui::TextureId::default() {
            Sampler::Font(&atlas)
        } else {
            match textures.get(&mesh.texture_id) {
                Some(texture) => Sampler::Image(texture),
                None => continue,
            }
        };
        for indices in mesh.indices.chunks(3) {
            let [a, b, c] = [
                mesh.vertices[indices[0] as usize],
                mesh.vertices[indices[1] as usize],
                mesh.vertices[indices[2] as usize],
            ];
            triangle(&mut canvas, &sampler, [a, b, c], primitive.clip_rect);
        }
    }
    let bytes = canvas.into_rgba8();
    alas_viz::raster::encode_png_rgba(width as u32, height as u32, &bytes).expect("encode png")
}

/// Rasterize one textured triangle with barycentric interpolation.
fn triangle(
    canvas: &mut Canvas,
    sampler: &Sampler<'_>,
    vertices: [egui::epaint::Vertex; 3],
    clip: egui::Rect,
) {
    let positions = vertices.map(|vertex| vertex.pos);
    if positions
        .iter()
        .any(|position| !position.x.is_finite() || !position.y.is_finite())
    {
        return;
    }
    let area = edge(positions[0], positions[1], positions[2]);
    if area.abs() < 1.0e-9 {
        return;
    }
    let left = positions
        .iter()
        .map(|position| position.x)
        .fold(f32::INFINITY, f32::min)
        .max(clip.left())
        .max(0.0)
        .floor() as i64;
    let right = positions
        .iter()
        .map(|position| position.x)
        .fold(f32::NEG_INFINITY, f32::max)
        .min(clip.right())
        .min(canvas.width as f32 - 1.0)
        .ceil() as i64;
    let top = positions
        .iter()
        .map(|position| position.y)
        .fold(f32::INFINITY, f32::min)
        .max(clip.top())
        .max(0.0)
        .floor() as i64;
    let bottom = positions
        .iter()
        .map(|position| position.y)
        .fold(f32::NEG_INFINITY, f32::max)
        .min(clip.bottom())
        .min(canvas.height as f32 - 1.0)
        .ceil() as i64;
    if right < left || bottom < top {
        return;
    }
    let inverse_area = 1.0 / area;
    for y in top..=bottom {
        for x in left..=right {
            let point = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
            let w0 = edge(positions[1], positions[2], point) * inverse_area;
            let w1 = edge(positions[2], positions[0], point) * inverse_area;
            let w2 = edge(positions[0], positions[1], point) * inverse_area;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }
            let u = w0 * vertices[0].uv.x + w1 * vertices[1].uv.x + w2 * vertices[2].uv.x;
            let v = w0 * vertices[0].uv.y + w1 * vertices[1].uv.y + w2 * vertices[2].uv.y;
            // The vertex colour tints either sampler; for an image mesh egui
            // normally emits opaque white, so the texel passes through.
            let mut source = [0.0_f32; 4];
            for (weight, vertex) in [w0, w1, w2].into_iter().zip(vertices.iter()) {
                let color = vertex.color;
                source[0] += weight * f32::from(color.r()) / 255.0;
                source[1] += weight * f32::from(color.g()) / 255.0;
                source[2] += weight * f32::from(color.b()) / 255.0;
                source[3] += weight * f32::from(color.a()) / 255.0;
            }
            if source[3] <= 0.002 {
                continue;
            }
            match sampler {
                Sampler::Font(atlas) => {
                    let coverage = atlas.sample(u, v);
                    if coverage <= 0.002 {
                        continue;
                    }
                    for channel in &mut source {
                        *channel *= coverage;
                    }
                }
                Sampler::Image(texture) => {
                    let texel = texture.sample(u, v);
                    if texel[3] <= 0.002 {
                        continue;
                    }
                    for (channel, value) in source.iter_mut().enumerate() {
                        *value *= texel[channel];
                    }
                }
            }
            canvas.blend(x as usize, y as usize, source);
        }
    }
}

/// Signed area of the triangle (a, b, p), the standard edge function.
fn edge(a: egui::Pos2, b: egui::Pos2, p: egui::Pos2) -> f32 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}
