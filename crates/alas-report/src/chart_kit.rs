// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Composable chart chrome built from [`crate::scene`] primitives: colorbars
//! and legends. Kept separate from `scene` so the primitive scene graph
//! stays backend-neutral while these helpers can grow without bloating it.

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

/// Draw a vertical colorbar: gradient strip, border, min/mid/max ticks, and
/// a horizontal label above it.
#[allow(clippy::too_many_arguments)]
pub fn draw_colorbar(
    scene: &mut Scene,
    rect: (f64, f64, f64, f64),
    cmap: Colormap,
    vmin: f64,
    vmax: f64,
    label: &str,
    pal: &Palette,
) {
    let (x, y, w, h) = rect;
    let steps = 64;
    for i in 0..steps {
        let t = i as f64 / (steps - 1) as f64;
        let color = cmap.sample(t);
        let seg_h = h / steps as f64;
        // High values at the top: sample increases as i increases, drawn bottom-up.
        let seg_y = y + h - (i as f64 + 1.0) * seg_h;
        scene.add(SceneElement::Rect {
            x,
            y: seg_y,
            width: w,
            height: seg_h + 0.5,
            rx: 0.0,
            fill: Some(Fill::new(color)),
            stroke: None,
        });
    }
    scene.add(SceneElement::Rect {
        x,
        y,
        width: w,
        height: h,
        rx: 0.0,
        fill: None,
        stroke: Some(Stroke::new(Color::from_hex(pal.spine), 1.0)),
    });

    let tick_labels = [
        (0.0, format_colorbar_tick(vmin, vmin, vmax)),
        (0.5, format_colorbar_tick((vmin + vmax) * 0.5, vmin, vmax)),
        (1.0, format_colorbar_tick(vmax, vmin, vmax)),
    ];
    for (frac, text) in &tick_labels {
        let ty = y + h - frac * h;
        scene.add(SceneElement::Text {
            text: text.clone(),
            pos: [x + w + 4.0, ty],
            font_size: 8.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }

    scene.add(SceneElement::Text {
        text: title_case_label(label),
        pos: [x + w * 0.5, y - 6.0],
        font_size: 9.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: false,
    });
}

fn format_colorbar_tick(value: f64, vmin: f64, vmax: f64) -> String {
    let magnitude = vmin.abs().max(vmax.abs()).max(value.abs());
    let decimals = if magnitude >= 1_000.0 {
        0
    } else if magnitude >= 100.0 {
        1
    } else if magnitude >= 10.0 {
        2
    } else {
        3
    };
    format!("{value:.decimals$}")
}

fn title_case_label(label: &str) -> String {
    let mut changed = false;
    label
        .chars()
        .flat_map(|character| {
            if !changed && character.is_alphabetic() {
                changed = true;
                character.to_uppercase().collect::<Vec<_>>()
            } else {
                vec![character]
            }
        })
        .collect()
}

/// Swatch style for one legend row.
#[derive(Debug, Clone)]
pub enum LegendMarker {
    /// A short line segment, styled with `stroke` (solid or dashed).
    Line(Stroke),
    /// A filled rectangular patch (for stacked-bar / filled-region legends).
    Patch(Color),
    /// A filled circular marker (for scatter series).
    Circle(Color),
}

/// Draw a top-left-anchored legend box: one row per `(label, marker)` entry.
pub fn draw_legend(
    scene: &mut Scene,
    pos: Point2D,
    entries: &[(String, LegendMarker)],
    pal: &Palette,
    font_size: f64,
) {
    let line_h = font_size + 6.0;
    for (i, (label, marker)) in entries.iter().enumerate() {
        let y = pos[1] + (i as f64) * line_h;
        let mid_y = y + font_size * 0.5;
        match marker {
            LegendMarker::Line(stroke) => {
                scene.add(SceneElement::Line {
                    p1: [pos[0], mid_y],
                    p2: [pos[0] + 18.0, mid_y],
                    stroke: stroke.clone(),
                });
            }
            LegendMarker::Patch(color) => {
                scene.add(SceneElement::Rect {
                    x: pos[0],
                    y,
                    width: 14.0,
                    height: font_size,
                    rx: 1.0,
                    fill: Some(Fill::new(*color)),
                    stroke: None,
                });
            }
            LegendMarker::Circle(color) => {
                scene.add(SceneElement::Circle {
                    center: [pos[0] + 7.0, mid_y],
                    radius: 5.0,
                    fill: Some(Fill::new(*color)),
                    stroke: None,
                });
            }
        }
        scene.add(SceneElement::Text {
            text: title_case_label(label),
            pos: [pos[0] + 24.0, mid_y],
            font_size,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }
}

/// Draw a single horizontal legend row.
///
/// Figure families use this for legends below a plot. The width estimate is
/// deliberately conservative because the backend-neutral scene has no text
/// measurement primitive; leaving a little extra space keeps the final label
/// inside the canvas in both SVG and raster backends.
pub fn draw_horizontal_legend(
    scene: &mut Scene,
    pos: Point2D,
    entries: &[(String, LegendMarker)],
    pal: &Palette,
    font_size: f64,
) {
    let estimated_width = entries.iter().fold(0.0, |width, (label, _)| {
        width + 42.0 + label.chars().count() as f64 * font_size * 1.20
    });
    let available_width = (scene.width - pos[0] - 4.0).max(1.0);
    draw_horizontal_legend_impl(
        scene,
        pos,
        entries,
        pal,
        font_size,
        entries.len() > 2 || estimated_width > available_width,
    );
}

/// Draw a horizontal legend in fixed columns, keeping long localized labels
/// from colliding while retaining one centered row of entries.
pub fn draw_horizontal_legend_columns(
    scene: &mut Scene,
    pos: Point2D,
    entries: &[(String, LegendMarker)],
    pal: &Palette,
    font_size: f64,
) {
    draw_horizontal_legend_impl(scene, pos, entries, pal, font_size, true);
}

fn draw_horizontal_legend_impl(
    scene: &mut Scene,
    pos: Point2D,
    entries: &[(String, LegendMarker)],
    pal: &Palette,
    font_size: f64,
    use_columns: bool,
) {
    if entries.is_empty() {
        return;
    }
    let available_width = (scene.width - pos[0] - 4.0).max(1.0);
    let column_width = available_width / entries.len() as f64;
    let wrap_label = |label: &str, max_width: f64| {
        let max_chars = (max_width / (font_size * 0.70)).floor().max(1.0) as usize;
        let mut lines = Vec::new();
        let mut current = String::new();
        for word in label.split_whitespace() {
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
    };
    let mut x = pos[0];
    let mid_y = pos[1] + font_size * 0.5;
    for (index, (label, marker)) in entries.iter().enumerate() {
        if use_columns {
            x = pos[0] + index as f64 * column_width;
        }
        let label = title_case_label(label);
        let max_label_width = if use_columns {
            (column_width - 28.0).max(1.0)
        } else {
            (scene.width - x - 28.0).max(1.0)
        };
        let wrapped_label = wrap_label(&label, max_label_width);
        match marker {
            LegendMarker::Line(stroke) => scene.add(SceneElement::Line {
                p1: [x, mid_y],
                p2: [x + 18.0, mid_y],
                stroke: stroke.clone(),
            }),
            LegendMarker::Patch(color) => scene.add(SceneElement::Rect {
                x,
                y: pos[1],
                width: 14.0,
                height: font_size,
                rx: 1.0,
                fill: Some(Fill::new(*color)),
                stroke: None,
            }),
            LegendMarker::Circle(color) => scene.add(SceneElement::Circle {
                center: [x + 7.0, mid_y],
                radius: 5.0,
                fill: Some(Fill::new(*color)),
                stroke: None,
            }),
        }
        scene.add(SceneElement::Text {
            text: wrapped_label.clone(),
            pos: [x + 24.0, mid_y],
            font_size,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
        if !use_columns {
            let longest_line = wrapped_label
                .lines()
                .map(str::len)
                .max()
                .unwrap_or_default();
            x += 42.0 + longest_line as f64 * font_size * 0.90;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Scene;
    use crate::theme::PALETTE_LIGHT;

    #[test]
    fn colorbar_emits_gradient_border_and_three_ticks_plus_label() {
        let mut scene = Scene::new(200.0, 200.0, None);
        draw_colorbar(
            &mut scene,
            (10.0, 10.0, 20.0, 100.0),
            Colormap::Viridis,
            0.0,
            1.0,
            "Mach",
            &PALETTE_LIGHT,
        );
        let rects = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Rect { .. }))
            .count();
        let texts = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Text { .. }))
            .count();
        assert_eq!(rects, 65); // 64 gradient segments + 1 border
        assert_eq!(texts, 4); // 3 ticks + 1 rotated label
    }

    #[test]
    fn colorbar_label_is_horizontal_and_above_the_gradient() {
        let mut scene = Scene::new(220.0, 200.0, None);
        draw_colorbar(
            &mut scene,
            (10.0, 10.0, 20.0, 100.0),
            Colormap::Viridis,
            215_000.0,
            254_000.0,
            "Total Mass (kg)",
            &PALETTE_LIGHT,
        );
        let mut label = None;
        for element in &scene.elements {
            if let SceneElement::Text {
                text,
                pos,
                angle_deg,
                ..
            } = element
            {
                if text == "Total Mass (kg)" {
                    label = Some((*pos, *angle_deg));
                }
            }
        }
        assert_eq!(label, Some(([20.0, 4.0], 0.0)));
    }

    #[test]
    fn legend_emits_one_swatch_and_label_per_entry() {
        let mut scene = Scene::new(200.0, 200.0, None);
        let entries = vec![
            (
                "baseline".to_owned(),
                LegendMarker::Line(Stroke::new(Color::rgb(0, 0, 0), 1.0)),
            ),
            (
                "optimized".to_owned(),
                LegendMarker::Circle(Color::rgb(255, 0, 0)),
            ),
        ];
        draw_legend(&mut scene, [5.0, 5.0], &entries, &PALETTE_LIGHT, 10.0);
        assert_eq!(scene.elements.len(), 4); // 2 markers + 2 text labels
        assert!(matches!(
            scene.elements.last(),
            Some(SceneElement::Text { text, .. }) if text == "Optimized"
        ));
    }

    #[test]
    fn horizontal_legend_keeps_labels_unrotated_and_capitalized() {
        let mut scene = Scene::new(320.0, 100.0, None);
        draw_horizontal_legend(
            &mut scene,
            [10.0, 70.0],
            &[(
                "best so far".to_owned(),
                LegendMarker::Line(Stroke::new(Color::rgb(0, 0, 0), 1.0)),
            )],
            &PALETTE_LIGHT,
            10.0,
        );
        assert!(scene.elements.iter().any(|element| matches!(
            element,
            SceneElement::Text { text, angle_deg, .. }
                if text == "Best so far" && *angle_deg == 0.0
        )));
    }

    #[test]
    fn horizontal_legend_wraps_a_long_label_inside_the_canvas() {
        let mut scene = Scene::new(180.0, 100.0, None);
        draw_horizontal_legend(
            &mut scene,
            [10.0, 70.0],
            &[(
                "minimum nose load needed for steering authority".to_owned(),
                LegendMarker::Line(Stroke::new(Color::rgb(0, 0, 0), 1.0)),
            )],
            &PALETTE_LIGHT,
            10.0,
        );
        assert!(scene.elements.iter().any(|element| matches!(
            element,
            SceneElement::Text { text, angle_deg, .. }
                if text.contains('\n') && *angle_deg == 0.0
        )));
    }

    #[test]
    fn axes_emit_grid_ticks_numeric_labels_and_axis_labels() {
        let axes = Axes2D::new((60.0, 20.0, 240.0, 150.0), (0.0, 100.0), (-2.0, 8.0));
        let mut scene = Scene::new(360.0, 240.0, None);
        draw_axes(
            &axes,
            &mut scene,
            &PALETTE_LIGHT,
            Some("Distance [m]"),
            Some("Load [kN]"),
        );

        let lines = scene
            .elements
            .iter()
            .filter(|element| matches!(element, SceneElement::Line { .. }))
            .count();
        let labels = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        // Six-ish vertical and horizontal grids, with a tick mark for every
        // numeric label, proves this is more than the old rectangle-only
        // frame. Exact counts intentionally remain flexible as nice-step
        // rounding changes with chart dimensions.
        assert!(lines >= 12);
        assert!(labels.contains(&"Distance [m]"));
        assert!(labels.contains(&"Load [kN]"));
        assert!(labels.contains(&"0"));
        assert!(labels.contains(&"4"));
    }

    #[test]
    fn logarithmic_axes_use_decade_tick_labels() {
        let axes = Axes2D::new((30.0, 20.0, 240.0, 120.0), (1.0, 1_000.0), (0.0, 1.0))
            .with_x_scale(Scale::Log10);
        let mut scene = Scene::new(300.0, 180.0, None);
        draw_axes(&axes, &mut scene, &PALETTE_LIGHT, None, None);
        let labels = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(labels.contains(&"1"));
        assert!(labels.contains(&"100"));
        assert!(labels.contains(&"1000"));
    }

    #[test]
    fn chart_chrome_supports_titles_annotations_categories_and_responsive_margins() {
        let margins = ChartMargins::for_canvas(320.0, 220.0);
        let rect = margins.plot_rect(320.0, 220.0);
        let axes = Axes2D::new(rect, (0.0, 2.0), (0.0, 1.0));
        let mut scene = Scene::new(320.0, 220.0, None);
        draw_title(&mut scene, "Mission profile", &PALETTE_LIGHT);
        draw_annotation(&axes, &mut scene, "cruise", (1.0, 0.5), &PALETTE_LIGHT);
        draw_categorical_x_axis(&axes, &mut scene, &["OEW", "MZFW", "MTOW"], &PALETTE_LIGHT);
        assert!(scene.elements.iter().any(|element| matches!(
            element,
            SceneElement::Text { text, bold: true, .. } if text == "Mission profile"
        )));
        assert!(scene.elements.iter().any(|element| matches!(
            element,
            SceneElement::Text { text, .. } if text == "cruise"
        )));
        assert!(scene.elements.iter().any(|element| matches!(
            element,
            SceneElement::Text { text, .. } if text == "MZFW"
        )));
    }
}
