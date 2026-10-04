// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Stat tiles, semantic frames and the run status banner.

use alas_pipeline::feasibility::{FindingSeverity, MissionFuelStatus};
use egui::{Frame, Margin, RichText, Rounding, Stroke, Ui};

use crate::views::{tr, tr_fields};

pub(super) fn stat_tile(ui: &mut Ui, label: &str, value: String) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.vertical(|ui| {
            ui.label(RichText::new(tr(label)).small());
            ui.add(egui::Label::new(RichText::new(value).strong().size(16.0)).wrap());
        });
    });
}

/// One stat tile, as wide as a tile of a [`show_stat_tiles`] row, whose
/// label explains itself on hover.
pub(super) fn show_explained_tile(ui: &mut Ui, label: &str, value: String, hover: String) {
    let columns = summary_column_count(ui.available_width());
    ui.columns(columns, |columns| {
        let ui = &mut columns[0];
        crate::theme::card_frame(ui).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.vertical(|ui| {
                ui.add(egui::Label::new(RichText::new(tr(label)).small()).wrap())
                    .on_hover_text(hover.clone());
                ui.add(egui::Label::new(RichText::new(value).strong().size(16.0)).wrap())
                    .on_hover_text(hover);
            });
        });
    });
    ui.add_space(8.0);
}

pub(super) fn semantic_frame(ui: &Ui, color: egui::Color32) -> Frame {
    Frame::group(ui.style())
        .fill(ui.visuals().window_fill())
        .stroke(Stroke::new(1.5_f32, color))
        .inner_margin(Margin::symmetric(12.0, 8.0))
        .rounding(Rounding::same(12.0))
}

pub(super) fn status_banner_title(completed: bool, errors: usize, warnings: usize) -> &'static str {
    if !completed {
        "Assessment incomplete"
    } else if errors > 0 {
        "Infeasible under implemented checks"
    } else if warnings > 0 {
        "Feasible with engineering warnings"
    } else {
        "Feasible under implemented checks"
    }
}

pub(super) fn show_status_banner(
    ui: &mut Ui,
    result: &alas_pipeline::PipelineResult,
    completed: bool,
) {
    let errors = result
        .feasibility
        .findings
        .iter()
        .filter(|finding| finding.severity == FindingSeverity::Error)
        .count();
    let warnings = result.feasibility.findings.len().saturating_sub(errors);
    let counts = tr_fields(
        "{errors} blocking finding(s), {warnings} warning(s).",
        &[
            ("errors", errors.to_string()),
            ("warnings", warnings.to_string()),
        ],
    );
    let (color, title, detail) = if !completed {
        (
            ui.visuals().warn_fg_color,
            tr(status_banner_title(false, errors, warnings)),
            tr("The figures below come from finished analysis stages, but the overall feasibility assessment is incomplete. Finalized report exports are unavailable until the pipeline completes."),
        )
    } else if errors > 0 {
        (
            ui.visuals().error_fg_color,
            tr(status_banner_title(true, errors, warnings)),
            format!("{counts} {}", mission_status_label(result)),
        )
    } else if warnings > 0 {
        (
            ui.visuals().warn_fg_color,
            tr(status_banner_title(true, errors, warnings)),
            format!("{counts} {}", mission_status_label(result)),
        )
    } else {
        (
            crate::theme::success_color(ui.visuals()),
            tr(status_banner_title(true, errors, warnings)),
            mission_status_label(result),
        )
    };

    semantic_frame(ui, color).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(title).strong().size(19.0).color(color))
                .on_hover_text(tr(
                    "This verdict covers only the physical checks implemented by this run; it is not a certification finding.",
                ));
            ui.label(RichText::new(detail));
        });
    });
}

pub(super) fn mission_status_label(result: &alas_pipeline::PipelineResult) -> String {
    let status = match result.feasibility.fuel_loading.mission.status {
        MissionFuelStatus::NotRequested => "Mission not requested",
        MissionFuelStatus::Unavailable => "Mission unavailable",
        MissionFuelStatus::NotConverged => "Mission did not converge",
        MissionFuelStatus::Completed => "Mission completed",
        MissionFuelStatus::Exhausted => "Mission stopped: fuel exhausted",
    };
    tr(status)
}

pub(super) fn maximum_mission_range_km(result: &alas_pipeline::PipelineResult) -> Option<f64> {
    result.mission_result.as_ref().and_then(|mission| {
        mission
            .segments
            .iter()
            .flat_map(|segment| segment.conditions.aircraft_range_m.iter().copied())
            .filter(|range| range.is_finite())
            .reduce(f64::max)
            .map(|range_m| range_m / 1_000.0)
    })
}

pub(super) fn summary_column_count(available_width: f32) -> usize {
    ((available_width / 245.0).floor() as usize).clamp(1, 4)
}

pub(super) fn show_stat_tiles<L: AsRef<str>>(ui: &mut Ui, metrics: &[(L, String)]) {
    if metrics.is_empty() {
        return;
    }
    let columns = summary_column_count(ui.available_width()).min(metrics.len());
    for row in metrics.chunks(columns) {
        ui.columns(columns, |columns| {
            for (index, (label, value)) in row.iter().enumerate() {
                stat_tile(&mut columns[index], label.as_ref(), value.clone());
            }
        });
        ui.add_space(8.0);
    }
}

pub(super) fn section_title(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(tr(title)).strong().size(18.0));
    ui.add_space(5.0);
}
