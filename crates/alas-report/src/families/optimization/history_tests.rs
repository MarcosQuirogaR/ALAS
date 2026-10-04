// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;
use alas_config::design_variables::DesignVector;

/// A deterministic run of `n` requests: a screening quarter, a refinement,
/// a short restoration and three verifications. Roughly one request in
/// eight is valid, one in ten failed, and a share of the rejections never
/// reached analysis.
fn synthetic_trace(n: usize) -> EvaluationTrace {
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let mut uniform = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut trace = EvaluationTrace::default();
    for index in 0..n {
        let stage = if index < n / 4 {
            TraceStage::Screening
        } else if index + 40 < n {
            TraceStage::Refinement
        } else if index + 3 < n {
            TraceStage::Restoration
        } else {
            TraceStage::Verification
        };
        let progress = index as f64 / n as f64;
        let fuel = 7_000.0 + 4_000.0 * (1.0 - progress) * uniform() + 300.0 * uniform();
        let draw = uniform();
        let (class, objective) = if draw < 0.125 || stage == TraceStage::Verification {
            (TraceClass::Valid, Some(fuel as f32))
        } else if draw < 0.225 {
            (TraceClass::Failed, None)
        } else if draw < 0.4 {
            (TraceClass::Rejected, None)
        } else {
            (TraceClass::Rejected, Some((fuel * 0.85) as f32))
        };
        trace.push(
            stage,
            class,
            objective,
            objective.map(|value| value / 9_000.0),
        );
    }
    trace
}

fn circles(scene: &Scene, radius: f64) -> Vec<(Color, [f64; 2])> {
    scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::Circle {
                center,
                radius: r,
                fill: Some(fill),
                ..
            } if (*r - radius).abs() < 1e-12 => Some((fill.color, *center)),
            _ => None,
        })
        .collect()
}

fn texts(scene: &Scene) -> Vec<(String, [f64; 2])> {
    scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::Text { text, pos, .. } => Some((text.clone(), *pos)),
            _ => None,
        })
        .collect()
}

fn best_line(scene: &Scene) -> Vec<[f64; 2]> {
    scene
        .elements
        .iter()
        .find_map(|element| match element {
            SceneElement::Polyline { points, stroke }
                if stroke.color == Color::from_hex(OPTIMIZED_COLOR) =>
            {
                Some(points.clone())
            }
            _ => None,
        })
        .unwrap_or_default()
}

#[test]
fn every_class_is_counted_and_only_valid_candidates_are_highlighted() {
    let trace = synthetic_trace(2_000);
    for theme in ["light", "dark"] {
        let scene = figure_evaluation_trace(&trace, Some(ObjectiveKind::BlockFuel), Some(theme));
        let labels: Vec<String> = texts(&scene).into_iter().map(|(text, _)| text).collect();
        for (label, class) in [
            (VALID_LABEL, TraceClass::Valid),
            (REJECTED_LABEL, TraceClass::Rejected),
            (FAILED_LABEL, TraceClass::Failed),
        ] {
            let expected = format!("{label} ({})", search_count(&trace, class));
            assert!(labels.contains(&expected), "{expected} in {labels:?}");
        }
        assert!(labels.iter().any(|label| label == BEST_VALID_LABEL));
        assert!(labels.iter().any(|label| label == "Block fuel [kg]"));

        let accent = Color::from_hex(get_palette(Some(theme)).accent);
        let valid = circles(&scene, VALID_RADIUS);
        assert_eq!(valid.len(), search_count(&trace, TraceClass::Valid));
        assert!(valid
            .iter()
            .all(|(color, _)| (color.r, color.g, color.b) == (accent.r, accent.g, accent.b)));
        let muted = circles(&scene, MUTED_RADIUS);
        assert!(!muted.is_empty());
        assert!(muted.iter().all(|(color, _)| color.a < 128));
        // The strip holds a tick for the requests with no objective.
        let ticks = scene
            .elements
            .iter()
            .filter(|element| {
                matches!(element, SceneElement::Line { p1, p2, .. }
                if p1[0] == p2[0] && p1[1] >= PLOT_BOTTOM - STRIP_HEIGHT && p2[1] <= PLOT_BOTTOM)
            })
            .count();
        assert!(ticks > 0);
    }
}

