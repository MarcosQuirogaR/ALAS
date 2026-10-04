// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Optimization history: every candidate the run requested, stage by stage.
//!
//! The x axis is the request index across the whole run, stages in order,
//! with a light separator and a short tag at each stage boundary. Valid
//! candidates are drawn in the accent color at full size; rejected and failed
//! candidates are small and low-contrast. A candidate whose model produced no
//! finite objective is drawn as a tick in a strip below the plot, so the
//! count of everything explored stays visible. The running best is taken over
//! valid candidates only; when the screening stage scored with its own
//! cheaper model its running best is a separate dashed line and the main line
//! restarts at the refinement. Reporting-fidelity verifications re-evaluate
//! designs the search already counted: they are rings, outside the counts.
//!
//! Muted points are thinned for display on a pixel grid of the canvas
//! ([`MUTED_DISPLAY_CAP`]); valid points are never thinned, and every count
//! in the legend is of the full trace.

use std::collections::HashSet;

use crate::chart_kit::{
    draw_axes_without_x_tick_labels, draw_horizontal_legend_columns, draw_title, format_tick,
    major_ticks, tick_step, LegendMarker,
};
use crate::scene::{
    Axes2D, Color, Fill, Scale, Scene, SceneElement, Stroke, TextAlign, TextBaseline,
};
use crate::theme::{get_palette, Palette, OPTIMIZED_COLOR};
use alas_config::ObjectiveKind;
use alas_opt::{
    EvaluationTrace, OptimizationHistory, OptimizationResult, TraceClass, TraceStage,
    TracedEvaluation,
};
use alas_pipeline::optimizer_summary::{
    objective::{objective_label, RANKING_LABEL},
    OptimizerRunSummary,
};

/// Figure title.
pub const HISTORY_TITLE: &str = "Optimization History";
/// X-axis label.
pub const HISTORY_X_LABEL: &str = "Evaluation #";
/// Legend label of the valid candidates (followed by their count).
pub const VALID_LABEL: &str = "Valid";
/// Legend label of the rejected candidates (followed by their count).
pub const REJECTED_LABEL: &str = "Rejected";
/// Legend label of the failed analyses (followed by their count).
pub const FAILED_LABEL: &str = "Failed";
/// Legend label of the running best over valid candidates.
pub const BEST_VALID_LABEL: &str = "Best valid";

/// Most muted markers drawn; the thinning grid coarsens until the muted
/// points fit; fifteen thousand shapes convert to egui shapes in a few ms.
pub const MUTED_DISPLAY_CAP: usize = 15_000;
/// Marker radius of a valid candidate, px.
pub const VALID_RADIUS: f64 = 3.0;
/// Ring radius of a reporting-fidelity verification, px.
pub const VERIFIED_RADIUS: f64 = 4.5;
/// Marker radius of a rejected or failed candidate, px.
pub const MUTED_RADIUS: f64 = 1.5;

const WIDTH: f64 = 720.0;
const HEIGHT: f64 = 480.0;
const PLOT_LEFT: f64 = 78.0;
const PLOT_RIGHT: f64 = 698.0;
const PLOT_TOP: f64 = 40.0;
/// Bottom of the plot frame, or of the strip when the strip is drawn.
const PLOT_BOTTOM: f64 = 418.0;
const STRIP_HEIGHT: f64 = 14.0;
const STRIP_GAP: f64 = 3.0;
const TAG_Y: f64 = 32.0;
const TAG_FONT: f64 = 8.0;
const LEGEND_FONT: f64 = 9.0;

/// History of an optimization result: its recorded trace of every stage
/// when it has one, else the trace its history supports.
pub fn figure_optimization_run(
    result: &OptimizationResult,
    summary: Option<&OptimizerRunSummary>,
    theme: Option<&str>,
) -> Scene {
    figure_evaluation_trace(
        &result.evaluation_trace(),
        summary.map(|summary| summary.objective_kind),
        theme,
    )
}

/// History of the rows of `history` alone, on the ranking cost.
pub fn figure_optimization_history(history: &OptimizationHistory, theme: Option<&str>) -> Scene {
    figure_optimization_history_labelled(history, None, theme)
}

/// History of the rows of `history` alone, on the recorded physical
/// objective when `summary` names one.
pub fn figure_optimization_history_labelled(
    history: &OptimizationHistory,
    summary: Option<&OptimizerRunSummary>,
    theme: Option<&str>,
) -> Scene {
    figure_evaluation_trace(
        &EvaluationTrace::from_history(history),
        summary.map(|summary| summary.objective_kind),
        theme,
    )
}

/// The y value a record is plotted at.
#[derive(Clone, Copy)]
enum Quantity {
    Physical,
    Ranking,
}

impl Quantity {
    fn of(self, evaluation: &TracedEvaluation) -> Option<f64> {
        match self {
            Self::Physical => evaluation.objective,
            Self::Ranking => evaluation.cost,
        }
        .map(f64::from)
        .filter(|value| value.is_finite())
    }
}

