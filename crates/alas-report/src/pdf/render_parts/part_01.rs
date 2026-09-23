// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::win_ansi::pdf_literal;
use super::{PdfFigure, PdfSection};
use crate::scene::{
    text_line_center_offsets, wrap_text_to_width, Color, Fill, Point2D, Scene, SceneElement,
    Stroke, TextAlign, TextBaseline, CSS_PIXELS_PER_POINT, TEXT_LINE_HEIGHT_EM,
};

const PAGE_WIDTH: f64 = 595.28;
const PAGE_HEIGHT: f64 = 841.89;
const PAGE_MARGIN: f64 = 42.0;
const HEADER_HEIGHT: f64 = 54.0;

/// Mean Helvetica advance per character [em], for anchoring centered and
/// right-aligned text without glyph metrics.
const HELVETICA_MEAN_ADVANCE_EM: f64 = 0.52;
/// Distance from the em-box center down to the baseline [em]: half of the
/// 0.8 em ascent minus 0.2 em descent. SVG places each line with
/// `dominant-baseline="central"`, i.e. on this center.
const CENTRAL_TO_BASELINE_EM: f64 = 0.3;

pub(super) fn render_sections(sections: &[&PdfSection]) -> Vec<u8> {
    let mut pages = Vec::new();
    for section in sections {
        pages.push(section_page(section));
        for figure in &section.figures {
            pages.push(figure_page(&section.title, figure));
        }
    }
    assemble_pdf(&pages)
}

fn section_page(section: &PdfSection) -> String {
    let mut page = String::new();
    page.push_str("0.04 0.12 0.25 rg\n0 0 595.28 841.89 re f\n");
    let figure_count = format!(
        "{} vector figures follow in registry order.",
        section.figures.len()
    );
    for (offset, size, color, text) in [
        (150.0, 32.0, Color::rgb(255, 255, 255), section.title.as_str()),
        (194.0, 14.0, Color::rgb(191, 219, 254), "ALAS design report section"),
        (250.0, 12.0, Color::rgb(226, 232, 240), figure_count.as_str()),
        (
            276.0,
            11.0,
            Color::rgb(226, 232, 240),
            "The companion archive contains the matching SVG source for each page.",
        ),
    ] {
        append_page_text(&mut page, PAGE_MARGIN, PAGE_HEIGHT - offset, size, color, text);
    }
    page
}

fn figure_page(section_title: &str, figure: &PdfFigure) -> String {
    let mut page = String::new();
    append_page_text(
        &mut page,
        PAGE_MARGIN,
        PAGE_HEIGHT - 28.0,
        11.0,
        Color::rgb(71, 85, 105),
        section_title,
    );
    append_page_text(
        &mut page,
        PAGE_MARGIN,
        PAGE_HEIGHT - 46.0,
        16.0,
        Color::rgb(15, 23, 42),
        &figure.title,
    );
    append_page_text(
        &mut page,
        PAGE_WIDTH - PAGE_MARGIN,
        PAGE_HEIGHT - 28.0,
        9.0,
        Color::rgb(100, 116, 139),
        &figure.file_name,
    );

    let width = PAGE_WIDTH - 2.0 * PAGE_MARGIN;
    let height = PAGE_HEIGHT - PAGE_MARGIN - HEADER_HEIGHT - PAGE_MARGIN;
    let scale = (width / figure.scene.width).min(height / figure.scene.height);
    let drawn_width = figure.scene.width * scale;
    let drawn_height = figure.scene.height * scale;
    let x = PAGE_MARGIN + (width - drawn_width) * 0.5;
    let y = PAGE_MARGIN + (height - drawn_height) * 0.5;

    page.push_str("q\n");
    page.push_str(&format!(
        "{} {} {} {} re W n\n",
        format_number(PAGE_MARGIN),
        format_number(PAGE_MARGIN),
        format_number(width),
        format_number(height)
    ));
    // Scene units are CSS pixels with y down; this maps them onto the page
    // with y up, so every scene operator below uses scene coordinates.
    page.push_str(&format!(
        "{} 0 0 -{} {} {} cm\n",
        format_number(scale),
        format_number(scale),
        format_number(x),
        format_number(y + drawn_height),
    ));
    append_scene(&mut page, &figure.scene);
    page.push_str("Q\n");
    page
}

