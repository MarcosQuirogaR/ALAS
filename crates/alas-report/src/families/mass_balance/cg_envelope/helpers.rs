// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py:figure_cg_envelope (L2354-2835)
// Reference: alas @ rust-port-baseline.

use super::super::with_alpha;
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
/// Evenly spaced samples over `[a, b]`, inclusive of both ends -- `np.linspace`.
pub(super) fn linspace(a: f64, b: f64, n: usize) -> Vec<f64> {
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![a];
    }
    let step = (b - a) / (n - 1) as f64;
    (0..n).map(|i| a + step * i as f64).collect()
}

/// Linear interpolation over an ascending `xs`, clamped at the ends -- `np.interp`.
pub(super) fn interp(x: f64, xs: &[f64], ys: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    if x <= xs[0] {
        return ys[0];
    }
    let last = xs.len() - 1;
    if x >= xs[last] {
        return ys[last];
    }
    for i in 0..last {
        if x >= xs[i] && x <= xs[i + 1] {
            let span = xs[i + 1] - xs[i];
            let t = if span.abs() > 1e-12 {
                (x - xs[i]) / span
            } else {
                0.0
            };
            return ys[i] + t * (ys[i + 1] - ys[i]);
        }
    }
    ys[last]
}

/// Draw a dash-dot-approximated limit curve plus a label anchored at the
/// curve's crossing of `y_target` (in kg), interpolated from the real curve
/// data -- upstream's `ax.plot(...)` + `ax.annotate(...)` pair.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_curve_with_label(
    scene: &mut Scene,
    axes: &Axes2D,
    w_calc: &[f64],
    curve_pct: &[f64],
    color: &str,
    dash: f64,
    gap: f64,
    y_target: f64,
    label: &str,
    label_color: &str,
    dx: f64,
) {
    let stroke = Stroke::dashed(Color::from_hex(color), 1.5, dash, gap);
    let series: Vec<(f64, f64)> = w_calc
        .iter()
        .zip(curve_pct)
        .map(|(&w, &c)| (c, w / 1000.0))
        .collect();
    axes.add_line_series(scene, &series, stroke);

    if !label.is_empty() {
        let x_target = interp(y_target, w_calc, curve_pct);
        let p = axes.map_point(
            x_target.clamp(axes.x_min, axes.x_max),
            (y_target / 1000.0).clamp(axes.y_min, axes.y_max),
        );
        scene.add(SceneElement::Text {
            text: label.to_owned(),
            pos: [p[0] + dx, p[1]],
            font_size: 9.0,
            color: Color::from_hex(label_color),
            align: if dx < 0.0 {
                TextAlign::Right
            } else {
                TextAlign::Left
            },
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }
}

/// Draw a vertical limit line spanning the whole plot height plus a horizontal
/// label. `dash`/`gap` of `0.0` draws a solid line (the TIP-OVER boundary).
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_vline(
    scene: &mut Scene,
    axes: &Axes2D,
    x_pct: f64,
    color: &str,
    width: f64,
    dash: f64,
    gap: f64,
    alpha: f64,
    label: &str,
    label_offset_y: f64,
) {
    let base = Color::from_hex(color);
    let stroke = if dash > 0.0 {
        Stroke::dashed(with_alpha(base, alpha), width, dash, gap)
    } else {
        Stroke::new(with_alpha(base, alpha), width)
    };
    axes.add_line_series(scene, &[(x_pct, axes.y_min), (x_pct, axes.y_max)], stroke);
    let p = axes.map_point(x_pct.clamp(axes.x_min, axes.x_max), axes.y_max);
    let on_right = p[0] > axes.left + axes.width * 0.78;
    scene.add(SceneElement::Text {
        text: label.to_owned(),
        pos: [
            p[0] + if on_right { -4.0 } else { 4.0 },
            p[1] + 4.0 + label_offset_y,
        ],
        font_size: 9.0,
        color: base,
        align: if on_right {
            TextAlign::Right
        } else {
            TextAlign::Left
        },
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: true,
    });
}

/// Draw one loading-trajectory polyline with a circular marker at each
/// sampled point -- upstream's `ax.plot(..., marker="o")`.
pub(super) fn draw_trajectory(
    scene: &mut Scene,
    axes: &Axes2D,
    cg_pct: &[f64],
    weight_kg: &[f64],
    stroke: Stroke,
) {
    let series: Vec<(f64, f64)> = cg_pct
        .iter()
        .zip(weight_kg)
        .map(|(&c, &w)| (c, w / 1000.0))
        .collect();
    axes.add_line_series(scene, &series, stroke.clone());
    for &(c, w) in &series {
        let p = axes.map_point(
            c.clamp(axes.x_min, axes.x_max),
            w.clamp(axes.y_min, axes.y_max),
        );
        scene.add(SceneElement::Circle {
            center: p,
            radius: 3.0,
            fill: Some(Fill::new(stroke.color)),
            stroke: None,
        });
    }
}

/// Small bold annotation offset a fixed pixel amount from a data point --
/// upstream's `ax.annotate(..., textcoords="offset points")`.
// The figure API keeps each annotation property explicit to mirror the source call site.
#[allow(clippy::too_many_arguments)]
pub(super) fn annotate(
    scene: &mut Scene,
    axes: &Axes2D,
    x_pct: f64,
    w_tonnes: f64,
    label: &str,
    color: &str,
    dx: f64,
    dy: f64,
) {
    let p = axes.map_point(
        x_pct.clamp(axes.x_min, axes.x_max),
        w_tonnes.clamp(axes.y_min, axes.y_max),
    );
    scene.add(SceneElement::Text {
        text: label.to_owned(),
        pos: [p[0] + dx, p[1] + dy],
        font_size: 8.0,
        color: Color::from_hex(color),
        align: TextAlign::Left,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: true,
    });
}