#[test]
fn the_running_best_is_monotone_and_ends_at_the_best_valid_value() {
    let trace = synthetic_trace(3_000);
    let values: Vec<Option<f64>> = trace
        .evaluations
        .iter()
        .map(|evaluation| evaluation.objective.map(f64::from))
        .collect();
    let runs = best_valid_runs(&trace, &values, trace.len());
    assert_eq!(runs.len(), 1, "one model scored every stage");
    assert!(!runs[0].screening);
    let steps = &runs[0].steps;
    assert!(steps.windows(2).all(|pair| pair[1].1 <= pair[0].1));
    assert!(steps.windows(2).all(|pair| pair[1].0 >= pair[0].0));
    let best_valid = trace
        .evaluations
        .iter()
        .filter(|evaluation| evaluation.class == TraceClass::Valid)
        .filter_map(|evaluation| evaluation.objective.map(f64::from))
        .fold(f64::INFINITY, f64::min);
    let best_any = values
        .iter()
        .flatten()
        .copied()
        .fold(f64::INFINITY, f64::min);
    assert!(
        best_any < best_valid,
        "a rejected design is better than any valid one"
    );
    assert_eq!(steps.last().map(|step| step.1), Some(best_valid));
    let scene = figure_evaluation_trace(&trace, Some(ObjectiveKind::BlockFuel), Some("light"));
    let line = best_line(&scene);
    assert!(line.windows(2).all(|pair| pair[1][1] >= pair[0][1] - 1e-9));
}

#[test]
fn stages_are_separated_and_tagged_inside_the_frame() {
    let trace = synthetic_trace(1_000);
    let scene = figure_evaluation_trace(&trace, Some(ObjectiveKind::BlockFuel), Some("dark"));
    let texts = texts(&scene);
    let tags: Vec<&(String, [f64; 2])> = texts
        .iter()
        .filter(|(text, _)| TraceStage::ALL.iter().any(|stage| stage.label() == text))
        .collect();
    assert_eq!(tags.len(), 4);
    // Tags are ordered left to right and do not overlap.
    for pair in tags.windows(2) {
        assert!(
            pair[0].1[0] + tag_width(&pair[0].0) < pair[1].1[0],
            "{pair:?}"
        );
    }
    let last = tags[3];
    assert!(last.1[0] + tag_width(&last.0) <= PLOT_RIGHT + 1e-9);
    let separators = scene
        .elements
        .iter()
        .filter(|element| {
            matches!(element, SceneElement::Line { p1, p2, .. }
            if p1[0] == p2[0] && p1[1] < PLOT_TOP && p2[1] >= PLOT_BOTTOM)
        })
        .count();
    assert_eq!(separators, 3);
}

#[test]
fn every_stage_tag_stays_beside_its_own_separator_at_any_run_length() {
    for n in [60, 1_000, 40_000] {
        let trace = synthetic_trace(n);
        let scene = figure_evaluation_trace(&trace, Some(ObjectiveKind::BlockFuel), Some("light"));
        let separators: Vec<f64> = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Line { p1, p2, .. }
                    if p1[0] == p2[0] && p1[1] < PLOT_TOP && p2[1] >= PLOT_BOTTOM =>
                {
                    Some(p1[0])
                }
                _ => None,
            })
            .collect();
        let spans: Vec<(String, f64, f64)> = texts(&scene)
            .into_iter()
            .filter(|(text, _)| {
                text == TAG_JOIN || TraceStage::ALL.iter().any(|stage| stage.label() == text)
            })
            .map(|(text, pos)| {
                let width = tag_width(&text);
                (text, pos[0], pos[0] + width)
            })
            .collect();
        // Every separator has a tag touching it, on one side or the other,
        // except one merged into the previous narrow stage's group, which
        // lies a few stage widths from that group.
        let merged = spans.iter().any(|(text, _, _)| text == TAG_JOIN);
        for &x in &separators {
            let gap = spans
                .iter()
                .map(|(_, left, right)| (left - x).abs().min((x - right).abs()))
                .fold(f64::INFINITY, f64::min);
            let allowed = if merged { 40.0 } else { 6.0 };
            assert!(
                gap <= allowed,
                "{n}: separator at {x} is {gap} px from a tag"
            );
        }
        // A stage's tag lies within one merged group of its own boundary.
        let segments = stage_segments(&trace);
        for (index, &(stage, _, _)) in segments.iter().enumerate().skip(1) {
            let (_, left, right) = spans
                .iter()
                .find(|(text, _, _)| text == stage.label())
                .unwrap_or_else(|| panic!("{n}: no tag for {stage:?}"));
            let x = separators[index - 1];
            let distance = if x < *left {
                left - x
            } else {
                (x - right).max(0.0)
            };
            assert!(distance <= 80.0, "{n}: {stage:?} tag is {distance} px away");
        }
    }
}

