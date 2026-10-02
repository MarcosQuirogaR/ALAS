// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py

//! Optimization convergence history and airfoil shape evolution.

use crate::chart_kit::{draw_colorbar, draw_legend, draw_title, LegendMarker};
use crate::colormap::Colormap;
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::{get_palette, BASELINE_COLOR, OPTIMIZED_COLOR};
use alas_opt::history::OptimizationHistory;
use alas_pipeline::optimizer_summary::{OptimizerRunSummary, SCOPE_LABEL};

/// The objective value of every valid evaluation, colored by span, with
/// the running best: the mission quantity the search minimises where the
/// native objective recorded one, and the ranking cost otherwise (a
/// delegated evaluator reports only that).
pub fn figure_optimization_history(history: &OptimizationHistory, theme: Option<&str>) -> Scene {
    figure_optimization_history_labelled(history, None, theme)
}

/// [`figure_optimization_history`] with the run's scope, outcome, budget,
/// evaluations, termination, wall time, seed and same-model baseline delta
/// written under the plot when `summary` is given.
pub fn figure_optimization_history_labelled(
    history: &OptimizationHistory,
    summary: Option<&OptimizerRunSummary>,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let height = summary.map_or(480.0, labelled_height);
    let mut scene = Scene::new(720.0, height, Some(Color::from_hex(pal.bg)));
    if let Some(summary) = summary {
        draw_run_label(&mut scene, summary, pal);
    }
    let valid: Vec<usize> = (0..history.n_evaluations())
        .filter(|&index| history.valid.get(index).copied().unwrap_or(false))
        .collect();
    let n_eval = valid.len();
    let title = format!("Optimization convergence ({n_eval} valid evaluations)");
    scene.title = Some(title.clone());
    draw_title(&mut scene, &title, pal);
    scene.suppress_derived_title();

    if n_eval == 0 {
        return scene;
    }

    let objective: Vec<f64> = valid
        .iter()
        .map(|&index| {
            history
                .objective_value
                .get(index)
                .copied()
                .filter(|value| value.is_finite())
                .unwrap_or_else(|| history.cost.get(index).copied().unwrap_or(f64::NAN))
        })
        .collect();
    let spans: Vec<f64> = valid
        .iter()
        .map(|&index| history.span_m.get(index).copied().unwrap_or(0.0))
        .collect();
    let (y_min, y_max) = padded_range(&objective);
    let (span_min, span_max) = finite_range(&spans);
    let axes = Axes2D::new(
        (64.0, 42.0, 520.0, 350.0),
        (0.5, n_eval as f64 + 0.5),
        (y_min, y_max),
    );
    axes.draw_frame_with_labels(&mut scene, pal, "valid evaluation #", "objective");

    let cmap = Colormap::Viridis;
    let span_delta = (span_max - span_min).max(1e-12);
    for (index, &value) in objective.iter().enumerate() {
        if !value.is_finite() {
            continue;
        }
        let color = cmap.sample((spans[index] - span_min) / span_delta);
        let point = axes.map_point(index as f64 + 1.0, value);
        scene.add(SceneElement::Circle {
            center: point,
            radius: 3.5,
            fill: Some(Fill::new(Color::rgba(color.r, color.g, color.b, 179))),
            stroke: None,
        });
    }

    let mut best = f64::INFINITY;
    let mut best_so_far = Vec::with_capacity(n_eval);
    for (index, &value) in objective.iter().enumerate() {
        if value.is_finite() {
            best = best.min(value);
        }
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

/// Top of the run label under the plot, px.
const RUN_LABEL_TOP_Y: f64 = 445.0;
/// Row pitch of the run label, px.
const RUN_LABEL_ROW_PITCH: f64 = 14.0;

/// Write the scope line and the run lines in two columns under the plot.
fn draw_run_label(scene: &mut Scene, summary: &OptimizerRunSummary, pal: &crate::theme::Palette) {
    const TOP_Y: f64 = RUN_LABEL_TOP_Y;
    const ROW_PITCH: f64 = RUN_LABEL_ROW_PITCH;
    scene.add(SceneElement::Text {
        text: SCOPE_LABEL.to_owned(),
        pos: [64.0, TOP_Y],
        font_size: 11.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: true,
    });
    let (short, long) = run_label_lines(summary);
    let per_column = short.len().div_ceil(2);
    for (index, text) in short.into_iter().enumerate() {
        let (column, row) = (index / per_column, index % per_column);
        scene.add(SceneElement::Text {
            text,
            pos: [
                64.0 + 320.0 * column as f64,
                TOP_Y + ROW_PITCH * (row as f64 + 1.5),
            ],
            font_size: 10.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }
    for (index, text) in long.into_iter().enumerate() {
        scene.add(SceneElement::TextBlock {
            text,
            pos: [
                64.0,
                TOP_Y + ROW_PITCH * (per_column as f64 + 1.0 + 2.0 * index as f64),
            ],
            width: 620.0,
            font_size: 10.0,
            color: Color::from_hex(pal.tick),
            bold: false,
        });
    }
}

/// The run label's `label: value` lines: those that fit a half-width column,
/// and the longer ones, each given a full-width two-row block.
fn run_label_lines(summary: &OptimizerRunSummary) -> (Vec<String>, Vec<String>) {
    const HALF_WIDTH_CHARACTERS: usize = 56;
    summary
        .label_lines()
        .into_iter()
        .map(|(label, value)| format!("{label}: {value}"))
        .partition(|line| line.len() <= HALF_WIDTH_CHARACTERS)
}

/// Height of the labelled figure for `summary`, px.
fn labelled_height(summary: &OptimizerRunSummary) -> f64 {
    let (short, long) = run_label_lines(summary);
    let rows = short.len().div_ceil(2) + 2 * long.len();
    RUN_LABEL_TOP_Y + RUN_LABEL_ROW_PITCH * (rows as f64 + 2.5)
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
    fn history_plots_objective_scatter_span_colorbar_and_running_best() {
        let mut history = OptimizationHistory::new();
        let dv = DesignVector::default();
        for (fuel, span) in [(9_000.0, 28.0), (8_500.0, 30.0), (8_800.0, 29.0)] {
            history.record_mission_sized(
                dv, true, 0.5, 17.0, span, 2.0, 400.0, 1.0, "", fuel, 80_000.0, fuel, 0.0, 0.0,
            );
        }
        // An invalid evaluation is not plotted.
        history.record(dv, false, 1.0e6, 0.0, 0.0, 0.0, 0.0, 0.0, "trim_solve");

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
        assert!(labels.contains(&"objective"));
        assert!(labels.contains(&"Span [m]"));
        assert!(labels.contains(&"Best so far"));
    }

    #[test]
    fn labelled_history_states_scope_outcome_budget_seed_and_baseline_delta() {
        let mut history = OptimizationHistory::new();
        let dv = DesignVector::default();
        for fuel in [9_000.0, 8_500.0] {
            history.record_mission_sized(
                dv, true, fuel, 17.0, 30.0, 2.0, 400.0, 1.0, "", fuel, 80_000.0, fuel, 0.0, 0.0,
            );
        }
        let result = alas_opt::OptimizationResult {
            best_design: dv,
            best_cost: 8_500.0,
            best_valid: false,
            history: history.clone(),
            wall_time_s: 4.0,
            method: "differential_evolution".to_owned(),
            strategy: String::new(),
            termination: "evaluation_budget".to_owned(),
            pareto_front: Vec::new(),
            search_diagnostics: None,
            delivered_acceptance: None,
        };
        let mut config = alas_config::AlasConfig::default();
        config.optimizer.solver.seed = Some(3);
        let mut summary = OptimizerRunSummary::from_result(&result, &config, None);
        summary.stages = ["screening", "refinement"]
            .map(|stage| alas_opt::StageSummary {
                stage: stage.to_owned(),
                max_evaluations: 100,
                planned_evaluations: 100,
                reserved_evaluations: 0,
                time_limit_s: 30.0,
                time_limited: true,
                evaluations: 64,
                restoration_evaluations: 0,
                cancelled_unstarted: 0,
                pre_gate_rejects: 0,
                analysis_evaluations: 64,
                generations: 1,
                feasible: 10,
                elite_size: 4,
                wall_time_s: 31.0,
                candidate_time_s: 1.0,
                lane_utilization: 0.5,
                termination: "time_budget".to_owned(),
                sizing_work: None,
            })
            .to_vec();
        let scene = figure_optimization_history_labelled(&history, Some(&summary), Some("light"));
        let labels: Vec<String> = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, .. } | SceneElement::TextBlock { text, .. } => {
                    Some(text.clone())
                }
                _ => None,
            })
            .collect();
        assert!(labels.iter().any(|label| label == SCOPE_LABEL));
        for (label, value) in summary.label_lines() {
            let line = format!("{label}: {value}");
            assert!(labels.contains(&line), "missing {line}: {labels:?}");
        }
        assert!(labels.iter().any(|label| label == "Random seed: 3"));
        assert!(!labels.iter().any(|label| label == "Outcome: Completed"));
        // Every row sits inside the figure.
        let bottom = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { pos, .. } | SceneElement::TextBlock { pos, .. } => {
                    Some(pos[1])
                }
                _ => None,
            })
            .fold(0.0, f64::max);
        assert!(bottom < scene.height, "{bottom} vs {}", scene.height);
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
