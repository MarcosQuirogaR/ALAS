// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py

//! Optimization history and airfoil shape evolution.

use crate::chart_kit::{draw_legend, draw_title, LegendMarker};
use crate::scene::{Axes2D, Color, Scene, Stroke};
use crate::theme::{get_palette, BASELINE_COLOR, OPTIMIZED_COLOR};

mod history;
pub use history::{
    figure_evaluation_trace, figure_optimization_history, figure_optimization_history_labelled,
    figure_optimization_run, BEST_VALID_LABEL, FAILED_LABEL, HISTORY_TITLE, HISTORY_X_LABEL,
    MUTED_DISPLAY_CAP, MUTED_RADIUS, REJECTED_LABEL, VALID_LABEL, VALID_RADIUS, VERIFIED_RADIUS,
};

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
    use crate::scene::SceneElement;

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
