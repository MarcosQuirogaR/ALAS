// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};

pub(super) const BAR_HEIGHT: f64 = 0.5;
/// Greedily assign a row index to each (already x-sorted) label so any two
/// labels sharing a row are at least `min_sep` apart -- `_assign_label_rows`.
pub(super) fn assign_label_rows(xs: &[f64], min_sep: f64) -> Vec<usize> {
    let mut last_x_per_row: Vec<f64> = Vec::new();
    let mut rows = Vec::with_capacity(xs.len());
    for &x in xs {
        let mut placed = false;
        for (row, last_x) in last_x_per_row.iter_mut().enumerate() {
            if x - *last_x >= min_sep {
                *last_x = x;
                rows.push(row);
                placed = true;
                break;
            }
        }
        if !placed {
            last_x_per_row.push(x);
            rows.push(last_x_per_row.len() - 1);
        }
    }
    rows
}

pub(super) fn draw_hbar(
    scene: &mut Scene,
    axes: &Axes2D,
    row: f64,
    left: f64,
    val: f64,
    fill: Color,
) {
    let p_tl = axes.map_point(left, row + BAR_HEIGHT / 2.0);
    let p_br = axes.map_point(left + val, row - BAR_HEIGHT / 2.0);
    scene.add(SceneElement::Rect {
        x: p_tl[0],
        y: p_tl[1],
        width: (p_br[0] - p_tl[0]).max(0.5),
        height: p_br[1] - p_tl[1],
        rx: 0.0,
        fill: Some(Fill::new(fill)),
        stroke: Some(Stroke::new(Color::rgb(255, 255, 255), 1.0)),
    });
}

/// Two centered, stacked text lines (name + value) inside a bar segment.
pub(super) fn bar_label(
    scene: &mut Scene,
    axes: &Axes2D,
    cx: f64,
    row: f64,
    line1: &str,
    line2: &str,
    font: f64,
) {
    let center = axes.map_point(cx, row);
    scene.add(SceneElement::Text {
        text: line1.to_owned(),
        pos: [center[0], center[1] - font * 0.55],
        font_size: font,
        color: Color::rgb(255, 255, 255),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: true,
    });
    scene.add(SceneElement::Text {
        text: line2.to_owned(),
        pos: [center[0], center[1] + font * 0.65],
        font_size: font,
        color: Color::rgb(255, 255, 255),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: true,
    });
}
