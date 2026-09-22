// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Small dependency-free painters used by the CFD window.

use crate::views::tr;
use egui::{vec2, Ui};

/// Thickness-to-chord extent of the resolved section, used to size the preview
/// box so the outline fills it instead of floating inside an empty rectangle.
/// A missing or degenerate section falls back to a conventional 12 % section.
pub(crate) fn outline_aspect(coordinates: Option<&[(f64, f64)]>) -> f32 {
    const FALLBACK_ASPECT: f32 = 0.12;
    let Some(points) = coordinates else {
        return FALLBACK_ASPECT;
    };
    if points.len() < 2 {
        return FALLBACK_ASPECT;
    }
    let (mut xmin, mut xmax, mut ymin, mut ymax) = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    );
    for &(x, y) in points {
        xmin = xmin.min(x);
        xmax = xmax.max(x);
        ymin = ymin.min(y);
        ymax = ymax.max(y);
    }
    let (chord, thickness) = (xmax - xmin, ymax - ymin);
    if !chord.is_finite() || !thickness.is_finite() || chord <= 0.0 || thickness <= 0.0 {
        return FALLBACK_ASPECT;
    }
    (thickness / chord) as f32
}

pub(crate) fn paint_airfoil_outline(ui: &Ui, rect: egui::Rect, coordinates: Option<&[(f64, f64)]>) {
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
    let Some(points) = coordinates else {
        return;
    };
    if points.len() < 2 {
        return;
    }
    let (mut xmin, mut xmax, mut ymin, mut ymax) = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    );
    for &(x, y) in points {
        xmin = xmin.min(x);
        xmax = xmax.max(x);
        ymin = ymin.min(y);
        ymax = ymax.max(y);
    }
    let scale = (rect.width() as f64 / (xmax - xmin).max(1e-9))
        .min(rect.height() as f64 / (ymax - ymin).max(1e-9));
    let projected = points
        .iter()
        .map(|&(x, y)| {
            egui::pos2(
                rect.center().x + ((x - (xmin + xmax) * 0.5) * scale) as f32,
                rect.center().y - ((y - (ymin + ymax) * 0.5) * scale) as f32,
            )
        })
        .collect::<Vec<_>>();
    painter.add(egui::Shape::line(
        projected,
        egui::Stroke::new(2.0_f32, ui.visuals().hyperlink_color),
    ));
    painter.line_segment(
        [
            egui::pos2(rect.left(), rect.center().y),
            egui::pos2(rect.right(), rect.center().y),
        ],
        egui::Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.fg_stroke.color),
    );
}

/// Paint one series, optionally with the vertical axis inverted.
///
/// Inversion is a display convention only: the values and their tick labels
/// are unchanged, the axis simply increases downward, which is how a pressure
/// coefficient is conventionally read so that suction points up. It is never a
/// sign change.
pub(crate) fn paint_line_plot_with(
    ui: &Ui,
    rect: egui::Rect,
    points: &[(f64, f64)],
    invert_y: bool,
) {
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
    if points.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            tr("Unavailable: no parsed samples"),
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().warn_fg_color,
        );
        return;
    }
    let (xmin, xmax) = bounds(points.iter().map(|(x, _)| *x));
    let (ymin, ymax) = bounds(points.iter().map(|(_, y)| *y));
    let plot = rect.shrink2(vec2(34.0, 20.0));
    let axis_text = ui.visuals().weak_text_color();
    let text_style = egui::TextStyle::Small.resolve(ui.style());
    for fraction in [0.0_f32, 0.5, 1.0] {
        let x = egui::lerp(plot.left()..=plot.right(), fraction);
        let y = egui::lerp(plot.bottom()..=plot.top(), fraction);
        painter.line_segment(
            [egui::pos2(x, plot.top()), egui::pos2(x, plot.bottom())],
            egui::Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );
        painter.line_segment(
            [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
            egui::Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );
        let x_value = xmin + (xmax - xmin) * fraction as f64;
        let y_value = if invert_y {
            ymax - (ymax - ymin) * fraction as f64
        } else {
            ymin + (ymax - ymin) * fraction as f64
        };
        painter.text(
            egui::pos2(x, plot.bottom() + 3.0),
            egui::Align2::CENTER_TOP,
            axis_value_label(x_value),
            text_style.clone(),
            axis_text,
        );
        painter.text(
            egui::pos2(plot.left() - 5.0, y),
            egui::Align2::RIGHT_CENTER,
            axis_value_label(y_value),
            text_style.clone(),
            axis_text,
        );
    }
    let projected = points
        .iter()
        .map(|&(x, y)| {
            egui::pos2(
                egui::remap_clamp(x, xmin..=xmax, plot.left() as f64..=plot.right() as f64) as f32,
                if invert_y {
                    egui::remap_clamp(y, ymin..=ymax, plot.top() as f64..=plot.bottom() as f64)
                        as f32
                } else {
                    egui::remap_clamp(y, ymin..=ymax, plot.bottom() as f64..=plot.top() as f64)
                        as f32
                },
            )
        })
        .collect::<Vec<_>>();
    painter.add(egui::Shape::line(
        projected,
        egui::Stroke::new(2.0_f32, ui.visuals().hyperlink_color),
    ));
}

/// One actual data series with the style the legend repeats outside the plot.
pub(crate) struct PlotSeries {
    /// Legend label, already grouped by physical quantity.
    pub name: String,
    /// Parsed samples; never resampled or interpolated by the painter.
    pub points: Vec<(f64, f64)>,
    /// Shared colour for the quantity this series belongs to.
    pub color: egui::Color32,
    /// Dashed strokes separate the second series of a pair (the final
    /// residual) from the first (the initial residual) without spending a
    /// second colour that would be harder to tell apart.
    pub dashed: bool,
}

