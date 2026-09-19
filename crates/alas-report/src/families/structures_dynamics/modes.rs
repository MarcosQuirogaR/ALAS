// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py: figure_structures_modes
// (L5969-6080) and _nearest_freq_index (L5954-5966).
// Reference: alas @ rust-port-baseline.

//! Rayleigh-versus-MSC/NASTRAN-95 normal-mode frequencies and spanwise shapes.
//!
//! Each solver's modes are paired to the analytical trial modes by nearest
//! frequency, not by list position. A real finite-element solve may contain
//! torsional or local-panel modes that have no Rayleigh counterpart.

use alas_pipeline::structural::StructuralAnalysisResult;
use alas_struct::nastran::ResultStatus;

use super::layout::{draw_bar, panel_rects};
use super::status::{resolve_structural_result, status_message_scene, with_alpha, TAB10};
use crate::chart_kit::{draw_legend, draw_title, LegendMarker};
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

const TITLE: &str = "Structural Analysis: Modes";
const BLUE: &str = "tab:blue";
const RED: &str = "tab:red";
const ORANGE: &str = "tab:orange";

/// Return the index of the NASTRAN frequency nearest to an analytical one.
pub fn nearest_frequency_index(frequency_hz: f64, nastran_frequencies_hz: &[f64]) -> Option<usize> {
    nastran_frequencies_hz
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (*a - frequency_hz)
                .abs()
                .total_cmp(&(*b - frequency_hz).abs())
        })
        .map(|(index, _)| index)
}