fn append_scene(out: &mut String, scene: &Scene) {
    if let Some(background) = scene.background {
        append_fill(out, background);
        out.push_str(&format!(
            "0 0 {} {} re f\n",
            format_number(scene.width),
            format_number(scene.height)
        ));
    }
    for element in &scene.elements {
        match element {
            SceneElement::Line { p1, p2, stroke } => {
                append_stroke_style(out, stroke);
                out.push_str(&format!(
                    "{} {} m {} {} l S\n",
                    format_number(p1[0]),
                    format_number(p1[1]),
                    format_number(p2[0]),
                    format_number(p2[1])
                ));
            }
            SceneElement::Polyline { points, stroke } if points.len() >= 2 => {
                append_stroke_style(out, stroke);
                append_path(out, points);
                out.push_str(" S\n");
            }
            SceneElement::Polyline { .. } => {}
            SceneElement::Polygon {
                points,
                fill,
                stroke,
            } if points.len() >= 2 => {
                append_paint_style(out, fill.as_ref(), stroke.as_ref());
                append_path(out, points);
                out.push_str(" h ");
                append_paint_operator(out, fill.is_some(), stroke.is_some());
            }
            SceneElement::Polygon { .. } => {}
            SceneElement::Rect {
                x,
                y,
                width,
                height,
                fill,
                stroke,
                ..
            } => {
                append_paint_style(out, fill.as_ref(), stroke.as_ref());
                out.push_str(&format!(
                    "{} {} {} {} re ",
                    format_number(*x),
                    format_number(*y),
                    format_number(*width),
                    format_number(*height)
                ));
                append_paint_operator(out, fill.is_some(), stroke.is_some());
            }
            SceneElement::Circle {
                center,
                radius,
                fill,
                stroke,
            } => {
                append_paint_style(out, fill.as_ref(), stroke.as_ref());
                append_circle_path(out, center[0], center[1], *radius);
                append_paint_operator(out, fill.is_some(), stroke.is_some());
            }
            SceneElement::Image {
                source,
                x,
                y,
                width,
                height,
                ..
            } => append_external_image_notice(out, source, [*x, *y, *width, *height]),
            SceneElement::SphericalImage {
                center,
                radius,
                source,
                ..
            } => append_external_image_notice(
                out,
                source,
                [center[0] - radius, center[1] - radius, radius * 2.0, radius * 2.0],
            ),
            SceneElement::Text {
                text,
                pos,
                font_size,
                color,
                align,
                baseline,
                angle_deg,
                ..
            } => {
                let lines = text.split('\n').collect::<Vec<_>>();
                let style = SceneTextStyle {
                    font_size_pt: *font_size,
                    color: *color,
                    align: *align,
                    baseline: *baseline,
                    angle_deg: *angle_deg,
                };
                append_scene_text(out, &lines, *pos, &style);
            }
            SceneElement::TextBlock {
                text,
                pos,
                width,
                font_size,
                color,
                ..
            } => {
                let wrapped = wrap_text_to_width(text, *font_size, *width);
                let lines = wrapped.lines().collect::<Vec<_>>();
                let style = SceneTextStyle {
                    font_size_pt: *font_size,
                    color: *color,
                    align: TextAlign::Left,
                    baseline: TextBaseline::Top,
                    angle_deg: 0.0,
                };
                append_scene_text(out, &lines, *pos, &style);
            }
        }
    }
}

/// `m` to the first point and `l` to each following one, without a paint
/// operator.
fn append_path(out: &mut String, points: &[Point2D]) {
    for (index, point) in points.iter().enumerate() {
        out.push_str(&format!(
            "{}{} {} {}",
            if index == 0 { "" } else { " " },
            format_number(point[0]),
            format_number(point[1]),
            if index == 0 { "m" } else { "l" },
        ));
    }
}

fn append_paint_style(out: &mut String, fill: Option<&Fill>, stroke: Option<&Stroke>) {
    if let Some(fill) = fill {
        append_fill(out, fill.color);
    }
    if let Some(stroke) = stroke {
        append_stroke_style(out, stroke);
    }
}

fn append_stroke_style(out: &mut String, stroke: &Stroke) {
    append_stroke(
        out,
        stroke.color,
        stroke.width,
        stroke.dash_array.as_deref(),
    );
}

fn append_paint_operator(out: &mut String, fill: bool, stroke: bool) {
    out.push_str(match (fill, stroke) {
        (true, true) => "B\n",
        (true, false) => "f\n",
        (false, true) => "S\n",
        (false, false) => "n\n",
    });
}

