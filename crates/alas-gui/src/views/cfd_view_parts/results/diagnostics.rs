// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mesh-quality and solver-diagnostic plots for the Airfoil CFD results tab.

use super::super::drawing::{
    paint_multi_line_plot, plot_height, plot_legend, series_color, PlotSeries,
};
use super::super::widgets::card_title;
use super::{plot_footer, show_line_plot, tr};
use alas_cfd::CfdResults;
use egui::{Sense, Ui};

pub(super) fn show_residual_plot(result: &CfdResults, ui: &mut Ui) {
    // Keep one record per (equation, outer iteration).  OpenFOAM's SIMPLE
    // pressure equation can be solved more than once per outer iteration; the
    // classifier uses the largest initial residual, so retaining that same
    // record makes the diagnostic directly comparable with the status.
    let mut by_field_and_iteration = std::collections::BTreeMap::<(String, u64), (f64, f64)>::new();
    for sample in &result.residuals {
        if sample.initial.is_finite()
            && sample.initial > 0.0
            && sample.final_residual.is_finite()
            && sample.final_residual > 0.0
        {
            let key = (sample.field.clone(), sample.iteration);
            let replace = by_field_and_iteration
                .get(&key)
                .is_none_or(|(initial, _)| sample.initial >= *initial);
            if replace {
                by_field_and_iteration.insert(key, (sample.initial, sample.final_residual));
            }
        }
    }
    // Group by equation so the initial and the final residual of one field
    // share a colour and are told apart by the stroke pattern.  Ten same-width
    // curves in ten colours were what made the previous legend unreadable.
    let mut by_field =
        std::collections::BTreeMap::<String, (Vec<(f64, f64)>, Vec<(f64, f64)>)>::new();
    for ((field, iteration), (initial, final_residual)) in by_field_and_iteration {
        let entry = by_field.entry(field).or_default();
        entry.0.push((iteration as f64, initial.log10()));
        entry.1.push((iteration as f64, final_residual.log10()));
    }
    let mut series = Vec::with_capacity(2 * by_field.len());
    for (index, (field, (mut initial, mut final_residual))) in by_field.into_iter().enumerate() {
        initial.sort_by(|left, right| left.0.total_cmp(&right.0));
        final_residual.sort_by(|left, right| left.0.total_cmp(&right.0));
        let color = series_color(ui, index);
        series.push(PlotSeries {
            name: format!("{field} initial"),
            points: initial,
            color,
            dashed: false,
        });
        series.push(PlotSeries {
            name: format!("{field} final"),
            points: final_residual,
            color,
            dashed: true,
        });
    }
    show_residual_card(ui, &series);
}

/// The residual card: title, plot, wrapped legend below the plot rectangle,
/// then the axis captions and the honest equation-sample count.
fn show_residual_card(ui: &mut Ui, series: &[PlotSeries]) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        card_title(
            ui,
            "Residual histories (log10)",
            "One record per equation and outer SIMPLE iteration. Solid strokes are the initial residual, which is the status criterion; dashed strokes are the final residual of the same equation.",
        );
        let width = ui.available_width().max(200.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, plot_height(width)), Sense::hover());
        paint_multi_line_plot(ui, rect, series);
        plot_legend(ui, series);
        let samples = series.iter().map(|entry| entry.points.len()).sum::<usize>();
        plot_footer(
            ui,
            "outer iteration",
            "log10(residual); initial is the status criterion",
            samples,
        );
    });
}

pub(super) fn show_mesh_quality_plots(result: &CfdResults, ui: &mut Ui) {
    let distributions = &result.mesh_quality.distributions;
    let wall_distribution = result.mesh_quality.near_wall_distribution.as_ref();
    if distributions.is_empty() && wall_distribution.is_none() {
        crate::theme::card_frame(ui).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            card_title(ui, "Mesh quality distributions", "");
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr("Native cell-quality fields were not found. Scalar checkMesh metrics remain available; no distribution is inferred from them."),
            );
        });
        return;
    }
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        card_title(
            ui,
            "Mesh quality distributions",
            "Percentile curves are computed from finite native OpenFOAM cell fields. The wall y+ curve is a solved wall-face diagnostic and is shown separately.",
        );
    });
    // Two percentile curves share a row on a wide window so five distributions
    // do not push the rest of the tab several screens down.
    let mut plots = distributions
        .iter()
        .map(|distribution| {
            (
                format!("{} ({})", distribution.label, distribution.field),
                format!("{} [{}]", distribution.label, distribution.unit),
                distribution_points(distribution),
            )
        })
        .collect::<Vec<_>>();
    if let Some(distribution) = wall_distribution {
        plots.push((
            tr("Wall y+ (solved wall-face diagnostic)"),
            "y+ [-]".to_owned(),
            distribution_points(distribution),
        ));
    }
    let wide = ui.available_width() >= super::WIDE_ROW_WIDTH;
    for pair in plots.chunks(if wide { 2 } else { 1 }) {
        ui.add_space(6.0);
        if pair.len() == 2 {
            ui.columns(2, |columns| {
                for (column, (title, y_label, points)) in columns.iter_mut().zip(pair.iter()) {
                    show_line_plot(column, title, "percentile [%]", y_label, points);
                }
            });
        } else {
            let (title, y_label, points) = &pair[0];
            show_line_plot(ui, title, "percentile [%]", y_label, points);
        }
    }
}

fn distribution_points(distribution: &alas_cfd::ScalarDistribution) -> Vec<(f64, f64)> {
    distribution
        .percentiles
        .iter()
        .copied()
        .zip(distribution.values.iter().copied())
        .filter(|(percentile, value)| percentile.is_finite() && value.is_finite())
        .collect()
}
