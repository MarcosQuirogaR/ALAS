// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one placeholder every figure family uses when it cannot draw its
//! result: a single colored status title and a wrapped diagnostic paragraph
//! that stays inside the canvas in every theme and on every backend.
//!
//! Ported from `alas/reporting/visualization.py`, `figure_status_message`
//! (L650-692). The body is a [`SceneElement::TextBlock`], so the interactive
//! view wraps it with real glyph metrics while SVG and PDF exports wrap it
//! with the conservative character budget; the retained failure text is
//! never truncated, only reflowed.

use crate::scene::{
    text_block_height, Color, Scene, SceneElement, TextAlign, TextBaseline, CSS_PIXELS_PER_POINT,
    TEXT_LINE_HEIGHT_EM,
};
use crate::theme::{get_palette, Palette};

/// Default canvas width of a status figure in scene pixels.
pub const STATUS_WIDTH: f64 = 760.0;
/// Lateral margin on both sides of the title and body.
pub const STATUS_MARGIN: f64 = 32.0;
/// Body font size in points.
pub const STATUS_BODY_FONT_SIZE: f64 = 12.0;
/// Title color for a failed or missing analysis (upstream `#c0392b`).
pub const FAILURE_COLOR: &str = "#c0392b";
/// Title color for an informational success note (upstream `#27ae60`).
pub const OK_COLOR: &str = "#27ae60";

const TITLE_FONT_SIZE: f64 = 16.0;
const TITLE_TOP: f64 = 34.0;
const MESSAGE_TOP: f64 = 82.0;
const MIN_HEIGHT: f64 = 240.0;
// `TextBaseline::Top` anchors each line by its line-box center, so reserve a
// little more than one line box of descent below the last row.
const BOTTOM_MARGIN: f64 = 24.0;

/// Build a status figure for `theme` at the default width.
pub fn figure_status_message(title: &str, message: &str, ok: bool, theme: Option<&str>) -> Scene {
    status_scene(title, message, ok, get_palette(theme))
}

/// Build a status figure from an explicit palette at the default width.
pub fn status_scene(title: &str, message: &str, ok: bool, pal: &Palette) -> Scene {
    sized_status_scene(title, message, ok, pal, STATUS_WIDTH)
}

/// Build a status figure `width` scene pixels wide.
///
/// The title is the only colored text; `message` is retained verbatim in a
/// wrapped body block whose height grows with the conservative line count so
/// the last row stays inside the scene. An empty message yields the title
/// alone.
pub fn sized_status_scene(
    title: &str,
    message: &str,
    ok: bool,
    pal: &Palette,
    width: f64,
) -> Scene {
    let width = width.max(4.0 * STATUS_MARGIN);
    let content_width = width - 2.0 * STATUS_MARGIN;
    let message = message.trim();
    let body_height = if message.is_empty() {
        0.0
    } else {
        text_block_height(message, STATUS_BODY_FONT_SIZE, content_width)
    };
    let height = MIN_HEIGHT.max(MESSAGE_TOP + body_height + BOTTOM_MARGIN);
    let mut scene = Scene::new(width, height, Some(Color::from_hex(pal.bg)));
    scene.title = Some(title.to_owned());
    scene.suppress_derived_title();
    scene.add(SceneElement::Text {
        text: title.to_owned(),
        pos: [STATUS_MARGIN, TITLE_TOP],
        font_size: TITLE_FONT_SIZE,
        color: Color::from_hex(if ok { OK_COLOR } else { FAILURE_COLOR }),
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: true,
    });
    if !message.is_empty() {
        scene.add(SceneElement::TextBlock {
            text: message.to_owned(),
            pos: [STATUS_MARGIN, MESSAGE_TOP],
            width: content_width,
            font_size: STATUS_BODY_FONT_SIZE,
            color: Color::from_hex(pal.tick),
            bold: false,
        });
    }
    scene
}

