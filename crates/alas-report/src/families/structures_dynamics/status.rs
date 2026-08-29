// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py: figure_status_message
// (L650-692), _structures_unavailable_message (L5565-5576), plt_cm_tab10
// (L5815-5816).
// Reference: alas @ rust-port-baseline.

//! The shared "why is this figure blank" note, the availability check every
//! figure in this family runs first, and the two small color tables they
//! share.

use alas_pipeline::structural::StructuralAnalysisResult;

use crate::scene::{Color, Scene, SceneElement, TextAlign, TextBaseline};
use crate::theme::Palette;

/// `figure_status_message`'s failure-note color (its `ok=True` branch is
/// never reached by anything in this family: every early return here is a
/// missing or failed analysis).
const ERROR_COLOR: &str = "#c0392b";

/// A minimal, chart-sized status note -- ported from `figure_status_message`,
/// scoped to the failure styling every caller in this module needs.
pub fn status_message_scene(title: &str, message: &str, ok: bool, pal: &Palette) -> Scene {
    let mut scene = Scene::new(900.0, 220.0, Some(Color::from_hex(pal.bg)));
    let color = Color::from_hex(if ok { "#27ae60" } else { ERROR_COLOR });
    scene.add(SceneElement::Text {
        text: title.to_owned(),
        pos: [12.0, 34.0],
        font_size: 15.0,
        color,
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: true,
    });
    scene.add(SceneElement::Text {
        text: message.to_owned(),
        pos: [12.0, 78.0],
        font_size: 11.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
    scene
}

/// Port of `_structures_unavailable_message`, folded together with the
/// early-return it always feeds: `Ok(result)` only when `result` is present
/// and its `status` is `"ok"`, otherwise the placeholder scene the failed
/// check produces upstream.
pub fn resolve_structural_result<'a>(
    result: Option<&'a StructuralAnalysisResult>,
    title: &str,
    pal: &Palette,
) -> Result<&'a StructuralAnalysisResult, Scene> {
    let Some(result) = result else {
        return Err(status_message_scene(
            title,
            "Structural analysis was not run for this design (Advanced Settings -> Structural \
             Analysis).",
            false,
            pal,
        ));
    };
    if result.status != "ok" {
        return Err(status_message_scene(
            title,
            &format!(
                "Structural analysis failed: {}",
                result.error.as_deref().unwrap_or("unknown error")
            ),
            false,
            pal,
        ));
    }
    Ok(result)
}

/// `plt_cm_tab10`'s six colors, as hex: `Color::from_hex` only special-cases
/// four of matplotlib's `tab:` names, and the mode-shape panels sample all
/// six this program's own helper returns.
pub const TAB10: [&str; 6] = [
    "#1f77b4", "#ff7f0e", "#2ca02c", "#d62728", "#9467bd", "#8c564b",
];

/// The load-case color map every figure here uses: pull-up red, push-down
/// blue, level green, anything else matplotlib's `tab:gray` default.
pub fn load_case_color(name: &str) -> &'static str {
    match name {
        "pull-up" => "tab:red",
        "push-down" => "tab:blue",
        "level" => "tab:green",
        _ => "#7f7f7f",
    }
}

/// `hex` at a given 8-bit alpha -- the `alpha=0.88` matplotlib bars in this
/// family use.
pub fn with_alpha(hex: &str, alpha: u8) -> Color {
    let c = Color::from_hex(hex);
    Color::rgba(c.r, c.g, c.b, alpha)
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::PALETTE_LIGHT;

    #[test]
    fn missing_result_reports_not_run() {
        let err = resolve_structural_result(None, "T", &PALETTE_LIGHT).unwrap_err();
        let has_message = err
            .elements
            .iter()
            .any(|e| matches!(e, SceneElement::Text { text, .. } if text.contains("not run")));
        assert!(has_message);
    }

    #[test]
    fn failed_result_reports_its_error() {
        let result = StructuralAnalysisResult {
            status: "error".to_owned(),
            error: Some("bad material".to_owned()),
            ..StructuralAnalysisResult::default()
        };
        let err = resolve_structural_result(Some(&result), "T", &PALETTE_LIGHT).unwrap_err();
        let has_message = err
            .elements
            .iter()
            .any(|e| matches!(e, SceneElement::Text { text, .. } if text.contains("bad material")));
        assert!(has_message);
    }

    #[test]
    fn ok_result_resolves_to_itself() {
        let result = StructuralAnalysisResult {
            status: "ok".to_owned(),
            ..StructuralAnalysisResult::default()
        };
        let resolved = resolve_structural_result(Some(&result), "T", &PALETTE_LIGHT).unwrap();
        assert_eq!(resolved.status, "ok");
    }

    #[test]
    fn load_case_colors_match_the_reference_dict() {
        assert_eq!(load_case_color("pull-up"), "tab:red");
        assert_eq!(load_case_color("push-down"), "tab:blue");
        assert_eq!(load_case_color("level"), "tab:green");
        assert_eq!(load_case_color("other"), "#7f7f7f");
    }

    #[test]
    fn alpha_helper_keeps_rgb_and_overrides_alpha() {
        let c = with_alpha("tab:blue", 224);
        assert_eq!((c.r, c.g, c.b, c.a), (31, 119, 180, 224));
    }
}
