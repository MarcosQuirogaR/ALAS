// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! SVG vector graphics renderer for [`Scene`].
//!
//! Generates standalone, valid SVG documents from backend-neutral scenes
//! without requiring any external drawing or graphical library dependencies.

use crate::scene::{
    text_line_center_offsets, visual_title, wrap_text_to_width, Fill, Scene, SceneElement, Stroke,
    TextAlign, TextBaseline, CSS_PIXELS_PER_POINT, TEXT_LINE_HEIGHT_EM,
};

/// Render a complete [`Scene`] to a standalone XML SVG string.
pub fn render_svg(scene: &Scene) -> String {
    let mut out = String::with_capacity(4096);

    out.push_str(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w:.1} {h:.1}" width="{w:.1}" height="{h:.1}">"#,
        w = scene.width,
        h = scene.height
    ));
    out.push('\n');

    // Background rect if specified and this layer should paint it (a scene
    // composited over pre-drawn raster content keeps `background` set for
    // `visual_title`'s contrast decision while suppressing the opaque rect
    // that would otherwise hide that content).
    if let (true, Some(bg)) = (scene.paint_background, scene.background) {
        out.push_str(&format!(
            r#"  <rect width="{w:.1}" height="{h:.1}" fill="{fill}" fill-opacity="{alpha:.3}"/>"#,
            w = scene.width,
            h = scene.height,
            fill = bg.to_hex_rgb(),
            alpha = bg.alpha_f64()
        ));
        out.push('\n');
    }

    if let Some(title) = &scene.title {
        out.push_str(&format!("  <title>{}</title>\n", escape_xml(title)));
    }

    if let Some(title) = visual_title(scene) {
        render_element(&mut out, &title);
        out.push('\n');
    }

    for elem in &scene.elements {
        render_element(&mut out, elem);
        out.push('\n');
    }

    out.push_str("</svg>\n");
    out
}