fn append_external_image_notice(out: &mut String, source: &str, [x, y, width, height]: [f64; 4]) {
    append_stroke(out, Color::rgb(100, 116, 139), 1.0, Some(&[4.0, 3.0]));
    out.push_str(&format!(
        "{} {} {} {} re S\n",
        format_number(x),
        format_number(y),
        format_number(width),
        format_number(height)
    ));
    let notice = format!("External image: {source}");
    let style = SceneTextStyle {
        font_size_pt: 11.0,
        color: Color::rgb(71, 85, 105),
        align: TextAlign::Left,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
    };
    append_scene_text(out, &[notice.as_str()], [x + 8.0, y + height * 0.5], &style);
}

fn append_circle_path(out: &mut String, x: f64, y: f64, radius: f64) {
    let k = radius * 0.552_284_749_8;
    out.push_str(&format!(
        "{} {} m",
        format_number(x + radius),
        format_number(y)
    ));
    for (a, b, c, d, e, f) in [
        (x + radius, y + k, x + k, y + radius, x, y + radius),
        (x - k, y + radius, x - radius, y + k, x - radius, y),
        (x - radius, y - k, x - k, y - radius, x, y - radius),
        (x + k, y - radius, x + radius, y - k, x + radius, y),
    ] {
        out.push_str(&format!(
            " {} {} {} {} {} {} c",
            format_number(a),
            format_number(b),
            format_number(c),
            format_number(d),
            format_number(e),
            format_number(f)
        ));
    }
    out.push_str(" h\n");
}

/// How a scene text element is set: the fields of [`SceneElement::Text`]
/// other than its content and anchor point.
struct SceneTextStyle {
    font_size_pt: f64,
    color: Color,
    align: TextAlign,
    baseline: TextBaseline,
    angle_deg: f64,
}

/// Set text lines in scene coordinates with the same line model as the SVG
/// exporter: the font size is in points on a CSS-pixel canvas, each line is
/// centered on [`text_line_center_offsets`], and the whole block is rotated
/// clockwise by `angle_deg` about `pos`, as SVG `rotate()` does on a y-down
/// canvas.
fn append_scene_text(out: &mut String, lines: &[&str], pos: Point2D, style: &SceneTextStyle) {
    let font_px = style.font_size_pt * CSS_PIXELS_PER_POINT;
    let line_height = font_px * TEXT_LINE_HEIGHT_EM;
    let centers = text_line_center_offsets(lines.len(), line_height, style.baseline);
    let (sin, cos) = style.angle_deg.to_radians().sin_cos();
    for (line, center) in lines.iter().zip(centers) {
        let width = line.chars().count() as f64 * font_px * HELVETICA_MEAN_ADVANCE_EM;
        let along = match style.align {
            TextAlign::Left => 0.0,
            TextAlign::Center => -width * 0.5,
            TextAlign::Right => -width,
        };
        let across = center + font_px * CENTRAL_TO_BASELINE_EM;
        let x = pos[0] + along * cos - across * sin;
        let y = pos[1] + along * sin + across * cos;
        out.push_str("BT\n/F1 ");
        out.push_str(&format_number(font_px));
        out.push_str(" Tf\n");
        append_fill(out, style.color);
        // Glyph space is y-up; the second column flips it against the
        // y-down scene so the text stands upright, then both rotate.
        out.push_str(&format!(
            "{} {} {} {} {} {} Tm\n{} Tj\nET\n",
            format_number(cos),
            format_number(sin),
            format_number(sin),
            format_number(-cos),
            format_number(x),
            format_number(y),
            pdf_literal(line)
        ));
    }
}

fn append_page_text(out: &mut String, x: f64, y: f64, font_size: f64, color: Color, text: &str) {
    out.push_str("BT\n/F1 ");
    out.push_str(&format_number(font_size));
    out.push_str(" Tf\n");
    append_fill(out, color);
    out.push_str(&format!(
        "{} {} Td\n{} Tj\nET\n",
        format_number(x),
        format_number(y),
        pdf_literal(text)
    ));
}

fn append_fill(out: &mut String, color: Color) {
    out.push_str(&format!(
        "{} {} {} rg\n",
        color_component(color.r),
        color_component(color.g),
        color_component(color.b)
    ));
}

fn append_stroke(out: &mut String, color: Color, width: f64, dash: Option<&[f64]>) {
    out.push_str(&format!(
        "{} {} {} RG\n{} w\n",
        color_component(color.r),
        color_component(color.g),
        color_component(color.b),
        format_number(width)
    ));
    match dash {
        Some(dash) if !dash.is_empty() => {
            let items = dash
                .iter()
                .map(|value| format_number(*value))
                .collect::<Vec<_>>();
            out.push_str(&format!("[{}] 0 d\n", items.join(" ")));
        }
        _ => out.push_str("[] 0 d\n"),
    }
}
