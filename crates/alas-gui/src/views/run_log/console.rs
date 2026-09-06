// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Console row formatting, filtering, localization and export.

use crate::state::{AppState, LogKind, LogLine};
use crate::views::{tr, tr_fields};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) struct RenderedLine {
    pub(super) context: String,
    pub(super) timing: String,
    pub(super) severity: &'static str,
    pub(super) message: String,
    pub(super) kind: LogKind,
    pub(super) plaintext: String,
}

pub(super) fn rendered_visible_lines(state: &AppState) -> Vec<RenderedLine> {
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

pub(super) fn rendered_all_lines(state: &AppState) -> Vec<String> {
    state
        .logs
        .iter()
        .map(|line| render_line(line).plaintext)
        .collect()
}

pub(super) fn render_line(line: &LogLine) -> RenderedLine {
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

pub(super) fn format_elapsed(elapsed: std::time::Duration) -> String {
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

pub(super) fn export_log(state: &AppState) -> String {
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

pub(super) fn localize_log_text(text: &str) -> String {
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
