// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py: figure_structures_vibration
// (L6083-6177).
// Reference: alas @ rust-port-baseline.

//! NASTRAN harmonic-response and force-PSD RMS figures.
//!
//! The Miles-equation comparison is deliberately not synthesized from the
//! analytical structural report. The upstream figure requires the actual
//! SOL 111 response result and reports a status scene when that result is
//! absent or failed.

use alas_pipeline::structural::StructuralAnalysisResult;
use alas_struct::nastran::ResultStatus;

use super::layout::panel_rects;
use super::status::{resolve_structural_result, status_message_scene};
use crate::chart_kit::{
    draw_axes_without_x_tick_labels, draw_categorical_x_axis, draw_legend, LegendMarker,
};
use crate::scene::{
    Axes2D, Color, Fill, Scale, Scene, SceneElement, Stroke, TextAlign, TextBaseline,
};
use crate::theme::get_palette;

const TITLE: &str = "Structural Analysis -- Vibration";
const BLUE: Color = Color::rgba(31, 119, 180, 224);
const RED: Color = Color::rgba(214, 39, 40, 224);

/// Sine-sweep FRF and force-PSD RMS response from NASTRAN results.
pub fn figure_structures_vibration(
    result: Option<&StructuralAnalysisResult>,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let result = match resolve_structural_result(result, TITLE, pal) {
        Ok(result) => result,
        Err(scene) => return scene,
    };
    let Some(nastran) = result.nastran.as_ref() else {
        return unavailable_vibration(pal, "NASTRAN was not run for this design.");
    };
    let vib = &nastran.vibration;
    if vib.status != ResultStatus::Ok {
        return unavailable_vibration(
            pal,
            &format!(
                "NASTRAN vibration solve failed: {}",
                vib.error.as_deref().unwrap_or("unknown error")
            ),
        );
    }

    let mut scene = Scene::new(1200.0, 550.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Structural vibration response".to_owned());
    let rects = panel_rects(2, (60.0, 55.0, 1120.0, 420.0), 42.0);
    draw_frf_panel(&mut scene, rects[0], vib, pal);
    draw_rms_panel(&mut scene, rects[1], vib, pal);
    scene
}

fn unavailable_vibration(pal: &crate::theme::Palette, detail: &str) -> Scene {
    status_message_scene(
        TITLE,
        &format!(
            "Requires a real NASTRAN SOL 111 sine sweep (no analytical fallback exists for this check): {detail}"
        ),
        false,
        pal,
    )
}

fn draw_frf_panel(
    scene: &mut Scene,
    rect: (f64, f64, f64, f64),
    vib: &alas_struct::nastran::VibrationResult,
    pal: &crate::theme::Palette,
) {
    let frequencies = vib.frf_freq_hz.as_deref().unwrap_or(&[]);
    let responses = vib.frf_tip_abs_m_per_n.as_deref().unwrap_or(&[]);
    let x_range = positive_range(frequencies, 1.0);
    let y_values: Vec<f64> = responses
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .map(|v| v + 1e-20)
        .collect();
    let y_range = log_range(&y_values);
    let axes = Axes2D::new(rect, x_range, y_range).with_y_scale(Scale::Log10);
    axes.draw_frame_with_labels(scene, pal, "Frequency [Hz]", "|H(f)| [m/N]");
    let points: Vec<(f64, f64)> = frequencies
        .iter()
        .copied()
        .zip(responses.iter().copied())
        .filter(|(frequency, response)| frequency.is_finite() && response.is_finite())
        .map(|(frequency, response)| (frequency, response + 1e-20))
        .collect();
    axes.add_line_series(
        scene,
        &points,
        Stroke::new(Color::from_hex("tab:blue"), 2.0),
    );
    if points.is_empty() {
        scene.add(SceneElement::Text {
            text: "Sine-sweep response data not available".to_owned(),
            pos: [rect.0 + rect.2 / 2.0, rect.1 + rect.3 / 2.0],
            font_size: 11.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Center,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }
    let mut legend = vec![(
        "|H(f)| at tip".to_owned(),
        LegendMarker::Line(Stroke::new(Color::from_hex("tab:blue"), 2.0)),
    )];
    if vib.peak_freq_hz.is_finite() && vib.peak_freq_hz != 0.0 {
        scene.add(SceneElement::Line {
            p1: axes.map_point(vib.peak_freq_hz, axes.y_min),
            p2: axes.map_point(vib.peak_freq_hz, axes.y_max),
            stroke: Stroke::dashed(Color::from_hex("tab:red"), 1.5, 5.0, 4.0),
        });
        legend.push((
            format!("Peak: {:.2} Hz", vib.peak_freq_hz),
            LegendMarker::Line(Stroke::dashed(Color::from_hex("tab:red"), 1.5, 5.0, 4.0)),
        ));
    }
    draw_legend(scene, [rect.0 + 8.0, rect.1 + 8.0], &legend, pal, 8.0);
    add_panel_title(scene, &axes, "Sine-sweep frequency response (tip)", pal);
}

fn draw_rms_panel(
    scene: &mut Scene,
    rect: (f64, f64, f64, f64),
    vib: &alas_struct::nastran::VibrationResult,
    pal: &crate::theme::Palette,
) {
    let mut labels = vib.miles_rms_m.labels();
    for label in vib.nastran_rms_m.labels() {
        if !labels.contains(&label) {
            labels.push(label);
        }
    }
    let values: Vec<f64> = labels
        .iter()
        .flat_map(|label| [vib.miles_rms_m.get(label), vib.nastran_rms_m.get(label)])
        .flatten()
        .filter(|v| v.is_finite() && *v > 0.0)
        .collect();
    let axes = Axes2D::new(
        rect,
        if labels.is_empty() {
            (-0.5, 0.5)
        } else {
            (-0.5, labels.len() as f64 - 0.5)
        },
        log_range(&values),
    )
    .with_y_scale(Scale::Log10);
    draw_axes_without_x_tick_labels(&axes, scene, pal, None, Some("RMS displacement [m, log]"));
    let floor = axes.y_min;
    let has_miles = !vib.miles_rms_m.is_empty();
    let has_nastran = !vib.nastran_rms_m.is_empty();
    for (index, label) in labels.iter().enumerate() {
        let x = index as f64;
        if let Some(value) = vib.miles_rms_m.get(label).filter(|v| *v > 0.0) {
            let (left, right) = if has_nastran {
                (x - 0.34, x - 0.02)
            } else {
                (x - 0.22, x + 0.22)
            };
            draw_log_bar(scene, &axes, left, right, value, floor, BLUE);
            draw_rms_value(scene, &axes, (left + right) * 0.5, value, floor, BLUE);
        }
        if let Some(value) = vib.nastran_rms_m.get(label).filter(|v| *v > 0.0) {
            let (left, right) = if has_miles {
                (x + 0.02, x + 0.34)
            } else {
                (x - 0.22, x + 0.22)
            };
            draw_log_bar(scene, &axes, left, right, value, floor, RED);
            draw_rms_value(scene, &axes, (left + right) * 0.5, value, floor, RED);
        }
    }
    draw_categorical_x_axis(&axes, scene, &labels, pal);
    if labels.is_empty() {
        let detail = vib
            .random_response_error
            .as_deref()
            .map(|reason| reason.replace(": ", ":\n").replace("; ", "\n"))
            .unwrap_or_else(|| "Force-PSD RMS data\nnot available".to_owned());
        scene.add(SceneElement::Text {
            text: detail,
            pos: [rect.0 + rect.2 / 2.0, rect.1 + rect.3 / 2.0],
            font_size: 10.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Center,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }
    let mut legend = Vec::new();
    if has_miles {
        legend.push((
            "Miles equation (base PSD)".to_owned(),
            LegendMarker::Patch(BLUE),
        ));
    }
    if has_nastran {
        legend.push((
            "NASTRAN SOL 111 (force PSD)".to_owned(),
            LegendMarker::Patch(RED),
        ));
    }
    if !legend.is_empty() {
        draw_legend(scene, [rect.0 + 8.0, rect.1 + 8.0], &legend, pal, 8.0);
    }
    add_panel_title(scene, &axes, "Force-PSD RMS from SOL 111", pal);
}

fn draw_log_bar(
    scene: &mut Scene,
    axes: &Axes2D,
    x0: f64,
    x1: f64,
    value: f64,
    floor: f64,
    color: Color,
) {
    let top = axes.map_point(x0, value.max(floor));
    let bottom = axes.map_point(x1, floor);
    scene.add(SceneElement::Rect {
        x: top[0].min(bottom[0]),
        y: top[1].min(bottom[1]),
        width: (bottom[0] - top[0]).abs(),
        height: (bottom[1] - top[1]).abs().max(0.5),
        rx: 0.0,
        fill: Some(Fill::new(color)),
        stroke: None,
    });
}

/// Put the numeric RMS value at its bar instead of forcing the user to infer a
/// magnitude from a short logarithmic axis in a compact panel.
fn draw_rms_value(scene: &mut Scene, axes: &Axes2D, x: f64, value: f64, floor: f64, color: Color) {
    let point = axes.map_point(x, value.max(floor));
    scene.add(SceneElement::Text {
        text: format!("{value:.2e} m"),
        pos: [point[0], point[1] - 4.0],
        font_size: 8.0,
        color,
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: false,
    });
}

fn positive_range(values: &[f64], fallback_max: f64) -> (f64, f64) {
    let lo = values
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(f64::INFINITY, f64::min);
    let hi = values
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(f64::NEG_INFINITY, f64::max);
    if lo.is_finite() && hi.is_finite() && hi > lo {
        (lo, hi)
    } else if lo.is_finite() {
        (lo, lo + fallback_max)
    } else {
        (0.0, fallback_max)
    }
}

fn log_range(values: &[f64]) -> (f64, f64) {
    let positive: Vec<f64> = values
        .iter()
        .copied()
        .filter(|v| v.is_finite() && *v > 0.0)
        .collect();
    if positive.is_empty() {
        return (1e-20, 1.0);
    }
    let lo = positive.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = positive.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if hi > lo {
        (lo * 0.8, hi * 1.25)
    } else {
        (lo * 0.5, hi * 2.0)
    }
}

fn add_panel_title(scene: &mut Scene, axes: &Axes2D, title: &str, pal: &crate::theme::Palette) {
    scene.add(SceneElement::Text {
        text: title.to_owned(),
        pos: [axes.left + axes.width / 2.0, axes.top - 10.0],
        font_size: 11.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_struct::nastran::VibrationResult;

    #[test]
    fn empty_log_data_stays_inside_a_positive_axis_range() {
        assert_eq!(log_range(&[]), (1e-20, 1.0));
    }

    #[test]
    fn unavailable_vibration_result_is_a_status_scene() {
        let result = StructuralAnalysisResult {
            status: "ok".to_owned(),
            ..StructuralAnalysisResult::default()
        };
        let scene = figure_structures_vibration(Some(&result), None);
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text.contains("NASTRAN"))
        }));
    }

    #[test]
    fn a_successful_empty_vibration_result_keeps_the_rms_placeholder() {
        let result = StructuralAnalysisResult {
            status: "ok".to_owned(),
            nastran: Some(alas_struct::nastran::NastranResults {
                vibration: VibrationResult {
                    status: ResultStatus::Ok,
                    ..VibrationResult::default()
                },
                ..alas_struct::nastran::NastranResults::default()
            }),
            ..StructuralAnalysisResult::default()
        };
        let scene = figure_structures_vibration(Some(&result), None);
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text.contains("not available"))
        }));
    }

    #[test]
    fn force_psd_rms_without_miles_values_is_still_drawn() {
        let result = StructuralAnalysisResult {
            status: "ok".to_owned(),
            nastran: Some(alas_struct::nastran::NastranResults {
                vibration: VibrationResult {
                    status: ResultStatus::Ok,
                    nastran_rms_m: {
                        let mut values = alas_struct::nastran::LabelledValues::default();
                        values.push("tip", 1.0e-3);
                        values
                    },
                    ..VibrationResult::default()
                },
                ..alas_struct::nastran::NastranResults::default()
            }),
            ..StructuralAnalysisResult::default()
        };
        let scene = figure_structures_vibration(Some(&result), None);
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text.contains("Force-PSD RMS from SOL 111"))
        }));
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text == "tip")
        }));
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text == "1.00e-3 m")
        }));
    }
}
