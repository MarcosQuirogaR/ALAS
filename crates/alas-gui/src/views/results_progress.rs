// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Results that are already available while the background pipeline is still
//! running. The final gallery needs a complete `PipelineResult`; this view
//! gives the user a useful, live account of each completed stage instead of
//! hiding the run behind a blank page until the last worker returns.

use std::collections::BTreeMap;

use alas_pipeline::{RunEvent, RunEventKind, RunEventSeverity};
use egui::{Color32, RichText, ScrollArea, Ui};

use crate::state::AppState;
use crate::views::tr;

pub(super) fn show_progressive_results(state: &AppState, ui: &mut Ui) {
    ScrollArea::vertical()
        .id_salt("progressive_results")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.heading(tr("Results in progress"));
            ui.label(if state.is_running {
                tr("The pipeline is still running. Completed stages and diagnostics appear here as they arrive.")
            } else {
                tr("No completed pipeline result is available yet. Start a run from the Inputs page.")
            });
            ui.add_space(12.0);

            let elapsed_ms = state.elapsed_ms().min(u64::MAX as u128) as u64;
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("Current stage")).strong());
                ui.label(if state.stage.is_empty() {
                    tr("Waiting for the first event")
                } else {
                    state.stage.replace('_', " ")
                });
                if state.is_running {
                    ui.label(RichText::new(format_duration(elapsed_ms)).monospace().weak());
                }
            });

            let stages = latest_stages(&state.run_events);
            if !stages.is_empty() {
                ui.add_space(8.0);
                crate::theme::card_frame(ui).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    for event in stages.values() {
                        let fraction = if matches!(event.kind, RunEventKind::StageCompleted) {
                            1.0
                        } else {
                            event.fraction.unwrap_or(0.0).clamp(0.0, 1.0) as f32
                        };
                        ui.horizontal(|ui| {
                            ui.label(event.stage.replace('_', " "));
                            ui.add(
                                egui::ProgressBar::new(fraction)
                                    .desired_width(ui.available_width().max(40.0))
                                    .animate(
                                        state.is_running
                                            && matches!(
                                                event.kind,
                                                RunEventKind::StageStarted | RunEventKind::Progress
                                            ),
                                    ),
                            );
                        });
                    }
                });
            }

            let diagnostics = state
                .run_events
                .iter()
                .filter(|event| matches!(event.kind, RunEventKind::Diagnostic))
                .collect::<Vec<_>>();
            let snapshots = diagnostics
                .iter()
                .filter(|event| is_result_snapshot(&event.message))
                .copied()
                .collect::<Vec<_>>();
            if !snapshots.is_empty() {
                ui.add_space(12.0);
                ui.label(RichText::new(tr("Partial numerical results")).strong());
                crate::theme::card_frame(ui).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    for event in snapshots {
                        ui.label(
                            RichText::new(format!(
                                "{} \u{00B7} {}",
                                event.stage.replace('_', " "),
                                event.message
                            ))
                            .monospace(),
                        );
                    }
                });
            }
            if !diagnostics.is_empty() {
                ui.add_space(12.0);
                ui.label(RichText::new(tr("Available results and tool diagnostics")).strong());
                crate::theme::card_frame(ui).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    for event in diagnostics
                        .into_iter()
                        .filter(|event| !is_result_snapshot(&event.message))
                    {
                        let color = match event.severity {
                            RunEventSeverity::Info => ui.visuals().text_color(),
                            RunEventSeverity::Warning => Color32::from_rgb(232, 181, 77),
                            RunEventSeverity::Error => ui.visuals().error_fg_color,
                        };
                        ui.colored_label(
                            color,
                            RichText::new(format!(
                                "{} \u{00B7} {}",
                                event.stage.replace('_', " "),
                                event.message
                            ))
                            .small(),
                        );
                    }
                });
            }

            if state.run_events.is_empty() {
                ui.add_space(12.0);
                ui.label(
                    RichText::new(tr("Stage results will be listed here during the run.")).weak(),
                );
            }
        });
}

/// Identify diagnostics that carry computed values rather than lifecycle or
/// tool availability text. Keeping this small and prefix-based makes the
/// display compatible with older serialized run events while giving the
/// user a clearly separated numeric snapshot during a live run.
fn is_result_snapshot(message: &str) -> bool {
    message.starts_with("Partial result available:") || message.starts_with("MSES polar ")
}

fn latest_stages(events: &[RunEvent]) -> BTreeMap<String, &RunEvent> {
    let mut latest = BTreeMap::new();
    for event in events {
        if !matches!(
            event.kind,
            RunEventKind::StageStarted | RunEventKind::StageCompleted | RunEventKind::Progress
        ) {
            continue;
        }
        latest.insert(event.stage.clone(), event);
    }
    latest
}

fn format_duration(milliseconds: u64) -> String {
    if milliseconds < 1_000 {
        format!("{milliseconds} ms")
    } else {
        format!("{:.1} s", milliseconds as f64 / 1_000.0)
    }
}
