// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Figures built from VSPAERO's native `.lod` sectional-load export.
//!
//! The text file is produced alongside the polar by the installed solver. It
//! contains the panel/strip loading for every requested angle, so it is a
//! better source for a native span-load diagnostic than rebuilding a second
//! in-process VLM result. The parser deliberately selects the widest vortex
//! sheet (normally the main wing) and keeps its signed span coordinate.

use std::collections::BTreeMap;

use alas_pipeline::VspaeroAnalysisResult;

use super::status::figure_status_message;
use super::support::padded_range;
use crate::chart_kit::{draw_horizontal_legend_columns, LegendMarker};
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

const NATIVE_COLOR: [&str; 3] = ["#0072b2", "#d55e00", "#009e73"];

#[derive(Debug, Clone, Copy)]
struct LodRow {
    vortex_sheet: i64,
    y_avg: f64,
    lift: f64,
    induced_drag: f64,
}

#[derive(Debug, Clone)]
struct LodCase {
    alpha_deg: f64,
    rows: Vec<LodRow>,
}

fn parse_lod(text: &str) -> Result<Vec<LodCase>, String> {
    let mut cases = Vec::new();
    let mut current: Option<LodCase> = None;
    let mut columns: Option<[usize; 5]> = None;

    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("AoA_") {
            if let Some(case) = current.take() {
                if !case.rows.is_empty() {
                    cases.push(case);
                }
            }
            let alpha_deg = trimmed
                .split_whitespace()
                .nth(1)
                .and_then(|value| value.parse::<f64>().ok())
                .ok_or_else(|| "VSPAERO LOD block has an invalid AoA".to_owned())?;
            current = Some(LodCase {
                alpha_deg,
                rows: Vec::new(),
            });
            columns = None;
            continue;
        }
        if trimmed.starts_with("Iter ") || trimmed == "Iter" {
            let headers = trimmed.split_whitespace().collect::<Vec<_>>();
            let names = ["VortexSheet", "Yavg", "Cl", "Cdi", "StallFact"];
            let Some(indices) = names
                .map(|name| headers.iter().position(|header| *header == name))
                .into_iter()
                .collect::<Option<Vec<_>>>()
                .and_then(|indices| indices.try_into().ok())
            else {
                return Err(
                    "VSPAERO LOD is missing VortexSheet/Yavg/Cl/Cdi/StallFact columns".to_owned(),
                );
            };
            columns = Some(indices);
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
        let sheet = fields
            .get(indices[0])
            .and_then(|value| value.parse::<i64>().ok())
            .ok_or_else(|| "VSPAERO LOD has a malformed vortex-sheet index".to_owned())?;
        let y_avg = fields
            .get(indices[1])
            .and_then(|value| value.parse::<f64>().ok())
            .ok_or_else(|| "VSPAERO LOD has a malformed Yavg value".to_owned())?;
        let lift = fields
            .get(indices[2])
            .and_then(|value| value.parse::<f64>().ok())
            .ok_or_else(|| "VSPAERO LOD has a malformed Cl value".to_owned())?;
        let induced_drag = fields
            .get(indices[3])
            .and_then(|value| value.parse::<f64>().ok())
            .ok_or_else(|| "VSPAERO LOD has a malformed Cdi value".to_owned())?;
        // Parse StallFact as part of the required schema even though this
        // figure does not draw it; a partial row must not be mistaken for a
        // complete native load record.
        let stall = fields
            .get(indices[4])
            .and_then(|value| value.parse::<f64>().ok())
            .ok_or_else(|| "VSPAERO LOD has a malformed StallFact value".to_owned())?;
        if [y_avg, lift, induced_drag, stall]
            .into_iter()
            .all(f64::is_finite)
        {
            if let Some(case) = current.as_mut() {
                case.rows.push(LodRow {
                    vortex_sheet: sheet,
                    y_avg,
                    lift,
                    induced_drag,
                });
            }
        }
    }
    if let Some(case) = current {
        if !case.rows.is_empty() {
            cases.push(case);
        }
    }
    if cases.is_empty() {
        return Err("VSPAERO LOD contains no finite sectional-load rows".to_owned());
    }
    Ok(cases)
}

fn widest_sheet_rows(case: &LodCase, span_m: f64) -> Vec<(f64, f64, f64)> {
    let mut groups: BTreeMap<i64, Vec<LodRow>> = BTreeMap::new();
    for row in &case.rows {
        groups.entry(row.vortex_sheet).or_default().push(*row);
    }
    let Some((_, rows)) = groups.into_iter().max_by(|(_, left), (_, right)| {
        let left_span = left
            .iter()
            .map(|row| row.y_avg)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), y| {
                (min.min(y), max.max(y))
            });
        let right_span = right
            .iter()
            .map(|row| row.y_avg)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), y| {
                (min.min(y), max.max(y))
            });
        (left_span.1 - left_span.0).total_cmp(&(right_span.1 - right_span.0))
    }) else {
        return Vec::new();
    };
    let denominator = span_m.abs().max(1e-12);
    let mut points = rows
        .into_iter()
        .filter(|row| row.y_avg.is_finite())
        .map(|row| (row.y_avg / denominator, row.lift, row.induced_drag))
        .collect::<Vec<_>>();
    points.sort_by(|left, right| left.0.total_cmp(&right.0));
    points
}

