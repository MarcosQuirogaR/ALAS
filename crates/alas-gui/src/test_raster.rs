// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A small software rasteriser for egui frames (no GPU), so a view can be laid out in the
//! application's theme at the size of a review screenshot, checked for overlapping texts, and
//! written out as a PNG for a look. Triangles are filled with the vertex colours times the
//! texture sample (nearest), alpha-blended onto an opaque canvas; that is enough for the
//! plots, the text atlas and the airframe textures the pages draw.

use std::collections::HashMap;
use std::path::Path;

use egui::epaint::Primitive;
use egui::{pos2, vec2, Color32, Rect, TextureId};

struct Texture {
    size: [usize; 2],
    pixels: Vec<Color32>,
}

/// One rendered frame: the canvas and the texts painted on it with their rectangles.
pub(crate) struct Shot {
    pub pixels: Vec<Color32>,
    pub width: usize,
    pub height: usize,
    pub texts: Vec<(String, Rect)>,
    /// The rectangles painted (boxes, frames, padlock bodies), in screen points.
    pub rects: Vec<Rect>,
}

/// Run `frames` frames of `content` in a `width` x `height` pt screen with the application's
/// dark theme (and its fonts), `pause_ms` apart (work that lands on a worker thread, such as the
/// airframe render, needs a few frames), and rasterise the last one.
pub(crate) fn render(
    width: f32,
    height: f32,
    frames: usize,
    pause_ms: u64,
    content: impl FnMut(&mut egui::Ui),
) -> Shot {
    render_hovering(width, height, frames, pause_ms, None, content)
}

/// [`render`] with the pointer resting at `pointer` (screen points) in every frame, so what a
/// hover shows (a tooltip, a highlight) is part of the picture.
pub(crate) fn render_hovering(
    width: f32,
    height: f32,
    frames: usize,
    pause_ms: u64,
    pointer: Option<egui::Pos2>,
    mut content: impl FnMut(&mut egui::Ui),
) -> Shot {
    let ctx = egui::Context::default();
    crate::theme::apply_theme(crate::theme::AppTheme::Dark, &ctx);
    let background = ctx.style().visuals.panel_fill;
    let mut textures: HashMap<TextureId, Texture> = HashMap::new();
    let mut last = None;
    for frame in 0..frames.max(1) {
        if frame > 0 {
            std::thread::sleep(std::time::Duration::from_millis(pause_ms));
        }
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(width, height))),
                max_texture_side: Some(8_192),
                events: pointer.map(egui::Event::PointerMoved).into_iter().collect(),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::none().fill(background))
                    .show(ctx, |ui| content(ui));
            },
        );
        for (id, delta) in &output.textures_delta.set {
            let (size, pixels): ([usize; 2], Vec<Color32>) = match &delta.image {
                egui::ImageData::Color(image) => (image.size, image.pixels.clone()),
                egui::ImageData::Font(image) => (image.size, image.srgba_pixels(None).collect()),
            };
            let texture = textures.entry(*id).or_insert_with(|| Texture {
                size: [0, 0],
                pixels: Vec::new(),
            });
            match delta.pos {
                None => {
                    texture.size = size;
                    texture.pixels = pixels;
                }
                Some([x0, y0]) => {
                    for row in 0..size[1] {
                        for column in 0..size[0] {
                            let target = (y0 + row) * texture.size[0] + x0 + column;
                            if let Some(slot) = texture.pixels.get_mut(target) {
                                *slot = pixels[row * size[0] + column];
                            }
                        }
                    }
                }
            }
        }
        last = Some(output);
    }
    let Some(output) = last else {
        return Shot {
            pixels: Vec::new(),
            width: 0,
            height: 0,
            texts: Vec::new(),
            rects: Vec::new(),
        };
    };
    let mut texts = Vec::new();
    let mut rects = Vec::new();
    fn collect(shape: &egui::Shape, out: &mut Vec<(String, Rect)>, boxes: &mut Vec<Rect>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out, boxes)),
            egui::Shape::Text(text) => out.push((
                text.galley.text().to_owned(),
                Rect::from_min_size(text.pos, text.galley.size()),
            )),
            egui::Shape::Rect(rect) => boxes.push(rect.rect),
            _ => {}
        }
    }
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut texts, &mut rects);
    }
    let (w, h) = (width.ceil() as usize, height.ceil() as usize);
    let mut canvas = vec![background; w * h];
    for primitive in ctx.tessellate(output.shapes, 1.0) {
        let Primitive::Mesh(mesh) = primitive.primitive else {
            continue;
        };
        let texture = textures.get(&mesh.texture_id);
        for triangle in mesh.indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|k| mesh.vertices[triangle[k] as usize]);
            fill_triangle(&mut canvas, (w, h), primitive.clip_rect, texture, [a, b, c]);
        }
    }
    Shot {
        pixels: canvas,
        width: w,
        height: h,
        texts,
        rects,
    }
}

