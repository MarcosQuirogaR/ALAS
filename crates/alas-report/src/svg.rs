// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! SVG vector graphics renderer for [`Scene`].
//!
//! Generates standalone, valid SVG documents from backend-neutral scenes
//! without requiring any external drawing or graphical library dependencies.

use std::fmt::{self, Write};
use std::sync::OnceLock;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

use crate::scene::{
    text_line_center_offsets, visual_title, wrap_text_to_width, Color, Fill, Point2D, Scene,
    SceneElement, Stroke, TextAlign, TextBaseline, CSS_PIXELS_PER_POINT, TEXT_LINE_HEIGHT_EM,
};

/// Explicit family stack shared by SVG exports and the GUI rasterizer.
///
/// ALAS bundles the first two faces for in-app rendering. A generic final
/// fallback keeps exported SVG text readable when it is opened elsewhere.
const SVG_FONT_FAMILY: &str = "'Noto Sans', 'Noto Sans Math', sans-serif";

/// Source identifier of the bundled NASA Blue Marble texture.
const BLUE_MARBLE_SOURCE: &str = "embedded://nasa-blue-marble";

/// Render a complete [`Scene`] to a standalone XML SVG string.
pub fn render_svg(scene: &Scene) -> String {
    let mut out = String::with_capacity(4096);
    // Formatting into a `String` cannot fail: its `fmt::Write` impl is
    // infallible and every `Display` used here is a number or a `&str`.
    let _ = write_svg(&mut out, scene);
    out
}

fn write_svg(out: &mut String, scene: &Scene) -> fmt::Result {
    writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w:.1} {h:.1}" width="{w:.1}" height="{h:.1}">"#,
        w = scene.width,
        h = scene.height
    )?;

    // Background rect if specified and this layer should paint it (a scene
    // composited over pre-drawn raster content keeps `background` set for
    // `visual_title`'s contrast decision while suppressing the opaque rect
    // that would otherwise hide that content).
    if let (true, Some(bg)) = (scene.paint_background, scene.background) {
        writeln!(
            out,
            r#"  <rect width="{w:.1}" height="{h:.1}" fill="{fill}" fill-opacity="{alpha:.3}"/>"#,
            w = scene.width,
            h = scene.height,
            fill = bg.to_hex_rgb(),
            alpha = bg.alpha_f64()
        )?;
    }

    if let Some(title) = &scene.title {
        out.push_str("  <title>");
        push_escaped_xml(out, title);
        out.push_str("</title>\n");
    }

    if let Some(title) = visual_title(scene) {
        write_element(out, &title)?;
        out.push('\n');
    }

    for elem in &scene.elements {
        write_element(out, elem)?;
        out.push('\n');
    }

    out.push_str("</svg>\n");
    Ok(())
}

