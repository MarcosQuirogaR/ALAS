// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::{PdfFigure, PdfSection};
use crate::scene::{Color, Scene, SceneElement, TextAlign};

const PAGE_WIDTH: f64 = 595.28;
const PAGE_HEIGHT: f64 = 841.89;
const PAGE_MARGIN: f64 = 42.0;
const HEADER_HEIGHT: f64 = 54.0;

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
    append_page_text(
        &mut page,
        PAGE_MARGIN,
        PAGE_HEIGHT - 150.0,
        32.0,
        Color::rgb(255, 255, 255),
        &section.title,
    );
    append_page_text(
        &mut page,
        PAGE_MARGIN,
        PAGE_HEIGHT - 194.0,
        14.0,
        Color::rgb(191, 219, 254),
        "ALAS design report section",
    );
    append_page_text(
        &mut page,
        PAGE_MARGIN,
        PAGE_HEIGHT - 250.0,
        12.0,
        Color::rgb(226, 232, 240),
        &format!(
            "{} vector figures follow in registry order.",
            section.figures.len()
        ),
    );
    append_page_text(
        &mut page,
        PAGE_MARGIN,
        PAGE_HEIGHT - 276.0,
        11.0,
        Color::rgb(226, 232, 240),
        "The companion archive contains the matching SVG source for each page.",
    );
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
                append_stroke(
                    out,
                    stroke.color,
                    stroke.width,
                    stroke.dash_array.as_deref(),
                );
                out.push_str(&format!(
                    "{} {} m {} {} l S\n",
                    format_number(p1[0]),
                    format_number(p1[1]),
                    format_number(p2[0]),
                    format_number(p2[1])
                ));
            }
            SceneElement::Polyline { points, stroke } if points.len() >= 2 => {
                append_stroke(
                    out,
                    stroke.color,
                    stroke.width,
                    stroke.dash_array.as_deref(),
                );
                out.push_str(&format!(
                    "{} {} m",
                    format_number(points[0][0]),
                    format_number(points[0][1])
                ));
                for point in &points[1..] {
                    out.push_str(&format!(
                        " {} {} l",
                        format_number(point[0]),
                        format_number(point[1])
                    ));
                }
                out.push_str(" S\n");
            }
            SceneElement::Polyline { .. } => {}
            SceneElement::Polygon {
                points,
                fill,
                stroke,
            } if points.len() >= 2 => {
                if let Some(fill) = fill {
                    append_fill(out, fill.color);
                }
                if let Some(stroke) = stroke {
                    append_stroke(
                        out,
                        stroke.color,
                        stroke.width,
                        stroke.dash_array.as_deref(),
                    );
                }
                out.push_str(&format!(
                    "{} {} m",
                    format_number(points[0][0]),
                    format_number(points[0][1])
                ));
                for point in &points[1..] {
                    out.push_str(&format!(
                        " {} {} l",
                        format_number(point[0]),
                        format_number(point[1])
                    ));
                }
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
                if let Some(fill) = fill {
                    append_fill(out, fill.color);
                }
                if let Some(stroke) = stroke {
                    append_stroke(
                        out,
                        stroke.color,
                        stroke.width,
                        stroke.dash_array.as_deref(),
                    );
                }
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
                if let Some(fill) = fill {
                    append_fill(out, fill.color);
                }
                if let Some(stroke) = stroke {
                    append_stroke(
                        out,
                        stroke.color,
                        stroke.width,
                        stroke.dash_array.as_deref(),
                    );
                }
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
            } => append_external_image_notice(out, source, *x, *y, *width, *height),
            SceneElement::SphericalImage {
                center,
                radius,
                source,
                ..
            } => {
                append_external_image_notice(
                    out,
                    source,
                    center[0] - radius,
                    center[1] - radius,
                    radius * 2.0,
                    radius * 2.0,
                );
            }
            SceneElement::Text {
                text,
                pos,
                font_size,
                color,
                align,
                ..
            } => append_scene_text(out, text, pos[0], pos[1], *font_size, *color, *align),
        }
    }
}

fn append_paint_operator(out: &mut String, fill: bool, stroke: bool) {
    out.push_str(match (fill, stroke) {
        (true, true) => "B\n",
        (true, false) => "f\n",
        (false, true) => "S\n",
        (false, false) => "n\n",
    });
}

fn append_external_image_notice(
    out: &mut String,
    source: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) {
    append_stroke(out, Color::rgb(100, 116, 139), 1.0, Some(&[4.0, 3.0]));
    out.push_str(&format!(
        "{} {} {} {} re S\n",
        format_number(x),
        format_number(y),
        format_number(width),
        format_number(height)
    ));
    append_scene_text(
        out,
        &format!("External image: {source}"),
        x + 8.0,
        y + height * 0.5,
        11.0,
        Color::rgb(71, 85, 105),
        TextAlign::Left,
    );
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

fn append_scene_text(
    out: &mut String,
    text: &str,
    x: f64,
    y: f64,
    font_size: f64,
    color: Color,
    align: TextAlign,
) {
    let width = text.chars().count() as f64 * font_size * 0.52;
    let x = match align {
        TextAlign::Left => x,
        TextAlign::Center => x - width * 0.5,
        TextAlign::Right => x - width,
    };
    out.push_str("BT\n/F1 ");
    out.push_str(&format_number(font_size));
    out.push_str(" Tf\n");
    append_fill(out, color);
    out.push_str(&format!(
        "1 0 0 -1 {} {} Tm\n{} Tj\nET\n",
        format_number(x),
        format_number(y + font_size * 0.8),
        pdf_literal(text)
    ));
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