/// Distinct, theme-aware colours for grouped series.
pub(crate) fn series_color(ui: &Ui, index: usize) -> egui::Color32 {
    let palette = [
        ui.visuals().hyperlink_color,
        ui.visuals().warn_fg_color,
        crate::theme::success_color(ui.visuals()),
        egui::Color32::from_rgb(210, 120, 40),
        egui::Color32::from_rgb(170, 110, 220),
        egui::Color32::from_rgb(30, 170, 160),
        egui::Color32::from_rgb(220, 110, 150),
    ];
    palette[index % palette.len()]
}

/// Plot height for a given width: charts grow with the window and stay
/// bounded so a single plot never pushes the rest of the tab off-screen.
pub(crate) fn plot_height(width: f32) -> f32 {
    (width * 0.42).clamp(180.0, 320.0)
}

/// A compact wrapped legend drawn *below* the plot, so no label is ever
/// painted over a curve or an axis annotation.
pub(crate) fn plot_legend(ui: &mut Ui, series: &[PlotSeries]) {
    ui.horizontal_wrapped(|ui| {
        for entry in series.iter().filter(|entry| !entry.points.is_empty()) {
            let (rect, _) = ui.allocate_exact_size(vec2(18.0, 10.0), egui::Sense::hover());
            let stroke = egui::Stroke::new(2.0_f32, entry.color);
            let line = [
                egui::pos2(rect.left(), rect.center().y),
                egui::pos2(rect.right(), rect.center().y),
            ];
            if entry.dashed {
                ui.painter()
                    .extend(egui::Shape::dashed_line(&line, stroke, 3.0, 3.0));
            } else {
                ui.painter().line_segment(line, stroke);
            }
            ui.label(egui::RichText::new(tr(&entry.name)).small());
        }
    });
}

/// Paint several actual data series against shared numeric axes.
///
/// Each series keeps its own x ordering and colour.  The caller is responsible
/// for selecting finite values; this painter only draws data it receives and
/// never creates connecting or interpolated samples.  The legend is deliberately
/// not drawn here: [`plot_legend`] renders it outside the plot rectangle.
pub(crate) fn paint_multi_line_plot(ui: &Ui, rect: egui::Rect, series: &[PlotSeries]) {
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
    let finite_series = series
        .iter()
        .filter(|entry| !entry.points.is_empty())
        .collect::<Vec<_>>();
    if finite_series.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            tr("Unavailable: no parsed samples"),
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().warn_fg_color,
        );
        return;
    }
    let (xmin, xmax) = bounds(
        finite_series
            .iter()
            .flat_map(|entry| entry.points.iter().map(|(x, _)| *x)),
    );
    let (ymin, ymax) = bounds(
        finite_series
            .iter()
            .flat_map(|entry| entry.points.iter().map(|(_, y)| *y)),
    );
    let plot = rect.shrink2(vec2(42.0, 22.0));
    let axis_text = ui.visuals().weak_text_color();
    let text_style = egui::TextStyle::Small.resolve(ui.style());
    for fraction in [0.0_f32, 0.5, 1.0] {
        let x = egui::lerp(plot.left()..=plot.right(), fraction);
        let y = egui::lerp(plot.bottom()..=plot.top(), fraction);
        painter.line_segment(
            [egui::pos2(x, plot.top()), egui::pos2(x, plot.bottom())],
            egui::Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );
        painter.line_segment(
            [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
            egui::Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );
        let x_value = xmin + (xmax - xmin) * fraction as f64;
        let y_value = ymin + (ymax - ymin) * fraction as f64;
        painter.text(
            egui::pos2(x, plot.bottom() + 3.0),
            egui::Align2::CENTER_TOP,
            axis_value_label(x_value),
            text_style.clone(),
            axis_text,
        );
        painter.text(
            egui::pos2(plot.left() - 5.0, y),
            egui::Align2::RIGHT_CENTER,
            axis_value_label(y_value),
            text_style.clone(),
            axis_text,
        );
    }
    for entry in finite_series {
        let projected = entry
            .points
            .iter()
            .map(|&(x, y)| {
                egui::pos2(
                    egui::remap_clamp(x, xmin..=xmax, plot.left() as f64..=plot.right() as f64)
                        as f32,
                    egui::remap_clamp(y, ymin..=ymax, plot.bottom() as f64..=plot.top() as f64)
                        as f32,
                )
            })
            .collect::<Vec<_>>();
        let stroke = egui::Stroke::new(if entry.dashed { 1.4_f32 } else { 2.0_f32 }, entry.color);
        if entry.dashed {
            painter.extend(egui::Shape::dashed_line(&projected, stroke, 4.0, 3.0));
        } else {
            painter.add(egui::Shape::line(projected, stroke));
        }
    }
}

pub(crate) fn bounds(mut values: impl Iterator<Item = f64>) -> (f64, f64) {
    let Some(first) = values.next() else {
        return (0.0, 1.0);
    };
    let (mut min, mut max) = (first, first);
    for value in values {
        min = min.min(value);
        max = max.max(value);
    }
    if (max - min).abs() < 1e-12 {
        (min - 1.0, max + 1.0)
    } else {
        (min, max)
    }
}

/// Axis annotations carry only the precision the tick actually needs: an
/// iteration axis reads `2000`, not `2000.000`, and a tiny or huge physical
/// tick keeps scientific notation rather than a row of zeros.
fn axis_value_label(value: f64) -> String {
    let magnitude = value.abs();
    if (magnitude > 0.0 && magnitude < 1.0e-3) || magnitude >= 1.0e5 {
        format!("{value:.2e}")
    } else if (value - value.round()).abs() < 1.0e-9 {
        format!("{:.0}", value.round())
    } else if magnitude >= 100.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.3}")
    }
}
