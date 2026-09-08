// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Headless SVG writing for report scenes.
//!
//! The report crate owns scene construction and its canonical renderer. This
//! small dependency-free bridge exists because the headless application reaches
//! report scenes through the GUI crate's public scene factory, while the
//! pipeline crate must not depend back on `alas-report`.

use serde::Serialize;
use serde_json::Value;

/// Render a serialized backend-neutral scene as a standalone SVG document.
///
/// The scene types use externally tagged serde enums, so this renderer maps
/// the same primitives as `alas-report::svg` without taking a dependency edge
/// that would create a cycle through the report crate's document exports.
pub fn render_scene_svg(scene: &impl Serialize) -> Result<String, String> {
    let value =
        serde_json::to_value(scene).map_err(|e| format!("failed to serialize scene: {e}"))?;
    let width = field_number(&value, "width")?;
    let height = field_number(&value, "height")?;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width:.1} {height:.1}\" width=\"{width:.1}\" height=\"{height:.1}\">\n"
    );

    if let Some(color) = value.get("background").and_then(Value::as_object) {
        svg.push_str(&format!(
            "  <rect width=\"{width:.1}\" height=\"{height:.1}\" fill=\"{}\" fill-opacity=\"{:.3}\"/>\n",
            color_hex(color)?,
            color_alpha(color)?,
        ));
    }

    let elements = value
        .get("elements")
        .and_then(Value::as_array)
        .ok_or_else(|| "scene elements are not an array".to_owned())?;
    for element in elements {
        render_element(&mut svg, element)?;
        svg.push('\n');
    }
    svg.push_str("</svg>\n");
    Ok(svg)
}

fn render_element(svg: &mut String, element: &Value) -> Result<(), String> {
    let object = element
        .as_object()
        .ok_or_else(|| "scene element is not an object".to_owned())?;
    let (kind, data) = object
        .iter()
        .next()
        .ok_or_else(|| "scene element is empty".to_owned())?;
    match kind.as_str() {
        "Line" => svg.push_str(&format!(
            "  <line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" {} />",
            point_number(data, "p1", 0)?,
            point_number(data, "p1", 1)?,
            point_number(data, "p2", 0)?,
            point_number(data, "p2", 1)?,
            stroke(data.get("stroke"))?,
        )),
        "Polyline" | "Polygon" => render_path(svg, kind, data)?,
        "Rect" => svg.push_str(&format!(
            "  <rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"{:.2}\" {} {} />",
            field_number(data, "x")?,
            field_number(data, "y")?,
            field_number(data, "width")?,
            field_number(data, "height")?,
            field_number(data, "rx")?,
            fill(data.get("fill"))?,
            optional_stroke(data.get("stroke"))?,
        )),
        "Circle" => svg.push_str(&format!(
            "  <circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" {} {} />",
            point_number(data, "center", 0)?,
            point_number(data, "center", 1)?,
            field_number(data, "radius")?,
            fill(data.get("fill"))?,
            optional_stroke(data.get("stroke"))?,
        )),
        "Text" => render_text(svg, data)?,
        "Image" => render_image(svg, data)?,
        _ => return Err(format!("unsupported scene element: {kind}")),
    }
    Ok(())
}

fn render_path(svg: &mut String, kind: &str, data: &Value) -> Result<(), String> {
    let points = data
        .get("points")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{kind} points are not an array"))?;
    let points = points
        .iter()
        .map(|point| {
            let pair = point
                .as_array()
                .ok_or_else(|| format!("{kind} point is not an array"))?;
            Ok(format!(
                "{:.2},{:.2}",
                number(pair.first(), "point x")?,
                number(pair.get(1), "point y")?
            ))
        })
        .collect::<Result<Vec<_>, String>>()?
        .join(" ");
    if kind == "Polyline" {
        svg.push_str(&format!(
            "  <polyline points=\"{points}\" fill=\"none\" {} />",
            stroke(data.get("stroke"))?
        ));
    } else {
        svg.push_str(&format!(
            "  <polygon points=\"{points}\" {} {} />",
            fill(data.get("fill"))?,
            optional_stroke(data.get("stroke"))?,
        ));
    }
    Ok(())
}