fn render_element(out: &mut String, elem: &SceneElement) {
    match elem {
        SceneElement::Line { p1, p2, stroke } => {
            out.push_str(&format!(
                r#"  <line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" {}/>"#,
                p1[0],
                p1[1],
                p2[0],
                p2[1],
                format_stroke(stroke)
            ));
        }
        SceneElement::Polyline { points, stroke } => {
            let pts_str = points
                .iter()
                .map(|p| format!("{:.2},{:.2}", p[0], p[1]))
                .collect::<Vec<_>>()
                .join(" ");
            out.push_str(&format!(
                r#"  <polyline points="{}" fill="none" {}/>"#,
                pts_str,
                format_stroke(stroke)
            ));
        }
        SceneElement::Polygon {
            points,
            fill,
            stroke,
        } => {
            let pts_str = points
                .iter()
                .map(|p| format!("{:.2},{:.2}", p[0], p[1]))
                .collect::<Vec<_>>()
                .join(" ");
            out.push_str(&format!(
                r#"  <polygon points="{}" {} {}/>"#,
                pts_str,
                format_fill(fill),
                format_opt_stroke(stroke)
            ));
        }
        SceneElement::Rect {
            x,
            y,
            width,
            height,
            rx,
            fill,
            stroke,
        } => {
            out.push_str(&format!(
                r#"  <rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" rx="{:.2}" {} {}/>"#,
                x,
                y,
                width,
                height,
                rx,
                format_fill(fill),
                format_opt_stroke(stroke)
            ));
        }
        SceneElement::Circle {
            center,
            radius,
            fill,
            stroke,
        } => {
            out.push_str(&format!(
                r#"  <circle cx="{:.2}" cy="{:.2}" r="{:.2}" {} {}/>"#,
                center[0],
                center[1],
                radius,
                format_fill(fill),
                format_opt_stroke(stroke)
            ));
        }
        SceneElement::Image {
            source,
            x,
            y,
            width,
            height,
            source_rect,
        } => {
            render_image(out, source, *x, *y, *width, *height, *source_rect);
        }
        SceneElement::SphericalImage {
            center,
            radius,
            clip,
            ..
        } => {
            // The silhouette carries the same viewport bound as the texture
            // layer, so a zoomed globe does not paint over the surrounding
            // title and colorbar in a headless SVG either.
            let (open, close) = match clip {
                Some([x, y, width, height]) => {
                    let id = format!("globe-clip-{x:.0}-{y:.0}-{width:.0}-{height:.0}");
                    out.push_str(&format!(
                        r##"  <clipPath id="{id}"><rect x="{x:.2}" y="{y:.2}" width="{width:.2}" height="{height:.2}"/></clipPath>"##
                    ));
                    (format!(r##"<g clip-path="url(#{id})">"##), "</g>")
                }
                None => (String::new(), ""),
            };
            out.push_str(&format!(
                r##"  {open}<circle cx="{:.2}" cy="{:.2}" r="{:.2}" fill="#08213d"/>{close}"##,
                center[0], center[1], radius
            ));
        }
        SceneElement::Text {
            text,
            pos,
            font_size,
            color,
            align,
            baseline,
            angle_deg,
            bold,
        } => {
            let anchor = match align {
                TextAlign::Left => "start",
                TextAlign::Center => "middle",
                TextAlign::Right => "end",
            };
            let weight = if *bold { r#" font-weight="bold""# } else { "" };
            let rot = if angle_deg.abs() > 1e-3 {
                format!(
                    r#" transform="rotate({:.1} {:.2} {:.2})""#,
                    angle_deg, pos[0], pos[1]
                )
            } else {
                String::new()
            };

            let lines = text.split('\n').collect::<Vec<_>>();
            let line_height = font_size * CSS_PIXELS_PER_POINT * TEXT_LINE_HEIGHT_EM;
            let centers = text_line_center_offsets(lines.len(), line_height, *baseline);
            out.push_str(&format!(
                r#"  <text font-family="sans-serif" font-size="{:.1}pt" fill="{}" fill-opacity="{:.3}" text-anchor="{}" dominant-baseline="central"{}{}>"#,
                font_size,
                color.to_hex_rgb(),
                color.alpha_f64(),
                anchor,
                weight,
                rot,
            ));
            for (line, center) in lines.iter().zip(centers) {
                out.push_str(&format!(
                    r#"<tspan x="{:.2}" y="{:.2}">{}</tspan>"#,
                    pos[0],
                    pos[1] + center,
                    escape_xml(line),
                ));
            }
            out.push_str("</text>");
        }
        SceneElement::TextBlock {
            text,
            pos,
            width,
            font_size,
            color,
            bold,
        } => {
            // SVG has no metric-aware wrapping the viewer applies itself, so
            // reflow with the conservative budget and emit one row per line.
            let weight = if *bold { r#" font-weight="bold""# } else { "" };
            let wrapped = wrap_text_to_width(text, *font_size, *width);
            let lines = wrapped.lines().collect::<Vec<_>>();
            let line_height = font_size * CSS_PIXELS_PER_POINT * TEXT_LINE_HEIGHT_EM;
            let centers = text_line_center_offsets(lines.len(), line_height, TextBaseline::Top);
            out.push_str(&format!(
                r#"  <text font-family="sans-serif" font-size="{:.1}pt" fill="{}" fill-opacity="{:.3}" text-anchor="start" dominant-baseline="central"{}>"#,
                font_size,
                color.to_hex_rgb(),
                color.alpha_f64(),
                weight,
            ));
            for (line, center) in lines.iter().zip(centers) {
                out.push_str(&format!(
                    r#"<tspan x="{:.2}" y="{:.2}">{}</tspan>"#,
                    pos[0],
                    pos[1] + center,
                    escape_xml(line),
                ));
            }
            out.push_str("</text>");
        }
    }
}

fn format_stroke(stroke: &Stroke) -> String {
    let mut s = format!(
        r#"stroke="{}" stroke-width="{:.2}" stroke-opacity="{:.3}""#,
        stroke.color.to_hex_rgb(),
        stroke.width,
        stroke.color.alpha_f64()
    );
    if let Some(dash) = &stroke.dash_array {
        let d_str = dash
            .iter()
            .map(|v| format!("{v:.1}"))
            .collect::<Vec<_>>()
            .join(",");
        s.push_str(&format!(r#" stroke-dasharray="{d_str}""#));
    }
    s
}

fn render_image(
    out: &mut String,
    source: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    source_rect: Option<[f64; 4]>,
) {
    let href = image_href(source);
    if let Some([left, top, crop_width, crop_height]) = source_rect {
        let left = left.clamp(0.0, 1.0);
        let top = top.clamp(0.0, 1.0);
        let crop_width = crop_width.clamp(0.0, 1.0 - left);
        let crop_height = crop_height.clamp(0.0, 1.0 - top);
        if crop_width <= f64::EPSILON || crop_height <= f64::EPSILON {
            return;
        }
        out.push_str(&format!(
            r#"  <svg x="{x:.2}" y="{y:.2}" width="{width:.2}" height="{height:.2}" viewBox="{left:.8} {top:.8} {crop_width:.8} {crop_height:.8}" preserveAspectRatio="none" overflow="hidden"><image href="{href}" x="0" y="0" width="1" height="1" preserveAspectRatio="none"/></svg>"#
        ));
    } else {
        out.push_str(&format!(
            r#"  <image href="{href}" x="{x:.2}" y="{y:.2}" width="{width:.2}" height="{height:.2}" preserveAspectRatio="xMidYMid meet"/>"#
        ));
    }
}

fn image_href(source: &str) -> String {
    if source == "embedded://nasa-blue-marble" {
        let bytes = include_bytes!("../../../assets/textures/earth_blue_marble.png");
        return format!("data:image/png;base64,{}", encode_base64(bytes));
    }
    escape_xml(source)
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        output.push(TABLE[(a >> 2) as usize] as char);
        output.push(TABLE[(((a & 0b0000_0011) << 4) | (b >> 4)) as usize] as char);
        output.push(if chunk.len() >= 2 {
            TABLE[(((b & 0b0000_1111) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() == 3 {
            TABLE[(c & 0b0011_1111) as usize] as char
        } else {
            '='
        });
    }
    output
}

fn format_opt_stroke(stroke: &Option<Stroke>) -> String {
    match stroke {
        Some(s) => format_stroke(s),
        None => r#"stroke="none""#.to_owned(),
    }
}

fn format_fill(fill: &Option<Fill>) -> String {
    match fill {
        Some(f) => format!(
            r#"fill="{}" fill-opacity="{:.3}""#,
            f.color.to_hex_rgb(),
            f.color.alpha_f64()
        ),
        None => r#"fill="none""#.to_owned(),
    }
}

fn escape_xml(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Color, TextAlign, TextBaseline};

    #[test]
    fn empty_scene_renders_valid_svg_wrapper() {
        let scene = Scene::new(400.0, 300.0, Some(Color::rgb(255, 255, 255)));
        let svg = render_svg(&scene);
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>\n"));
        assert!(svg.contains("viewBox=\"0 0 400.0 300.0\""));
        assert!(svg.contains("fill=\"#ffffff\""));
    }

    #[test]
    fn hidden_background_paint_keeps_the_automatic_title_theme_aware() {
        // Regression for a bug where the GUI's static raster preview
        // (alas-viz::raster::render_scene_rgba_scaled) cleared
        // `scene.background` to `None` to keep the vector layer transparent
        // over pre-painted texture content. `visual_title` reads that same
        // field for its contrast decision, so every automatic figure title
        // silently fell back to its light-background (near-black) color on
        // Grey and Dark, regardless of the actual theme -- while explicitly
        // colored panel headings, driven by the palette directly, stayed
        // correct. `hide_background_paint` must suppress the rect without
        // blinding that decision.
        let dark_bg = Color::from_hex("#1e1e1e");
        let mut scene = Scene::new(200.0, 100.0, Some(dark_bg));
        scene.title = Some("Dark theme title".to_owned());
        scene.hide_background_paint();

        let svg = render_svg(&scene);

        assert!(
            !svg.contains("<rect"),
            "a hidden background must not paint an opaque rect: {svg}"
        );
        let title_line = svg
            .lines()
            .find(|line| line.contains("tspan") && line.contains("Dark theme title"))
            .expect("automatic title text element");
        assert!(
            title_line.contains("fill=\"#ffffff\""),
            "the automatic title must still contrast against the true (unpainted) \
             background instead of falling back to its light-theme color: {title_line}"
        );
    }

    #[test]
    fn scene_title_is_visible_as_well_as_document_metadata() {
        let mut scene = Scene::new(200.0, 100.0, None);
        scene.title = Some("Visible figure title".to_owned());

        let svg = render_svg(&scene);

        assert!(svg.contains("<title>Visible figure title</title>"));
        assert!(svg.contains(">Visible figure title</tspan>"));
    }

    #[test]
    fn image_elements_render_external_sources_with_xml_escaping() {
        let mut scene = Scene::new(200.0, 100.0, None);
        scene.add(SceneElement::Image {
            source: "C:/renders/a & b.png".to_owned(),
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
            source_rect: None,
        });
        let svg = render_svg(&scene);
        assert!(svg.contains("<image href=\"C:/renders/a &amp; b.png\""));
        assert!(svg.contains("preserveAspectRatio=\"xMidYMid meet\""));
    }

    #[test]
    fn scene_titles_are_preserved_as_accessible_svg_metadata() {
        let mut scene = Scene::new(200.0, 100.0, None);
        scene.title = Some("A & <chart>".to_owned());
        let svg = render_svg(&scene);
        assert!(svg.contains("<title>A &amp; &lt;chart&gt;</title>"));
    }

    #[test]
    fn multiline_text_uses_point_units_and_separate_line_boxes() {
        let mut scene = Scene::new(200.0, 100.0, None);
        scene.add(SceneElement::Text {
            text: "Required\nAvailable".to_owned(),
            pos: [100.0, 50.0],
            font_size: 12.0,
            color: Color::rgb(0, 0, 0),
            align: TextAlign::Center,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });

        let svg = render_svg(&scene);
        assert!(svg.contains("font-size=\"12.0pt\""));
        assert_eq!(svg.matches("<tspan").count(), 2);
        assert!(svg.contains("x=\"100.00\" y=\"40.40\">Required</tspan>"));
        assert!(svg.contains("x=\"100.00\" y=\"59.60\">Available</tspan>"));
    }
}
