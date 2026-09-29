// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The propulsion-cycle summary card and its localized labels.

use egui::{RichText, Ui};

use crate::views::tr;

use super::widgets::*;

pub(super) fn show_propulsion_cycle_summary(ui: &mut Ui, config: &alas_config::AlasConfig) {
    let lines = alas_report::families::propulsion::propulsion_cycle_summary(config);
    ui.label(RichText::new(tr("Propulsion cycle summary")).strong());
    ui.add_space(4.0);
    let entries = propulsion_summary_entries(&lines);
    if entries.is_empty() {
        return;
    }
    let columns = summary_column_count(ui.available_width()).min(entries.len());
    for row in entries.chunks(columns) {
        ui.columns(columns, |columns| {
            for (index, (label, value)) in row.iter().enumerate() {
                propulsion_metric_card(&mut columns[index], label, value);
            }
        });
        ui.add_space(8.0);
    }
}

/// Split the shared propulsion summary into independently readable cards.
///
/// The cycle producer keeps its report-oriented lines (including the compact
/// BPR/OPR/FPR/TIT row) as the source of truth. This adapter only changes the
/// presentation shape; it does not recalculate or round any physical value.
pub(super) fn propulsion_summary_entries(lines: &[String]) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    for line in lines
        .iter()
        .map(String::as_str)
        .filter(|line| !line.trim().is_empty())
    {
        if line.trim_start().starts_with("BPR =") {
            entries.extend(
                line.split("    ")
                    .filter(|metric| !metric.trim().is_empty())
                    .map(propulsion_summary_entry),
            );
        } else {
            entries.push(propulsion_summary_entry(line));
        }
    }
    entries
}

pub(super) fn propulsion_metric_card(ui: &mut Ui, label: &str, value: &str) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        if !label.is_empty() {
            ui.add(
                egui::Label::new(RichText::new(localized_propulsion_label(label)).small()).wrap(),
            );
        }
        ui.add(egui::Label::new(RichText::new(value).strong().size(14.0)).wrap());
    });
}

pub(super) fn localized_propulsion_label(label: &str) -> String {
    let compact = label.split_whitespace().collect::<Vec<_>>().join(" ");
    match compact.as_str() {
        "Engine" => tr("Engine:").trim().to_owned(),
        "Design point" => tr("Design point: ").trim().to_owned(),
        "Specific thrust SFn" => format!("{}  SFn", tr("Specific thrust")),
        "Fuel-air ratio f" => format!("{}  f", tr("Fuel-air ratio")),
        "TSFC (computed)" => tr("TSFC (computed)"),
        "TSFC (reference)" => tr("TSFC (reference)"),
        "Thermal efficiency (eta_t)" => format!("{} (eta_t)", tr("Thermal efficiency")),
        "Propulsive efficiency (eta_p)" => format!("{} (eta_p)", tr("Propulsive efficiency")),
        "Overall efficiency (eta_o)" => format!("{} (eta_o)", tr("Overall efficiency")),
        "Per-engine thrust, static (rated)" => tr("Per-engine thrust, static (rated)"),
        "Per-engine thrust, this cruise pt" => tr("Per-engine thrust, this cruise pt"),
        "Cycle infeasible at this design point" => tr("Cycle infeasible at this design point:")
            .trim_end_matches(':')
            .to_owned(),
        _ if compact.starts_with("Total installed thrust") => format!(
            "{} {}",
            tr("Total installed thrust"),
            compact.trim_start_matches("Total installed thrust").trim()
        ),
        _ => label.trim().to_owned(),
    }
}

pub(super) fn propulsion_summary_entry(line: &str) -> (String, String) {
    line.split_once(':')
        .or_else(|| line.split_once('='))
        .map(|(label, value)| (label.trim().to_owned(), value.trim().to_owned()))
        .unwrap_or_else(|| (String::new(), line.trim().to_owned()))
}
