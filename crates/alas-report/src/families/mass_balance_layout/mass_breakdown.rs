// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py:figure_mass_breakdown (L2105-2353)
// Reference: alas @ rust-port-baseline.

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
    let inline_frac = 0.12;
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

    let mut left_t = 0.0f64;
    let mut external: Vec<(f64, String)> = Vec::new();
    for &(val, _color, key) in &segments {
        let cx_t = left_t / 1000.0 + val / 2000.0;
        if val / safe_mtow <= inline_frac && val / safe_mtow > 0.015 {
            external.push((cx_t, format!("{key} {:.1}t", val / 1000.0)));
        }
        left_t += val;
    }

    let mut y_top_limit = 2.0 + BAR_HEIGHT / 2.0 + 0.2;
    let mut stagger_rows: Vec<usize> = Vec::new();
    if !external.is_empty() {
        let xs: Vec<f64> = external.iter().map(|&(cx, _)| cx).collect();
        let min_sep = safe_mtow / 1000.0 * 0.15;
        stagger_rows = assign_label_rows(&xs, min_sep);
        let y_top = 2.0 + BAR_HEIGHT / 2.0;
        let max_row = stagger_rows.iter().copied().max().unwrap_or(0);
        y_top_limit = y_top + 0.16 + (max_row as f64) * 0.30 + 0.20;
    }

    let x_max_t = (actual_mtow / 1000.0 * 1.12).max(0.1);
    let mut scene = Scene::new(760.0, 460.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Mass Breakdown: OEW -> MZFW -> MTOW".to_owned());
    let axes = Axes2D::new(
        (115.0, 60.0, 595.0, 300.0),
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
        (0.0, "MZFW -> MTOW"),
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
        if val / safe_mtow > inline_frac {
            bar_label(
                &mut scene,
                &axes,
                cx_t,
                2.0,
                key,
                &format!("{:.1}t", val / 1000.0),
                7.5,
            );
        }
        left_t += val;
    }
    // External (staggered) labels with leader lines.
    let y_top = 2.0 + BAR_HEIGHT / 2.0;
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
        scene.add(SceneElement::Text {
            text: label.clone(),
            pos: [p1[0], p1[1] - 2.0],
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
    );
    bar_label(
        &mut scene,
        &axes,
        (oew + payload / 2.0) / 1000.0,
        1.0,
        "Payload",
        &format!("{:.1} t", payload / 1000.0),
        8.5,
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
        bar_label(
            &mut scene,
            &axes,
            (mzfw + fuel / 2.0) / 1000.0,
            0.0,
            "Fuel",
            &format!("{:.1} t", fuel / 1000.0),
            8.5,
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
        bar_label(
            &mut scene,
            &axes,
            (mzfw + deficit / 2.0) / 1000.0,
            0.0,
            "DEFICIT",
            &format!("{:.1} t", deficit / 1000.0),
            8.5,
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
        text: format!("MTOW = {:.1} t", actual_mtow / 1000.0),
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
