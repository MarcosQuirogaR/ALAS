// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Headless software renders of Aircraft-page forms for
//! review. The pictures go to the folder named by `ALAS_RENDER_DIR` when it is set and exists.
//! The rasteriser is a small triangle filler over egui's tessellated meshes
//! (no GPU), enough to judge layout.

// A render test panics when its fixture is unusable; that is the failure.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use alas_gui::detail::Detail;
use alas_gui::state::AppState;
use egui::epaint::Primitive;
use egui::{pos2, vec2, Color32, Context, Rect, TextureId};

struct Texture {
    size: [usize; 2],
    pixels: Vec<Color32>,
}

struct Raster {
    pixels: Vec<Color32>,
    w: usize,
    h: usize,
    texts: Vec<(String, Rect)>,
}

fn collect_texts(shape: &egui::Shape, out: &mut Vec<(String, Rect)>) {
    match shape {
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect_texts(s, out)),
        egui::Shape::Text(text) => out.push((
            text.galley.text().to_owned(),
            text.galley.rect.translate(text.pos.to_vec2()),
        )),
        _ => {}
    }
}

/// Run `frames` frames of `add` on a `size` point screen and rasterise the last.
fn render(size: (f32, f32), frames: usize, mut add: impl FnMut(&Context, &mut egui::Ui)) -> Raster {
    let ctx = Context::default();
    alas_gui::apply_theme(alas_gui::AppTheme::Dark, &ctx);
    alas_gui::detail::publish(&ctx, Detail::Advanced);
    let mut textures: HashMap<TextureId, Texture> = HashMap::new();
    let mut last = None;
    for _ in 0..frames {
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(size.0, size.1))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::central_panel(ctx.style().as_ref())
                            .inner_margin(egui::Margin::symmetric(26.0, 16.0)),
                    )
                    .show(ctx, |ui| add(ctx, ui));
            },
        );
        for (id, delta) in &output.textures_delta.set {
            let (tsize, pixels): ([usize; 2], Vec<Color32>) = match &delta.image {
                egui::ImageData::Color(image) => (image.size, image.pixels.clone()),
                egui::ImageData::Font(image) => (image.size, image.srgba_pixels(None).collect()),
            };
            let texture = textures.entry(*id).or_insert_with(|| Texture {
                size: [0, 0],
                pixels: Vec::new(),
            });
            match delta.pos {
                None => {
                    texture.size = tsize;
                    texture.pixels = pixels;
                }
                Some([x0, y0]) => {
                    for row in 0..tsize[1] {
                        for column in 0..tsize[0] {
                            let target = (y0 + row) * texture.size[0] + x0 + column;
                            texture.pixels[target] = pixels[row * tsize[0] + column];
                        }
                    }
                }
            }
        }
        last = Some(output);
    }
    let output = last.expect("a frame");
    let mut texts = Vec::new();
    for clipped in &output.shapes {
        collect_texts(&clipped.shape, &mut texts);
    }
    let (w, h) = (size.0.ceil() as usize, size.1.ceil() as usize);
    let mut canvas = vec![Color32::from_rgb(30, 30, 30); w * h];
    for primitive in ctx.tessellate(output.shapes, 1.0) {
        let Primitive::Mesh(mesh) = primitive.primitive else {
            continue;
        };
        let texture = textures.get(&mesh.texture_id);
        for triangle in mesh.indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|k| mesh.vertices[triangle[k] as usize]);
            let area = (b.pos.x - a.pos.x) * (c.pos.y - a.pos.y)
                - (b.pos.y - a.pos.y) * (c.pos.x - a.pos.x);
            if area.abs() < 1e-6 {
                continue;
            }
            let clip = primitive.clip_rect;
            let min_x = a.pos.x.min(b.pos.x).min(c.pos.x).floor().max(clip.left());
            let max_x = a.pos.x.max(b.pos.x).max(c.pos.x).ceil().min(clip.right());
            let min_y = a.pos.y.min(b.pos.y).min(c.pos.y).floor().max(clip.top());
            let max_y = a.pos.y.max(b.pos.y).max(c.pos.y).ceil().min(clip.bottom());
            let mut y = min_y.max(0.0);
            while y < max_y.min(h as f32) {
                let mut x = min_x.max(0.0);
                while x < max_x.min(w as f32) {
                    let p = pos2(x + 0.5, y + 0.5);
                    let w0 = ((b.pos.x - p.x) * (c.pos.y - p.y)
                        - (b.pos.y - p.y) * (c.pos.x - p.x))
                        / area;
                    let w1 = ((c.pos.x - p.x) * (a.pos.y - p.y)
                        - (c.pos.y - p.y) * (a.pos.x - p.x))
                        / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 >= -0.001 && w1 >= -0.001 && w2 >= -0.001 {
                        let u = w0 * a.uv.x + w1 * b.uv.x + w2 * c.uv.x;
                        let v = w0 * a.uv.y + w1 * b.uv.y + w2 * c.uv.y;
                        let sample = texture.map_or(Color32::WHITE, |t| {
                            let tx =
                                ((u * t.size[0] as f32) as usize).min(t.size[0].saturating_sub(1));
                            let ty =
                                ((v * t.size[1] as f32) as usize).min(t.size[1].saturating_sub(1));
                            t.pixels
                                .get(ty * t.size[0] + tx)
                                .copied()
                                .unwrap_or(Color32::WHITE)
                        });
                        let channel = |k: usize| {
                            [a.color, b.color, c.color]
                                .iter()
                                .zip([w0, w1, w2])
                                .map(|(color, weight)| f32::from(color.to_array()[k]) * weight)
                                .sum::<f32>()
                        };
                        let mix = |k: usize| channel(k) * f32::from(sample.to_array()[k]) / 255.0;
                        let alpha = mix(3) / 255.0;
                        let target = &mut canvas[y as usize * w + x as usize];
                        let old = target.to_array();
                        let out = [0, 1, 2].map(|k| {
                            (mix(k) + f32::from(old[k]) * (1.0 - alpha)).clamp(0.0, 255.0) as u8
                        });
                        *target = Color32::from_rgb(out[0], out[1], out[2]);
                    }
                    x += 1.0;
                }
                y += 1.0;
            }
        }
    }
    Raster {
        pixels: canvas,
        w,
        h,
        texts,
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

/// A PNG (RGB, stored deflate blocks) of rows `y0..y1` of the raster.
fn png(raster: &Raster, y0: usize, y1: usize) -> Vec<u8> {
    let (w, h) = (raster.w, y1 - y0);
    let mut raw = Vec::with_capacity((w * 3 + 1) * h);
    for row in y0..y1 {
        raw.push(0);
        for pixel in &raster.pixels[row * w..(row + 1) * w] {
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

/// Write the raster in horizontal slices of at most `slice` rows.
fn write(name: &str, raster: &Raster, slice: usize) {
    let dir_name = std::env::var("ALAS_RENDER_DIR").unwrap_or_default();
    let dir = std::path::Path::new(&dir_name);
    if !dir.is_dir() {
        return;
    }
    // Trim the empty bottom: the last row that differs from the background.
    let background = Color32::from_rgb(30, 30, 30);
    let used = (0..raster.h)
        .rev()
        .find(|&row| {
            raster.pixels[row * raster.w..(row + 1) * raster.w]
                .iter()
                .any(|p| *p != background && p.to_array()[..3] != [27, 27, 27])
        })
        .map_or(raster.h, |row| (row + 24).min(raster.h));
    let mut start = 0;
    let mut index = 0;
    while start < used {
        let end = (start + slice).min(used);
        std::fs::write(
            dir.join(format!("{name}-{index}.png")),
            png(raster, start, end),
        )
        .expect("write png");
        start = end;
        index += 1;
    }
}

fn state_for(preset: &str) -> AppState {
    let mut state = AppState::default();
    state.config_values = serde_json::json!({"preset": preset});
    // Resolve the preset into full values as the app does.
    if let Ok(config) = alas_config::AlasConfig::from_value(&state.config_values) {
        state.config_values = serde_json::to_value(&config).unwrap();
    }
    state
}

fn page_shot(name: &str, page_id: &str, width: f32, height: f32) {
    let mut state = state_for("AVE");
    let page = alas_gui::nav::page(page_id).unwrap_or_else(|| panic!("page {page_id}"));
    let raster = render((width, height), 3, |_, ui| {
        alas_gui::views::form_page::show_form_page(&mut state, ui, page);
    });
    assert!(!raster.texts.is_empty());
    write(name, &raster, 1000);
}

#[test]
fn form_pages_render() {
    for (name, id, width, height) in [
        ("r31-geometry", "geometry", 1050.0, 3200.0),
        ("r31-propulsion", "engine_designer", 1050.0, 2400.0),
        ("r31-propulsion-adv", "propulsion_advanced", 1050.0, 3400.0),
        ("r31-optimizer", "optimizer", 1050.0, 2400.0),
        ("r31-cabin", "cabin", 1250.0, 2400.0),
    ] {
        if alas_gui::nav::page(id).is_some() {
            page_shot(name, id, width, height);
        } else {
            panic!("no page {id}");
        }
    }
}

#[test]
fn inputs_page_renders() {
    let mut state = state_for("AVE");
    let raster = render((1050.0, 2200.0), 3, |_, ui| {
        alas_gui::views::show_inputs_view(&mut state, ui);
    });
    assert!(!raster.texts.is_empty());
    write("r31-inputs", &raster, 1100);
}

#[test]
fn narrow_propulsion_renders() {
    page_shot(
        "r31-propulsion-narrow",
        "propulsion_advanced",
        560.0,
        1800.0,
    );
}

fn page_raster(page_id: &str, width: f32, height: f32) -> Raster {
    let mut state = state_for("AVE");
    let page = alas_gui::nav::page(page_id).unwrap_or_else(|| panic!("page {page_id}"));
    render((width, height), 3, |_, ui| {
        alas_gui::views::form_page::show_form_page(&mut state, ui, page);
    })
}

/// A1: no heading nests under another of the same card, and no title repeats.
#[test]
fn propulsion_and_optimizer_headings_are_flat() {
    let propulsion = page_raster("propulsion_advanced", 1050.0, 3400.0);
    assert!(
        !propulsion
            .texts
            .iter()
            .any(|(text, _)| text.starts_with("Advanced (")),
        "no Advanced (n) accordion inside the installation card"
    );
    assert!(propulsion
        .texts
        .iter()
        .any(|(text, _)| text == "Nacelle max radius"));
    let optimizer = page_raster("optimizer", 1050.0, 2400.0);
    let titles = optimizer
        .texts
        .iter()
        .filter(|(text, _)| text == "Objective")
        .count();
    assert_eq!(titles, 1, "the field label only, not a second card title");
}

/// A2: each header of the wing-station table ends where its values end.
#[test]
fn the_wing_station_table_headers_sit_over_their_columns() {
    let raster = page_raster("geometry", 1050.0, 3200.0);
    for header in ["Span fraction", "Ground height [m]", "Flight rise [m]"] {
        let (_, head) = raster
            .texts
            .iter()
            .find(|(text, _)| text == header)
            .unwrap_or_else(|| panic!("header {header}"));
        let aligned = raster.texts.iter().any(|(text, rect)| {
            rect.top() > head.bottom()
                && rect.top() < head.bottom() + 40.0
                && text.parse::<f64>().is_ok()
                && (rect.right() - head.right()).abs() < 2.0
        });
        assert!(aligned, "{header} {head:?}");
    }
}

/// A7: the two spanwise engine positions stand on one line.
#[test]
fn the_two_engine_positions_share_one_baseline() {
    let raster = page_raster("propulsion_advanced", 1050.0, 3400.0);
    let tops: Vec<f32> = raster
        .texts
        .iter()
        .filter(|(text, _)| text == "10.7 m" || text == "-10.7 m")
        .map(|(_, rect)| rect.center().y)
        .collect();
    assert_eq!(tops.len(), 2, "{tops:?}");
    assert!((tops[0] - tops[1]).abs() < 0.5, "{tops:?}");
}

/// The Propulsion settings tab never lays out wider than a narrow, short
/// window while the scroll bar appears (a grid placed columns from the previous
/// frame and overshot by the bar's width for one frame).
#[test]
fn the_propulsion_tab_never_overflows_a_narrow_window() {
    let mut state = state_for("A320-200");
    state.set_design_mode(alas_config::DesignMode::ReferenceAdaptation);
    state.sandbox.advanced_tab = "propulsion_advanced".to_owned();
    let ctx = Context::default();
    alas_gui::apply_theme(alas_gui::AppTheme::Dark, &ctx);
    for frame_no in 0..4 {
        let mut used = Rect::NOTHING;
        let mut content = Rect::NOTHING;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(560.0, 380.0))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    content = ui.max_rect();
                    alas_gui::sandbox::advanced::show_advanced_settings_contents(&mut state, ui);
                    used = ui.min_rect();
                });
            },
        );
        assert!(
            used.right() <= content.right() + 0.5,
            "frame {frame_no}: {used:?} in {content:?}"
        );
    }
}
