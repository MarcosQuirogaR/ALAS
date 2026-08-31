// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Searchable, colour-coded diagnostics emitted by an analysis run.

use crate::state::{AppState, LogKind, LogLine, RunLogTab};
use crate::views::{tr, tr_fields};
use alas_pipeline::{RunEvent, RunEventKind};
use egui::{RichText, ScrollArea, TextEdit, Ui};
use std::time::{SystemTime, UNIX_EPOCH};

/// Render the run log and its diagnostic toolbar.
pub fn show_run_log(state: &mut AppState, ui: &mut Ui) {
    #[cfg(debug_assertions)]
    crate::layout_debug::record_ui(
        ui.ctx(),
        "run log content",
        ui,
        crate::layout_debug::RegionKind::RunLog,
    );
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(tr("Run Log"))
                .strong()
                .small()
                .color(ui.visuals().hyperlink_color),
        );
        ui.label(
            RichText::new(tr_fields(
                "{count} lines",
                &[("count", state.logs.len().to_string())],
            ))
            .weak()
            .small(),
        )
        .on_hover_text(tr(
            "Drag the panel edge to resize; the scrollbar stays available for history.",
        ));
        ui.separator();
        if !state.is_running {
            ui.selectable_value(&mut state.run_log_tab, RunLogTab::Console, tr("Console"));
            ui.selectable_value(&mut state.run_log_tab, RunLogTab::Timings, tr("Timings"));
            ui.separator();
        }
        if state.is_running || state.run_log_tab == RunLogTab::Console {
            ui.add(
                TextEdit::singleline(&mut state.run_log_search)
                    .hint_text(tr("Search log"))
                    .desired_width(180.0),
            );
            ui.toggle_value(&mut state.run_log_show_info, tr("Info"));
            ui.toggle_value(&mut state.run_log_show_warn, tr("Warnings"));
            ui.toggle_value(&mut state.run_log_show_error, tr("Errors"));
            ui.separator();
            if ui.button(tr("Copy all")).clicked() {
                ui.ctx().copy_text(rendered_all_lines(state).join("\n"));
            }
            if ui.button(tr("Export...")).clicked() {
                state.run_log_export_status = Some(export_log(state));
            }
        }
    });
    if let Some(status) = &state.run_log_export_status {
        ui.label(RichText::new(status).weak().small());
    }

    let visible = rendered_visible_lines(state);
    let elapsed_ms = state.elapsed_ms().min(u64::MAX as u128) as u64;
    let card = if state.is_running {
        crate::theme::card_frame(ui).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_min_height(ui.available_height());
            ui.columns(2, |columns| {
                columns[0].label(RichText::new(tr("Console")).strong().small());
                show_console(&visible, &mut columns[0]);
                columns[1].label(RichText::new(tr("Timings")).strong().small());
                show_timings(&state.run_events, elapsed_ms, &mut columns[1]);
            });
        })
    } else {
        crate::theme::card_frame(ui).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_min_height(ui.available_height());
            match state.run_log_tab {
                RunLogTab::Console => show_console(&visible, ui),
                RunLogTab::Timings => show_timings(&state.run_events, elapsed_ms, ui),
            }
        })
    };
    #[cfg(debug_assertions)]
    crate::layout_debug::record(
        ui.ctx(),
        "run log card",
        card.response.rect,
        crate::layout_debug::RegionKind::RunLog,
    );
    #[cfg(not(debug_assertions))]
    let _ = card;
}

fn show_console(lines: &[RenderedLine], ui: &mut Ui) {
    ScrollArea::vertical()
        .id_salt("run_log_console_scroll")
        .auto_shrink([false, false])
        .max_height(ui.available_height())
        .stick_to_bottom(true)
        .show(ui, |ui| {
            #[cfg(debug_assertions)]
            crate::layout_debug::record_ui(
                ui.ctx(),
                "run log scroll",
                ui,
                crate::layout_debug::RegionKind::Scroll,
            );
            egui::Grid::new("run_log_rows")
                .num_columns(4)
                .striped(true)
                .spacing([8.0, 3.0])
                .show(ui, |ui| {
                    for line in lines {
                        ui.label(RichText::new(&line.context).monospace().small().strong());
                        ui.label(RichText::new(&line.timing).monospace().small().weak());
                        let color = match line.kind {
                            LogKind::Info => ui.visuals().weak_text_color(),
                            LogKind::Warn => ui.visuals().warn_fg_color,
                            LogKind::Error => ui.visuals().error_fg_color,
                        };
                        ui.label(
                            RichText::new(line.severity)
                                .monospace()
                                .small()
                                .strong()
                                .color(color),
                        );
                        ui.add(
                            egui::Label::new(RichText::new(&line.message).monospace().size(12.0))
                                .wrap(),
                        );
                        ui.end_row();
                    }
                });
        });
}

