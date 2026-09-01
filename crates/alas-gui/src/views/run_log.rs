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
                // Keep the context, optional elapsed time, and severity in a
                // single metadata cell.  System messages have no elapsed
                // time; keeping timing as a separate grid column left a
                // conspicuous blank gap between SYSTEM and INFO.
                .num_columns(2)
                .striped(true)
                .spacing([8.0, 3.0])
                .show(ui, |ui| {
                    for line in lines {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&line.context).monospace().small().strong());
                            if !line.timing.is_empty() {
                                ui.label(RichText::new(&line.timing).monospace().small().weak());
                            }
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
                        });
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
    ScrollArea::vertical()
        .id_salt("run_log_timings_scroll")
        .auto_shrink([false, false])
        .max_height(ui.available_height())
        .show(ui, |ui| show_stage_progress(events, elapsed_ms, ui));
}

fn show_stage_progress(events: &[RunEvent], elapsed_ms: u64, ui: &mut Ui) {
    let stages = stage_groups(events, None);
    let downstream = stage_groups(events, Some("downstream"));
    let mut rendered_downstream = false;

    for (first, event) in stages {
        show_stage_row(first, event, elapsed_ms, false, ui);
        if event.stage == "downstream" {
            rendered_downstream = true;
            for (child_first, child_event) in &downstream {
                show_stage_row(child_first, child_event, elapsed_ms, true, ui);
            }
        }
    }

    // Keep detail events useful even if a producer sends them without the
    // aggregate `downstream` stage event.  Normal pipeline runs include the
    // aggregate row, so this is primarily a defensive rendering fallback.
    if !rendered_downstream {
        for (first, event) in downstream {
            show_stage_row(first, event, elapsed_ms, true, ui);
        }
    }
}