fn render_text(svg: &mut String, data: &Value) -> Result<(), String> {
    let align = match data.get("align").and_then(Value::as_str) {
        Some("Left") => "start",
        Some("Center") => "middle",
        Some("Right") => "end",
        _ => return Err("text alignment is invalid".to_owned()),
    };
    let baseline = match data.get("baseline").and_then(Value::as_str) {
        Some("Top") => "hanging",
        Some("Middle") => "central",
        Some("Bottom") => "alphabetic",
        _ => return Err("text baseline is invalid".to_owned()),
    };
    let text = data
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| "text content is missing".to_owned())?;
    let color = data
        .get("color")
        .and_then(Value::as_object)
        .ok_or_else(|| "text color is missing".to_owned())?;
    let bold = data.get("bold").and_then(Value::as_bool).unwrap_or(false);
    let weight = if bold { " font-weight=\"bold\"" } else { "" };
    let angle = field_number(data, "angle_deg")?;
    let rotation = if angle.abs() > 1e-3 {
        format!(
            " transform=\"rotate({angle:.1} {:.2} {:.2})\"",
            point_number(data, "pos", 0)?,
            point_number(data, "pos", 1)?,
        )
    } else {
        String::new()
    };
    svg.push_str(&format!(
        "  <text x=\"{:.2}\" y=\"{:.2}\" font-family=\"sans-serif\" font-size=\"{:.1}\" fill=\"{}\" fill-opacity=\"{:.3}\" text-anchor=\"{align}\" dominant-baseline=\"{baseline}\"{weight}{rotation}>{}</text>",
        point_number(data, "pos", 0)?,
        point_number(data, "pos", 1)?,
        field_number(data, "font_size")?,
        color_hex(color)?,
        color_alpha(color)?,
        escape_xml(text),
    ));
    Ok(())
}

fn render_image(svg: &mut String, data: &Value) -> Result<(), String> {
    let source = data
        .get("source")
        .and_then(Value::as_str)
        .ok_or_else(|| "image source is missing".to_owned())?;
    let x = field_number(data, "x")?;
    let y = field_number(data, "y")?;
    let width = field_number(data, "width")?;
    let height = field_number(data, "height")?;
    let href = image_href(source);

    let source_rect = match data.get("source_rect") {
        None | Some(Value::Null) => None,
        Some(value) => {
            let corners = value
                .as_array()
                .ok_or_else(|| "image source_rect is not an array".to_owned())?;
            Some([
                number(corners.first(), "source_rect[0]")?,
                number(corners.get(1), "source_rect[1]")?,
                number(corners.get(2), "source_rect[2]")?,
                number(corners.get(3), "source_rect[3]")?,
            ])
        }
    };

    if let Some([left, top, crop_width, crop_height]) = source_rect {
        let left = left.clamp(0.0, 1.0);
        let top = top.clamp(0.0, 1.0);
        let crop_width = crop_width.clamp(0.0, 1.0 - left);
        let crop_height = crop_height.clamp(0.0, 1.0 - top);
        if crop_width <= f64::EPSILON || crop_height <= f64::EPSILON {
            return Ok(());
        }
        svg.push_str(&format!(
            r#"  <svg x="{x:.2}" y="{y:.2}" width="{width:.2}" height="{height:.2}" viewBox="{left:.8} {top:.8} {crop_width:.8} {crop_height:.8}" preserveAspectRatio="none" overflow="hidden"><image href="{href}" x="0" y="0" width="1" height="1" preserveAspectRatio="none"/></svg>"#
        ));
    } else {
        svg.push_str(&format!(
            r#"  <image href="{href}" x="{x:.2}" y="{y:.2}" width="{width:.2}" height="{height:.2}" preserveAspectRatio="xMidYMid meet"/>"#
        ));
    }
    Ok(())
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

fn number(value: Option<&Value>, name: &str) -> Result<f64, String> {
    let value = value
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("{name} is not a number"))?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("{name} is not finite"))
    }
}

fn field_number(value: &Value, name: &str) -> Result<f64, String> {
    number(value.get(name), name)
}

fn point_number(value: &Value, name: &str, index: usize) -> Result<f64, String> {
    let point = value
        .get(name)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{name} is not a point"))?;
    number(point.get(index), &format!("{name}[{index}]"))
}

fn color_hex(color: &serde_json::Map<String, Value>) -> Result<String, String> {
    let r = color_channel(color, "r")?;
    let g = color_channel(color, "g")?;
    let b = color_channel(color, "b")?;
    Ok(format!("#{r:02x}{g:02x}{b:02x}"))
}

fn color_channel(color: &serde_json::Map<String, Value>, name: &str) -> Result<u8, String> {
    let value = color
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("color channel {name} is invalid"))?;
    u8::try_from(value).map_err(|_| format!("color channel {name} is out of range"))
}

fn color_alpha(color: &serde_json::Map<String, Value>) -> Result<f64, String> {
    Ok(f64::from(color_channel(color, "a")?) / 255.0)
}

