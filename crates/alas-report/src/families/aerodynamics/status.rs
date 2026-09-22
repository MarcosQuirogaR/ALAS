// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py, `figure_status_message`
// (L650-692).
// Reference: alas @ rust-port-baseline.

use crate::scene::Scene;

/// A deliberately minimal, chart-sized status note that surfaces *why* an
/// optional analysis (MSES, mission analysis model mission, a VLM solve that failed) is
/// missing from a results tab instead of silently omitting it with no
/// explanation. Red/left-aligned text for a failure, green for an
/// informational success note, matching upstream's `#c0392b`/`#27ae60`.
///
/// The shared placeholder in [`crate::status_figure`] draws it: one colored
/// title and a wrapped body block that stays inside the canvas on every
/// backend.
pub fn figure_status_message(title: &str, message: &str, ok: bool, theme: Option<&str>) -> Scene {
    crate::status_figure::figure_status_message(title, message, ok, theme)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Color, SceneElement};

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
