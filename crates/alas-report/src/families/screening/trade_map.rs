// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/airfoil_sweep_figures.py (`fig_trade_map`)
// Reference: alas @ rust-port-baseline.

//! Trade map: cruise L/D vs resulting wing fuel-tank capacity, coloured by
//! section thickness -- the two design levers the ranking blends, plotted
//! directly as axes so the Pareto front is visible at a glance.

use crate::chart_kit::{draw_colorbar, draw_legend, LegendMarker};
use crate::colormap::Colormap;
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;
use alas_screen::AirfoilScreeningResult;

use super::{mark_references, ok_candidates, refined_candidates, REFERENCE_MARKER_COLOR};

/// Cruise L/D vs wing fuel-tank capacity, coloured by section thickness --
/// `fig_trade_map`. Uses the 3-D L/D once any candidate was refined, else
/// the 2-D proxy. `None` when screening found no usable candidate.
pub fn fig_trade_map(result: &AirfoilScreeningResult, theme: Option<&str>) -> Option<Scene> {
    let refined = refined_candidates(result);
    let use_3d = !refined.is_empty();
    let cands = if use_3d {
        refined
    } else {
        ok_candidates(result)
    };
    if cands.is_empty() {
        return None;
    }

    let xs: Vec<f64> = cands
        .iter()
        .map(|c| (if use_3d { c.l_over_d_3d } else { c.l_over_d }).unwrap_or(0.0))
        .collect();
    let ys: Vec<f64> = cands
        .iter()
        .map(|c| c.tank_capacity_kg.unwrap_or(0.0))
        .collect();
    let tc: Vec<f64> = cands
        .iter()
        .map(|c| c.max_thickness_frac.unwrap_or(0.0) * 100.0)
        .collect();

    let pal = get_palette(theme);
    let mut scene = Scene::new(680.0, 440.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Trade map -- L/D vs fuel capacity".to_owned());

    let (x_lo, x_hi) = padded_range(&xs, 0.08);
    let (y_lo, y_hi) = padded_range(&ys, 0.08);
    let axes = Axes2D::new((92.0, 40.0, 458.0, 340.0), (x_lo, x_hi), (y_lo, y_hi));
    axes.draw_frame_with_labels(
        &mut scene,
        pal,
        if use_3d {
            "Cruise L/D (3-D wing)"
        } else {
            "Cruise L/D (2-D proxy)"
        },
        "Wing fuel-tank capacity (kg)",
    );

    let (tc_lo, tc_hi) = padded_range(&tc, 0.0);
    let tc_span = (tc_hi - tc_lo).max(1e-9);
    let cmap = Colormap::Viridis;
    for ((&x, &y), &t) in xs.iter().zip(&ys).zip(&tc) {
        let color = cmap.sample((t - tc_lo) / tc_span);
        let p = axes.map_point(x, y);
        scene.add(SceneElement::Circle {
            center: p,
            radius: 5.0,
            fill: Some(Fill::new(color)),
            stroke: Some(Stroke::new(Color::from_hex(pal.spine), 0.7)),
        });
    }
    draw_colorbar(
        &mut scene,
        (560.0, 40.0, 16.0, 340.0),
        cmap,
        tc_lo,
        tc_hi,
        "Section t/c (%)",
        pal,
    );

    // Top pick label -- `cands[0]` (result order is best-rank-first).
    let top_p = axes.map_point(xs[0], ys[0]);
    scene.add(SceneElement::Text {
        text: format!("{} (best)", cands[0].name),
        pos: [top_p[0] + 6.0, top_p[1]],
        font_size: 9.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: true,
    });

    // Current (baseline) section: open-circle highlight + label.
    for ((cand, &x), &y) in cands.iter().zip(&xs).zip(&ys) {
        if cand.name != result.baseline_airfoil {
            continue;
        }
        let p = axes.map_point(x, y);
        scene.add(SceneElement::Circle {
            center: p,
            radius: 9.0,
            fill: None,
            stroke: Some(Stroke::new(Color::from_hex(pal.accent), 2.0)),
        });
        scene.add(SceneElement::Text {
            text: format!("{} (current)", cand.name),
            pos: [p[0] + 6.0, p[1]],
            font_size: 9.0,
            color: Color::from_hex(pal.accent),
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
        break;
    }

    let has_reference = mark_references(&mut scene, &axes, &cands, &xs, &ys, pal);
    if has_reference {
        draw_legend(
            &mut scene,
            [
                axes.left + axes.width - 170.0,
                axes.top + axes.height - 26.0,
            ],
            &[(
                "Real reference section".to_owned(),
                LegendMarker::Circle(Color::from_hex(REFERENCE_MARKER_COLOR)),
            )],
            pal,
            8.0,
        );
    }

    Some(scene)
}

/// Padded `[min, max]` over `values`, the substitute for Matplotlib's
/// autoscale margin: `pad_frac` grows the span before fitting; when every
/// value is equal, a fixed floor keeps the axis from collapsing to a point.
pub(super) fn padded_range(values: &[f64], pad_frac: f64) -> (f64, f64) {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for &v in values {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    if !lo.is_finite() {
        return (0.0, 1.0);
    }
    let span = (hi - lo).max(hi.abs().max(1.0) * 1e-3);
    let pad = span * pad_frac.max(0.02);
    (lo - pad, hi + pad)
}

#[cfg(test)]
mod tests {
    // These tests intentionally panic if their constructed fixture violates its precondition.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use alas_screen::AirfoilCandidateResult;

    fn candidate(name: &str, l_over_d: f64, tank_kg: f64, tc_frac: f64) -> AirfoilCandidateResult {
        AirfoilCandidateResult {
            name: name.to_owned(),
            status: "ok".to_owned(),
            l_over_d: Some(l_over_d),
            tank_capacity_kg: Some(tank_kg),
            max_thickness_frac: Some(tc_frac),
            ..Default::default()
        }
    }

    #[test]
    fn returns_none_when_no_candidate_survived_screening() {
        let result = AirfoilScreeningResult::default();
        assert!(fig_trade_map(&result, None).is_none());
    }

    #[test]
    fn axis_range_tracks_the_real_candidate_extent_not_a_fixed_window() {
        // A candidate set far outside the old hardcoded (0.08,0.16) x
        // (0.006,0.018) window must still land inside the computed axes.
        let result = AirfoilScreeningResult {
            candidates: vec![
                candidate("a", 40.0, 9000.0, 0.30),
                candidate("b", 45.0, 9500.0, 0.32),
            ],
            ..Default::default()
        };
        let scene = fig_trade_map(&result, None).expect("has candidates");
        let circles = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Circle { radius, .. } if (*radius - 5.0).abs() < 1e-9))
            .count();
        assert_eq!(circles, 2);
    }

    #[test]
    fn padded_range_gives_a_finite_floor_on_a_single_repeated_value() {
        let (lo, hi) = padded_range(&[5.0, 5.0, 5.0], 0.08);
        assert!(lo < 5.0 && hi > 5.0);
    }
}
