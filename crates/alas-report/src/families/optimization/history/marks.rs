// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The marks of the optimization history: stage boundaries, the muted
//! scatter and its strip, the valid scatter, the running best, the x axis
//! and the legend row.

use super::*;

pub(super) fn x_of(index: usize) -> f64 {
    index as f64 + 1.0
}

/// `value` mapped into the frame, clamped onto its edge when outside.
pub(super) fn clamped_point(axes: &Axes2D, index: usize, value: f64) -> [f64; 2] {
    axes.map_point(x_of(index), value.clamp(axes.y_min, axes.y_max))
}

/// Stage segments as `(stage, first index, past-the-end index)`.
pub(super) fn stage_segments(trace: &EvaluationTrace) -> Vec<(TraceStage, usize, usize)> {
    let mut segments: Vec<(TraceStage, usize, usize)> = Vec::new();
    for (index, evaluation) in trace.evaluations.iter().enumerate() {
        match segments.last_mut() {
            Some(last) if last.0 == evaluation.stage => last.2 = index + 1,
            _ => segments.push((evaluation.stage, index, index + 1)),
        }
    }
    segments
}

pub(super) fn draw_stage_boundaries(
    scene: &mut Scene,
    axes: &Axes2D,
    trace: &EvaluationTrace,
    pal: &Palette,
) {
    let segments = stage_segments(trace);
    let spine = Color::from_hex(pal.spine);
    let tick = Color::from_hex(pal.tick);
    let separator = Stroke::new(Color::rgba(spine.r, spine.g, spine.b, 150), 0.8);
    for &(_, start, _) in segments.iter().skip(1) {
        let x = axes.map_point(start as f64 + 0.5, axes.y_min)[0];
        scene.add(SceneElement::Line {
            p1: [x, TAG_Y + 4.0],
            p2: [x, PLOT_BOTTOM],
            stroke: separator.clone(),
        });
    }
    let color = Color::rgba(tick.r, tick.g, tick.b, 190);
    for (left, y, labels) in place_stage_tags(axes, &segments) {
        let mut x = left;
        for (index, label) in labels.iter().enumerate() {
            if index > 0 {
                add_tag(scene, TAG_JOIN, [x + tag_width(" "), y], color);
                x += tag_width(" / ");
            }
            add_tag(scene, label, [x, y], color);
            x += tag_width(label);
        }
    }
}

/// Separator between the tags of stages merged into one group.
pub(super) const TAG_JOIN: &str = "/";

/// Estimated width of a tag, px.
pub(super) fn tag_width(label: &str) -> f64 {
    label.chars().count() as f64 * TAG_FONT * 0.62
}

fn add_tag(scene: &mut Scene, text: &str, pos: [f64; 2], color: Color) {
    scene.add(SceneElement::Text {
        text: text.to_owned(),
        pos,
        font_size: TAG_FONT,
        color,
        align: TextAlign::Left,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });
}