/// Line advance of the body text in scene pixels.
pub fn body_line_height() -> f64 {
    STATUS_BODY_FONT_SIZE * CSS_PIXELS_PER_POINT * TEXT_LINE_HEIGHT_EM
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::conservative_char_budget;
    use crate::svg::render_svg;

    fn title_elements(scene: &Scene, title: &str) -> Vec<(Color, bool)> {
        scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text {
                    text, color, bold, ..
                } if text == title => Some((*color, *bold)),
                _ => None,
            })
            .collect()
    }

    fn body(scene: &Scene) -> Option<(&str, [f64; 2], f64, f64)> {
        scene.elements.iter().find_map(|element| match element {
            SceneElement::TextBlock {
                text,
                pos,
                width,
                font_size,
                ..
            } => Some((text.as_str(), *pos, *width, *font_size)),
            _ => None,
        })
    }

    fn tspan_rows(svg: &str) -> Vec<String> {
        svg.split("<tspan")
            .skip(1)
            .filter_map(|row| {
                let start = row.find('>')? + 1;
                let end = row.find("</tspan>")?;
                Some(row[start..end].to_owned())
            })
            .collect()
    }

    #[test]
    fn a_failed_status_has_one_red_bold_title_and_no_derived_heading() {
        let scene = figure_status_message("VSPAERO wake history unavailable", "why", false, None);
        assert_eq!(
            scene.title.as_deref(),
            Some("VSPAERO wake history unavailable")
        );
        assert!(!scene.render_title);
        assert_eq!(
            title_elements(&scene, "VSPAERO wake history unavailable"),
            vec![(Color::from_hex(FAILURE_COLOR), true)]
        );
    }

    #[test]
    fn an_ok_status_uses_the_upstream_success_color() {
        let scene = figure_status_message("Mission solved", "converged", true, Some("dark"));
        assert_eq!(
            title_elements(&scene, "Mission solved"),
            vec![(Color::from_hex(OK_COLOR), true)]
        );
    }

    #[test]
    fn the_body_is_one_text_block_spanning_the_content_box() {
        let scene = figure_status_message("Title", "  retained diagnostic  ", false, Some("grey"));
        let (text, pos, width, font_size) = body(&scene).expect("body block");
        assert_eq!(text, "retained diagnostic");
        assert_eq!(pos, [STATUS_MARGIN, 82.0]);
        assert_eq!(width, STATUS_WIDTH - 2.0 * STATUS_MARGIN);
        assert_eq!(font_size, STATUS_BODY_FONT_SIZE);
        assert_eq!(scene.width, STATUS_WIDTH);
        assert_eq!(scene.height, 240.0);
        let blocks = scene
            .elements
            .iter()
            .filter(|element| matches!(element, SceneElement::TextBlock { .. }))
            .count();
        assert_eq!(blocks, 1);
    }

    #[test]
    fn an_empty_message_yields_the_title_alone() {
        let scene = figure_status_message("VLM flow unavailable", "   ", false, None);
        assert!(body(&scene).is_none());
        assert_eq!(scene.elements.len(), 1);
    }

    #[test]
    fn the_canvas_grows_with_the_conservative_line_count() {
        let message = "word ".repeat(400);
        let scene = figure_status_message("Title", &message, false, None);
        let content_width = STATUS_WIDTH - 2.0 * STATUS_MARGIN;
        let rows =
            crate::scene::wrap_text_to_width(message.trim(), STATUS_BODY_FONT_SIZE, content_width)
                .lines()
                .count();
        assert!(rows > 10);
        let expected = 82.0 + rows as f64 * body_line_height() + 24.0;
        assert!((scene.height - expected).abs() < 1e-9);
    }

    #[test]
    fn svg_rows_never_exceed_the_conservative_budget_in_any_theme() {
        let long_path = format!(r"C:\runs\{}\aircraft.history", "diagnostic".repeat(40));
        let message =
            format!("{long_path}: The system cannot find the file specified. (os error 2)");
        let budget =
            conservative_char_budget(STATUS_BODY_FONT_SIZE, STATUS_WIDTH - 2.0 * STATUS_MARGIN);
        for theme in ["light", "grey", "dark"] {
            let scene = figure_status_message(
                "VSPAERO wake history unavailable",
                &message,
                false,
                Some(theme),
            );
            let svg = render_svg(&scene);
            assert!(svg.contains("viewBox=\"0 0 760.0"), "{theme}");
            let rows = tspan_rows(&svg);
            assert!(rows.len() > 4, "{theme}: long path should use several rows");
            assert!(
                rows.iter().all(|row| row.chars().count() <= budget),
                "{theme}: {rows:?}"
            );
            assert_eq!(svg.matches("x=\"32.00\"").count(), rows.len(), "{theme}");
            let joined: String = rows[1..]
                .iter()
                .flat_map(|row| row.chars())
                .filter(|c| !c.is_whitespace())
                .collect();
            let original: String = message.chars().filter(|c| !c.is_whitespace()).collect();
            assert_eq!(
                joined, original,
                "{theme}: every character of the diagnostic survives"
            );
        }
    }

    #[test]
    fn a_custom_width_keeps_the_lateral_margins() {
        let pal = get_palette(Some("light"));
        let scene = sized_status_scene(
            "Propulsion model unavailable",
            "binding error",
            false,
            pal,
            1000.0,
        );
        assert_eq!(scene.width, 1000.0);
        let (_, pos, width, _) = body(&scene).expect("body block");
        assert_eq!(pos[0], STATUS_MARGIN);
        assert_eq!(pos[0] + width, 1000.0 - STATUS_MARGIN);
    }
}