/// Natural frequencies and their Rayleigh/MSC/NASTRAN-95 mode-shape comparison.
pub fn figure_structures_modes(
    result: Option<&StructuralAnalysisResult>,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let result = match resolve_structural_result(result, TITLE, pal) {
        Ok(result) => result,
        Err(scene) => return scene,
    };
    let Some(analysis) = result.analysis.as_ref() else {
        return status_message_scene(
            TITLE,
            "Structural analysis reported \"ok\" but has no modal data.",
            false,
            pal,
        );
    };
    if analysis.modal.frequencies_hz.is_empty() {
        return status_message_scene(
            TITLE,
            "The public structural result contains no analytical modal frequencies to plot.",
            false,
            pal,
        );
    }

    let msc_modes = result.nastran.as_ref().map(|n| &n.modes);
    let has_msc =
        msc_modes.is_some_and(|m| m.status == ResultStatus::Ok && !m.frequencies_hz.is_empty());
    let msc_frequencies = msc_modes
        .filter(|_| has_msc)
        .map(|m| m.frequencies_hz.as_slice())
        .unwrap_or(&[]);
    let n95_modes = result.nastran95.as_ref().map(|n| &n.modes);
    let has_n95 =
        n95_modes.is_some_and(|m| m.status == ResultStatus::Ok && !m.frequencies_hz.is_empty());
    let n95_frequencies = n95_modes
        .filter(|_| has_n95)
        .map(|m| m.frequencies_hz.as_slice())
        .unwrap_or(&[]);
    let msc_matches: Vec<Option<usize>> = analysis
        .modal
        .frequencies_hz
        .iter()
        .map(|&frequency| nearest_frequency_index(frequency, msc_frequencies))
        .collect();
    let n95_matches: Vec<Option<usize>> = analysis
        .modal
        .frequencies_hz
        .iter()
        .map(|&frequency| nearest_frequency_index(frequency, n95_frequencies))
        .collect();
    let msc_shape_available = has_matched_shape(msc_modes, &msc_matches);
    let n95_shape_available = has_matched_shape(n95_modes, &n95_matches);

    let mut scene = Scene::new(1600.0, 550.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Structural normal modes".to_owned());
    draw_title(&mut scene, "Structural normal modes", pal);
    scene.suppress_derived_title();
    let rects = panel_rects(3, (60.0, 55.0, 1500.0, 420.0), 32.0);
    let n_modes = analysis.modal.frequencies_hz.len();
    let x_range = if n_modes == 0 {
        (0.5, 1.5)
    } else {
        (0.5, n_modes as f64 + 0.5)
    };
    let max_frequency = analysis
        .modal
        .frequencies_hz
        .iter()
        .copied()
        .chain(msc_frequencies.iter().copied())
        .chain(n95_frequencies.iter().copied())
        .filter(|v| v.is_finite() && *v >= 0.0)
        .fold(0.0, f64::max)
        .max(1.0)
        * 1.1;

    let frequency_axes = Axes2D::new(rects[0], x_range, (0.0, max_frequency));
    frequency_axes.draw_frame_with_labels(&mut scene, pal, "Mode", "Frequency [Hz]");
    let solver_count = usize::from(has_msc) + usize::from(has_n95);
    let bar_width = if solver_count == 0 {
        0.5
    } else {
        0.8 / (solver_count + 1) as f64
    };
    let group_width = bar_width * (solver_count + 1) as f64;
    for (index, &frequency) in analysis.modal.frequencies_hz.iter().enumerate() {
        let center = index as f64 + 1.0;
        let group_left = center - group_width / 2.0;
        let left = group_left;
        draw_bar(
            &mut scene,
            &frequency_axes,
            left,
            left + bar_width,
            frequency.max(0.0),
            Fill::new(Color::rgba(31, 119, 180, 224)),
        );
        if let Some(Some(msc_index)) = msc_matches.get(index) {
            let right = group_left + bar_width;
            draw_bar(
                &mut scene,
                &frequency_axes,
                right,
                right + bar_width,
                msc_frequencies[*msc_index].max(0.0),
                Fill::new(Color::rgba(214, 39, 40, 224)),
            );
        }
        if let Some(Some(n95_index)) = n95_matches.get(index) {
            let local_left = group_left + bar_width * (1.0 + usize::from(has_msc) as f64);
            draw_bar(
                &mut scene,
                &frequency_axes,
                local_left,
                local_left + bar_width,
                n95_frequencies[*n95_index].max(0.0),
                Fill::new(Color::rgba(255, 127, 14, 224)),
            );
        }
    }
    add_panel_title(
        &mut scene,
        &frequency_axes,
        "Natural frequencies (bending)",
        pal,
    );
    let mut entries = vec![(
        "Rayleigh (analytical)".to_owned(),
        LegendMarker::Patch(Color::from_hex(BLUE)),
    )];
    if has_msc {
        entries.push((
            if has_n95 {
                "MSC NASTRAN SOL 103 (nearest-frequency match)".to_owned()
            } else {
                "NASTRAN SOL 103 (nearest-frequency match)".to_owned()
            },
            LegendMarker::Patch(Color::from_hex(RED)),
        ));
    }
    if has_n95 {
        entries.push((
            "NASTRAN-95 SOL 103 (nearest-frequency match)".to_owned(),
            LegendMarker::Patch(Color::from_hex(ORANGE)),
        ));
    }
    draw_legend(
        &mut scene,
        [rects[0].0 + 8.0, rects[0].1 + 8.0],
        &entries,
        pal,
        8.0,
    );

    let y_range = span_range(&analysis.y);
    let rayleigh_axes = Axes2D::new(rects[1], y_range, (-1.1, 1.1));
    let nastran_axes = Axes2D::new(rects[2], y_range, (-1.1, 1.1));
    rayleigh_axes.draw_frame_with_labels(&mut scene, pal, "Spanwise position Y [m]", "");
    nastran_axes.draw_frame_with_labels(&mut scene, pal, "Spanwise position Y [m]", "");
    for (index, (shape, &frequency)) in analysis
        .modal
        .mode_shapes
        .iter()
        .zip(&analysis.modal.frequencies_hz)
        .enumerate()
    {
        let color = Color::from_hex(TAB10[index % TAB10.len()]);
        let rayleigh_points: Vec<(f64, f64)> = analysis
            .y
            .iter()
            .copied()
            .zip(shape.iter().copied())
            .filter(|(y, value)| y.is_finite() && value.is_finite())
            .collect();
        rayleigh_axes.add_line_series(
            &mut scene,
            &rayleigh_points,
            Stroke::dashed(color, 2.0, 6.0, 4.0),
        );
        if let (Some(modes), Some(Some(msc_index))) = (msc_modes, msc_matches.get(index)) {
            if let (Some(stations), Some(nastran_shape)) = (
                modes.mode_shape_y_m.as_ref(),
                modes.mode_shapes.get(*msc_index),
            ) {
                let aligned_shape = aligned_display_shape(shape, nastran_shape);
                let points: Vec<(f64, f64)> = stations
                    .iter()
                    .copied()
                    .zip(aligned_shape)
                    .filter(|(y, value)| y.is_finite() && value.is_finite())
                    .collect();
                nastran_axes.add_line_series(
                    &mut scene,
                    &points,
                    // Colour carries the Rayleigh mode match.  The solid
                    // stroke remains MSC-specific, so the two solvers are
                    // distinguishable without relying only on colour.
                    Stroke::new(color, 2.0),
                );
            }
        }
        if let (Some(modes), Some(Some(n95_index))) = (n95_modes, n95_matches.get(index)) {
            if let (Some(stations), Some(n95_shape)) = (
                modes.mode_shape_y_m.as_ref(),
                modes.mode_shapes.get(*n95_index),
            ) {
                let aligned_shape = aligned_display_shape(shape, n95_shape);
                let points: Vec<(f64, f64)> = stations
                    .iter()
                    .copied()
                    .zip(aligned_shape)
                    .filter(|(y, value)| y.is_finite() && value.is_finite())
                    .collect();
                nastran_axes.add_line_series(
                    &mut scene,
                    &points,
                    // NASTRAN-95 uses the same matched-mode colour and a
                    // dotted stroke. This makes a missing local vector
                    // visible instead of silently looking like MSC only.
                    Stroke::dashed(color, 2.0, 2.0, 2.0),
                );
            }
        }
        let rayleigh_label = format!("Mode {}: {frequency:.2} Hz", index + 1);
        add_legend_line(
            &mut scene,
            [rects[1].0 + 8.0, rects[1].1 + 8.0 + index as f64 * 14.0],
            &rayleigh_label,
            Stroke::dashed(color, 2.0, 6.0, 4.0),
            pal,
        );
    }
    add_zero_line(&mut scene, &rayleigh_axes, pal);
    add_zero_line(&mut scene, &nastran_axes, pal);
    add_panel_title(
        &mut scene,
        &rayleigh_axes,
        "Rayleigh trial mode shapes",
        pal,
    );
    let nastran_title = match (msc_shape_available, n95_shape_available) {
        (true, true) => "MSC / NASTRAN-95 mode shapes",
        (true, false) => "NASTRAN mode shapes",
        (false, true) => "NASTRAN-95 mode shapes",
        (false, false) => "NASTRAN mode shapes (unavailable)",
    };
    add_panel_title(&mut scene, &nastran_axes, nastran_title, pal);
    if !has_msc && !has_n95 {
        let reason = if let Some(m) = msc_modes {
            match m.status {
                ResultStatus::Error => format!(
                    "NASTRAN SOL 103 failed: {}",
                    m.error.as_deref().unwrap_or("unknown error")
                ),
                ResultStatus::NotRun => "NASTRAN SOL 103 was not run for this design.".to_owned(),
                ResultStatus::Ok => {
                    "NASTRAN SOL 103 returned no elastic mode frequencies.".to_owned()
                }
            }
        } else if let Some(m) = n95_modes {
            match m.status {
                ResultStatus::Error => format!(
                    "NASTRAN-95 SOL 103 failed: {}",
                    m.error.as_deref().unwrap_or("unknown error")
                ),
                ResultStatus::NotRun => {
                    "NASTRAN-95 SOL 103 was not run for this design.".to_owned()
                }
                ResultStatus::Ok => {
                    "NASTRAN-95 SOL 103 returned no elastic mode frequencies.".to_owned()
                }
            }
        } else {
            "NASTRAN SOL 103 was not run for this design.".to_owned()
        };
        scene.add(SceneElement::Text {
            text: wrap_text(&reason, 54),
            pos: [nastran_axes.left + 8.0, nastran_axes.top + 34.0],
            font_size: 8.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: false,
        });
    }
    let mut solver_entries = Vec::new();
    if msc_shape_available {
        solver_entries.push((
            "MSC NASTRAN - solid line".to_owned(),
            LegendMarker::Line(Stroke::new(Color::from_hex(pal.tick), 2.0)),
        ));
    }
    if n95_shape_available {
        solver_entries.push((
            "NASTRAN-95 - dotted line".to_owned(),
            LegendMarker::Line(Stroke::dashed(Color::from_hex(pal.tick), 2.0, 2.0, 2.0)),
        ));
    }
    if !solver_entries.is_empty() {
        solver_entries.push((
            "Colour = matched Rayleigh mode".to_owned(),
            LegendMarker::Line(Stroke::new(Color::from_hex(TAB10[0]), 2.0)),
        ));
    }
    if !solver_entries.is_empty() {
        draw_legend(
            &mut scene,
            [nastran_axes.left + 8.0, nastran_axes.top + 8.0],
            &solver_entries,
            pal,
            8.0,
        );
    }
    scene
}

fn wrap_text(text: &str, max_chars: usize) -> String {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_owned()
        } else {
            format!("{current} {word}")
        };
        if candidate.chars().count() <= max_chars || current.is_empty() {
            current = candidate;
        } else {
            lines.push(current);
            current = word.to_owned();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines.join("\n")
}

fn span_range(values: &[f64]) -> (f64, f64) {
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
    } else {
        (0.0, 1.0)
    }
}