fn fill_triangle(
    canvas: &mut [Color32],
    (w, h): (usize, usize),
    clip: Rect,
    texture: Option<&Texture>,
    [a, b, c]: [egui::epaint::Vertex; 3],
) {
    let area =
        (b.pos.x - a.pos.x) * (c.pos.y - a.pos.y) - (b.pos.y - a.pos.y) * (c.pos.x - a.pos.x);
    if area.abs() < 1e-6 {
        return;
    }
    let min_x = a
        .pos
        .x
        .min(b.pos.x)
        .min(c.pos.x)
        .floor()
        .max(clip.left())
        .max(0.0);
    let max_x = a
        .pos
        .x
        .max(b.pos.x)
        .max(c.pos.x)
        .ceil()
        .min(clip.right())
        .min(w as f32);
    let min_y = a
        .pos
        .y
        .min(b.pos.y)
        .min(c.pos.y)
        .floor()
        .max(clip.top())
        .max(0.0);
    let max_y = a
        .pos
        .y
        .max(b.pos.y)
        .max(c.pos.y)
        .ceil()
        .min(clip.bottom())
        .min(h as f32);
    let mut y = min_y;
    while y < max_y {
        let mut x = min_x;
        while x < max_x {
            let p = pos2(x + 0.5, y + 0.5);
            let w0 = ((b.pos.x - p.x) * (c.pos.y - p.y) - (b.pos.y - p.y) * (c.pos.x - p.x)) / area;
            let w1 = ((c.pos.x - p.x) * (a.pos.y - p.y) - (c.pos.y - p.y) * (a.pos.x - p.x)) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 >= -0.001 && w1 >= -0.001 && w2 >= -0.001 {
                let u = w0 * a.uv.x + w1 * b.uv.x + w2 * c.uv.x;
                let v = w0 * a.uv.y + w1 * b.uv.y + w2 * c.uv.y;
                let sample = texture.map_or(Color32::WHITE, |t| {
                    let tx = ((u * t.size[0] as f32) as usize).min(t.size[0].saturating_sub(1));
                    let ty = ((v * t.size[1] as f32) as usize).min(t.size[1].saturating_sub(1));
                    t.pixels
                        .get(ty * t.size[0] + tx)
                        .copied()
                        .unwrap_or(Color32::WHITE)
                });
                let vertex = [a.color, b.color, c.color];
                let mix = |k: usize| {
                    let channel: f32 = vertex
                        .iter()
                        .zip([w0, w1, w2])
                        .map(|(color, weight)| f32::from(color.to_array()[k]) * weight)
                        .sum();
                    channel * f32::from(sample.to_array()[k]) / 255.0
                };
                let alpha = mix(3) / 255.0;
                let target = &mut canvas[y as usize * w + x as usize];
                let old = target.to_array();
                let out = [0, 1, 2]
                    .map(|k| (mix(k) + f32::from(old[k]) * (1.0 - alpha)).clamp(0.0, 255.0) as u8);
                *target = Color32::from_rgb(out[0], out[1], out[2]);
            }
            x += 1.0;
        }
        y += 1.0;
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A PNG (RGB, stored deflate blocks) of `shot`.
pub(crate) fn png(shot: &Shot) -> Vec<u8> {
    let (w, h) = (shot.width, shot.height);
    let mut raw = Vec::with_capacity((w * 3 + 1) * h);
    for row in 0..h {
        raw.push(0);
        for pixel in &shot.pixels[row * w..(row + 1) * w] {
            raw.extend_from_slice(&pixel.to_array()[..3]);
        }
    }
    let mut zlib = vec![0x78, 0x01];
    let mut chunks = raw.chunks(65_535).peekable();
    while let Some(chunk) = chunks.next() {
        zlib.push(u8::from(chunks.peek().is_none()));
        zlib.extend_from_slice(&(chunk.len() as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(chunk.len() as u16)).to_le_bytes());
        zlib.extend_from_slice(chunk);
    }
    let (mut a, mut b) = (1_u32, 0_u32);
    for byte in &raw {
        a = (a + u32::from(*byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
    };
    let mut header = Vec::new();
    header.extend_from_slice(&(w as u32).to_be_bytes());
    header.extend_from_slice(&(h as u32).to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &header);
    chunk(b"IDAT", &zlib);
    chunk(b"IEND", &[]);
    out
}

/// Where the review renders are written: the folder named by `ALAS_RENDER_DIR`. With the variable
/// unset or the folder missing, the render is skipped and the layout assertions still run.
pub(crate) fn render_dir() -> String {
    std::env::var("ALAS_RENDER_DIR").unwrap_or_default()
}

/// Write `shot` as `name` (a PNG file name) into the review folder, when it exists.
pub(crate) fn write(name: &str, shot: &Shot) {
    write_in(&render_dir(), name, shot);
}

/// Write `shot` as `name` into `directory`, when that folder exists.
pub(crate) fn write_in(directory: &str, name: &str, shot: &Shot) {
    let directory = Path::new(directory);
    if directory.is_dir() {
        let _ = std::fs::write(directory.join(name), png(shot));
    }
}

/// Texts of `shot` that overlap another text by more than half a point each way: the pairs.
pub(crate) fn overlapping_texts(shot: &Shot) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for (i, (text_a, a)) in shot.texts.iter().enumerate() {
        for (text_b, b) in &shot.texts[i + 1..] {
            let overlap = a.intersect(*b);
            if overlap.width() > 0.5 && overlap.height() > 0.5 {
                pairs.push((text_a.clone(), text_b.clone()));
            }
        }
    }
    pairs
}
