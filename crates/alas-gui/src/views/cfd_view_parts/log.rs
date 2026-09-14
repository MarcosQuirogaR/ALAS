// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lifecycle and captured command-log view.

use crate::state::AppState;
use crate::views::tr;
use egui::{RichText, ScrollArea, TextEdit, Ui};

pub(crate) fn show_log_tab(state: &mut AppState, ui: &mut Ui) {
    ScrollArea::vertical()
        .id_salt("airfoil_cfd_log_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if state.cfd.events.is_empty() {
                ui.label(
                    RichText::new(tr("No CFD lifecycle events have been received yet.")).weak(),
                );
            }
            for event in &state.cfd.events {
                let color = match event.severity {
                    alas_cfd::CfdEventSeverity::Info => ui.visuals().text_color(),
                    alas_cfd::CfdEventSeverity::Warning => ui.visuals().warn_fg_color,
                    alas_cfd::CfdEventSeverity::Error => ui.visuals().error_fg_color,
                };
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!("{:.2}s", event.elapsed_seconds))
                            .weak()
                            .small(),
                    );
                    ui.label(RichText::new(tr(event.stage.as_str())).strong());
                    ui.colored_label(color, event.message.as_str());
                });
            }
            if let Some(result) = state.cfd.result.as_ref() {
                ui.separator();
                ui.label(RichText::new(tr("Captured utility logs")).strong());
                for (tool, log) in &result.command_logs {
                    ui.collapsing(tool, |ui| {
                        let mut text = log.clone();
                        ui.add(
                            TextEdit::multiline(&mut text)
                                .desired_rows(6)
                                .desired_width(ui.available_width())
                                .interactive(false),
                        );
                    });
                }
            }
        });
}
