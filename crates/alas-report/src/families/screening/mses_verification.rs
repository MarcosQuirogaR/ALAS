// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/airfoil_sweep_figures.py (`fig_mses_verification`)
// Reference: alas @ rust-port-baseline.

//! Stage-3 MSES verification: VLM+Korn (Stage 2) vs MSES (Stage 3) cruise
//! L/D per verified candidate, with wave drag annotated next to each MSES
//! bar. Neither the 2-D NeuralFoil proxy nor VLM+Korn model shocks, so this
//! is the only figure that can show whether a candidate actually handles
//! this cruise Mach well.

use crate::chart_kit::{draw_legend, LegendMarker};
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;
use alas_screen::AirfoilScreeningResult;

use super::mses_verified_candidates;

/// Bar colour for the Stage 2 (VLM + Korn wave-drag correlation) estimate.
const VLM_COLOR: &str = "#7aa2ff";
/// Bar colour for the Stage 3 (real MSES viscous/shock) result.
const MSES_COLOR: &str = "#ff8a5c";

/// `None` when no candidate was MSES-verified (Stage 3 off, MSES not
/// configured, or every attempt failed to converge).
pub fn fig_mses_verification(
    result: &AirfoilScreeningResult,
    theme: Option<&str>,
) -> Option<Scene> {
    let cands = mses_verified_candidates(result);
    if cands.is_empty() {
        return None;
    }

    let ld_vlm: Vec<f64> = cands.iter().map(|c| c.l_over_d_3d.unwrap_or(0.0)).collect();
    let ld_mses: Vec<f64> = cands
        .iter()
        .map(|c| c.l_over_d_mses.unwrap_or(0.0))
        .collect();
    let cdw: Vec<f64> = cands.iter().map(|c| c.cdw_mses.unwrap_or(0.0)).collect();
    let n = cands.len();

    let pal = get_palette(theme);
    let canvas_h = ((0.6 * n as f64 + 1.2).max(3.2)) * 100.0;
    let mut scene = Scene::new(650.0, canvas_h, Some(Color::from_hex(pal.bg)));
    scene.title =
        Some("MSES verification: real shock/viscous effects vs the VLM+Korn estimate".to_owned());

    let max_ld = ld_vlm
        .iter()
        .chain(ld_mses.iter())
        .cloned()
        .fold(0.0f64, f64::max)
        .max(1.0);
    let axes = Axes2D::new(
        (145.0, 30.0, 435.0, canvas_h - 90.0),
        (0.0, max_ld * 1.25),
        (-0.6, n as f64 - 0.4),
    );
    axes.draw_frame_with_labels(&mut scene, pal, "Cruise L/D", "");

    // The vertical coordinate is categorical. Mask the generated numeric
    // ticks before placing candidate names so the two label systems cannot
    // collide in the left gutter.
    scene.add(SceneElement::Rect {
        x: 30.0,
        y: axes.top,
        width: axes.left - 31.0,
        height: axes.height,
        rx: 0.0,
        fill: Some(Fill::new(Color::from_hex(pal.bg))),
        stroke: None,
    });

    let bar_h = 0.35;
    for i in 0..n {
        // `y = np.arange(len(cands))`, no `invert_yaxis()` upstream: the
        // first candidate sits at the bottom, not the top.
        let row = i as f64;
        for &(val, color, dy) in &[
            (ld_vlm[i], VLM_COLOR, bar_h / 2.0),
            (ld_mses[i], MSES_COLOR, -bar_h / 2.0),
        ] {
            let p_left = axes.map_point(0.0, row + dy);
            let p_top = axes.map_point(val, row + dy + bar_h / 2.0);
            let p_bot = axes.map_point(0.0, row + dy - bar_h / 2.0);
            scene.add(SceneElement::Rect {
                x: p_left[0].min(p_top[0]),
                y: p_top[1],
                width: (p_top[0] - p_left[0]).abs().max(1.0),
                height: (p_bot[1] - p_top[1]).abs().max(1.0),
                rx: 1.0,
                fill: Some(Fill::new(Color::from_hex(color))),
                stroke: Some(Stroke::new(Color::from_hex(pal.spine), 0.5)),
            });
        }

        let p_cdw = axes.map_point(ld_mses[i], row - bar_h / 2.0);
        scene.add(SceneElement::Text {
            text: format!("CDw={:.0} cts", cdw[i] * 1e4),
            pos: [p_cdw[0] + 4.0, p_cdw[1]],
            font_size: 7.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });

        let label = if cands[i].name == result.baseline_airfoil {
            format!("{} (current)", cands[i].name)
        } else {
            cands[i].name.clone()
        };
        let p_label = axes.map_point(0.0, row);
        scene.add(SceneElement::Text {
            text: label,
            pos: [p_label[0] - 10.0, p_label[1]],
            font_size: 8.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Right,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }

    draw_legend(
        &mut scene,
        [axes.left + 8.0, axes.top + 8.0],
        &[
            (
                "VLM + Korn (Stage 2)".to_owned(),
                LegendMarker::Patch(Color::from_hex(VLM_COLOR)),
            ),
            (
                "MSES (Stage 3, real shock/viscous)".to_owned(),
                LegendMarker::Patch(Color::from_hex(MSES_COLOR)),
            ),
        ],
        pal,
        8.0,
    );

    Some(scene)
}

#[cfg(test)]
mod tests {
    // These tests intentionally panic if their constructed fixture violates its precondition.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use alas_screen::AirfoilCandidateResult;

    fn candidate(
        name: &str,
        ld_3d: f64,
        ld_mses: f64,
        cdw: f64,
        verified: bool,
    ) -> AirfoilCandidateResult {
        AirfoilCandidateResult {
            name: name.to_owned(),
            status: "ok".to_owned(),
            l_over_d_3d: Some(ld_3d),
            l_over_d_mses: Some(ld_mses),
            cdw_mses: Some(cdw),
            mses_verified: verified,
            ..Default::default()
        }
    }

    #[test]
    fn returns_none_when_no_candidate_was_mses_verified() {
        let result = AirfoilScreeningResult {
            candidates: vec![candidate("a", 18.0, 17.0, 0.0004, false)],
            ..Default::default()
        };
        assert!(fig_mses_verification(&result, None).is_none());
    }

    #[test]
    fn one_verified_candidate_draws_a_paired_bar_and_a_wave_drag_annotation() {
        let result = AirfoilScreeningResult {
            candidates: vec![candidate("a", 18.0, 16.5, 0.0012, true)],
            ..Default::default()
        };
        let scene = fig_mses_verification(&result, None).expect("one verified candidate");
        let rects = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Rect { .. }))
            .count();
        // The scene background, categorical-tick mask, VLM and MSES bars and
        // their paired legend swatches are rectangles in the scene model.
        assert_eq!(rects, 6);
        let cdw_text = scene
            .elements
            .iter()
            .any(|e| matches!(e, SceneElement::Text { text, .. } if text.contains("CDw=12 cts")));
        assert!(cdw_text, "0.0012 wave drag renders as 12 counts");
    }
}