const MODE_SIGN_EPS: f64 = 1.0e-12;

/// Align a solver's displayed Y-component mode shape with its matched
/// Rayleigh mode. Normal-mode eigenvectors have an arbitrary global sign, so
/// this changes neither frequency, shape magnitude, nor normalization.
///
/// The last common station is the endpoint convention used for the report. If
/// that endpoint is zero/non-finite, the finite-vector correlation supplies a
/// deterministic fallback instead of forcing a sign from numerical noise.
fn aligned_display_shape(reference: &[f64], candidate: &[f64]) -> Vec<f64> {
    let sign = mode_sign(reference, candidate);
    candidate.iter().map(|value| value * sign).collect()
}

fn mode_sign(reference: &[f64], candidate: &[f64]) -> f64 {
    if let Some((&reference_endpoint, &candidate_endpoint)) =
        reference.iter().zip(candidate).next_back()
    {
        if reference_endpoint.is_finite()
            && candidate_endpoint.is_finite()
            && reference_endpoint.abs() > MODE_SIGN_EPS
            && candidate_endpoint.abs() > MODE_SIGN_EPS
        {
            return if reference_endpoint.signum() == candidate_endpoint.signum() {
                1.0
            } else {
                -1.0
            };
        }
    }

    let correlation = reference
        .iter()
        .zip(candidate)
        .filter(|(reference, candidate)| reference.is_finite() && candidate.is_finite())
        .map(|(&reference, &candidate)| reference * candidate)
        .sum::<f64>();
    if correlation.is_finite() && correlation < -MODE_SIGN_EPS {
        -1.0
    } else {
        1.0
    }
}

