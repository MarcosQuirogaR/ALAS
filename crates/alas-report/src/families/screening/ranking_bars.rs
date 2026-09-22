// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/airfoil_sweep_figures.py (`fig_ranking_bars`)
// Reference: alas @ rust-port-baseline.

//! Top-candidate ranking bar chart: the plain "what should I pick"
//! read-out.

use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;
use alas_screen::AirfoilScreeningResult;

use super::{ok_candidates, refined_candidates, REFERENCE_MARKER_COLOR};

/// Top candidates ranked by final cruise L/D (3-D where available), current
/// section highlighted: `fig_ranking_bars`. `None` when screening found no
/// usable candidate.
pub fn fig_ranking_bars(result: &AirfoilScreeningResult, theme: Option<&str>) -> Option<Scene> {
    let refined = refined_candidates(result);
    let use_3d = !refined.is_empty();
    let pool = if use_3d {
        refined
    } else {
        ok_candidates(result)
    };
    if pool.is_empty() {
        return None;
    }
    // `cands[:12]` upstream.
    let cands: Vec<_> = pool.into_iter().take(12).collect();
    let n = cands.len();

    let pal = get_palette(theme);
    let height = (0.42 * n as f64 + 1.0).max(3.2) * 100.0;
    let mut scene = Scene::new(650.0, height, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Top airfoils for this design  (* = real reference section)".to_owned());

    let vals: Vec<f64> = cands
        .iter()
        .map(|c| (if use_3d { c.l_over_d_3d } else { c.l_over_d }).unwrap_or(0.0))
        .collect();
    let max_val = vals.iter().cloned().fold(0.0f64, f64::max).max(1.0);

    let axes = Axes2D::new(
        // Keep a full text-height below the horizontal-axis label.  With the
        // former 70 px allowance, the short five-candidate card placed its
        // label only three pixels from the SVG edge.
        (120.0, 30.0, 460.0, height - 90.0),
        (0.0, max_val * 1.18),
        (0.0, n as f64),
    );
    axes.draw_frame_with_labels(
        &mut scene,
        pal,
        if use_3d {
            "Cruise L/D (3-D wing)"
        } else {
            "Cruise L/D (2-D proxy)"
        },
        "",
    );

    let bar_h = (axes.height / n as f64 * 0.6).min(24.0);
    for (i, (cand, &val)) in cands.iter().zip(&vals).enumerate() {
        // Best rank first in `cands`; drawn at the top of the panel,
        // `y = list(range(len(cands)))[::-1]`.
        let row = (n - 1 - i) as f64 + 0.5;
        let p_left = axes.map_point(0.0, row);
        let p_right = axes.map_point(val, row);

        let color = if cand.name == result.baseline_airfoil {
            pal.accent
        } else if cand.is_reference {
            REFERENCE_MARKER_COLOR
        } else {
            "#7aa2ff"
        };
        scene.add(SceneElement::Rect {
            x: p_left[0],
            y: p_left[1] - bar_h * 0.5,
            width: (p_right[0] - p_left[0]).max(1.0),
            height: bar_h,
            rx: 2.0,
            fill: Some(Fill::new(Color::from_hex(color))),
            stroke: Some(Stroke::new(Color::from_hex(pal.spine), 0.5)),
        });

        let label = if cand.name == result.baseline_airfoil {
            format!("{} (current)", cand.name)
        } else if cand.is_reference {
            format!("{} *", cand.name)
        } else {
            cand.name.clone()
        };
        scene.add(SceneElement::Text {
            text: label,
            pos: [p_left[0] - 6.0, p_left[1]],
            font_size: 8.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Right,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
        scene.add(SceneElement::Text {
            text: format!("{val:.1}"),
            pos: [p_right[0] + 4.0, p_right[1]],
            font_size: 8.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }

    Some(scene)
}

#[cfg(test)]
mod tests {
    // These tests intentionally panic if their constructed fixture violates its precondition.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::svg::render_svg;
    use alas_screen::AirfoilCandidateResult;

    fn candidate(
        name: &str,
        l_over_d: f64,
        l_over_d_3d: Option<f64>,
        refined: bool,
    ) -> AirfoilCandidateResult {
        AirfoilCandidateResult {
            name: name.to_owned(),
            status: "ok".to_owned(),
            l_over_d: Some(l_over_d),
            l_over_d_3d,
            refined,
            ..Default::default()
        }
    }

    #[test]
    fn returns_none_when_no_candidate_survived_screening() {
        let result = AirfoilScreeningResult::default();
        assert!(fig_ranking_bars(&result, None).is_none());
    }

    #[test]
    fn best_rank_first_candidate_gets_the_tallest_bar_and_sits_at_the_top() {
        let result = AirfoilScreeningResult {
            candidates: vec![
                candidate("best", 18.0, None, false),
                candidate("worst", 12.0, None, false),
            ],
            ..Default::default()
        };
        let scene = fig_ranking_bars(&result, None).expect("has candidates");
        let rects: Vec<_> = scene
            .elements
            .iter()
            .filter_map(|e| match e {
                SceneElement::Rect { y, width, .. } => Some((*y, *width)),
                _ => None,
            })
            .collect();
        // The first rectangle is the scene background; the two bars follow.
        assert_eq!(rects.len(), 3);
        let rects = &rects[1..];
        // The first candidate's bar (best L/D, widest) is drawn above the
        // second's (smaller `y`, since canvas Y grows downward).
        let (y_best, w_best) = rects[0];
        let (y_worst, w_worst) = rects[1];
        assert!(y_best < y_worst);
        assert!(w_best > w_worst);
    }

    #[test]
    fn switches_to_the_3d_l_over_d_only_once_a_candidate_was_refined() {
        let result_2d = AirfoilScreeningResult {
            candidates: vec![candidate("a", 15.0, Some(99.0), false)],
            ..Default::default()
        };
        let result_3d = AirfoilScreeningResult {
            candidates: vec![candidate("a", 15.0, Some(11.0), true)],
            ..Default::default()
        };
        // Widths are proportional to the plotted value; a switch from 15 to
        // 11 shows up as a narrower bar even though both scenes autoscale.
        let svg_2d = render_svg(&fig_ranking_bars(&result_2d, None).unwrap());
        let svg_3d = render_svg(&fig_ranking_bars(&result_3d, None).unwrap());
        assert!(svg_2d.contains("15.0"));
        assert!(svg_3d.contains("11.0"));
    }
}
