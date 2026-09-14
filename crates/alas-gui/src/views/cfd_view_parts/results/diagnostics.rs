// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mesh-quality and solver-diagnostic plots for the Airfoil CFD results tab.

use super::super::drawing::paint_multi_line_plot;
use super::{show_line_plot, tr, tr_fields};
use alas_cfd::CfdResults;
use egui::{RichText, Sense, Ui};

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
    let mut series = std::collections::BTreeMap::<String, Vec<(f64, f64)>>::new();
    for ((field, iteration), (initial, final_residual)) in by_field_and_iteration {
        series
            .entry(format!("{field} initial"))
            .or_default()
            .push((iteration as f64, initial.log10()));
        series
            .entry(format!("{field} final"))
            .or_default()
            .push((iteration as f64, final_residual.log10()));
    }
    let series = series
        .into_iter()
        .map(|(field, mut points)| {
            points.sort_by(|left, right| left.0.total_cmp(&right.0));
            (field, points)
        })
        .collect::<Vec<_>>();
    show_multi_line_plot(
        ui,
        "Residual histories by outer SIMPLE iteration (log10 initial and final)",
        "outer iteration",
        "log10(residual); initial is the status criterion",
        &series,
    );
}

fn show_multi_line_plot(
    ui: &mut Ui,
    title: &str,
    x_label: &str,
    y_label: &str,
    series: &[(String, Vec<(f64, f64)>)],
) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(RichText::new(tr(title)).strong());
        let width = ui.available_width().max(260.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 220.0), Sense::hover());
        paint_multi_line_plot(ui, rect, series);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr(x_label)).weak().small());
            ui.separator();
            ui.label(RichText::new(tr(y_label)).weak().small());
            ui.separator();
            let samples = series.iter().map(|(_, points)| points.len()).sum::<usize>();
            ui.label(
                RichText::new(if samples == 0 {
                    tr("Unavailable: no parsed samples")
                } else {
                    tr_fields(
                        "{count} actual equation samples; initial/final retain separate series",
                        &[("count", samples.to_string())],
                    )
                })
                .weak()
                .small(),
            );
        });
    });
}

pub(super) fn show_mesh_quality_plots(result: &CfdResults, ui: &mut Ui) {
    let distributions = &result.mesh_quality.distributions;
    let wall_distribution = result.mesh_quality.near_wall_distribution.as_ref();
    if distributions.is_empty() && wall_distribution.is_none() {
        crate::theme::card_frame(ui).show(ui, |ui| {
            ui.label(RichText::new(tr("Mesh quality distributions")).strong().size(16.0));
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr("Native cell-quality fields were not found. Scalar checkMesh metrics remain available; no distribution is inferred from them."),
            );
        });
        return;
    }
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(RichText::new(tr("Mesh quality distributions")).strong().size(16.0));
        ui.label(
            RichText::new(tr("Percentile curves are computed from finite native OpenFOAM cell fields. The wall y+ curve is a solved wall-face diagnostic and is shown separately.")).weak().small(),
        );
    });
    for distribution in distributions {
        let points = distribution_points(distribution);
        let y_label = format!("{} [{}]", distribution.label, distribution.unit);
        let title = format!(
            "{} distribution (native {})",
            distribution.label, distribution.field
        );
        show_line_plot(ui, &title, "percentile [%]", &y_label, &points);
        ui.add_space(8.0);
    }
    if let Some(distribution) = wall_distribution {
        let points = distribution_points(distribution);
        show_line_plot(
            ui,
            "Wall y+ distribution (solved wall-face diagnostic)",
            "percentile [%]",
            "y+ [-]",
            &points,
        );
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
