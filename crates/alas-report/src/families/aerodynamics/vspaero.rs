// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Figures built from VSPAERO's retained native products.
//!
//! A native polar is useful evidence even when the strict whole-aircraft
//! comparison gate rejects it.  These figures therefore consume the parsed
//! polar directly and label its comparison verdict instead of silently
//! dropping the external result from the live report.

use alas_aero::vspaero::VspaeroPolarPoint;
use alas_pipeline::VspaeroAnalysisResult;
#[cfg(test)]
use alas_pipeline::VspaeroComparisonStatus;

use super::support::padded_range;
use crate::chart_kit::{draw_horizontal_legend_columns, LegendMarker};
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

const NATIVE_COLOR: &str = "#d55e00";
const INDUCED_COLOR: &str = "#009e73";
const VISCOUS_COLOR: &str = "#56b4e9";
const REJECTED_COLOR: &str = "#d62728";
const ACCEPTED_COLOR: &str = "#27ae60";
const WAKE_TOLERANCE: f64 = 1.0e-4;

fn title(scene: &mut Scene, axes: &Axes2D, text: &str, color: Color) {
    scene.add(SceneElement::Text {
        text: text.to_owned(),
        pos: [axes.left, axes.top - 10.0],
        font_size: 10.0,
        color,
        align: TextAlign::Left,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
}

fn status_scene(title_text: &str, message: &str, ok: bool, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    const MESSAGE_TOP: f64 = 88.0;
    const LINE_HEIGHT: f64 = 17.0;
    const BOTTOM_MARGIN: f64 = 16.0;
    let wrapped = crate::chart_kit::wrap_text(message, 130);
    let line_count = wrapped.lines().count().max(1) as f64;
    let height = (300.0_f64).max(MESSAGE_TOP + line_count * LINE_HEIGHT + BOTTOM_MARGIN);
    let mut scene = Scene::new(900.0, height, Some(Color::from_hex(pal.bg)));
    scene.title = Some(title_text.to_owned());
    scene.suppress_derived_title();
    scene.add(SceneElement::Text {
        text: title_text.to_owned(),
        pos: [24.0, 42.0],
        font_size: 16.0,
        color: Color::from_hex(if ok { ACCEPTED_COLOR } else { REJECTED_COLOR }),
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: true,
    });
    scene.add(SceneElement::Text {
        text: wrapped,
        pos: [24.0, MESSAGE_TOP],
        font_size: 12.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
    scene
}

fn point_is_finite(point: &VspaeroPolarPoint) -> bool {
    [
        point.alpha_deg,
        point.lift_coefficient,
        point.induced_drag_coefficient,
        point.total_drag_coefficient,
        point.lift_to_drag,
        point.pitching_moment_coefficient,
    ]
    .into_iter()
    .all(f64::is_finite)
}

fn add_markers(scene: &mut Scene, axes: &Axes2D, points: &[(f64, f64)], color: Color) {
    for &(x, y) in points {
        scene.add(SceneElement::Circle {
            center: axes.map_point(x, y),
            radius: 2.4,
            fill: Some(Fill::new(color)),
            stroke: None,
        });
    }
}

/// Plot the parsed VSPAERO polar without requiring the strict overlay verdict.
///
/// The native result is deliberately kept separate from the ALAS model: a
/// rejected comparison remains visible and auditable, but it is never implied
/// to be an ALAS coefficient curve.
pub fn figure_vspaero_polar(result: &VspaeroAnalysisResult, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let Some(polar) = result.polar.as_ref() else {
        return status_scene(
            "VSPAERO native polar unavailable",
            result
                .error
                .as_deref()
                .unwrap_or("VSPAERO did not retain a parsed native polar."),
            false,
            theme,
        );
    };
    let points = polar
        .points
        .iter()
        .filter(|point| point_is_finite(point))
        .collect::<Vec<_>>();
    if points.is_empty() {
        return status_scene(
            "VSPAERO native polar unavailable",
            "The native polar was present but contained no finite coefficient rows.",
            false,
            theme,
        );
    }

    let alpha = padded_range(points.iter().map(|point| point.alpha_deg), 0.08);
    let cl = padded_range(points.iter().map(|point| point.lift_coefficient), 0.08);
    let cd = padded_range(
        points.iter().map(|point| point.total_drag_coefficient),
        0.08,
    );
    let cm = padded_range(
        points.iter().map(|point| point.pitching_moment_coefficient),
        0.08,
    );
    let ld = padded_range(points.iter().map(|point| point.lift_to_drag), 0.08);
    // Row 2 starts 70 px below row 1's frame (was 60): row 1's x-axis title
    // extends about 41 px below its frame and row 2's panel headings sit
    // about 22 px above theirs, so anything under ~63 px risks the shared
    // "alpha [deg]" label colliding with the row below it.
    let row2_top = 375.0;
    let axes = [
        Axes2D::new((60.0, 55.0, 370.0, 250.0), alpha, cl),
        Axes2D::new((480.0, 55.0, 370.0, 250.0), cl, cd).with_y_tick_decimals(2),
        Axes2D::new((60.0, row2_top, 370.0, 250.0), alpha, cm),
        Axes2D::new((480.0, row2_top, 370.0, 250.0), alpha, ld),
    ];
    let mut scene = Scene::new(900.0, 730.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("VSPAERO Native Polar".to_owned());
    for (axis, (plot_title, x_label, y_label)) in axes.iter().zip([
        ("Native lift curve", "alpha [deg]", "CL"),
        ("Native drag polar", "CL", "CDtot"),
        ("Native pitching moment", "alpha [deg]", "CMytot"),
        ("Native efficiency", "alpha [deg]", "L/D"),
    ]) {
        axis.draw_frame_with_labels(&mut scene, pal, x_label, y_label);
        title(&mut scene, axis, plot_title, Color::from_hex(pal.title));
    }

    let native = Stroke::new(Color::from_hex(NATIVE_COLOR), 1.8);
    let induced = Stroke::dashed(Color::from_hex(INDUCED_COLOR), 1.4, 5.0, 3.0);
    let viscous = Stroke::dashed(Color::from_hex(VISCOUS_COLOR), 1.4, 3.0, 3.0);
    let lift = points
        .iter()
        .map(|point| (point.alpha_deg, point.lift_coefficient))
        .collect::<Vec<_>>();
    let polar_drag = points
        .iter()
        .map(|point| (point.lift_coefficient, point.total_drag_coefficient))
        .collect::<Vec<_>>();
    let moment = points
        .iter()
        .map(|point| (point.alpha_deg, point.pitching_moment_coefficient))
        .collect::<Vec<_>>();
    let efficiency = points
        .iter()
        .map(|point| (point.alpha_deg, point.lift_to_drag))
        .collect::<Vec<_>>();
    axes[0].add_line_series(&mut scene, &lift, native.clone());
    axes[1].add_line_series(&mut scene, &polar_drag, native.clone());
    axes[2].add_line_series(&mut scene, &moment, native.clone());
    axes[3].add_line_series(&mut scene, &efficiency, native.clone());
    add_markers(&mut scene, &axes[0], &lift, Color::from_hex(NATIVE_COLOR));
    add_markers(
        &mut scene,
        &axes[1],
        &polar_drag,
        Color::from_hex(NATIVE_COLOR),
    );
    add_markers(&mut scene, &axes[2], &moment, Color::from_hex(NATIVE_COLOR));
    add_markers(
        &mut scene,
        &axes[3],
        &efficiency,
        Color::from_hex(NATIVE_COLOR),
    );
    axes[1].add_line_series(
        &mut scene,
        &points
            .iter()
            .map(|point| (point.lift_coefficient, point.induced_drag_coefficient))
            .collect::<Vec<_>>(),
        induced.clone(),
    );
    axes[1].add_line_series(
        &mut scene,
        &points
            .iter()
            .map(|point| {
                (
                    point.lift_coefficient,
                    point.total_drag_coefficient - point.induced_drag_coefficient,
                )
            })
            .collect::<Vec<_>>(),
        viscous.clone(),
    );

    draw_horizontal_legend_columns(
        &mut scene,
        [60.0, 690.0],
        &[
            ("VSPAERO native".to_owned(), LegendMarker::Line(native)),
            ("Induced drag".to_owned(), LegendMarker::Line(induced)),
            ("Non-induced CD".to_owned(), LegendMarker::Line(viscous)),
        ],
        pal,
        8.0,
    );
    scene
}

#[derive(Debug, Clone)]
struct WakeCase {
    alpha_deg: f64,
    rows: Vec<[f64; 3]>,
}

fn parse_wake_history(text: &str) -> Result<Vec<WakeCase>, String> {
    let mut cases = Vec::new();
    let mut current: Option<WakeCase> = None;
    let mut pending_alpha = None;
    let mut columns = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("AoA_") {
            pending_alpha = trimmed
                .split_whitespace()
                .nth(1)
                .and_then(|value| value.parse::<f64>().ok());
            continue;
        }
        if trimmed.starts_with("Solver Case:") {
            if let Some(case) = current.take() {
                if !case.rows.is_empty() {
                    cases.push(case);
                }
            }
            current = Some(WakeCase {
                alpha_deg: pending_alpha.take().unwrap_or(cases.len() as f64),
                rows: Vec::new(),
            });
            columns = None;
            continue;
        }
        if trimmed.starts_with("Iter ") || trimmed == "Iter" {
            let headers = trimmed.split_whitespace().collect::<Vec<_>>();
            let mut found = [usize::MAX; 3];
            for (slot, name) in ["CLtot", "CDi", "CMytot"].into_iter().enumerate() {
                found[slot] = headers
                    .iter()
                    .position(|header| *header == name)
                    .unwrap_or(usize::MAX);
            }
            if found.contains(&usize::MAX) {
                return Err("VSPAERO wake history is missing CLtot/CDi/CMytot columns".to_owned());
            }
            columns = Some(found);
            continue;
        }
        let Some(indices) = columns else {
            continue;
        };
        let fields = trimmed.split_whitespace().collect::<Vec<_>>();
        if fields
            .first()
            .and_then(|value| value.parse::<usize>().ok())
            .is_none()
        {
            continue;
        }
        let mut row = [0.0; 3];
        for (slot, &index) in indices.iter().enumerate() {
            let Some(token) = fields.get(index) else {
                return Err("VSPAERO wake history contains a truncated iteration row".to_owned());
            };
            row[slot] = token.parse::<f64>().map_err(|_| {
                format!("VSPAERO wake history contains malformed coefficient '{token}'")
            })?;
            if !row[slot].is_finite() {
                return Err("VSPAERO wake history contains a non-finite coefficient".to_owned());
            }
        }
        if let Some(case) = current.as_mut() {
            case.rows.push(row);
        }
    }
    if let Some(case) = current {
        if !case.rows.is_empty() {
            cases.push(case);
        }
    }
    if cases.is_empty() {
        return Err("VSPAERO wake history contains no iteration cases".to_owned());
    }
    if cases.iter().any(|case| case.rows.len() < 2) {
        return Err(
            "VSPAERO wake history contains a case with fewer than two iterations".to_owned(),
        );
    }
    Ok(cases)
}

fn final_change(case: &WakeCase) -> f64 {
    let previous = case.rows[case.rows.len() - 2];
    let final_row = case.rows[case.rows.len() - 1];
    previous
        .into_iter()
        .zip(final_row)
        .map(|(left, right)| (right - left).abs())
        .fold(0.0, f64::max)
}

/// Plot the final coefficient residual and iteration count from the native
/// `.history` file, including cases rejected by the comparison gate.
pub fn figure_vspaero_wake_convergence(
    result: &VspaeroAnalysisResult,
    theme: Option<&str>,
) -> Scene {
    let history_path = result.case_path.with_extension("history");
    let text = match std::fs::read_to_string(&history_path) {
        Ok(text) => text,
        Err(error) => {
            return status_scene(
                "VSPAERO wake history unavailable",
                &format!("{}: {error}", history_path.display()),
                false,
                theme,
            )
        }
    };
    let cases = match parse_wake_history(&text) {
        Ok(cases) => cases,
        Err(error) => {
            return status_scene("VSPAERO wake history unavailable", &error, false, theme)
        }
    };
    let pal = get_palette(theme);
    let alpha = padded_range(cases.iter().map(|case| case.alpha_deg), 0.08);
    let changes = cases.iter().map(final_change).collect::<Vec<_>>();
    let max_change = changes.iter().copied().fold(0.0, f64::max);
    let residual = (0.0, (max_change * 1.18).max(WAKE_TOLERANCE * 2.0));
    let max_iterations = cases
        .iter()
        .map(|case| case.rows.len() as f64)
        .fold(0.0, f64::max);
    let axes = [
        Axes2D::new((60.0, 55.0, 370.0, 270.0), alpha, residual),
        Axes2D::new(
            (480.0, 55.0, 370.0, 270.0),
            alpha,
            (0.0, (max_iterations + 1.0).max(2.0)),
        ),
    ];
    let mut scene = Scene::new(900.0, 520.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("VSPAERO Native Wake Convergence".to_owned());
    axes[0].draw_frame_with_labels(
        &mut scene,
        pal,
        "alpha [deg]",
        "max final |delta coefficient|",
    );
    axes[1].draw_frame_with_labels(&mut scene, pal, "alpha [deg]", "wake iterations");
    title(
        &mut scene,
        &axes[0],
        "Final wake residual",
        Color::from_hex(pal.title),
    );
    title(
        &mut scene,
        &axes[1],
        "Iterations retained by VSPAERO",
        Color::from_hex(pal.title),
    );
    let residual_points = cases
        .iter()
        .zip(&changes)
        .map(|(case, &change)| (case.alpha_deg, change))
        .collect::<Vec<_>>();
    let iteration_points = cases
        .iter()
        .map(|case| (case.alpha_deg, case.rows.len() as f64))
        .collect::<Vec<_>>();
    let residual_stroke = Stroke::new(Color::from_hex(NATIVE_COLOR), 1.6);
    let iteration_stroke = Stroke::new(Color::from_hex("#56b4e9"), 1.6);
    axes[0].add_line_series(&mut scene, &residual_points, residual_stroke.clone());
    axes[1].add_line_series(&mut scene, &iteration_points, iteration_stroke.clone());
    for (&(alpha_deg, change), point) in residual_points.iter().zip(&cases) {
        let color = if change <= WAKE_TOLERANCE {
            Color::from_hex(ACCEPTED_COLOR)
        } else {
            Color::from_hex(REJECTED_COLOR)
        };
        scene.add(SceneElement::Circle {
            center: axes[0].map_point(alpha_deg, change),
            radius: 3.0,
            fill: Some(Fill::new(color)),
            stroke: None,
        });
        let _ = point;
    }
    scene.add(SceneElement::Line {
        p1: axes[0].map_point(alpha.0, WAKE_TOLERANCE),
        p2: axes[0].map_point(alpha.1, WAKE_TOLERANCE),
        stroke: Stroke::dashed(Color::from_hex(ACCEPTED_COLOR), 1.0, 5.0, 3.0),
    });
    draw_horizontal_legend_columns(
        &mut scene,
        [60.0, 455.0],
        &[
            (
                "native residual".to_owned(),
                LegendMarker::Line(residual_stroke),
            ),
            (
                "iteration count".to_owned(),
                LegendMarker::Line(iteration_stroke),
            ),
            (
                "acceptance tolerance".to_owned(),
                LegendMarker::Line(Stroke::dashed(
                    Color::from_hex(ACCEPTED_COLOR),
                    1.0,
                    5.0,
                    3.0,
                )),
            ),
        ],
        pal,
        8.0,
    );
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn result_with_polar() -> VspaeroAnalysisResult {
        let path = PathBuf::from("test-case");
        VspaeroAnalysisResult {
            status: alas_pipeline::VspaeroAnalysisStatus::CompletedNotComparable,
            runtime_executable: None,
            case_path: path.clone(),
            geometry_path: path.with_extension("vspgeom"),
            setup_path: path.with_extension("vspaero"),
            polar_path: path.with_extension("polar"),
            stdout_path: path.with_extension("stdout"),
            stderr_path: path.with_extension("stderr"),
            polar: Some(alas_aero::vspaero::VspaeroPolar {
                reference: alas_aero::vspaero::VspaeroReference {
                    area_m2: 1.0,
                    chord_m: 1.0,
                    span_m: 1.0,
                    moment_reference_m: [0.0; 3],
                },
                model: alas_aero::vspaero::VspaeroModel::ALAS_VLM,
                points: vec![alas_aero::vspaero::VspaeroPolarPoint {
                    beta_deg: 0.0,
                    mach: 0.8,
                    alpha_deg: 0.0,
                    reynolds: 1.0e6,
                    lift_coefficient: 0.2,
                    induced_drag_coefficient: 0.01,
                    total_drag_coefficient: 0.02,
                    side_force_coefficient: 0.0,
                    lift_to_drag: 10.0,
                    span_efficiency: None,
                    rolling_moment_coefficient: 0.0,
                    pitching_moment_coefficient: -0.01,
                    yawing_moment_coefficient: 0.0,
                }],
            }),
            comparison: VspaeroComparisonStatus::Rejected("wake residual".to_owned()),
            error: Some("wake residual".to_owned()),
        }
    }

    #[test]
    fn native_polar_scene_keeps_a_rejected_but_parsed_result_visible_without_status_prose() {
        let scene = figure_vspaero_polar(&result_with_polar(), Some("dark"));
        assert!(scene
            .elements
            .iter()
            .any(|element| matches!(element, SceneElement::Circle { .. })));
        assert!(!scene.elements.iter().any(|element| matches!(
            element,
            SceneElement::Text { text, .. } if text.contains("comparison rejected")
        )));
    }

    #[test]
    fn wake_history_parser_retains_case_angles_and_final_residuals() {
        let text = "# Name Value Units\nAoA_ 2.0 deg\nSolver Case: 1\n Iter Mach AoA Beta CLtot CDi CMytot\n 1 0.8 2 0 0.4 0.02 -0.1\n 2 0.8 2 0 0.40001 0.02001 -0.10001\n";
        let cases = parse_wake_history(text).expect("parse native history");
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].alpha_deg, 2.0);
        assert!((final_change(&cases[0]) - 1.0e-5).abs() < 1.0e-12);
    }
}