/// Stage tags as `(left x, y, labels)` groups.
///
/// A tag starts just right of its separator. Consecutive stages too narrow
/// for their own tags share one group, anchored at the first one's
/// separator. A group that does not fit to the right of its anchor ends
/// just left of it instead, so every group touches its separator; one that
/// would then overlap the previous group drops to a second row inside the
/// frame.
pub(super) fn place_stage_tags(
    axes: &Axes2D,
    segments: &[(TraceStage, usize, usize)],
) -> Vec<(f64, f64, Vec<&'static str>)> {
    let x_at = |index: usize| axes.map_point(index as f64 + 0.5, axes.y_min)[0];
    let mut groups: Vec<(f64, Vec<&'static str>)> = Vec::new();
    let mut previous_narrow = false;
    for (position, &(stage, start, end)) in segments.iter().enumerate() {
        let anchor = if position == 0 {
            axes.left
        } else {
            x_at(start)
        };
        let label = stage.label();
        let narrow = x_at(end) - anchor < tag_width(label) + 8.0;
        match groups.last_mut() {
            Some((_, labels)) if narrow && previous_narrow => labels.push(label),
            _ => groups.push((anchor, vec![label])),
        }
        previous_narrow = narrow;
    }
    let right = axes.left + axes.width;
    let mut previous_end = f64::NEG_INFINITY;
    groups
        .into_iter()
        .map(|(anchor, labels)| {
            let width = labels.iter().map(|label| tag_width(label)).sum::<f64>()
                + (labels.len() - 1) as f64 * tag_width(" / ");
            let after = anchor + 3.0;
            let before = anchor - 3.0 - width;
            let (left, y) = if after + width <= right && after >= previous_end + 6.0 {
                (after, TAG_Y)
            } else if before >= previous_end + 6.0 && before >= axes.left {
                (before, TAG_Y)
            } else {
                return (
                    (after.min(right - width)).max(axes.left),
                    PLOT_TOP + 9.0,
                    labels,
                );
            };
            previous_end = left + width;
            (left, y, labels)
        })
        .collect()
}

/// Muted markers, thinned to one per class and grid cell.
pub(super) fn draw_muted_points(
    scene: &mut Scene,
    axes: &Axes2D,
    trace: &EvaluationTrace,
    values: &[Option<f64>],
    colors: &Colors,
) {
    let muted: Vec<(TraceClass, [f64; 2])> = trace
        .evaluations
        .iter()
        .zip(values)
        .enumerate()
        .filter(|(_, (evaluation, _))| {
            evaluation.class != TraceClass::Valid && evaluation.stage != TraceStage::Verification
        })
        .filter_map(|(index, (evaluation, value))| {
            value.map(|value| (evaluation.class, clamped_point(axes, index, value)))
        })
        .collect();
    for (class, center) in thin(&muted, MUTED_DISPLAY_CAP) {
        scene.add(SceneElement::Circle {
            center,
            radius: MUTED_RADIUS,
            fill: Some(Fill::new(colors.of(class))),
            stroke: None,
        });
    }
}

/// `points` thinned to one per class and square grid cell, the grid
/// coarsening until at most `cap` remain; the first point of each cell, in
/// request order, is kept.
pub(super) fn thin(points: &[(TraceClass, [f64; 2])], cap: usize) -> Vec<(TraceClass, [f64; 2])> {
    let mut kept = Vec::new();
    for cell in [1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0] {
        let mut seen = HashSet::new();
        kept = points
            .iter()
            .copied()
            .filter(|(class, [x, y])| {
                seen.insert((*class, (x / cell).floor() as i64, (y / cell).floor() as i64))
            })
            .collect();
        if kept.len() <= cap {
            break;
        }
    }
    kept
}

/// Ticks for records with no finite value: rejected and valid in the upper
/// half of the strip, failed in the lower, one per pixel column and class.
pub(super) fn draw_strip(
    scene: &mut Scene,
    axes: &Axes2D,
    trace: &EvaluationTrace,
    values: &[Option<f64>],
    colors: &Colors,
    pal: &Palette,
) {
    let top = PLOT_BOTTOM - STRIP_HEIGHT;
    let spine = Color::from_hex(pal.spine);
    scene.add(SceneElement::Rect {
        x: axes.left,
        y: top,
        width: axes.width,
        height: STRIP_HEIGHT,
        rx: 0.0,
        fill: Some(Fill::new(Color::rgba(spine.r, spine.g, spine.b, 24))),
        stroke: None,
    });
    let mut seen = HashSet::new();
    for (index, (evaluation, value)) in trace.evaluations.iter().zip(values).enumerate() {
        if value.is_some() {
            continue;
        }
        let x = axes.map_point(x_of(index), axes.y_min)[0];
        if !seen.insert((evaluation.class, x.round() as i64)) {
            continue;
        }
        let (y0, y1) = if evaluation.class == TraceClass::Failed {
            (top + STRIP_HEIGHT * 0.5 + 1.0, top + STRIP_HEIGHT - 1.0)
        } else {
            (top + 1.0, top + STRIP_HEIGHT * 0.5 - 1.0)
        };
        let color = colors.of(evaluation.class);
        scene.add(SceneElement::Line {
            p1: [x, y0],
            p2: [x, y1],
            stroke: Stroke::new(
                Color::rgba(color.r, color.g, color.b, color.a.max(140)),
                1.0,
            ),
        });
    }
}

pub(super) fn draw_valid_points(
    scene: &mut Scene,
    axes: &Axes2D,
    trace: &EvaluationTrace,
    values: &[Option<f64>],
    colors: &Colors,
) {
    for (index, (evaluation, value)) in trace.evaluations.iter().zip(values).enumerate() {
        if evaluation.stage == TraceStage::Verification {
            continue;
        }
        if let (TraceClass::Valid, Some(value)) = (evaluation.class, value) {
            scene.add(SceneElement::Circle {
                center: axes.map_point(x_of(index), *value),
                radius: VALID_RADIUS,
                fill: Some(Fill::new(colors.valid)),
                stroke: None,
            });
        }
    }
}

/// Re-evaluations of finalists the search already scored, drawn as rings:
/// accent when accepted, muted otherwise.
pub(super) fn draw_verification_points(
    scene: &mut Scene,
    axes: &Axes2D,
    trace: &EvaluationTrace,
    values: &[Option<f64>],
    colors: &Colors,
) {
    for (index, (evaluation, value)) in trace.evaluations.iter().zip(values).enumerate() {
        let Some(value) = value.filter(|_| evaluation.stage == TraceStage::Verification) else {
            continue;
        };
        scene.add(SceneElement::Circle {
            center: clamped_point(axes, index, value),
            radius: VERIFIED_RADIUS,
            fill: None,
            stroke: Some(Stroke::new(legend_swatch(colors.of(evaluation.class)), 1.4)),
        });
    }
}

/// The search records of `class`: verifications re-evaluate designs the
/// search already counted, so they are not counted again.
pub(super) fn search_count(trace: &EvaluationTrace, class: TraceClass) -> usize {
    trace
        .evaluations
        .iter()
        .filter(|evaluation| {
            evaluation.class == class && evaluation.stage != TraceStage::Verification
        })
        .count()
}

/// The running best over valid records, as step lines; whether any was
/// drawn. A separately modelled screening stage has its own dashed line.
pub(super) fn draw_best_valid(
    scene: &mut Scene,
    axes: &Axes2D,
    trace: &EvaluationTrace,
    values: &[Option<f64>],
    n: usize,
    colors: &Colors,
) -> bool {
    let runs = best_valid_runs(trace, values, n);
    for run in &runs {
        scene.add(SceneElement::Polyline {
            points: run
                .steps
                .iter()
                .map(|&(x, value)| axes.map_point(x, value))
                .collect(),
            stroke: if run.screening {
                colors.screening_best.clone()
            } else {
                colors.best.clone()
            },
        });
    }
    !runs.is_empty()
}

/// One running-best step line.
pub(super) struct BestRun {
    /// Whether it is the running best of a separately modelled screening.
    pub(super) screening: bool,
    /// Vertices `(x, best)`.
    pub(super) steps: Vec<(f64, f64)>,
}

/// The running best over valid records, ending at record `n`.
///
/// When the screening stage scored with its own (cheaper) model its values
/// are not the refinement's, so its running best is a run of its own and
/// the refinement's restarts at the first record after it: the main line
/// then never holds a value no full-model design reached.
pub(super) fn best_valid_runs(
    trace: &EvaluationTrace,
    values: &[Option<f64>],
    n: usize,
) -> Vec<BestRun> {
    let split = |evaluation: &TracedEvaluation| {
        trace.screening_separate && evaluation.stage == TraceStage::Screening
    };
    let mut runs = Vec::new();
    let mut screening = trace.evaluations.first().is_some_and(split);
    let mut best = f64::INFINITY;
    let mut steps: Vec<(f64, f64)> = Vec::new();
    let mut close = |screening: bool, best: f64, mut steps: Vec<(f64, f64)>, end: f64| {
        if best.is_finite() {
            steps.push((end, best));
            runs.push(BestRun { screening, steps });
        }
    };
    for (index, (evaluation, value)) in trace.evaluations.iter().zip(values).enumerate() {
        if split(evaluation) != screening {
            close(
                screening,
                best,
                std::mem::take(&mut steps),
                x_of(index.saturating_sub(1)),
            );
            screening = split(evaluation);
            best = f64::INFINITY;
        }
        let Some(value) = value.filter(|_| evaluation.class == TraceClass::Valid) else {
            continue;
        };
        if value < best {
            if best.is_finite() {
                steps.push((x_of(index), best));
            }
            steps.push((x_of(index), value));
            best = value;
        }
    }
    close(screening, best, steps, x_of(n.saturating_sub(1)));
    runs
}

/// The rotated y label, clear of the widest y tick label.
pub(super) fn draw_y_label(scene: &mut Scene, axes: &Axes2D, pal: &Palette, label: &str) {
    let step = tick_step(axes.y_min, axes.y_max, Scale::Linear, 6);
    let widest = major_ticks(axes.y_min, axes.y_max, Scale::Linear, 6)
        .into_iter()
        .map(|value| format_tick(value, step, None).chars().count())
        .max()
        .unwrap_or(1) as f64;
    let offset = 7.0 + widest * 8.5 * 0.65 + 10.0;
    scene.add(SceneElement::Text {
        text: label.to_owned(),
        pos: [(axes.left - offset).max(8.0), axes.top + axes.height * 0.5],
        font_size: 10.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: -90.0,
        bold: false,
    });
}

pub(super) fn draw_x_axis(scene: &mut Scene, axes: &Axes2D, pal: &Palette) {
    let spine = Color::from_hex(pal.spine);
    let tick = Color::from_hex(pal.tick);
    let step = tick_step(axes.x_min, axes.x_max, Scale::Linear, 6);
    for value in major_ticks(axes.x_min, axes.x_max, Scale::Linear, 6) {
        let x = axes.map_point(value, axes.y_min)[0];
        scene.add(SceneElement::Line {
            p1: [x, PLOT_BOTTOM],
            p2: [x, PLOT_BOTTOM + 3.0],
            stroke: Stroke::new(spine, 1.0),
        });
        scene.add(SceneElement::Text {
            text: format_tick(value, step, None),
            pos: [x, PLOT_BOTTOM + 5.0],
            font_size: 8.5,
            color: tick,
            align: TextAlign::Center,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: false,
        });
    }
    scene.add(SceneElement::Text {
        text: HISTORY_X_LABEL.to_owned(),
        pos: [axes.left + axes.width * 0.5, PLOT_BOTTOM + 20.0],
        font_size: 10.0,
        color: tick,
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
}

pub(super) fn draw_legend_row(
    scene: &mut Scene,
    trace: &EvaluationTrace,
    has_best: bool,
    colors: &Colors,
    pal: &Palette,
) {
    let mut entries = vec![
        (
            format!("{VALID_LABEL} ({})", search_count(trace, TraceClass::Valid)),
            LegendMarker::Circle(colors.valid),
        ),
        (
            format!(
                "{REJECTED_LABEL} ({})",
                search_count(trace, TraceClass::Rejected)
            ),
            LegendMarker::Circle(legend_swatch(colors.rejected)),
        ),
        (
            format!(
                "{FAILED_LABEL} ({})",
                search_count(trace, TraceClass::Failed)
            ),
            LegendMarker::Circle(legend_swatch(colors.failed)),
        ),
    ];
    if has_best {
        entries.push((
            BEST_VALID_LABEL.to_owned(),
            LegendMarker::Line(colors.best.clone()),
        ));
    }
    draw_horizontal_legend_columns(
        scene,
        [PLOT_LEFT, PLOT_BOTTOM + 44.0],
        &entries,
        pal,
        LEGEND_FONT,
    );
}

/// A muted class color strong enough to read as a legend swatch.
fn legend_swatch(color: Color) -> Color {
    Color::rgba(color.r, color.g, color.b, color.a.max(170))
}