/// A solver is only said to have shapes when a nearest-frequency pairing
/// yields a drawable curve. A frequency list alone must not produce a legend
/// that promises a curve which the parser could not supply.
fn has_matched_shape(
    modes: Option<&alas_struct::nastran::ModesResult>,
    matches: &[Option<usize>],
) -> bool {
    let Some(modes) = modes else {
        return false;
    };
    let Some(stations) = modes.mode_shape_y_m.as_ref() else {
        return false;
    };
    matches.iter().flatten().any(|&index| {
        modes.mode_shapes.get(index).is_some_and(|shape| {
            stations
                .iter()
                .zip(shape)
                .filter(|(y, value)| y.is_finite() && value.is_finite())
                .take(2)
                .count()
                == 2
        })
    })
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

fn add_zero_line(scene: &mut Scene, axes: &Axes2D, pal: &crate::theme::Palette) {
    scene.add(SceneElement::Line {
        p1: axes.map_point(axes.x_min, 0.0),
        p2: axes.map_point(axes.x_max, 0.0),
        stroke: Stroke::new(with_alpha(pal.spine, 102), 0.7),
    });
}

fn add_legend_line(
    scene: &mut Scene,
    pos: [f64; 2],
    label: &str,
    stroke: Stroke,
    pal: &crate::theme::Palette,
) {
    draw_legend(
        scene,
        pos,
        &[(label.to_owned(), LegendMarker::Line(stroke))],
        pal,
        8.0,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_struct::analytical::{ModalResult, StructuralAnalysisReport};
    use alas_struct::nastran::{ModesResult, NastranResults};

    #[test]
    fn nearest_frequency_matching_chooses_the_physically_closest_mode() {
        assert_eq!(nearest_frequency_index(9.7, &[3.0, 10.0, 22.0]), Some(1));
        assert_eq!(nearest_frequency_index(1.0, &[]), None);
    }

    #[test]
    fn an_unrequested_mode_solve_is_not_mislabeled_as_empty_solver_output() {
        let result = StructuralAnalysisResult {
            status: "ok".to_owned(),
            analysis: Some(StructuralAnalysisReport {
                y: vec![0.0, 1.0],
                ei_nm2: Vec::new(),
                load_cases: Vec::new(),
                modal: ModalResult {
                    frequencies_hz: vec![2.0],
                    mode_shapes: vec![vec![0.0, 1.0]],
                },
            }),
            nastran: Some(NastranResults::default()),
            ..StructuralAnalysisResult::default()
        };

        let scene = figure_structures_modes(Some(&result), Some("light"));
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text.contains("was not run"))
        }));
    }

    #[test]
    fn both_solver_mode_frequencies_and_shapes_are_rendered_together() {
        let result = StructuralAnalysisResult {
            status: "ok".to_owned(),
            analysis: Some(StructuralAnalysisReport {
                y: vec![0.0, 1.0, 2.0],
                ei_nm2: Vec::new(),
                load_cases: Vec::new(),
                modal: ModalResult {
                    frequencies_hz: vec![2.0, 4.0],
                    mode_shapes: vec![vec![0.0, 0.5, 1.0], vec![0.0, -0.5, -1.0]],
                },
            }),
            nastran: Some(NastranResults {
                modes: ModesResult {
                    status: ResultStatus::Ok,
                    frequencies_hz: vec![2.1, 4.2],
                    mode_shapes: vec![vec![0.0, 0.4, 1.0], vec![0.0, -0.4, -1.0]],
                    mode_shape_y_m: Some(vec![0.0, 1.0, 2.0]),
                    ..Default::default()
                },
                ..Default::default()
            }),
            nastran95: Some(NastranResults {
                modes: ModesResult {
                    status: ResultStatus::Ok,
                    frequencies_hz: vec![1.9, 4.1],
                    mode_shapes: vec![vec![0.0, 0.6, 1.0], vec![0.0, -0.6, -1.0]],
                    mode_shape_y_m: Some(vec![0.0, 1.0, 2.0]),
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        };

        let scene = figure_structures_modes(Some(&result), Some("light"));
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text.contains("MSC NASTRAN SOL 103"))
        }));
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text.contains("NASTRAN-95 SOL 103"))
        }));
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text == "MSC / NASTRAN-95 mode shapes")
        }));
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Polyline { stroke, .. }
                if stroke.color == Color::from_hex(TAB10[0]) && stroke.dash_array.is_none())
        }));
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Polyline { stroke, .. }
                if stroke.color == Color::from_hex(TAB10[0])
                    && stroke.dash_array == Some(vec![2.0, 2.0]))
        }));
    }

    #[test]
    fn matched_solver_shapes_flip_as_a_whole_to_match_the_rayleigh_endpoint() {
        let reference = [0.0, 0.4, 1.0];
        let candidate = [0.0, -0.4, -1.0];
        assert_eq!(
            aligned_display_shape(&reference, &candidate),
            reference.to_vec()
        );
    }

    #[test]
    fn zero_endpoint_uses_finite_vector_correlation_for_sign_alignment() {
        let reference = [0.5, 0.0, 0.0];
        let candidate = [-0.5, 0.0, 0.0];
        assert_eq!(
            aligned_display_shape(&reference, &candidate),
            reference.to_vec()
        );
    }
}