fn show_timings(events: &[RunEvent], elapsed_ms: u64, ui: &mut Ui) {
    let estimate = timing_estimate(events, elapsed_ms);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Estimated remaining")).weak().small());
        ui.label(RichText::new(estimate).monospace().strong());
    });
    ui.separator();
    ScrollArea::vertical()
        .id_salt("run_log_timings_scroll")
        .auto_shrink([false, false])
        .max_height(ui.available_height())
        .show(ui, |ui| show_stage_progress(events, elapsed_ms, ui));
}

fn show_stage_progress(events: &[RunEvent], elapsed_ms: u64, ui: &mut Ui) {
    let mut stages: Vec<(&RunEvent, &RunEvent)> = Vec::new();
    for event in events {
        if matches!(event.kind, RunEventKind::Diagnostic) {
            continue;
        }
        if let Some((_, latest)) = stages
            .iter_mut()
            .find(|(_, latest)| latest.stage == event.stage)
        {
            *latest = event;
        } else {
            stages.push((event, event));
        }
    }
    stages.sort_by_key(|(_, event)| event.stage_index.unwrap_or(u8::MAX));

    for (first, event) in stages {
        let completed = matches!(event.kind, RunEventKind::StageCompleted);
        let fraction = if completed {
            1.0
        } else {
            event.fraction.unwrap_or(0.0).clamp(0.0, 1.0) as f32
        };
        let timing_ms = event
            .duration_ms
            .unwrap_or_else(|| elapsed_ms.saturating_sub(first.elapsed_ms));
        let timing = format_duration(timing_ms);
        let stage_name = event.stage.replace('_', " ");
        let stage_number = match (event.stage_index, event.stage_count) {
            (Some(index), Some(count)) => format!("{index}/{count}  "),
            _ => String::new(),
        };
        ui.horizontal(|ui| {
            ui.add_sized(
                [190.0, 18.0],
                egui::Label::new(
                    RichText::new(format!("{stage_number}{stage_name}"))
                        .strong()
                        .small(),
                ),
            );
            ui.add(
                egui::ProgressBar::new(fraction)
                    .desired_width((ui.available_width() - 90.0).max(80.0))
                    .animate(!completed)
                    .text(if completed { tr("Done") } else { tr("Running") }),
            );
            ui.add_sized(
                [75.0, 18.0],
                egui::Label::new(RichText::new(timing).monospace().small()),
            );
        })
        .response
        .on_hover_text(&event.message);
    }
}

fn format_duration(milliseconds: u64) -> String {
    if milliseconds < 1_000 {
        format!("{milliseconds} ms")
    } else {
        format!("{:.2} s", milliseconds as f64 / 1_000.0)
    }
}

struct RenderedLine {
    context: String,
    timing: String,
    severity: &'static str,
    message: String,
    kind: LogKind,
    plaintext: String,
}

fn rendered_visible_lines(state: &AppState) -> Vec<RenderedLine> {
    let query = state.run_log_search.trim().to_lowercase();
    state
        .logs
        .iter()
        .filter(|line| match line.kind {
            LogKind::Info => state.run_log_show_info,
            LogKind::Warn => state.run_log_show_warn,
            LogKind::Error => state.run_log_show_error,
        })
        .filter_map(|line| {
            let rendered = render_line(line);
            (query.is_empty() || rendered.plaintext.to_lowercase().contains(&query))
                .then_some(rendered)
        })
        .collect()
}

fn rendered_all_lines(state: &AppState) -> Vec<String> {
    state
        .logs
        .iter()
        .map(|line| render_line(line).plaintext)
        .collect()
}

fn render_line(line: &LogLine) -> RenderedLine {
    let severity = match line.kind {
        LogKind::Info => "INFO",
        LogKind::Warn => "WARN",
        LogKind::Error => "ERROR",
    };
    let (context, timing) = match line.elapsed {
        Some(elapsed) => (
            format!("RUN {:03}", line.run_id),
            format!(
                "+{:02}:{:02}.{:03}",
                elapsed.as_secs() / 60,
                elapsed.as_secs() % 60,
                elapsed.subsec_millis()
            ),
        ),
        None => ("SYSTEM".to_owned(), String::new()),
    };
    let message = localize_log_text(&line.text);
    let plaintext = if timing.is_empty() {
        format!("{context} {severity} {message}")
    } else {
        format!("{context} {timing} {severity} {message}")
    };
    RenderedLine {
        context,
        timing,
        severity,
        message,
        kind: line.kind,
        plaintext,
    }
}

