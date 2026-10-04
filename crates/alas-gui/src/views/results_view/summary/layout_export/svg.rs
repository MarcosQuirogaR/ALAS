// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Clipped SVG export and text-bound checks of actual egui paint output.

use super::shell::Capture;
use egui::{epaint::ClippedShape, Color32, Rect, Shape};

fn visit(shape: &Shape, callback: &mut impl FnMut(&egui::epaint::TextShape)) {
    match shape {
        Shape::Text(text) => callback(text),
        Shape::Vec(shapes) => {
            for shape in shapes {
                visit(shape, callback);
            }
        }
        _ => {}
    }
}

pub(super) fn all_text(shapes: &[ClippedShape]) -> String {
    let mut result = String::new();
    for shape in shapes {
        visit(&shape.shape, &mut |text| {
            result.push_str(text.galley.text());
            result.push('\n');
        });
    }
    result
}

pub(super) fn text_rect(shapes: &[ClippedShape], expected: &str) -> Option<Rect> {
    let mut result = None;
    for shape in shapes {
        visit(&shape.shape, &mut |text| {
            if text.galley.text().contains(expected) {
                result = Some(text.galley.rect.translate(text.pos.to_vec2()));
            }
        });
    }
    result
}

pub(super) fn assert_bounds(shapes: &[ClippedShape]) {
    for shape in shapes {
        visit(&shape.shape, &mut |text| {
            let bounds = text.galley.rect.translate(text.pos.to_vec2());
            if shape.clip_rect.intersects(bounds) {
                assert!(
                    bounds.left() >= shape.clip_rect.left()
                        && bounds.right() <= shape.clip_rect.right(),
                    "horizontal text clipping: {} at {bounds:?}, clip {:?}",
                    text.galley.text(),
                    shape.clip_rect
                );
            }
            for removed in ["Findings", "Local refinement around the preset"] {
                assert_ne!(text.galley.text(), crate::views::tr(removed));
            }
        });
    }
}

fn color(value: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", value.r(), value.g(), value.b())
}
fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn export_shape(shape: &Shape, svg: &mut String) {
    use std::fmt::Write;
    match shape {
        Shape::Vec(shapes) => {
            for shape in shapes {
                export_shape(shape, svg);
            }
        }
        Shape::Rect(rect) => {
            let _ = write!(svg, "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{}\" fill-opacity=\"{}\" stroke=\"{}\" stroke-width=\"{}\"/>", rect.rect.left(), rect.rect.top(), rect.rect.width(), rect.rect.height(), rect.rounding.nw, color(rect.fill), f32::from(rect.fill.a()) / 255.0, color(rect.stroke.color), rect.stroke.width);
        }
        Shape::Text(text) => {
            let size = text
                .galley
                .job
                .sections
                .first()
                .map_or(14.0, |section| section.format.font_id.size);
            for row in &text.galley.rows {
                let line: String = row.glyphs.iter().map(|glyph| glyph.chr).collect();
                if line.is_empty() {
                    continue;
                }
                let _ = write!(svg, "<text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"{size}\" fill=\"{}\" textLength=\"{}\" lengthAdjust=\"spacingAndGlyphs\">{}</text>", text.pos.x + row.rect.left(), text.pos.y + row.rect.bottom() - 0.2 * size, color(text.fallback_color), row.rect.width(), escaped(&line));
            }
        }
        _ => {}
    }
}

pub(super) fn write(path: &std::path::Path, capture: &Capture, expected: &str) {
    use std::fmt::Write;
    assert_bounds(&capture.output.shapes);
    assert!(all_text(&capture.output.shapes).contains(expected));
    let mut svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\"><desc>Results content: x={} width={} px; seeded A220-300 product optimization.</desc>", capture.viewport.width(), capture.viewport.height(), capture.viewport.width(), capture.viewport.height(), capture.content.left(), capture.content.width());
    for (index, shape) in capture.output.shapes.iter().enumerate() {
        let clip = shape.clip_rect.intersect(capture.viewport);
        let _ = write!(svg, "<defs><clipPath id=\"clip{index}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath></defs><g clip-path=\"url(#clip{index})\">", clip.left(), clip.top(), clip.width().max(0.0), clip.height().max(0.0));
        export_shape(&shape.shape, &mut svg);
        svg.push_str("</g>");
    }
    svg.push_str("</svg>");
    std::fs::write(path, svg).expect("summary SVG");
}
