// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compact line charts for the Wing Analysis results.
//!
//! The window draws its curves directly with the egui painter rather than
//! building report scenes: these are live inspection plots inside one tool
//! window, and they must redraw at interactive rates while a user edits the
//! condition. Axis ranges are drawn from the data and labelled with their
//! actual end values, so nothing is implied by an unlabelled axis.

use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Sense, Stroke, Ui, Vec2};

/// Inner padding of a chart, in points: left, right, top, bottom.
const PAD: (f32, f32, f32, f32) = (54.0, 14.0, 16.0, 26.0);

/// One labelled curve.
pub(crate) struct Series<'a> {
    /// Legend text, already translated.
    pub label: String,
    /// Curve colour.
    pub color: Color32,
    /// Points in data coordinates, `[x, y]`.
    pub points: &'a [[f64; 2]],
}

/// Draw `series` in a box of `size`, with `x_label` and `y_label` stating the
/// quantity and its unit.
///
/// Returns the painted rectangle. An empty series set paints the frame and the
/// labels only, which is what an unsolved state should look like.
pub(crate) fn line_chart(
    ui: &mut Ui,
    size: Vec2,
    series: &[Series<'_>],
    x_label: &str,
    y_label: &str,
) -> Rect {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.rect_filled(rect, 4.0, visuals.extreme_bg_color);
    painter.rect_stroke(rect, 4.0, visuals.widgets.noninteractive.bg_stroke);
    let plot = Rect::from_min_max(
        pos2(rect.left() + PAD.0, rect.top() + PAD.2),
        pos2(rect.right() - PAD.1, rect.bottom() - PAD.3),
    );
    if plot.width() <= 1.0 || plot.height() <= 1.0 {
        return rect;
    }

    let mut bounds: Option<[f64; 4]> = None;
    for entry in series {
        for point in entry.points {
            let value = bounds.get_or_insert([point[0], point[0], point[1], point[1]]);
            value[0] = value[0].min(point[0]);
            value[1] = value[1].max(point[0]);
            value[2] = value[2].min(point[1]);
            value[3] = value[3].max(point[1]);
        }
    }
    let Some([mut x0, mut x1, mut y0, mut y1]) = bounds else {
        label_axes(ui, &painter, plot, x_label, y_label, None);
        return rect;
    };
    if (x1 - x0).abs() < 1.0e-12 {
        x0 -= 0.5;
        x1 += 0.5;
    }
    if (y1 - y0).abs() < 1.0e-12 {
        y0 -= 0.5;
        y1 += 0.5;
    }
    let margin = 0.05 * (y1 - y0);
    y0 -= margin;
    y1 += margin;
    let to_screen = |point: [f64; 2]| {
        let x = (point[0] - x0) / (x1 - x0);
        let y = (point[1] - y0) / (y1 - y0);
        pos2(
            plot.left() + plot.width() * x as f32,
            plot.bottom() - plot.height() * y as f32,
        )
    };

    let axis = visuals
        .widgets
        .noninteractive
        .fg_stroke
        .color
        .gamma_multiply(0.6);
    if y0 < 0.0 && y1 > 0.0 {
        let zero = to_screen([x0, 0.0]).y;
        painter.line_segment(
            [pos2(plot.left(), zero), pos2(plot.right(), zero)],
            Stroke::new(1.0_f32, axis),
        );
    }
    if x0 < 0.0 && x1 > 0.0 {
        let zero = to_screen([0.0, y0]).x;
        painter.line_segment(
            [pos2(zero, plot.top()), pos2(zero, plot.bottom())],
            Stroke::new(1.0_f32, axis),
        );
    }
    painter.rect_stroke(plot, 0.0, Stroke::new(1.0_f32, axis));

    for entry in series {
        let points: Vec<_> = entry.points.iter().map(|point| to_screen(*point)).collect();
        if points.len() >= 2 {
            painter.add(egui::Shape::line(points, Stroke::new(1.6_f32, entry.color)));
        } else if let Some(point) = points.first() {
            painter.circle_filled(*point, 2.5, entry.color);
        }
    }
    label_axes(ui, &painter, plot, x_label, y_label, Some([x0, x1, y0, y1]));

    let font = FontId::proportional(11.0);
    let mut legend = plot.left_top() + vec2(6.0, 4.0);
    for entry in series {
        painter.text(
            legend,
            Align2::LEFT_TOP,
            &entry.label,
            font.clone(),
            entry.color,
        );
        legend.y += 13.0;
    }
    rect
}

/// Axis titles and the four end values, so no axis is read by assumption.
fn label_axes(
    ui: &Ui,
    painter: &egui::Painter,
    plot: Rect,
    x_label: &str,
    y_label: &str,
    bounds: Option<[f64; 4]>,
) {
    let font = FontId::proportional(10.0);
    let color = ui.visuals().weak_text_color();
    painter.text(
        plot.center_bottom() + vec2(0.0, 14.0),
        Align2::CENTER_TOP,
        x_label,
        font.clone(),
        color,
    );
    painter.text(
        plot.left_top() + vec2(-40.0, -12.0),
        Align2::LEFT_TOP,
        y_label,
        font.clone(),
        color,
    );
    let Some([x0, x1, y0, y1]) = bounds else {
        return;
    };
    for (anchor, position, value) in [
        (Align2::LEFT_TOP, plot.left_bottom() + vec2(0.0, 2.0), x0),
        (Align2::RIGHT_TOP, plot.right_bottom() + vec2(0.0, 2.0), x1),
        (Align2::RIGHT_BOTTOM, plot.left_top() + vec2(-2.0, 8.0), y1),
        (Align2::RIGHT_TOP, plot.left_bottom() + vec2(-2.0, -8.0), y0),
    ] {
        painter.text(position, anchor, compact(value), font.clone(), color);
    }
}

/// An axis end value in at most eight characters.
///
/// A span loading runs to tens of thousands of newtons per metre while a
/// moment coefficient is a few thousandths: a single fixed format either
/// overflows the axis gutter or rounds the small end to zero.
fn compact(value: f64) -> String {
    let magnitude = value.abs();
    if magnitude >= 9_999.5 || (magnitude > 0.0 && magnitude < 0.001) {
        format!("{value:.2e}")
    } else if magnitude >= 100.0 {
        format!("{value:.0}")
    } else if magnitude >= 10.0 {
        format!("{value:.2}")
    } else {
        format!("{value:.3}")
    }
}