/// The physical objective is plotted only when every valid search record
/// carries it; a mixed axis would put mass and a dimensionless cost on one
/// scale.
fn plotted_quantity(trace: &EvaluationTrace, kind: Option<ObjectiveKind>) -> Option<ObjectiveKind> {
    let mut valid = trace.evaluations.iter().filter(|evaluation| {
        evaluation.class == TraceClass::Valid && evaluation.stage != TraceStage::Verification
    });
    let first = valid.next()?;
    (first.objective.is_some() && valid.all(|evaluation| evaluation.objective.is_some()))
        .then_some(kind)
        .flatten()
}

/// The figure for `trace`, its y axis the physical objective `kind` when
/// every valid record carries it, else the ranking cost.
pub fn figure_evaluation_trace(
    trace: &EvaluationTrace,
    kind: Option<ObjectiveKind>,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(WIDTH, HEIGHT, Some(Color::from_hex(pal.bg)));
    scene.title = Some(HISTORY_TITLE.to_owned());
    draw_title(&mut scene, HISTORY_TITLE, pal);
    scene.suppress_derived_title();
    let n = trace.len();
    if n == 0 {
        return scene;
    }
    let physical = plotted_quantity(trace, kind);
    let quantity = physical.map_or(Quantity::Ranking, |_| Quantity::Physical);
    let values: Vec<Option<f64>> = trace
        .evaluations
        .iter()
        .map(|evaluation| quantity.of(evaluation))
        .collect();
    let has_strip = values.iter().any(Option::is_none);
    let frame_bottom = if has_strip {
        PLOT_BOTTOM - STRIP_HEIGHT - STRIP_GAP
    } else {
        PLOT_BOTTOM
    };
    let axes = Axes2D::new(
        (
            PLOT_LEFT,
            PLOT_TOP,
            PLOT_RIGHT - PLOT_LEFT,
            frame_bottom - PLOT_TOP,
        ),
        (0.5, n.max(5) as f64 + 0.5),
        y_range(trace, &values),
    );
    draw_axes_without_x_tick_labels(&axes, &mut scene, pal, None, None);
    draw_y_label(
        &mut scene,
        &axes,
        pal,
        physical.map_or(RANKING_LABEL, objective_label),
    );
    let colors = Colors::new(pal);
    draw_stage_boundaries(&mut scene, &axes, trace, pal);
    draw_muted_points(&mut scene, &axes, trace, &values, &colors);
    if has_strip {
        draw_strip(&mut scene, &axes, trace, &values, &colors, pal);
    }
    draw_valid_points(&mut scene, &axes, trace, &values, &colors);
    draw_verification_points(&mut scene, &axes, trace, &values, &colors);
    let has_best = draw_best_valid(&mut scene, &axes, trace, &values, n, &colors);
    draw_x_axis(&mut scene, &axes, pal);
    draw_legend_row(&mut scene, trace, has_best, &colors, pal);
    scene
}

struct Colors {
    valid: Color,
    rejected: Color,
    failed: Color,
    best: Stroke,
    /// The running best of a separately modelled screening stage.
    screening_best: Stroke,
}

impl Colors {
    fn new(pal: &Palette) -> Self {
        let tick = Color::from_hex(pal.tick);
        let accent = Color::from_hex(pal.accent);
        let best = Color::from_hex(OPTIMIZED_COLOR);
        Self {
            valid: Color::rgba(accent.r, accent.g, accent.b, 230),
            rejected: Color::rgba(tick.r, tick.g, tick.b, 70),
            failed: Color::rgba(196, 112, 92, 110),
            best: Stroke::new(best, 1.8),
            screening_best: Stroke::dashed(Color::rgba(best.r, best.g, best.b, 140), 1.3, 4.0, 3.0),
        }
    }

    fn of(&self, class: TraceClass) -> Color {
        match class {
            TraceClass::Valid => self.valid,
            TraceClass::Rejected => self.rejected,
            TraceClass::Failed => self.failed,
        }
    }
}

/// The y range: every valid value, and the central 96 % of the others so a
/// few wild misses do not flatten the search; misses beyond it sit on the
/// frame edge.
fn y_range(trace: &EvaluationTrace, values: &[Option<f64>]) -> (f64, f64) {
    let mut valid = Vec::new();
    let mut muted = Vec::new();
    for (evaluation, value) in trace.evaluations.iter().zip(values) {
        if let Some(value) = *value {
            if evaluation.class == TraceClass::Valid {
                valid.push(value);
            } else {
                muted.push(value);
            }
        }
    }
    muted.sort_by(f64::total_cmp);
    let quantile = |q: f64| {
        let index = ((muted.len() - 1) as f64 * q).round() as usize;
        muted[index]
    };
    let mut lo = valid.iter().copied().fold(f64::INFINITY, f64::min);
    let mut hi = valid.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !muted.is_empty() {
        lo = lo.min(quantile(0.02));
        hi = hi.max(quantile(0.98));
    }
    if !(lo.is_finite() && hi.is_finite()) {
        return (0.0, 1.0);
    }
    let span = (hi - lo).max(hi.abs().max(1.0) * 1e-3);
    let pad = span * 0.06;
    (lo - pad, hi + pad)
}

mod marks;
use marks::*;

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