#[test]
fn a_separate_screening_model_never_lends_its_values_to_the_refinement_best() {
    // Screening on a model biased 3 % low: its best is below anything the
    // refinement reaches.
    let mut trace = EvaluationTrace {
        screening_separate: true,
        ..EvaluationTrace::default()
    };
    for index in 0..300 {
        let (stage, value) = if index < 100 {
            (TraceStage::Screening, 9_700.0 - index as f32)
        } else {
            (TraceStage::Refinement, 10_200.0 - index as f32)
        };
        trace.push(stage, TraceClass::Valid, Some(value), Some(value / 9_000.0));
    }
    let values: Vec<Option<f64>> = trace
        .evaluations
        .iter()
        .map(|evaluation| evaluation.objective.map(f64::from))
        .collect();
    let runs = best_valid_runs(&trace, &values, trace.len());
    assert_eq!(runs.len(), 2);
    assert!(runs[0].screening && !runs[1].screening);
    let refinement_best = 10_200.0 - 299.0;
    assert!(runs[1]
        .steps
        .iter()
        .all(|&(x, best)| x > 100.0 && best >= refinement_best));
    assert_eq!(
        runs[1].steps.last().map(|step| step.1),
        Some(refinement_best)
    );
    assert!(runs[0].steps.iter().all(|&(x, _)| x <= 100.0));
    let scene = figure_evaluation_trace(&trace, Some(ObjectiveKind::BlockFuel), Some("dark"));
    let lines: Vec<&Stroke> = scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::Polyline { stroke, .. } => Some(stroke),
            _ => None,
        })
        .collect();
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().any(|stroke| stroke.dash_array.is_some()));
    // With one model the screening values are comparable and the line runs on.
    trace.screening_separate = false;
    let runs = best_valid_runs(&trace, &values, trace.len());
    assert_eq!(runs.len(), 1);
    assert_eq!(
        runs[0].steps.last().map(|step| step.1),
        Some(9_700.0 - 99.0)
    );
}

#[test]
fn verifications_are_rings_and_are_not_counted_twice() {
    let trace = synthetic_trace(500);
    let scene = figure_evaluation_trace(&trace, Some(ObjectiveKind::BlockFuel), Some("light"));
    let verifications = trace
        .evaluations
        .iter()
        .filter(|evaluation| evaluation.stage == TraceStage::Verification)
        .count();
    assert!(verifications > 0);
    let rings = scene
        .elements
        .iter()
        .filter(|element| {
            matches!(element, SceneElement::Circle { radius, fill: None, stroke: Some(_), .. }
                if (*radius - VERIFIED_RADIUS).abs() < 1e-12)
        })
        .count();
    assert_eq!(rings, verifications);
    let searched = search_count(&trace, TraceClass::Valid);
    assert_eq!(searched + verifications, trace.count(TraceClass::Valid));
    let labels: Vec<String> = texts(&scene).into_iter().map(|(text, _)| text).collect();
    assert!(labels.contains(&format!("{VALID_LABEL} ({searched})")));
    assert_eq!(circles(&scene, VALID_RADIUS).len(), searched);
}

#[test]
fn the_figure_fits_its_canvas_without_overflow_or_an_empty_band() {
    for n in [1, 7, 500, 40_000] {
        let trace = synthetic_trace(n);
        for theme in ["light", "dark"] {
            let scene =
                figure_evaluation_trace(&trace, Some(ObjectiveKind::BlockFuel), Some(theme));
            let mut lowest: f64 = 0.0;
            for element in &scene.elements {
                let points: Vec<[f64; 2]> = match element {
                    SceneElement::Circle { center, radius, .. } => vec![
                        [center[0] - radius, center[1] - radius],
                        [center[0] + radius, center[1] + radius],
                    ],
                    SceneElement::Line { p1, p2, .. } => vec![*p1, *p2],
                    SceneElement::Polyline { points, .. } => points.clone(),
                    SceneElement::Text { pos, font_size, .. } => {
                        vec![*pos, [pos[0], pos[1] + font_size]]
                    }
                    _ => Vec::new(),
                };
                for [x, y] in points {
                    assert!((0.0..=scene.width).contains(&x), "{n} {element:?}");
                    assert!((0.0..=scene.height).contains(&y), "{n} {element:?}");
                    lowest = lowest.max(y);
                }
            }
            assert!(
                lowest > scene.height - 20.0,
                "{n}: blank band below {lowest}"
            );
        }
    }
}

