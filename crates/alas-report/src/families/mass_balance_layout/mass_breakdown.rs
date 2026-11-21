// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py:figure_mass_breakdown (L2105-2353)

use super::common::BAR_HEIGHT;
use super::common::{assign_label_rows, bar_label, draw_hbar};
use crate::scene::{Axes2D, Color, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;
use alas_mass::breakdown::{
    FUEL, FURNISHINGS, FUSELAGE, GEAR, H_STAB, PAYLOAD, PROPULSION, SYSTEMS, V_STAB, WING,
};
use alas_pipeline::full_analysis::AnalysisReport;
/// `Wing::aerodynamic_center`'s `chord_fraction` at native aerodynamic model's default:
/// upstream never passes its own.
pub(super) const AC_CHORD_FRACTION: f64 = 0.25;

// ---------------------------------------------------------------------------
// figure_mass_breakdown
// ---------------------------------------------------------------------------

pub(super) const STRUCTURE_KEYS: [&str; 5] = [WING, H_STAB, V_STAB, FUSELAGE, GEAR];
pub(super) const STRUCTURE_COLORS: [&str; 5] =
    ["#2980b9", "#3498db", "#5dade2", "#7f8c8d", "#34495e"];
pub(super) const OTHER_KEYS: [&str; 3] = [PROPULSION, SYSTEMS, FURNISHINGS];
pub(super) const OTHER_COLORS: [&str; 3] = ["#c0392b", "#8e44ad", "#a569bd"];
pub(super) const PAYLOAD_COLOR: &str = "#27ae60";
pub(super) const FUEL_COLOR: &str = "#e67e22";
pub(super) const FUEL_NEG_COLOR: &str = "#e74c3c";

/// Label of the total-mass line. The bars sum the load case the analysis
/// carries (payload plus the fuel the tanks and the mass limit allow), which is
/// below the aircraft's MTOW limit whenever the tanks are the binding limit,
/// so the line names the loaded takeoff mass and states the limit beside it.
fn loaded_mass_label(loaded_kg: f64, report: &AnalysisReport) -> String {
    let limit_kg = report
        .geometry_summary
        .get("analysis_mtow_limit_kg")
        .copied()
        .filter(|kg| kg.is_finite() && *kg > 0.0);
    match limit_kg {
        Some(limit) if (limit - loaded_kg).abs() > 0.005 * limit => format!(
            "Loaded takeoff mass = {:.1} t, MTOW {:.1} t",
            loaded_kg / 1000.0,
            limit / 1000.0
        ),
        _ => format!("Takeoff mass = MTOW = {:.1} t", loaded_kg / 1000.0),
    }
}

/// Draw the stacked OEW, payload, fuel, and MTOW mass breakdown figure.
pub fn figure_mass_breakdown(report: &AnalysisReport, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let masses = &report.component_masses;

    if masses.is_empty() {
        let mut scene = Scene::new(760.0, 460.0, Some(Color::from_hex(pal.bg)));
        scene.add(SceneElement::Text {
            text: "No mass data available".to_owned(),
            pos: [380.0, 230.0],
            font_size: 12.0,
            color: Color::from_hex(pal.title),
            align: TextAlign::Center,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
        return scene;
    }

    let get = |k: &str| masses.get(k).copied().unwrap_or(0.0);
    let structure_vals: Vec<f64> = STRUCTURE_KEYS.iter().map(|&k| get(k)).collect();
    let other_vals: Vec<f64> = OTHER_KEYS.iter().map(|&k| get(k)).collect();
    let payload = get(PAYLOAD);
    let fuel = get(FUEL);

    let oew: f64 = structure_vals.iter().sum::<f64>() + other_vals.iter().sum::<f64>();
    let mzfw = oew + payload;
    let actual_mtow: f64 = masses.values().map(|&v| v.max(0.0)).sum();
    let safe_mtow = actual_mtow.max(1e-9);

    // --- Row 2 (structure): compute segment placement and stagger the
    // external labels before the axes are sized, matching upstream's flow.
    let mut segments: Vec<(f64, &str, &str)> = Vec::new();
    for (i, &v) in structure_vals.iter().enumerate() {
        if v > 0.0 {
            segments.push((v, STRUCTURE_COLORS[i], STRUCTURE_KEYS[i]));
        }
    }
    for (i, &v) in other_vals.iter().enumerate() {
        if v > 0.0 {
            segments.push((v, OTHER_COLORS[i], OTHER_KEYS[i]));
        }
    }

    let x_max_t = (actual_mtow / 1000.0 * 1.12).max(0.1);
    let px_per_t = PLOT_WIDTH_PX / x_max_t;
    let mut left_t = 0.0f64;
    let mut external: Vec<(f64, String)> = Vec::new();
    for &(val, _color, key) in &segments {
        let cx_t = left_t / 1000.0 + val / 2000.0;
        if !segment_label_fits(key, val, px_per_t) && val / safe_mtow > 0.004 {
            external.push((cx_t, format!("{key} {:.1}t", val / 1000.0)));
        }
        left_t += val;
    }

    let mut y_top_limit = 2.0 + BAR_HEIGHT / 2.0 + 0.2;
    let mut stagger_rows: Vec<usize> = Vec::new();
    if !external.is_empty() {
        let xs: Vec<f64> = external.iter().map(|&(cx, _)| cx).collect();
        // Labels on one row must not touch: the widest label plus a gap, in tonnes.
        let widest_px = external
            .iter()
            .map(|(_, label)| text_width_px(label, 7.5))
            .fold(0.0_f64, f64::max);
        let min_sep = (widest_px + 6.0) / px_per_t;
        stagger_rows = assign_label_rows(&xs, min_sep);
        let y_top = 2.0 + BAR_HEIGHT / 2.0;
        let max_row = stagger_rows.iter().copied().max().unwrap_or(0);
        y_top_limit = y_top + 0.16 + (max_row as f64) * 0.30 + 0.20;
    }

    let mut scene = Scene::new(760.0, 460.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Mass Breakdown: OEW -> MZFW -> Takeoff".to_owned());
    crate::chart_kit::draw_basis_subtitle(&mut scene, crate::chart_kit::MTOW_SIZING_BASIS, fuel, pal);
    let axes = Axes2D::new(
        (115.0, 60.0, PLOT_WIDTH_PX, 300.0),
        (0.0, x_max_t),
        (-BAR_HEIGHT / 2.0 - 0.15, y_top_limit),
    );
    axes.draw_frame_with_labels(&mut scene, pal, "Mass [t]", "");
    // The row names below are categorical labels, not a second numeric axis.
    // Clear the generated Y tick labels so they cannot collide with the long
    // row names at the left edge of the plot.
    scene.add(SceneElement::Rect {
        x: 0.0,
        y: 40.0,
        width: axes.left - 2.0,
        height: axes.height + 35.0,
        rx: 0.0,
        fill: Some(crate::scene::Fill::new(Color::from_hex(pal.bg))),
        stroke: None,
    });

    // The three bars are a sequence rather than three anonymous numeric
    // rows. Keeping their names outside the colored segments makes the chart
    // readable when a component is too small to carry an inline label.
    for (row, label) in [
        (2.0, "Components"),
        (1.0, "OEW -> MZFW"),
        (0.0, "MZFW -> Takeoff"),
    ] {
        let point = axes.map_point(0.0, row);
        scene.add(SceneElement::Text {
            text: label.to_owned(),
            pos: [axes.left - 10.0, point[1]],
            font_size: 8.5,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Right,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: true,
        });
    }

    // Row 2: structure breakdown segments.
    let mut left_t = 0.0f64;
    for &(val, color, key) in &segments {
        draw_hbar(
            &mut scene,
            &axes,
            2.0,
            left_t / 1000.0,
            val / 1000.0,
            Color::from_hex(color),
        );
        let cx_t = left_t / 1000.0 + val / 2000.0;
        if segment_label_fits(key, val, px_per_t) {
            bar_label(
                &mut scene,
                &axes,
                cx_t,
                2.0,
                key,
                &format!("{:.1}t", val / 1000.0),
                7.5,
                Color::from_hex(color),
            );
        }
        left_t += val;
    }
    // External (staggered) labels with leader lines.
    let y_top = 2.0 + BAR_HEIGHT / 2.0;
    // Leader foot of every label, and the text centre that keeps its text clear of the leaders of
    // the labels above it: a leader rises through every row above its own, so a higher label's
    // leader that falls inside a lower label's text is crossed by it. The lower label's text is
    // moved sideways, away from that leader, and joined to its own leader by the elbow line.
    let foot_x: Vec<f64> = external
        .iter()
        .map(|(cx_t, _)| axes.map_point(*cx_t, y_top)[0])
        .collect();
    let halves: Vec<f64> = external
        .iter()
        .map(|(_, label)| 0.5 * text_width_px(label, 7.5))
        .collect();
    let mut text_x = foot_x.clone();
    for i in 0..external.len() {
        let row_i = stagger_rows.get(i).copied().unwrap_or(0);
        for j in 0..external.len() {
            let row_j = stagger_rows.get(j).copied().unwrap_or(0);
            if row_j <= row_i {
                continue;
            }
            let leader = foot_x[j];
            if (leader - text_x[i]).abs() < halves[i] + 3.0 {
                text_x[i] = if leader >= foot_x[i] {
                    leader - 4.0 - halves[i]
                } else {
                    leader + 4.0 + halves[i]
                };
            }
        }
    }
    for (i, (cx_t, label)) in external.iter().enumerate() {
        let row = stagger_rows.get(i).copied().unwrap_or(0);
        let y_text = y_top + 0.16 + (row as f64) * 0.30;
        let p0 = axes.map_point(*cx_t, y_top);
        let p1 = axes.map_point(*cx_t, y_text);
        scene.add(SceneElement::Line {
            p1: p0,
            p2: p1,
            stroke: Stroke::new(Color::from_hex("#999999"), 0.8),
        });
        // The text is kept inside the plot frame; its leader still ends at the
        // segment it names.
        let half = halves[i];
        let x_text = text_x[i].clamp(axes.left + half, axes.left + axes.width - half);
        if (x_text - p1[0]).abs() > 0.5 {
            scene.add(SceneElement::Line {
                p1,
                p2: [x_text, p1[1]],
                stroke: Stroke::new(Color::from_hex("#999999"), 0.8),
            });
        }
        scene.add(SceneElement::Text {
            text: label.clone(),
            pos: [x_text, p1[1] - 2.0],
            font_size: 7.5,
            color: Color::from_hex(pal.title),
            align: TextAlign::Center,
            baseline: TextBaseline::Bottom,
            angle_deg: 0.0,
            bold: true,
        });
    }

    // Row 1: OEW -> MZFW.
    draw_hbar(
        &mut scene,
        &axes,
        1.0,
        0.0,
        oew / 1000.0,
        Color::from_hex(pal.panel),
    );
    draw_hbar(
        &mut scene,
        &axes,
        1.0,
        oew / 1000.0,
        payload / 1000.0,
        Color::from_hex(PAYLOAD_COLOR),
    );
    bar_label(
        &mut scene,
        &axes,
        oew / 2000.0,
        1.0,
        "OEW",
        &format!("{:.1} t", oew / 1000.0),
        8.5,
        Color::from_hex(pal.panel),
    );
    fitted_bar_label(
        &mut scene,
        &axes,
        (oew / 1000.0, payload / 1000.0),
        1.0,
        ("Payload", &format!("{:.1} t", payload / 1000.0)),
        8.5,
        Color::from_hex(PAYLOAD_COLOR),
        Color::from_hex(pal.title),
    );

    // Row 0: MZFW -> MTOW, with fuel or a deficit warning.
    draw_hbar(
        &mut scene,
        &axes,
        0.0,
        0.0,
        mzfw / 1000.0,
        Color::from_hex(pal.panel),
    );
    if fuel >= 0.0 {
        draw_hbar(
            &mut scene,
            &axes,
            0.0,
            mzfw / 1000.0,
            fuel / 1000.0,
            Color::from_hex(FUEL_COLOR),
        );
        fitted_bar_label(
            &mut scene,
            &axes,
            (mzfw / 1000.0, fuel / 1000.0),
            0.0,
            ("Fuel", &format!("{:.1} t", fuel / 1000.0)),
            8.5,
            Color::from_hex(FUEL_COLOR),
            Color::from_hex(pal.title),
        );
    } else {
        let deficit = -fuel;
        draw_hbar(
            &mut scene,
            &axes,
            0.0,
            mzfw / 1000.0,
            deficit / 1000.0,
            Color::from_hex(FUEL_NEG_COLOR),
        );
        fitted_bar_label(
            &mut scene,
            &axes,
            (mzfw / 1000.0, deficit / 1000.0),
            0.0,
            ("DEFICIT", &format!("{:.1} t", deficit / 1000.0)),
            8.5,
            Color::from_hex(FUEL_NEG_COLOR),
            Color::from_hex(FUEL_NEG_COLOR),
        );
    }
    bar_label(
        &mut scene,
        &axes,
        mzfw / 2000.0,
        0.0,
        "MZFW",
        &format!("{:.1} t", mzfw / 1000.0),
        8.5,
        Color::from_hex(pal.panel),
    );

    // Reference MTOW line.
    let p0 = axes.map_point(actual_mtow / 1000.0, -BAR_HEIGHT / 2.0 - 0.15);
    let p1 = axes.map_point(actual_mtow / 1000.0, y_top_limit);
    scene.add(SceneElement::Line {
        p1: p0,
        p2: p1,
        stroke: Stroke::dashed(Color::from_hex("#e74c3c"), 2.0, 6.0, 4.0),
    });
    scene.add(SceneElement::Text {
        text: loaded_mass_label(actual_mtow, report),
        pos: [p1[0], p1[1] - 4.0],
        font_size: 8.5,
        color: Color::from_hex("#e74c3c"),
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });

    scene
}

/// Approximate advance of one bold character, as a share of the font size.
const CHAR_EM: f64 = 0.62;

/// Width of the plot area in scene pixels.
const PLOT_WIDTH_PX: f64 = 595.0;

/// Rendered width in scene pixels of one bold line at `font` points (the scene is in pixels,
/// font sizes in points).
fn text_width_px(text: &str, font: f64) -> f64 {
    font * crate::scene::CSS_PIXELS_PER_POINT * CHAR_EM * text.chars().count() as f64
}

/// Whether the two-line label of a Components segment (name over mass) fits inside the segment.
fn segment_label_fits(key: &str, value_kg: f64, px_per_t: f64) -> bool {
    let value = format!("{:.1}t", value_kg / 1000.0);
    let widest = text_width_px(key, 7.5).max(text_width_px(&value, 7.5));
    widest + 6.0 <= value_kg / 1000.0 * px_per_t
}

/// Label a bar segment inside it when the text fits its width, else on one
/// line above the bar, kept inside the plot frame.
// Segment, row, text, font and fill are independent inputs, as in `bar_label`.
#[allow(clippy::too_many_arguments)]
fn fitted_bar_label(
    scene: &mut Scene,
    axes: &Axes2D,
    segment_t: (f64, f64),
    row: f64,
    names: (&str, &str),
    font: f64,
    fill: Color,
    outside_ink: Color,
) {
    let (start_t, width_t) = segment_t;
    let cx = start_t + 0.5 * width_t;
    let width_px = axes.map_point(start_t + width_t, row)[0] - axes.map_point(start_t, row)[0];
    let needed = text_width_px(names.0, font).max(text_width_px(names.1, font));
    if needed <= width_px - 4.0 {
        bar_label(scene, axes, cx, row, names.0, names.1, font, fill);
        return;
    }
    let text = format!("{} {}", names.0, names.1);
    let half = 0.5 * text_width_px(&text, font);
    let top = axes.map_point(cx, row + BAR_HEIGHT / 2.0);
    let x = top[0].clamp(axes.left + half, axes.left + axes.width - half);
    scene.add(SceneElement::Text {
        text,
        pos: [x, top[1] - 3.0],
        font_size: font,
        color: outside_ink,
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
}
