// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use crate::colormap::Colormap;
use crate::scene::{
    Axes2D, Color, Fill, Point2D, Scale, Scene, SceneElement, Stroke, TextAlign, TextBaseline,
};
use crate::theme::Palette;

/// Responsive outer margins used by ordinary scientific charts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartMargins {
    /// Left margin, including numeric y tick labels.
    pub left: f64,
    /// Right margin, including legends or colorbars.
    pub right: f64,
    /// Top margin, including the title.
    pub top: f64,
    /// Bottom margin, including x tick and axis labels.
    pub bottom: f64,
}

impl ChartMargins {
    /// Choose readable margins without allowing a small card to waste its plot area.
    pub fn for_canvas(width: f64, height: f64) -> Self {
        let scale = (width.min(height) / 600.0).clamp(0.75, 1.35);
        Self {
            left: 52.0 * scale,
            right: 18.0 * scale,
            top: 38.0 * scale,
            bottom: 45.0 * scale,
        }
    }

    /// Return the plot rectangle inside a canvas of the supplied size.
    pub fn plot_rect(self, width: f64, height: f64) -> (f64, f64, f64, f64) {
        (
            self.left,
            self.top,
            (width - self.left - self.right).max(1.0),
            (height - self.top - self.bottom).max(1.0),
        )
    }
}

/// Add a chart title using the palette's title color and shared typography.
pub fn draw_title(scene: &mut Scene, title: &str, pal: &Palette) {
    if title.trim().is_empty() {
        return;
    }
    scene.add(SceneElement::Text {
        text: title.to_owned(),
        pos: [scene.width * 0.5, 18.0],
        font_size: 13.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: true,
    });
}

/// Add a bounded annotation anchored to a data coordinate.
pub fn draw_annotation(
    axes: &Axes2D,
    scene: &mut Scene,
    text: &str,
    data_pos: (f64, f64),
    pal: &Palette,
) {
    if text.trim().is_empty() {
        return;
    }
    let pos = axes.map_point(data_pos.0, data_pos.1);
    let pos = [
        pos[0].clamp(axes.left, axes.left + axes.width),
        pos[1].clamp(axes.top, axes.top + axes.height),
    ];
    scene.add(SceneElement::Text {
        text: text.to_owned(),
        pos,
        font_size: 9.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Left,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });
}

/// Draw labels for a categorical x axis without pretending categories are numeric.
pub fn draw_categorical_x_axis(axes: &Axes2D, scene: &mut Scene, labels: &[&str], pal: &Palette) {
    if labels.is_empty() {
        return;
    }
    let spine = Color::from_hex(pal.spine);
    let tick = Color::from_hex(pal.tick);
    let denominator = (labels.len() - 1).max(1) as f64;
    for (index, label) in labels.iter().enumerate() {
        let x = axes.left + axes.width * index as f64 / denominator;
        scene.add(SceneElement::Line {
            p1: [x, axes.top + axes.height],
            p2: [x, axes.top + axes.height + 4.0],
            stroke: Stroke::new(spine, 1.0),
        });
        scene.add(SceneElement::Text {
            text: (*label).to_owned(),
            pos: [x, axes.top + axes.height + 7.0],
            font_size: 8.5,
            color: tick,
            align: TextAlign::Center,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: false,
        });
    }
}

pub(crate) fn equal_aspect_ranges(axes: &Axes2D) -> (f64, f64, f64, f64) {
    let x_span = (axes.x_max - axes.x_min).abs().max(1e-12);
    let y_span = (axes.y_max - axes.y_min).abs().max(1e-12);
    let target_y = x_span * axes.height / axes.width.max(1e-12);
    let target_x = y_span * axes.width / axes.height.max(1e-12);
    if target_y > y_span {
        let center = (axes.y_min + axes.y_max) * 0.5;
        (
            axes.x_min,
            axes.x_max,
            center - target_y * 0.5,
            center + target_y * 0.5,
        )
    } else {
        let center = (axes.x_min + axes.x_max) * 0.5;
        (
            center - target_x * 0.5,
            center + target_x * 0.5,
            axes.y_min,
            axes.y_max,
        )
    }
}

