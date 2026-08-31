// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Searchable, colour-coded diagnostics emitted by an analysis run.

use crate::state::{AppState, LogKind, LogLine};
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
    let visible = rendered_visible_lines(state);
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
    });
    if let Some(status) = &state.run_log_export_status {
        ui.label(RichText::new(status).weak().small());
    }

    let card = crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.set_min_height(ui.available_height());
        ScrollArea::vertical()
            .id_salt("run_log_scroll")
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
                show_stage_progress(&state.run_events, ui);
                if !state.run_events.is_empty() {
                    ui.add_space(3.0);
                    ui.separator();
                    ui.add_space(3.0);
                }
                for (rendered, kind) in &visible {
                    let color = match kind {
                        LogKind::Info => None,
                        LogKind::Warn => Some(ui.visuals().warn_fg_color),
                        LogKind::Error => Some(ui.visuals().error_fg_color),
                    };
                    let mut text = RichText::new(rendered).monospace().size(12.0);
                    if let Some(color) = color {
                        text = text.color(color);
                    }
                    ui.label(text);
                }
            });
    });
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

fn show_stage_progress(events: &[RunEvent], ui: &mut Ui) {
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
            .unwrap_or_else(|| event.elapsed_ms.saturating_sub(first.elapsed_ms));
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

fn rendered_visible_lines(state: &AppState) -> Vec<(String, LogKind)> {
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
            (query.is_empty() || rendered.to_lowercase().contains(&query))
                .then_some((rendered, line.kind))
        })
        .collect()
}

fn rendered_all_lines(state: &AppState) -> Vec<String> {
    state.logs.iter().map(render_line).collect()
}

fn render_line(line: &LogLine) -> String {
    let severity = match line.kind {
        LogKind::Info => "INFO ",
        LogKind::Warn => "WARN ",
        LogKind::Error => "ERROR",
    };
    let context = match line.elapsed {
        Some(elapsed) => format!(
            "run {:03} +{:02}:{:02}.{:03}",
            line.run_id,
            elapsed.as_secs() / 60,
            elapsed.as_secs() % 60,
            elapsed.subsec_millis()
        ),
        None => "system            ".to_owned(),
    };
    format!("[{context}] [{severity}] {}", localize_log_text(&line.text))
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
    use super::localize_log_text;
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
