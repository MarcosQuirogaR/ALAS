// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py, `figure_status_message`
// (L650-692).
// Reference: alas @ rust-port-baseline.

use crate::scene::{Color, Scene, SceneElement, TextAlign, TextBaseline};
use crate::theme::get_palette;

/// A deliberately minimal, chart-sized status note -- for surfacing *why* an
/// optional analysis (MSES, mission analysis model mission, a VLM solve that failed) is
/// missing from a results tab instead of silently omitting it with no
/// explanation. Red/left-aligned text for a failure, green for an
/// informational success note, matching upstream's `#c0392b`/`#27ae60`.
pub fn figure_status_message(title: &str, message: &str, ok: bool, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(560.0, 140.0, Some(Color::from_hex(pal.bg)));

    let title_color = if ok { "#27ae60" } else { "#c0392b" };
    scene.add(SceneElement::Text {
        text: title.to_owned(),
        pos: [12.0, 24.0],
        font_size: 16.0,
        color: Color::from_hex(title_color),
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: true,
    });

    // Upstream wraps the message text (`wrap=True`); this scene graph has no
    // text-flow primitive, so it is drawn as a single line and left to the
    // caller's viewport to clip or scroll -- the closest faithful
    // approximation available without a text-layout dependency.
    scene.add(SceneElement::Text {
        text: message.to_owned(),
        pos: [12.0, 60.0],
        font_size: 12.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });

    scene
}

#[cfg(test)]
mod tests {
    use super::*;

    fn title_color(scene: &Scene, title: &str) -> Option<Color> {
        scene.elements.iter().find_map(|e| match e {
            SceneElement::Text { text, color, .. } if text == title => Some(*color),
            _ => None,
        })
    }

    #[test]
    fn a_failure_message_renders_in_the_upstream_failure_color() {
        let scene =
            figure_status_message("MSES unavailable", "no mses_dir configured", false, None);
        assert_eq!(
            title_color(&scene, "MSES unavailable"),
            Some(Color::from_hex("#c0392b"))
        );
    }

    #[test]
    fn a_success_message_renders_in_the_upstream_ok_color() {
        let scene = figure_status_message("Mission solved", "converged", true, None);
        assert_eq!(
            title_color(&scene, "Mission solved"),
            Some(Color::from_hex("#27ae60"))
        );
    }
}