fn stroke(value: Option<&Value>) -> Result<String, String> {
    let stroke = value
        .and_then(Value::as_object)
        .ok_or_else(|| "stroke is missing".to_owned())?;
    let color = stroke
        .get("color")
        .and_then(Value::as_object)
        .ok_or_else(|| "stroke color is missing".to_owned())?;
    let mut result = format!(
        "stroke=\"{}\" stroke-width=\"{:.2}\" stroke-opacity=\"{:.3}\"",
        color_hex(color)?,
        number(stroke.get("width"), "stroke width")?,
        color_alpha(color)?,
    );
    if let Some(dash) = stroke.get("dash_array").and_then(Value::as_array) {
        let values = dash
            .iter()
            .map(|value| Ok(format!("{:.1}", number(Some(value), "dash")?)))
            .collect::<Result<Vec<_>, String>>()?
            .join(",");
        result.push_str(&format!(" stroke-dasharray=\"{values}\""));
    }
    Ok(result)
}

fn optional_stroke(value: Option<&Value>) -> Result<String, String> {
    match value.and_then(Value::as_object) {
        Some(_) => stroke(value),
        None => Ok("stroke=\"none\"".to_owned()),
    }
}

fn fill(value: Option<&Value>) -> Result<String, String> {
    match value.and_then(Value::as_object) {
        Some(fill) => {
            let color = fill
                .get("color")
                .and_then(Value::as_object)
                .ok_or_else(|| "fill color is missing".to_owned())?;
            Ok(format!(
                "fill=\"{}\" fill-opacity=\"{:.3}\"",
                color_hex(color)?,
                color_alpha(color)?,
            ))
        }
        None => Ok("fill=\"none\"".to_owned()),
    }
}

fn escape_xml(text: &str) -> String {
    text.chars()
        .map(|ch| match ch {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            '\'' => "&apos;".to_owned(),
            other => other.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::render_scene_svg;
    use serde_json::json;

    #[test]
    fn a_serialized_scene_renders_all_basic_svg_primitives() {
        let scene = json!({
            "width": 100.0,
            "height": 80.0,
            "background": {"r": 255, "g": 255, "b": 255, "a": 255},
            "elements": [
                {"Line": {"p1": [1.0, 2.0], "p2": [3.0, 4.0], "stroke": {
                    "color": {"r": 0, "g": 0, "b": 0, "a": 255}, "width": 1.0, "dash_array": null
                } }},
                {"Polyline": {"points": [[1.0, 2.0], [3.0, 4.0]], "stroke": {
                    "color": {"r": 31, "g": 119, "b": 180, "a": 255}, "width": 2.0, "dash_array": null
                } }},
                {"Polygon": {"points": [[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]], "fill": {
                    "color": {"r": 255, "g": 127, "b": 14, "a": 128}
                }, "stroke": null }},
                {"Rect": {"x": 1.0, "y": 2.0, "width": 3.0, "height": 4.0, "rx": 0.0,
                    "fill": null, "stroke": null }},
                {"Circle": {"center": [5.0, 6.0], "radius": 2.0, "fill": null, "stroke": null }},
                {"Text": {"text": "A < B", "pos": [5.0, 6.0], "font_size": 10.0,
                    "color": {"r": 0, "g": 0, "b": 0, "a": 255}, "align": "Center",
                    "baseline": "Middle", "angle_deg": 0.0, "bold": false }}
            ]
        });

        let rendered = render_scene_svg(&scene);
        assert!(rendered.is_ok());
        let svg = rendered.unwrap_or_default();
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("<line"));
        assert!(svg.contains("<polyline"));
        assert!(svg.contains("<polygon"));
        assert!(svg.contains("<rect"));
        assert!(svg.contains("<circle"));
        assert!(svg.contains("A &lt; B"));
        assert!(svg.ends_with("</svg>\n"));
    }

    #[test]
    fn image_elements_render_as_svg_image_references() {
        let scene = json!({
            "width": 100.0,
            "height": 80.0,
            "background": null,
            "elements": [
                {"Image": {"source": "C:/renders/a & b.png", "x": 0.0, "y": 0.0,
                    "width": 100.0, "height": 80.0, "source_rect": null}}
            ]
        });

        let svg = render_scene_svg(&scene).unwrap_or_default();
        assert!(svg.contains("<image href=\"C:/renders/a &amp; b.png\""));
    }

    #[test]
    fn a_cropped_image_source_rect_becomes_a_nested_viewbox() {
        let scene = json!({
            "width": 100.0,
            "height": 80.0,
            "background": null,
            "elements": [
                {"Image": {"source": "texture.png", "x": 0.0, "y": 0.0,
                    "width": 100.0, "height": 80.0,
                    "source_rect": [0.25, 0.0, 0.5, 1.0]}}
            ]
        });

        let svg = render_scene_svg(&scene).unwrap_or_default();
        assert!(svg.contains("viewBox=\"0.25000000 0.00000000 0.50000000 1.00000000\""));
    }
}
