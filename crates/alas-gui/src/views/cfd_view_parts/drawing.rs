// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Small dependency-free painters used by the CFD window.

use crate::views::tr;
use egui::{vec2, Ui};

pub(crate) fn label_with_help(ui: &mut Ui, label: &str, help: &str) {
    ui.label(tr(label)).on_hover_text(tr(help));
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

pub(crate) fn paint_line_plot(ui: &Ui, rect: egui::Rect, points: &[(f64, f64)]) {
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
    }
    let projected = points
        .iter()
        .map(|&(x, y)| {
            egui::pos2(
                egui::remap_clamp(x, xmin..=xmax, plot.left() as f64..=plot.right() as f64) as f32,
                egui::remap_clamp(y, ymin..=ymax, plot.bottom() as f64..=plot.top() as f64) as f32,
            )
        })
        .collect::<Vec<_>>();
    painter.add(egui::Shape::line(
        projected,
        egui::Stroke::new(2.0_f32, ui.visuals().hyperlink_color),
    ));
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
