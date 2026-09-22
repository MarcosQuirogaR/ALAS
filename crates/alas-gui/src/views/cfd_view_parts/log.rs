// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lifecycle and captured command-log view.
//!
//! The events are shown as an aligned elapsed/stage/message table rather than
//! as ragged rows, with a compact header that counts warnings and errors so a
//! failed stage stays visible without scrolling the whole log.

use super::widgets::{card_title, details};
use crate::state::AppState;
use crate::views::{tr, tr_fields};
use egui::{Grid, RichText, ScrollArea, TextEdit, Ui};

pub(crate) fn show_log_tab(state: &mut AppState, ui: &mut Ui) {
    ScrollArea::vertical()
        .id_salt("airfoil_cfd_log_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            show_log_header(state, ui);
            if state.cfd.events.is_empty() {
                ui.label(
                    RichText::new(tr("No CFD lifecycle events have been received yet.")).weak(),
                );
            } else {
                show_events(state, ui);
            }
            if let Some(result) = state.cfd.result.as_ref() {
                ui.add_space(6.0);
                for (tool, log) in &result.command_logs {
                    details(ui, tool, tool, |ui| {
                        let mut text = log.clone();
                        ui.add(
                            TextEdit::multiline(&mut text)
                                .desired_rows(8)
                                .desired_width(ui.available_width())
                                .interactive(false),
                        );
                    });
                }
            }
        });
}

/// Counts that stay honest: a warning or an error is reported here even when
/// the run finished, never summarised away.
fn show_log_header(state: &AppState, ui: &mut Ui) {
    let warnings = state
        .cfd
        .events
        .iter()
        .filter(|event| event.severity == alas_cfd::CfdEventSeverity::Warning)
        .count();
    let errors = state
        .cfd
        .events
        .iter()
        .filter(|event| event.severity == alas_cfd::CfdEventSeverity::Error)
        .count();
    ui.horizontal_wrapped(|ui| {
        card_title(
            ui,
            "Run log",
            "Lifecycle events emitted by the background study worker, in the order they were received.",
        );
        ui.label(
            RichText::new(tr_fields(
                "{count} events",
                &[("count", state.cfd.events.len().to_string())],
            ))
            .weak()
            .small(),
        );
        if warnings > 0 {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr_fields("{count} warnings", &[("count", warnings.to_string())]),
            );
        }
        if errors > 0 {
            ui.colored_label(
                ui.visuals().error_fg_color,
                tr_fields("{count} errors", &[("count", errors.to_string())]),
            );
        }
        if state.cfd.result.is_some() {
            ui.separator();
            ui.label(RichText::new(tr("Captured utility logs")).weak().small());
        }
    });
}

fn show_events(state: &AppState, ui: &mut Ui) {
    Grid::new("airfoil_cfd_log_grid")
        .num_columns(3)
        .striped(true)
        .spacing([10.0, 3.0])
        .max_col_width((ui.available_width() - 200.0).max(160.0))
        .show(ui, |ui| {
            for event in &state.cfd.events {
                let color = match event.severity {
                    alas_cfd::CfdEventSeverity::Info => ui.visuals().text_color(),
                    alas_cfd::CfdEventSeverity::Warning => ui.visuals().warn_fg_color,
                    alas_cfd::CfdEventSeverity::Error => ui.visuals().error_fg_color,
                };
                ui.label(
                    RichText::new(format!("{:.2}s", event.elapsed_seconds))
                        .weak()
                        .small(),
                );
                ui.label(RichText::new(tr(event.stage.as_str())).strong().small());
                ui.colored_label(color, event.message.as_str());
                ui.end_row();
            }
        });
}