#[test]
fn forty_thousand_requests_draw_every_valid_point_and_thin_the_muted_ones() {
    let trace = synthetic_trace(40_000);
    let scene = figure_evaluation_trace(&trace, Some(ObjectiveKind::BlockFuel), Some("light"));
    let valid = circles(&scene, VALID_RADIUS);
    assert_eq!(valid.len(), search_count(&trace, TraceClass::Valid));
    let muted = circles(&scene, MUTED_RADIUS);
    let muted_with_value = trace
        .evaluations
        .iter()
        .filter(|evaluation| {
            evaluation.class != TraceClass::Valid && evaluation.objective.is_some()
        })
        .count();
    assert!(muted.len() <= MUTED_DISPLAY_CAP, "{}", muted.len());
    assert!(muted.len() < muted_with_value);
    let bound = MUTED_DISPLAY_CAP + valid.len() + 2_000;
    assert!(scene.elements.len() < bound, "{}", scene.elements.len());
}

#[test]
fn thinning_keeps_one_point_per_cell_and_class() {
    let points: Vec<(TraceClass, [f64; 2])> = (0..1_000)
        .map(|index| {
            let class = if index % 2 == 0 {
                TraceClass::Rejected
            } else {
                TraceClass::Failed
            };
            (class, [100.0 + (index % 10) as f64 * 0.01, 200.0])
        })
        .collect();
    let kept = thin(&points, 10);
    assert_eq!(kept.len(), 2);
    assert_ne!(kept[0].0, kept[1].0);
}

#[test]
fn a_mixed_or_unnamed_objective_falls_back_to_the_ranking_cost() {
    let mut trace = synthetic_trace(200);
    let scene = figure_evaluation_trace(&trace, None, Some("light"));
    assert!(texts(&scene).iter().any(|(text, _)| text == RANKING_LABEL));
    if let Some(valid) = trace
        .evaluations
        .iter_mut()
        .find(|evaluation| evaluation.class == TraceClass::Valid)
    {
        valid.objective = None;
    }
    let scene = figure_evaluation_trace(&trace, Some(ObjectiveKind::BlockFuel), Some("light"));
    let labels = texts(&scene);
    assert!(labels.iter().any(|(text, _)| text == RANKING_LABEL));
    assert!(!labels.iter().any(|(text, _)| text == "Block fuel [kg]"));
}

#[test]
fn a_bare_history_plots_its_rows_with_failures_in_the_strip() {
    let mut history = OptimizationHistory::new();
    let dv = DesignVector::default();
    for (cost, valid) in [(0.9, true), (1.1, false), (0.6, true), (0.8, true)] {
        history.record(dv, valid, cost, 17.0, 30.0, 2.0, 120.0, 1.0, "");
    }
    history.record(dv, false, 1.0e6, 0.0, 0.0, 0.0, 0.0, 0.0, "trim_solve");
    let scene = figure_optimization_history(&history, Some("dark"));
    assert_eq!(circles(&scene, VALID_RADIUS).len(), 3);
    let labels: Vec<String> = texts(&scene).into_iter().map(|(text, _)| text).collect();
    assert!(labels.contains(&format!("{VALID_LABEL} (3)")));
    assert!(labels.contains(&format!("{REJECTED_LABEL} (1)")));
    assert!(labels.contains(&format!("{FAILED_LABEL} (1)")));
    let line = best_line(&scene);
    assert_eq!(line.len(), 4, "start, one improvement step, end");
    assert!(crate::svg::render_svg(&scene).contains("<svg"));
}

#[test]
fn an_empty_trace_draws_only_the_title() {
    let scene = figure_evaluation_trace(&EvaluationTrace::default(), None, Some("light"));
    assert_eq!(scene.title.as_deref(), Some(HISTORY_TITLE));
    assert!(circles(&scene, VALID_RADIUS).is_empty());
}
