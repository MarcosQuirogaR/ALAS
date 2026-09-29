// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Layout-derived aircraft metrics shown above the result figures.

mod findings;
mod findings_card;
mod metrics;
mod propulsion;
#[cfg(test)]
mod tests;
mod widgets;

use egui::Ui;

use crate::state::AppState;

use super::external::show_tool_status_cards;
use metrics::*;
use propulsion::*;
use widgets::*;

pub(super) fn show_summary(state: &AppState, ui: &mut Ui, result: &alas_pipeline::PipelineResult) {
    show_status_banner(ui, result, state.pipeline_result_complete);
    ui.add_space(10.0);
    if state.pipeline_result_complete {
        findings_card::show_findings_card(ui, &result.feasibility.findings);
    }

    section_title(ui, "External analyses");
    show_tool_status_cards(ui, result);

    section_title(ui, "Aircraft and mission");
    show_stat_tiles(ui, &aircraft_metrics(state, result));

    section_title(ui, "Mission");
    show_stat_tiles(ui, &mission_metrics(result));

    section_title(ui, "Mass and balance");
    show_stat_tiles(ui, &mass_metrics(result));

    section_title(ui, "Aerodynamics and trim");
    show_stat_tiles(ui, &trim_metrics(result));

    if let Some(layout) = result_payload_layout(result) {
        section_title(ui, "Payload and cabin");
        show_stat_tiles(ui, &payload_summary_metrics(layout));
    }

    section_title(ui, "Propulsion cycle details");
    show_propulsion_cycle_summary(ui, &result.config);
    ui.add_space(8.0);
}