/// Draw the reusable scientific-chart chrome for an [`Axes2D`].
///
/// The scene graph intentionally has no text measurement or layout engine, so
/// axes live here rather than in individual figure families.  Every figure
/// gets the same readable major ticks, light grid, and numeric labels while
/// retaining the simple backend-neutral primitives used by SVG and egui.
/// `x_label` and `y_label` are optional because small inset plots often have
/// enough context from their panel title alone.
pub fn draw_axes(
    axes: &Axes2D,
    scene: &mut Scene,
    pal: &Palette,
    x_label: Option<&str>,
    y_label: Option<&str>,
) {
    draw_axes_configured(axes, scene, pal, x_label, y_label, true);
}

/// Draw chart axes while keeping the x-axis grid but omitting numeric x ticks.
///
/// Categorical bars can carry their own labels below the frame; numeric x
/// ticks in that case add visual noise without conveying a quantity.
pub fn draw_axes_without_x_tick_labels(
    axes: &Axes2D,
    scene: &mut Scene,
    pal: &Palette,
    x_label: Option<&str>,
    y_label: Option<&str>,
) {
    draw_axes_configured(axes, scene, pal, x_label, y_label, false);
}

fn draw_axes_configured(
    axes: &Axes2D,
    scene: &mut Scene,
    pal: &Palette,
    x_label: Option<&str>,
    y_label: Option<&str>,
    show_x_tick_labels: bool,
) {
    let spine = Color::from_hex(pal.spine);
    let grid = Color::rgba(spine.r, spine.g, spine.b, 105);
    let tick_color = Color::from_hex(pal.tick);
    let frame_stroke = Stroke::new(spine, 1.0);
    let grid_stroke = Stroke::dashed(grid, 0.7, 2.0, 3.0);

    // Grid lines are emitted before the spine and before data series.  This
    // keeps the reference curves legible without requiring every family to
    // know how the chart background is rendered.
    let x_ticks = major_ticks(axes.x_min, axes.x_max, axes.x_scale, 6);
    let y_ticks = major_ticks(axes.y_min, axes.y_max, axes.y_scale, 6);
    let x_step = tick_step(axes.x_min, axes.x_max, axes.x_scale, 6);
    let y_step = tick_step(axes.y_min, axes.y_max, axes.y_scale, 6);

    for &value in &x_ticks {
        let p = axes.map_point(value, axes.y_min);
        scene.add(SceneElement::Line {
            p1: [p[0], axes.top],
            p2: [p[0], axes.top + axes.height],
            stroke: grid_stroke.clone(),
        });
    }
    for &value in &y_ticks {
        let p = axes.map_point(axes.x_min, value);
        scene.add(SceneElement::Line {
            p1: [axes.left, p[1]],
            p2: [axes.left + axes.width, p[1]],
            stroke: grid_stroke.clone(),
        });
    }

    scene.add(SceneElement::Rect {
        x: axes.left,
        y: axes.top,
        width: axes.width,
        height: axes.height,
        rx: 0.0,
        fill: None,
        stroke: Some(frame_stroke),
    });

    // Tick marks and labels are deliberately separate from the frame.  This
    // makes it possible for a renderer to clip the plot interior while still
    // leaving labels outside the data rectangle.
    if show_x_tick_labels {
        for &value in &x_ticks {
            let p = axes.map_point(value, axes.y_min);
            scene.add(SceneElement::Line {
                p1: [p[0], axes.top + axes.height],
                p2: [p[0], axes.top + axes.height + 4.0],
                stroke: Stroke::new(spine, 1.0),
            });
            scene.add(SceneElement::Text {
                text: format_tick(value, x_step),
                pos: [p[0], axes.top + axes.height + 7.0],
                font_size: 8.5,
                color: tick_color,
                align: TextAlign::Center,
                baseline: TextBaseline::Top,
                angle_deg: 0.0,
                bold: false,
            });
        }
    }
    for &value in &y_ticks {
        let p = axes.map_point(axes.x_min, value);
        scene.add(SceneElement::Line {
            p1: [axes.left - 4.0, p[1]],
            p2: [axes.left, p[1]],
            stroke: Stroke::new(spine, 1.0),
        });
        scene.add(SceneElement::Text {
            text: format_tick(value, y_step),
            pos: [axes.left - 7.0, p[1]],
            font_size: 8.5,
            color: tick_color,
            align: TextAlign::Right,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }

    if show_x_tick_labels {
        if let Some(label) = x_label.filter(|label| !label.trim().is_empty()) {
            scene.add(SceneElement::Text {
                text: label.to_owned(),
                pos: [axes.left + axes.width * 0.5, axes.top + axes.height + 29.0],
                font_size: 10.0,
                color: tick_color,
                align: TextAlign::Center,
                baseline: TextBaseline::Top,
                angle_deg: 0.0,
                bold: false,
            });
        }
    }
    if let Some(label) = y_label.filter(|label| !label.trim().is_empty()) {
        scene.add(SceneElement::Text {
            text: label.to_owned(),
            pos: [axes.left - 38.0, axes.top + axes.height * 0.5],
            font_size: 10.0,
            color: tick_color,
            align: TextAlign::Center,
            baseline: TextBaseline::Middle,
            angle_deg: -90.0,
            bold: false,
        });
    }
}

fn tick_step(min: f64, max: f64, scale: Scale, target: usize) -> f64 {
    let (lo, hi) = ordered_range(min, max, scale);
    let span = (hi - lo).max(1e-12);
    let raw = span / target.max(2) as f64;
    if !raw.is_finite() || raw <= 0.0 {
        return 1.0;
    }
    let power = 10.0_f64.powf(raw.log10().floor());
    let normalized = raw / power;
    let multiplier = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    };
    (multiplier * power).max(1e-12)
}

fn major_ticks(min: f64, max: f64, scale: Scale, target: usize) -> Vec<f64> {
    let (lo, hi) = ordered_range(min, max, scale);
    if !lo.is_finite() || !hi.is_finite() || hi <= lo {
        return vec![lo];
    }
    if matches!(scale, Scale::Log10) {
        if hi <= 0.0 {
            return vec![lo, hi];
        }
        let log_lo = lo.max(1e-300).log10().ceil() as i32;
        let log_hi = hi.log10().floor() as i32;
        let mut ticks = (log_lo..=log_hi)
            .take(32)
            .map(|exponent| 10.0_f64.powi(exponent))
            .filter(|&value| value >= lo && value <= hi)
            .collect::<Vec<_>>();
        if ticks.is_empty() {
            ticks.push(lo);
            ticks.push(hi);
        }
        return ticks;
    }

    let step = tick_step(lo, hi, scale, target);
    let first = (lo / step).ceil() * step;
    let mut ticks = Vec::new();
    let mut value = first;
    for _ in 0..64 {
        if value > hi + step * 1e-9 {
            break;
        }
        if value >= lo - step * 1e-9 {
            ticks.push(if value.abs() < step * 1e-9 {
                0.0
            } else {
                value
            });
        }
        value += step;
    }
    if ticks.len() < 2 {
        ticks = vec![lo, hi];
    }
    ticks
}

fn ordered_range(min: f64, max: f64, scale: Scale) -> (f64, f64) {
    let mut lo = min.min(max);
    let hi = min.max(max);
    if matches!(scale, Scale::Log10) {
        lo = lo.max(1e-300);
    }
    if !lo.is_finite() || !hi.is_finite() {
        (0.0, 1.0)
    } else {
        (lo, hi)
    }
}

fn format_tick(value: f64, step: f64) -> String {
    let value = if value.abs() < step.abs() * 1e-9 {
        0.0
    } else {
        value
    };
    let abs_step = step.abs();
    if abs_step >= 1e6 || (abs_step > 0.0 && abs_step < 1e-4) {
        return format!("{value:.2e}");
    }
    let decimals = if abs_step >= 1.0 {
        0
    } else {
        (-abs_step.log10().floor() as i32 + 1).clamp(0, 8) as usize
    };
    format!("{value:.decimals$}")
}