fn timing_estimate(events: &[RunEvent], elapsed_ms: u64) -> String {
    let mut completed_durations = Vec::new();
    let mut active_started = None;
    let mut stage_count = None;
    let mut completed_count = 0_u64;
    for event in events {
        stage_count = event.stage_count.or(stage_count);
        match event.kind {
            RunEventKind::StageStarted => active_started = Some(event.elapsed_ms),
            RunEventKind::StageCompleted => {
                completed_count += 1;
                if let Some(duration) = event.duration_ms {
                    completed_durations.push(duration);
                }
                active_started = None;
            }
            RunEventKind::Progress | RunEventKind::Diagnostic => {}
        }
    }
    let Some(total) = stage_count.map(u64::from) else {
        return tr("No timing data");
    };
    if completed_count >= total {
        return tr("Complete");
    }
    if completed_durations.is_empty() {
        return tr("Calculating...");
    }
    let mean_ms = completed_durations.iter().sum::<u64>() / completed_durations.len() as u64;
    let future_stages = total.saturating_sub(completed_count + u64::from(active_started.is_some()));
    let active_remaining = active_started
        .map(|started| mean_ms.saturating_sub(elapsed_ms.saturating_sub(started)))
        .unwrap_or(0);
    let remaining_ms = active_remaining.saturating_add(future_stages.saturating_mul(mean_ms));
    format!("~{}", format_duration(remaining_ms))
}

fn export_log(state: &AppState) -> String {
    let directory = state
        .tool_locator
        .preferences_path()
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(std::env::temp_dir);
    if let Err(error) = std::fs::create_dir_all(&directory) {
        return format!("Export failed: {error}");
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let contents = rendered_all_lines(state).join("\n");
    for suffix in 0..100_u8 {
        let filename = if suffix == 0 {
            format!("alas-run-log-{timestamp}.txt")
        } else {
            format!("alas-run-log-{timestamp}-{suffix}.txt")
        };
        let path = directory.join(filename);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                use std::io::Write as _;
                return match file.write_all(contents.as_bytes()) {
                    Ok(()) => format!("Saved {}", path.display()),
                    Err(error) => format!("Export failed: {error}"),
                };
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return format!("Export failed: {error}"),
        }
    }
    "Export failed: too many files share this timestamp".to_owned()
}

fn localize_log_text(text: &str) -> String {
    let exact = tr(text);
    if exact != text {
        return exact;
    }
    for (prefix, template, field) in [
        ("Loaded preset: ", "Loaded preset: {name}", "name"),
        ("Engine changed to: ", "Engine changed to: {name}", "name"),
        ("Run failed: ", "Run failed: {error}", "error"),
        ("Save failed: ", "Save failed: {error}", "error"),
        ("Load failed: ", "Load failed: {error}", "error"),
        (
            "Tool preferences not saved: ",
            "Tool preferences not saved: {error}",
            "error",
        ),
    ] {
        if let Some(value) = text.strip_prefix(prefix) {
            return tr_fields(template, &[(field, value.to_owned())]);
        }
    }
    text.to_owned()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use alas_pipeline::{RunEvent, RunEventKind, RunEventSeverity};

    use super::{localize_log_text, render_line, timing_estimate};
    use crate::state::{LogKind, LogLine};

    #[test]
    fn console_rows_do_not_use_space_padding_for_alignment() {
        let system = render_line(&LogLine {
            text: "Ready.".to_owned(),
            kind: LogKind::Info,
            elapsed: None,
            run_id: 0,
        });
        assert_eq!(system.plaintext, "SYSTEM INFO Ready.");
        assert!(!system.plaintext.contains("["));

        let run = render_line(&LogLine {
            text: "Working".to_owned(),
            kind: LogKind::Warn,
            elapsed: Some(Duration::from_millis(19)),
            run_id: 1,
        });
        assert_eq!(run.plaintext, "RUN 001 +00:00.019 WARN Working");
    }

    #[test]
    fn timing_estimate_uses_completed_stages_and_active_elapsed_time() {
        let event = |kind, elapsed_ms, duration_ms| RunEvent {
            stage: "stage".to_owned(),
            message: String::new(),
            fraction: None,
            kind,
            severity: RunEventSeverity::Info,
            stage_index: Some(1),
            stage_count: Some(3),
            elapsed_ms,
            duration_ms,
        };
        let events = vec![
            event(RunEventKind::StageStarted, 0, None),
            event(RunEventKind::StageCompleted, 1_000, Some(1_000)),
            event(RunEventKind::StageStarted, 1_000, None),
        ];
        assert_eq!(timing_estimate(&events, 1_250), "~1.75 s");
    }

    #[test]
    fn dynamic_log_lines_follow_a_language_change() {
        alas_i18n::es::install();
        alas_i18n::set_language(Some("es"));
        assert_eq!(
            localize_log_text("Loaded preset: AVE (Reference Twin)"),
            "Preajuste cargado: AVE (Reference Twin)"
        );
        assert_eq!(
            localize_log_text("Run failed: solver unavailable"),
            "La ejecuci\u{f3}n fall\u{f3}: solver unavailable"
        );
        alas_i18n::set_language(Some("en"));
    }
}