fn write_element(out: &mut String, elem: &SceneElement) -> fmt::Result {
    match elem {
        SceneElement::Line { p1, p2, stroke } => {
            write!(
                out,
                r#"  <line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" "#,
                p1[0], p1[1], p2[0], p2[1],
            )?;
            write_stroke(out, stroke)?;
            out.push_str("/>");
        }
        SceneElement::Polyline { points, stroke } => {
            out.push_str(r#"  <polyline points=""#);
            write_points(out, points)?;
            out.push_str(r#"" fill="none" "#);
            write_stroke(out, stroke)?;
            out.push_str("/>");
        }
        SceneElement::Polygon {
            points,
            fill,
            stroke,
        } => {
            out.push_str(r#"  <polygon points=""#);
            write_points(out, points)?;
            out.push_str("\" ");
            write_paint(out, fill.as_ref(), stroke.as_ref())?;
            out.push_str("/>");
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
            write!(
                out,
                r#"  <rect x="{x:.2}" y="{y:.2}" width="{width:.2}" height="{height:.2}" rx="{rx:.2}" "#
            )?;
            write_paint(out, fill.as_ref(), stroke.as_ref())?;
            out.push_str("/>");
        }
        SceneElement::Circle {
            center,
            radius,
            fill,
            stroke,
        } => {
            write!(
                out,
                r#"  <circle cx="{:.2}" cy="{:.2}" r="{radius:.2}" "#,
                center[0], center[1]
            )?;
            write_paint(out, fill.as_ref(), stroke.as_ref())?;
            out.push_str("/>");
        }
        SceneElement::Image {
            source,
            x,
            y,
            width,
            height,
            source_rect,
        } => {
            write_image(out, source, [*x, *y, *width, *height], *source_rect)?;
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
            let close = match clip {
                Some([x, y, width, height]) => {
                    let id = format!("globe-clip-{x:.0}-{y:.0}-{width:.0}-{height:.0}");
                    write!(
                        out,
                        r##"  <clipPath id="{id}"><rect x="{x:.2}" y="{y:.2}" width="{width:.2}" height="{height:.2}"/></clipPath>  <g clip-path="url(#{id})">"##
                    )?;
                    "</g>"
                }
                None => {
                    out.push_str("  ");
                    ""
                }
            };
            write!(
                out,
                r##"<circle cx="{:.2}" cy="{:.2}" r="{radius:.2}" fill="#08213d"/>{close}"##,
                center[0], center[1]
            )?;
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
            write_text_open(out, *font_size, *color, anchor, *bold)?;
            if angle_deg.abs() > 1e-3 {
                write!(
                    out,
                    r#" transform="rotate({angle_deg:.1} {:.2} {:.2})""#,
                    pos[0], pos[1]
                )?;
            }
            out.push('>');
            let lines = text.split('\n').collect::<Vec<_>>();
            write_tspans(out, &lines, *pos, *font_size, *baseline)?;
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
            let wrapped = wrap_text_to_width(text, *font_size, *width);
            let lines = wrapped.lines().collect::<Vec<_>>();
            write_text_open(out, *font_size, *color, "start", *bold)?;
            out.push('>');
            write_tspans(out, &lines, *pos, *font_size, TextBaseline::Top)?;
        }
    }
    Ok(())
}

/// Opening `<text` tag up to (not including) its closing `>`, so a caller can
/// append a transform.
fn write_text_open(
    out: &mut String,
    font_size: f64,
    color: Color,
    anchor: &str,
    bold: bool,
) -> fmt::Result {
    write!(
        out,
        r#"  <text font-family="{SVG_FONT_FAMILY}" font-size="{font_size:.1}pt" fill="{}" fill-opacity="{:.3}" text-anchor="{anchor}" dominant-baseline="central""#,
        color.to_hex_rgb(),
        color.alpha_f64(),
    )?;
    if bold {
        out.push_str(r#" font-weight="bold""#);
    }
    Ok(())
}

/// One `<tspan>` per line, vertically placed by the shared line model, then
/// the closing `</text>`.
fn write_tspans(
    out: &mut String,
    lines: &[&str],
    pos: Point2D,
    font_size: f64,
    baseline: TextBaseline,
) -> fmt::Result {
    let line_height = font_size * CSS_PIXELS_PER_POINT * TEXT_LINE_HEIGHT_EM;
    let centers = text_line_center_offsets(lines.len(), line_height, baseline);
    for (line, center) in lines.iter().zip(centers) {
        write!(
            out,
            r#"<tspan x="{:.2}" y="{:.2}">"#,
            pos[0],
            pos[1] + center
        )?;
        push_escaped_xml(out, line);
        out.push_str("</tspan>");
    }
    out.push_str("</text>");
    Ok(())
}

fn write_points(out: &mut String, points: &[Point2D]) -> fmt::Result {
    for (index, p) in points.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        write!(out, "{:.2},{:.2}", p[0], p[1])?;
    }
    Ok(())
}

fn write_stroke(out: &mut String, stroke: &Stroke) -> fmt::Result {
    write!(
        out,
        r#"stroke="{}" stroke-width="{:.2}" stroke-opacity="{:.3}""#,
        stroke.color.to_hex_rgb(),
        stroke.width,
        stroke.color.alpha_f64()
    )?;
    if let Some(dash) = &stroke.dash_array {
        out.push_str(r#" stroke-dasharray=""#);
        for (index, value) in dash.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            write!(out, "{value:.1}")?;
        }
        out.push('"');
    }
    Ok(())
}

/// `fill` then `stroke` attributes, separated by one space, each `none` when
/// absent.
fn write_paint(out: &mut String, fill: Option<&Fill>, stroke: Option<&Stroke>) -> fmt::Result {
    match fill {
        Some(f) => write!(
            out,
            r#"fill="{}" fill-opacity="{:.3}""#,
            f.color.to_hex_rgb(),
            f.color.alpha_f64()
        )?,
        None => out.push_str(r#"fill="none""#),
    }
    out.push(' ');
    match stroke {
        Some(s) => write_stroke(out, s),
        None => {
            out.push_str(r#"stroke="none""#);
            Ok(())
        }
    }
}

fn write_image(
    out: &mut String,
    source: &str,
    [x, y, width, height]: [f64; 4],
    source_rect: Option<[f64; 4]>,
) -> fmt::Result {
    if let Some([left, top, crop_width, crop_height]) = source_rect {
        let left = left.clamp(0.0, 1.0);
        let top = top.clamp(0.0, 1.0);
        let crop_width = crop_width.clamp(0.0, 1.0 - left);
        let crop_height = crop_height.clamp(0.0, 1.0 - top);
        if crop_width <= f64::EPSILON || crop_height <= f64::EPSILON {
            return Ok(());
        }
        write!(
            out,
            r#"  <svg x="{x:.2}" y="{y:.2}" width="{width:.2}" height="{height:.2}" viewBox="{left:.8} {top:.8} {crop_width:.8} {crop_height:.8}" preserveAspectRatio="none" overflow="hidden"><image href=""#
        )?;
        push_image_href(out, source);
        out.push_str(r#"" x="0" y="0" width="1" height="1" preserveAspectRatio="none"/></svg>"#);
    } else {
        out.push_str(r#"  <image href=""#);
        push_image_href(out, source);
        write!(
            out,
            r#"" x="{x:.2}" y="{y:.2}" width="{width:.2}" height="{height:.2}" preserveAspectRatio="xMidYMid meet"/>"#
        )?;
    }
    Ok(())
}

fn push_image_href(out: &mut String, source: &str) {
    if source == BLUE_MARBLE_SOURCE {
        out.push_str(blue_marble_data_uri());
    } else {
        push_escaped_xml(out, source);
    }
}

/// The bundled texture as a PNG data URI, encoded once per process: the PNG
/// is about 7 MB, and a route figure references it from more than one
/// element.
fn blue_marble_data_uri() -> &'static str {
    static URI: OnceLock<String> = OnceLock::new();
    URI.get_or_init(|| {
        let bytes = include_bytes!("../../../assets/textures/earth_blue_marble.png");
        let mut uri = String::from("data:image/png;base64,");
        BASE64.encode_string(bytes, &mut uri);
        uri
    })
}

/// Append `s` with the five XML special characters replaced by entities, so
/// the result is valid both as element content and inside a double- or
/// single-quoted attribute.
///
/// C0 control characters other than tab, line feed and carriage return are
/// not allowed anywhere in an XML 1.0 document, not even as character
/// references, and one of them in solver text would make the whole figure
/// unparseable; they become U+FFFD.
fn push_escaped_xml(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if c.is_ascii_control() && c != '\u{7f}' => out.push('\u{fffd}'),
            other => out.push(other),
        }
    }
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
    fn xml_forbidden_control_characters_are_replaced_not_emitted() {
        let mut scene = Scene::new(200.0, 100.0, None);
        scene.title = Some("solver\u{1b}[0m log\u{0}".to_owned());
        let svg = render_svg(&scene);
        assert!(svg.contains("<title>solver\u{fffd}[0m log\u{fffd}</title>"));
        assert!(!svg.chars().any(|c| c.is_ascii_control() && c != '\n'));
    }

    #[test]
    fn the_bundled_texture_is_embedded_as_a_base64_png_data_uri() {
        use base64::Engine as _;
        let mut scene = Scene::new(200.0, 100.0, None);
        scene.add(SceneElement::Image {
            source: BLUE_MARBLE_SOURCE.to_owned(),
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
            source_rect: Some([0.25, 0.0, 0.5, 1.0]),
        });
        let svg = render_svg(&scene);
        let start = svg.find("data:image/png;base64,").unwrap() + 22;
        let end = start + svg[start..].find('"').unwrap();
        let png = BASE64.decode(&svg[start..end]).unwrap();
        assert_eq!(
            png,
            include_bytes!("../../../assets/textures/earth_blue_marble.png")
        );
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

    #[test]
    fn text_uses_the_bundled_engineering_font_stack() {
        let mut scene = Scene::new(200.0, 100.0, None);
        scene.add(SceneElement::Text {
            text: "\u{03B7}\u{209C}\u{2095} = \u{03B7}\u{209A} \u{22C5} \u{03B7}\u{2092}"
                .to_owned(),
            pos: [100.0, 50.0],
            font_size: 12.0,
            color: Color::rgb(0, 0, 0),
            align: TextAlign::Center,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });

        let svg = render_svg(&scene);

        assert!(svg.contains("font-family=\"'Noto Sans', 'Noto Sans Math', sans-serif\""));
        assert!(
            svg.contains("\u{03B7}\u{209C}\u{2095} = \u{03B7}\u{209A} \u{22C5} \u{03B7}\u{2092}")
        );
    }
}
