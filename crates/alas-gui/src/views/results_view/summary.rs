// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Layout-derived aircraft metrics shown above the result figures.

#[cfg(test)]
mod layout_export;
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

/// The delivered design's objective in the unit its label states, with its
/// change from the starting design when the run compared the two under the
/// same model. A run whose plotted rows do not all record the physical
/// quantity reports its dimensionless ranking cost instead.
pub(super) fn objective_value_text(
    result: &alas_pipeline::PipelineResult,
    kind: Option<alas_config::ObjectiveKind>,
) -> String {
    let format = |value: f64| {
        let magnitude = value.abs();
        if magnitude >= 100.0 {
            format!("{value:.0}")
        } else if magnitude >= 1.0 {
            format!("{value:.2}")
        } else {
            format!("{value:.4}")
        }
    };
    let delta = alas_pipeline::optimizer_summary::OptimizerRunSummary::from_pipeline(result)
        .and_then(|summary| summary.baseline);
    if let (Some(_), Some(delta)) = (kind, delta) {
        if delta.winner_objective.is_finite() {
            return match delta.relative_objective_change() {
                Some(change) => format!(
                    "{} ({:+.1} %)",
                    format(delta.winner_objective),
                    100.0 * change
                ),
                None => format(delta.winner_objective),
            };
        }
    }
    // Otherwise the delivered design's own row: never the best row of the
    // history, which can be a candidate the run did not deliver.
    let Some(optimization) = result.optimization_result.as_ref() else {
        return "\u{2013}".to_owned();
    };
    let value = if kind.is_some() {
        let history = &optimization.history;
        history
            .design_vectors
            .iter()
            .rposition(|design| *design == optimization.best_design)
            .and_then(|row| history.objective_value.get(row).copied())
    } else {
        Some(optimization.best_cost)
    };
    value
        .filter(|value| value.is_finite())
        .map_or_else(|| "\u{2013}".to_owned(), format)
}

pub(super) fn show_summary(state: &AppState, ui: &mut Ui, result: &alas_pipeline::PipelineResult) {
    show_status_banner(ui, result, state.pipeline_result_complete);
    ui.add_space(10.0);
    if let Some(optimization) = &result.optimization_result {
        use alas_pipeline::optimizer_summary::objective::{
            history_objective_kind, objective_help, objective_label, RANKING_HELP, RANKING_LABEL,
        };
        let kind = history_objective_kind(
            &optimization.history,
            result.config.optimizer.objective.kind,
        );
        let label = kind.map_or(RANKING_LABEL, objective_label);
        let help = kind.map_or_else(
            || crate::views::tr(RANKING_HELP),
            |kind| {
                format!(
                    "{}\n\n{}",
                    crate::views::tr(objective_help(kind)),
                    crate::views::tr(RANKING_HELP)
                )
            },
        );
        section_title(ui, "Objective");
        show_explained_tile(ui, label, objective_value_text(result, kind), help);
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