/// Return the first and latest lifecycle event for each stage in a scope.
///
/// A stage ID containing a slash is a detail row.  Only direct children of
/// `downstream/` are rendered here; this prevents an arbitrary diagnostic
/// string containing another slash from accidentally becoming a timing row.
fn stage_groups<'a>(
    events: &'a [RunEvent],
    parent: Option<&str>,
) -> Vec<(&'a RunEvent, &'a RunEvent)> {
    let mut stages: Vec<(&RunEvent, &RunEvent)> = Vec::new();
    for event in events {
        if matches!(event.kind, RunEventKind::Diagnostic)
            || !stage_is_in_scope(&event.stage, parent)
        {
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
    if parent.is_some() {
        stages.sort_by_key(|(_, event)| event.stage.as_str());
    } else {
        stages.sort_by_key(|(_, event)| event.stage_index.unwrap_or(u8::MAX));
    }
    stages
}

fn stage_is_in_scope(stage: &str, parent: Option<&str>) -> bool {
    match parent {
        None => !stage.contains('/'),
        Some(parent) => stage
            .strip_prefix(parent)
            .and_then(|suffix| suffix.strip_prefix('/'))
            .is_some_and(|child| !child.is_empty() && !child.contains('/')),
    }
}

fn show_stage_row(first: &RunEvent, event: &RunEvent, elapsed_ms: u64, nested: bool, ui: &mut Ui) {
    // This row deliberately does not use a horizontal child layout.  In an
    // `egui` ScrollArea, a child UI can expand to its contents' natural size;
    // a long label then pushes the status and timing widgets beyond the
    // viewport. Allocate one exact row, reserve the fixed right-side cells,
    // and derive the bar rect from the interval left between them.
    const ROW_HEIGHT: f32 = 24.0;
    const MIN_LABEL_WIDTH: f32 = 132.0;
    const PREFERRED_LABEL_WIDTH: f32 = 280.0;
    const MIN_PROGRESS_WIDTH: f32 = 48.0;
    const STATUS_WIDTH: f32 = 82.0;
    const TIMING_WIDTH: f32 = 92.0;
    const TOP_LEVEL_CHEVRON_WIDTH: f32 = 14.0;
    const NESTED_INDENT_WIDTH: f32 = 18.0;

    let status = stage_status(event);
    let fraction = match status {
        StageStatus::Done => 1.0,
        StageStatus::Skipped => 0.0,
        StageStatus::Running => event.fraction.unwrap_or(0.0).clamp(0.0, 1.0) as f32,
    };
    let timing_ms = event
        .duration_ms
        .unwrap_or_else(|| elapsed_ms.saturating_sub(first.elapsed_ms));
    let timing = format_duration(timing_ms);
    let stage_name = display_stage_name(&event.stage, nested);
    let status_text = match status {
        StageStatus::Done => tr("Done"),
        StageStatus::Running => tr("Running"),
        StageStatus::Skipped => tr("Skipped"),
    };

    let row_width = ui.available_rect_before_wrap().width().max(1.0);
    let (row_rect, response) =
        ui.allocate_exact_size(egui::vec2(row_width, ROW_HEIGHT), egui::Sense::hover());
    let spacing = ui.spacing().item_spacing.x;

    // Right-align these cells first. This keeps status and elapsed duration
    // visible while the timing pane gets narrower; the progress bar is the
    // only flexible column.
    let timing_rect = egui::Rect::from_min_max(
        egui::pos2(row_rect.right() - TIMING_WIDTH, row_rect.top()),
        row_rect.right_bottom(),
    );
    let status_rect = egui::Rect::from_min_max(
        egui::pos2(timing_rect.left() - spacing - STATUS_WIDTH, row_rect.top()),
        egui::pos2(timing_rect.left() - spacing, row_rect.bottom()),
    );
    let available_label_width =
        (status_rect.left() - row_rect.left() - spacing * 2.0 - MIN_PROGRESS_WIDTH).max(0.0);
    let preferred_label_width = (row_width * 0.38).min(PREFERRED_LABEL_WIDTH);
    let label_width = preferred_label_width
        .min(available_label_width)
        .max(MIN_LABEL_WIDTH.min(available_label_width));
    let label_rect = egui::Rect::from_min_max(
        row_rect.left_top(),
        egui::pos2(row_rect.left() + label_width, row_rect.bottom()),
    );
    let progress_rect = egui::Rect::from_min_max(
        egui::pos2(label_rect.right() + spacing, row_rect.top()),
        egui::pos2(status_rect.left() - spacing, row_rect.bottom()),
    );

    let mut label_ui = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(("timing-label", &event.stage))
            .max_rect(label_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    if nested {
        // A single tab-width indent keeps downstream components visually
        // grouped without introducing a glyph that may be absent from the
        // platform font.
        label_ui.add_space(NESTED_INDENT_WIDTH);
    } else {
        // A painter-drawn chevron communicates the stage sequence without
        // coupling the UI to a run-total such as "1/7". It also avoids any
        // dependence on optional Unicode icon fonts.
        let (chevron_rect, _) = label_ui.allocate_exact_size(
            egui::vec2(TOP_LEVEL_CHEVRON_WIDTH, ROW_HEIGHT),
            egui::Sense::hover(),
        );
        let center = chevron_rect.center();
        let stroke = egui::Stroke::new(1.4_f32, label_ui.visuals().weak_text_color());
        label_ui.painter().line_segment(
            [
                egui::pos2(center.x - 2.5, center.y - 3.5),
                egui::pos2(center.x + 2.5, center.y),
            ],
            stroke,
        );
        label_ui.painter().line_segment(
            [
                egui::pos2(center.x + 2.5, center.y),
                egui::pos2(center.x - 2.5, center.y + 3.5),
            ],
            stroke,
        );
        label_ui.add_space(2.0);
    }
    label_ui.add(
        egui::Label::new(RichText::new(stage_name).strong().small().color(if nested {
            label_ui.visuals().weak_text_color()
        } else {
            label_ui.visuals().text_color()
        }))
        .truncate(),
    );

    let mut progress_ui = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(("timing-progress", &event.stage))
            .max_rect(progress_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    progress_ui.add_sized(
        progress_rect.size(),
        egui::ProgressBar::new(fraction).animate(matches!(status, StageStatus::Running)),
    );

    show_right_aligned_cell(
        ui,
        status_rect,
        ("timing-status", &event.stage),
        RichText::new(status_text).small().strong(),
    );
    show_right_aligned_cell(
        ui,
        timing_rect,
        ("timing-duration", &event.stage),
        RichText::new(timing).monospace().small(),
    );

    response.on_hover_text(&event.message);
}

/// Render a right-aligned fixed-width cell without allowing its contents to
/// reflow the row. The caller has already allocated the row's exact rect.
fn show_right_aligned_cell(
    ui: &mut Ui,
    rect: egui::Rect,
    id_salt: impl std::hash::Hash,
    text: RichText,
) {
    let mut cell = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id_salt)
            .max_rect(rect)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    cell.add(egui::Label::new(text).halign(egui::Align::RIGHT).truncate());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StageStatus {
    Running,
    Done,
    Skipped,
}

fn stage_status(event: &RunEvent) -> StageStatus {
    if !matches!(event.kind, RunEventKind::StageCompleted) {
        return StageStatus::Running;
    }
    if event
        .message
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("skipped")
    {
        StageStatus::Skipped
    } else {
        StageStatus::Done
    }
}

fn display_stage_name(stage: &str, nested: bool) -> String {
    if !nested {
        return sentence_case_stage_name(stage);
    }
    let component = stage.strip_prefix("downstream/").unwrap_or(stage);
    let component = match component.to_ascii_lowercase().as_str() {
        "mses" => "MSES".to_owned(),
        "msc" | "msc_nastran" | "mscnastran" => "MSC Nastran".to_owned(),
        "nastran95" | "nastran_95" => "Nastran95".to_owned(),
        "openvsp" => "OpenVSP".to_owned(),
        "vspaero" => "VSPAERO".to_owned(),
        "flowunsteady" => "FLOWUnsteady".to_owned(),
        "avl" => "AVL".to_owned(),
        "baseline_analysis" => "Baseline comparison".to_owned(),
        "mission" => "Mission and route".to_owned(),
        "patran" => "Patran".to_owned(),
        "structural" => "Structural sizing and analysis".to_owned(),
        _ => component.replace('_', " "),
    };
    format!("Downstream / {component}")
}

/// Present stage identifiers as human-readable section labels.  Pipeline
/// identifiers stay lowercase snake_case; only their UI representation is
/// capitalized.
fn sentence_case_stage_name(stage: &str) -> String {
    let mut label = stage.replace('_', " ");
    if let Some(first) = label.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    label
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
        Some(elapsed) => ("RUN".to_owned(), format_elapsed(elapsed)),
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

fn format_elapsed(elapsed: std::time::Duration) -> String {
    let total_seconds = elapsed.as_secs();
    let seconds = total_seconds % 60;
    let minutes = (total_seconds / 60) % 60;
    let hours = total_seconds / 3_600;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
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

    use super::{
        display_stage_name, format_elapsed, localize_log_text, render_line, stage_groups,
        stage_status, StageStatus,
    };
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
            elapsed: Some(Duration::from_millis(65_019)),
            run_id: 1,
        });
        assert_eq!(run.plaintext, "RUN 01:05 WARN Working");
        assert_eq!(format_elapsed(Duration::from_secs(3_661)), "01:01:01");
    }

    #[test]
    fn downstream_stage_events_are_grouped_without_top_level_numbers() {
        let event = |stage: &str,
                     kind: RunEventKind,
                     elapsed_ms: u64,
                     duration_ms: Option<u64>,
                     message: &str| RunEvent {
            stage: stage.to_owned(),
            message: message.to_owned(),
            fraction: Some(if matches!(kind, RunEventKind::StageCompleted) {
                1.0
            } else {
                0.4
            }),
            kind,
            severity: RunEventSeverity::Info,
            stage_index: None,
            stage_count: None,
            elapsed_ms,
            duration_ms,
        };
        let events = vec![
            event(
                "downstream",
                RunEventKind::StageStarted,
                0,
                None,
                "Downstream",
            ),
            event(
                "downstream/mses",
                RunEventKind::StageStarted,
                10,
                None,
                "MSES",
            ),
            event(
                "downstream/mses",
                RunEventKind::StageCompleted,
                110,
                Some(100),
                "Completed in 100 ms",
            ),
            event(
                "downstream/nastran95",
                RunEventKind::StageStarted,
                20,
                None,
                "Nastran95",
            ),
            event(
                "downstream/nastran95",
                RunEventKind::StageCompleted,
                120,
                Some(100),
                "Skipped (not configured)",
            ),
            event(
                "unexpected/nested/detail",
                RunEventKind::StageStarted,
                0,
                None,
                "ignored",
            ),
        ];
        let top_level = stage_groups(&events, None);
        assert_eq!(top_level.len(), 1);
        assert_eq!(top_level[0].1.stage, "downstream");
        let children = stage_groups(&events, Some("downstream"));
        assert_eq!(
            children
                .iter()
                .map(|(_, event)| event.stage.as_str())
                .collect::<Vec<_>>(),
            vec!["downstream/mses", "downstream/nastran95"]
        );
        assert_eq!(stage_status(children[0].1), StageStatus::Done);
        assert_eq!(stage_status(children[1].1), StageStatus::Skipped);
        assert_eq!(
            display_stage_name("downstream/mses", true),
            "Downstream / MSES"
        );
        assert_eq!(display_stage_name("full_analysis", false), "Full analysis");
    }

    #[test]
    fn completed_stage_status_is_not_inferred_from_fraction() {
        let event = RunEvent {
            stage: "downstream/msc".to_owned(),
            message: String::new(),
            fraction: Some(1.0),
            kind: RunEventKind::StageCompleted,
            severity: RunEventSeverity::Info,
            stage_index: None,
            stage_count: None,
            elapsed_ms: 100,
            duration_ms: Some(100),
        };
        assert_eq!(stage_status(&event), StageStatus::Done);
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
