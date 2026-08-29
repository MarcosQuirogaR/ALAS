// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py
// Reference: alas @ rust-port-baseline.

//! Optimization convergence history, design variable trajectories, and airfoil shape evolution.

use crate::chart_kit::{draw_colorbar, draw_legend, draw_title, LegendMarker};
use crate::colormap::Colormap;
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke};
use crate::theme::{get_palette, BASELINE_COLOR, OPTIMIZED_COLOR};
use alas_opt::history::OptimizationHistory;

/// Generate L/D convergence history, colored by span, vs valid evaluations.
pub fn figure_optimization_history(history: &OptimizationHistory, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(720.0, 480.0, Some(Color::from_hex(pal.bg)));
    let n_eval = history.l_over_d.len();
    let title = format!("Optimization convergence ({n_eval} valid evaluations)");
    scene.title = Some(title.clone());
    draw_title(&mut scene, &title, pal);
    scene.suppress_derived_title();

    if n_eval == 0 {
        return scene;
    }

    let lds = &history.l_over_d;
    let spans: Vec<f64> = (0..n_eval)
        .map(|index| history.span_m.get(index).copied().unwrap_or(0.0))
        .collect();
    let (y_min, y_max) = padded_range(lds);
    let (span_min, span_max) = finite_range(&spans);
    let axes = Axes2D::new(
        (64.0, 42.0, 520.0, 350.0),
        (0.5, n_eval as f64 + 0.5),
        (y_min, y_max),
    );
    axes.draw_frame_with_labels(&mut scene, pal, "valid evaluation #", "L/D");

    let cmap = Colormap::Viridis;
    let span_delta = (span_max - span_min).max(1e-12);
    for (index, &ld) in lds.iter().enumerate() {
        if !ld.is_finite() {
            continue;
        }
        let color = cmap.sample((spans[index] - span_min) / span_delta);
        let point = axes.map_point(index as f64 + 1.0, ld);
        scene.add(SceneElement::Circle {
            center: point,
            radius: 3.5,
            fill: Some(Fill::new(Color::rgba(color.r, color.g, color.b, 179))),
            stroke: None,
        });
    }

    let mut best = f64::NEG_INFINITY;
    let mut best_so_far = Vec::with_capacity(n_eval);
    for (index, &ld) in lds.iter().enumerate() {
        best = best.max(ld);
        if best.is_finite() {
            best_so_far.push((index as f64 + 1.0, best));
        }
    }
    axes.add_line_series(
        &mut scene,
        &best_so_far,
        Stroke::new(Color::from_hex(OPTIMIZED_COLOR), 2.0),
    );

    draw_legend(
        &mut scene,
        [
            axes.left + axes.width - 142.0,
            axes.top + axes.height - 42.0,
        ],
        &[
            (
                "Evaluation".to_owned(),
                LegendMarker::Circle(Color::from_hex("#35a27f")),
            ),
            (
                "best so far".to_owned(),
                LegendMarker::Line(Stroke::new(Color::from_hex(OPTIMIZED_COLOR), 2.0)),
            ),
        ],
        pal,
        8.0,
    );
    draw_colorbar(
        &mut scene,
        (610.0, axes.top, 16.0, axes.height),
        cmap,
        span_min,
        span_max,
        "span [m]",
        pal,
    );

    scene
}

fn finite_range(values: &[f64]) -> (f64, f64) {
    let finite: Vec<f64> = values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect();
    if finite.is_empty() {
        return (0.0, 1.0);
    }
    let lo = finite.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = finite.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    (lo, hi)
}

fn padded_range(values: &[f64]) -> (f64, f64) {
    let (lo, hi) = finite_range(values);
    let span = (hi - lo).max(hi.abs().max(1.0) * 1e-3);
    let pad = span * 0.08;
    (lo - pad, hi + pad)
}

/// Generate airfoil cross-section geometry comparison (Initial vs Optimized).
pub fn figure_airfoil_comparison(
    initial_coords: &[(f64, f64)],
    opt_coords: &[(f64, f64)],
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(600.0, 350.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Airfoil Section Shape Comparison (Initial vs Optimized)".to_owned());
    draw_title(
        &mut scene,
        "Airfoil Section Shape Comparison (Initial vs Optimized)",
        pal,
    );
    scene.suppress_derived_title();

    let axes =
        Axes2D::new((60.0, 40.0, 480.0, 250.0), (-0.05, 1.05), (-0.2, 0.2)).with_equal_aspect();
    axes.draw_frame_with_labels(&mut scene, pal, "x/c", "y/c");

    // Initial baseline airfoil (dashed blue).
    let init_stroke = Stroke::dashed(Color::from_hex(BASELINE_COLOR), 1.8, 4.0, 4.0);
    axes.add_line_series(&mut scene, initial_coords, init_stroke);

    // Optimized airfoil (solid red).
    let opt_stroke = Stroke::new(Color::from_hex(OPTIMIZED_COLOR), 2.0);
    axes.add_line_series(&mut scene, opt_coords, opt_stroke);

    draw_legend(
        &mut scene,
        [axes.left + 8.0, axes.top + 8.0],
        &[
            (
                "Initial".to_owned(),
                LegendMarker::Line(Stroke::dashed(
                    Color::from_hex(BASELINE_COLOR),
                    1.8,
                    4.0,
                    4.0,
                )),
            ),
            (
                "Optimized".to_owned(),
                LegendMarker::Line(Stroke::new(Color::from_hex(OPTIMIZED_COLOR), 2.0)),
            ),
        ],
        pal,
        9.0,
    );

    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::design_variables::DesignVector;

    #[test]
    fn history_plots_ld_scatter_span_colorbar_and_running_best() {
        let mut history = OptimizationHistory::new();
        let dv = DesignVector::default();
        for (ld, span) in [(12.0, 28.0), (15.0, 30.0), (13.0, 29.0)] {
            history.record(dv, true, 0.0, ld, span, 0.0, 0.0, 0.0, "");
        }

        let scene = figure_optimization_history(&history, Some("dark"));
        let circles = scene
            .elements
            .iter()
            .filter(|element| matches!(element, SceneElement::Circle { .. }))
            .count();
        let colorbar_cells = scene
            .elements
            .iter()
            .filter(|element| matches!(element, SceneElement::Rect { fill: Some(_), .. }))
            .count();
        let polylines = scene
            .elements
            .iter()
            .filter(|element| matches!(element, SceneElement::Polyline { .. }))
            .count();
        let labels: Vec<&str> = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(circles, 4, "three evaluations plus one legend marker");
        assert!(colorbar_cells >= 64, "Viridis colorbar cells are present");
        assert_eq!(polylines, 1, "running-best overlay is a line series");
        assert!(labels.contains(&"valid evaluation #"));
        assert!(labels.contains(&"L/D"));
        assert!(labels.contains(&"Span [m]"));
        assert!(labels.contains(&"Best so far"));
    }

    #[test]
    fn airfoil_comparison_has_a_palette_colored_visible_title() {
        let scene = figure_airfoil_comparison(
            &[(0.0, 0.0), (0.5, 0.1), (1.0, 0.0)],
            &[(0.0, 0.0), (0.5, 0.08), (1.0, 0.0)],
            Some("dark"),
        );
        assert!(!scene.render_title);
        assert!(scene.elements.iter().any(|element| {
            matches!(
                element,
                SceneElement::Text { text, color, .. }
                    if text == "Airfoil Section Shape Comparison (Initial vs Optimized)"
                        && *color == Color::from_hex("#ffffff")
            )
        }));
    }
}
