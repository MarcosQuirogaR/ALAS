// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The per-stage table of a flown mission plan: what each stage burned, flew
//! and weighed, from the mass and centre-of-gravity walk of the run.

use egui::{Grid, Ui};

use crate::views::tr;

use super::widgets::section_title;

/// Shows one row per flown stage under a "Mission stages" title. Shows
/// nothing when the run flew no plan.
pub(super) fn show_mission_stages(ui: &mut Ui, result: &alas_pipeline::PipelineResult) {
    let Some(history) = result
        .feasibility
        .mass_balance
        .as_ref()
        .and_then(|balance| balance.history.as_ref())
    else {
        return;
    };
    if history.stages.is_empty() {
        return;
    }
    section_title(ui, &tr("Mission stages"));
    egui::ScrollArea::horizontal()
        .id_salt("mission_stages")
        .show(ui, |ui| {
            Grid::new("mission_stage_table")
                .striped(true)
                .num_columns(9)
                .show(ui, |ui| {
                    for header in [
                        "Stage",
                        "Kind",
                        "Time",
                        "Distance",
                        "Fuel burned",
                        "Mass at start",
                        "Mass at end",
                        "CG at end",
                        "Within envelope",
                    ] {
                        ui.strong(tr(header));
                    }
                    ui.end_row();
                    for stage in &history.stages {
                        ui.label(stage.id.as_str());
                        ui.label(tr(stage.label));
                        ui.label(format!("{:.1} min", stage.time_s / 60.0));
                        ui.label(format!("{:.1} km", stage.distance_m / 1_000.0));
                        ui.label(format!("{:.0} kg", stage.fuel_kg));
                        ui.label(format!("{:.2} t", stage.mass_start_kg / 1_000.0));
                        ui.label(format!("{:.2} t", stage.mass_end_kg / 1_000.0));
                        ui.label(format!("{:.1} %MAC", stage.cg_end_pct_mac));
                        ui.label(tr(if stage.within_envelope { "Yes" } else { "No" }));
                        ui.end_row();
                    }
                });
        });
    ui.add_space(8.0);
}
