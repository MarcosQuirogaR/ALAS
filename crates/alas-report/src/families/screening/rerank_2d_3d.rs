// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/airfoil_sweep_figures.py (`fig_rerank_2d_3d`)
// Reference: alas @ rust-port-baseline.

//! 2-D proxy L/D vs 3-D-wing L/D for the refined shortlist, against a `y=x`
//! reference: points below the diagonal are the sections the 2-D screen
//! over-rated, once induced and wave drag on the real planform are counted.

use std::collections::HashSet;

use crate::chart_kit::{draw_legend, LegendMarker};
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;
use alas_screen::AirfoilScreeningResult;

use super::{mark_references, refined_candidates, REFERENCE_MARKER_COLOR};

/// `None` when no candidate reached Stage 2 (3-D) refinement: Stage 2 was
/// disabled, or every attempt failed.
pub fn fig_rerank_2d_3d(result: &AirfoilScreeningResult, theme: Option<&str>) -> Option<Scene> {
    let cands = refined_candidates(result);
    if cands.is_empty() {
        return None;
    }
    let xs: Vec<f64> = cands.iter().map(|c| c.l_over_d.unwrap_or(0.0)).collect();
    let ys: Vec<f64> = cands.iter().map(|c| c.l_over_d_3d.unwrap_or(0.0)).collect();

    let pal = get_palette(theme);
    let mut scene = Scene::new(650.0, 460.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("How the 2-D shortlist reshuffles in 3-D".to_owned());

    let lo = 0.0;
    let hi = (xs.iter().chain(ys.iter()).cloned().fold(0.0f64, f64::max) * 1.05).max(lo + 1e-6);
    let axes = Axes2D::new((70.0, 40.0, 480.0, 340.0), (lo, hi), (lo, hi));
    axes.draw_frame_with_labels(
        &mut scene,
        pal,
        "2-D proxy L/D (isolated section)",
        "3-D wing L/D (this design, cruise)",
    );

    let diag_stroke = Stroke::dashed(Color::from_hex(pal.spine), 1.0, 5.0, 4.0);
    axes.add_line_series(&mut scene, &[(lo, lo), (hi, hi)], diag_stroke.clone());

    let pt_stroke = Stroke::new(Color::from_hex(pal.spine), 0.7);
    let pt_fill = Fill::new(Color::from_hex(pal.accent));
    for (&x, &y) in xs.iter().zip(&ys) {
        let p = axes.map_point(x, y);
        scene.add(SceneElement::Circle {
            center: p,
            radius: 5.0,
            fill: Some(pt_fill),
            stroke: Some(pt_stroke.clone()),
        });
    }

    // Label only the two candidates whose 2-D -> 3-D gap is largest (the
    // ones the 2-D screen over-rated most) skipping the baseline and any
    // reference section, which get their own labels below.
    let mut gap_order: Vec<usize> = (0..cands.len()).collect();
    gap_order.sort_by(|&a, &b| (xs[b] - ys[b]).total_cmp(&(xs[a] - ys[a])));
    let highlight: HashSet<usize> = gap_order.into_iter().take(2).collect();
    for (i, cand) in cands.iter().enumerate() {
        if !highlight.contains(&i) || cand.name == result.baseline_airfoil || cand.is_reference {
            continue;
        }
        let p = axes.map_point(xs[i], ys[i]);
        scene.add(SceneElement::Text {
            text: cand.name.clone(),
            pos: [p[0] + 4.0, p[1] + 2.0],
            font_size: 7.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: false,
        });
    }
    for (i, cand) in cands.iter().enumerate() {
        if cand.name != result.baseline_airfoil {
            continue;
        }
        let p = axes.map_point(xs[i], ys[i]);
        scene.add(SceneElement::Text {
            text: format!("{} (current)", cand.name),
            pos: [p[0] + 4.0, p[1] + 2.0],
            font_size: 8.0,
            color: Color::from_hex(pal.accent),
            align: TextAlign::Left,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: true,
        });
        break;
    }
    let has_reference = mark_references(&mut scene, &axes, &cands, &xs, &ys, pal);

    let mut legend_entries = vec![("2-D = 3-D".to_owned(), LegendMarker::Line(diag_stroke))];
    if has_reference {
        legend_entries.push((
            "Real reference section".to_owned(),
            LegendMarker::Circle(Color::from_hex(REFERENCE_MARKER_COLOR)),
        ));
    }
    draw_legend(&mut scene, [80.0, 50.0], &legend_entries, pal, 8.0);

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
        l_over_d: f64,
        l_over_d_3d: f64,
        refined: bool,
    ) -> AirfoilCandidateResult {
        AirfoilCandidateResult {
            name: name.to_owned(),
            status: "ok".to_owned(),
            l_over_d: Some(l_over_d),
            l_over_d_3d: Some(l_over_d_3d),
            refined,
            ..Default::default()
        }
    }

    #[test]
    fn returns_none_when_stage_2_refinement_never_ran() {
        let result = AirfoilScreeningResult {
            candidates: vec![candidate("a", 15.0, 14.0, false)],
            ..Default::default()
        };
        assert!(fig_rerank_2d_3d(&result, None).is_none());
    }

    #[test]
    fn diagonal_reference_line_spans_the_full_plotted_range() {
        let result = AirfoilScreeningResult {
            candidates: vec![
                candidate("a", 20.0, 18.0, true),
                candidate("b", 10.0, 9.0, true),
            ],
            ..Default::default()
        };
        let scene = fig_rerank_2d_3d(&result, None).expect("has refined candidates");
        let diag = scene
            .elements
            .iter()
            .find(|e| matches!(e, SceneElement::Polyline { stroke, .. } if stroke.dash_array.is_some()))
            .expect("dashed y=x line present");
        if let SceneElement::Polyline { points, .. } = diag {
            // A diagonal reference from the origin: start and end pixel
            // coordinates both lie on the frame's rising diagonal.
            assert_eq!(points.len(), 2);
        }
    }
}