/// Plot the widest native VSPAERO lifting sheet for representative AoA cases.
///
/// The figure is explicitly native: it does not imply that a rejected polar
/// is physically comparable to the ALAS model, and it does not fabricate a
/// complete span load when the `.lod` file is absent.
pub fn figure_vspaero_load_distribution(
    result: &VspaeroAnalysisResult,
    theme: Option<&str>,
) -> Scene {
    let path = result.case_path.with_extension("lod");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            return figure_status_message(
                "VSPAERO native load distribution unavailable",
                &format!("{}: {error}", path.display()),
                false,
                theme,
            )
        }
    };
    let cases = match parse_lod(&text) {
        Ok(cases) => cases,
        Err(error) => {
            return figure_status_message(
                "VSPAERO native load distribution unavailable",
                &error,
                false,
                theme,
            )
        }
    };
    let span = result
        .polar
        .as_ref()
        .map(|polar| polar.reference.span_m)
        .filter(|span| span.is_finite() && span.abs() > 1e-9)
        .unwrap_or(1.0);
    let series = cases
        .iter()
        .map(|case| (case.alpha_deg, widest_sheet_rows(case, span)))
        .filter(|(_, points)| points.len() >= 2)
        .collect::<Vec<_>>();
    if series.is_empty() {
        return figure_status_message(
            "VSPAERO native load distribution unavailable",
            "The retained LOD file has no usable widest-sheet rows.",
            false,
            theme,
        );
    }

    let representative_indices = if series.len() <= 3 {
        (0..series.len()).collect::<Vec<_>>()
    } else {
        vec![0, series.len() / 2, series.len() - 1]
    };
    let representative = representative_indices
        .iter()
        .map(|&index| &series[index])
        .collect::<Vec<_>>();
    let pal = get_palette(theme);
    let y_range = padded_range(
        representative
            .iter()
            .flat_map(|(_, points)| points.iter().map(|point| point.0)),
        0.05,
    );
    let lift_range = padded_range(
        representative
            .iter()
            .flat_map(|(_, points)| points.iter().map(|point| point.1)),
        0.08,
    );
    let drag_range = padded_range(
        representative
            .iter()
            .flat_map(|(_, points)| points.iter().map(|point| point.2)),
        0.08,
    );
    let axes = [
        Axes2D::new((60.0, 55.0, 370.0, 245.0), y_range, lift_range),
        Axes2D::new((480.0, 55.0, 370.0, 245.0), y_range, drag_range),
    ];
    let mut scene = Scene::new(900.0, 460.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("VSPAERO Native Load Distribution".to_owned());
    for (axis, (title, y_label)) in axes.iter().zip([
        ("Native sectional lift", "Cl"),
        ("Native sectional induced drag", "Cdi"),
    ]) {
        axis.draw_frame_with_labels(&mut scene, pal, "Y/Bref", y_label);
        scene.add(SceneElement::Text {
            text: title.to_owned(),
            pos: [axis.left, axis.top - 10.0],
            font_size: 10.0,
            color: Color::from_hex(pal.title),
            align: TextAlign::Left,
            baseline: TextBaseline::Bottom,
            angle_deg: 0.0,
            bold: true,
        });
    }

    let mut legend = Vec::new();
    for (index, (alpha, points)) in representative.iter().enumerate() {
        let color = Color::from_hex(NATIVE_COLOR[index]);
        let lift = points
            .iter()
            .map(|&(y, value, _)| (y, value))
            .collect::<Vec<_>>();
        let drag = points
            .iter()
            .map(|&(y, _, value)| (y, value))
            .collect::<Vec<_>>();
        let stroke = Stroke::new(color, 1.7);
        axes[0].add_line_series(&mut scene, &lift, stroke.clone());
        axes[1].add_line_series(&mut scene, &drag, stroke.clone());
        for &(x, y) in &lift {
            scene.add(SceneElement::Circle {
                center: axes[0].map_point(x, y),
                radius: 2.0,
                fill: Some(Fill::new(color)),
                stroke: None,
            });
        }
        legend.push((
            format!("alpha = {alpha:.1} deg"),
            LegendMarker::Line(stroke),
        ));
    }
    draw_horizontal_legend_columns(&mut scene, [60.0, 372.0], &legend, pal, 8.0);
    scene
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lod_parser_keeps_native_sheet_and_coefficients() {
        let text = "AoA_ 2.0 deg\nIter VortexSheet TrailVort Xavg Yavg Zavg dSpan SoverB Chord dArea V/Vref Cl Cdi StallFact\n5 1 1 0 -5 0 1 1 1 1 1 0.4 0.02 1\n5 1 2 0 5 0 1 1 1 1 1 0.3 0.01 1\n";
        let cases = parse_lod(text).expect("native LOD parser");
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].rows.len(), 2);
        let points = widest_sheet_rows(&cases[0], 10.0);
        assert_eq!(points[0].0, -0.5);
        assert_eq!(points[1].2, 0.01);
    }
}
