// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Console row formatting, filtering, localization and export.

use crate::state::{AppState, LogKind, LogLine};
use crate::views::{tr, tr_fields};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) struct RenderedLine {
    pub(super) context: String,
    pub(super) timing: String,
    pub(super) severity: &'static str,
    pub(super) message: String,
    pub(super) kind: LogKind,
    pub(super) plaintext: String,
}

/// Everything a change to the rendered console rows depends on. Reused
/// across frames while unchanged instead of re-formatting, translating and
/// lowercasing every line (up to [`crate::state::MAX_LOG_LINES`]) on every
/// repaint, including idle ones.
#[derive(Clone, PartialEq)]
struct LinesCacheKey {
    fingerprint: u64,
    show_info: bool,
    show_warn: bool,
    show_error: bool,
    search: String,
    language: String,
}

#[derive(Clone)]
struct RenderedLinesCache {
    key: LinesCacheKey,
    lines: Arc<Vec<RenderedLine>>,
}

/// A stand-in for the log content: every line's identity, hashed. `logs`
/// saturates at [`crate::state::MAX_LOG_LINES`] and then evicts its oldest
/// entry on every push, so neither its length nor its ends identify the
/// content (pushing a repeated message can leave both unchanged while every
/// row shifts). Hashing about a hundred kilobytes costs well under the
/// formatting, translation and lowercasing this key lets a frame skip.
fn logs_fingerprint(logs: &[LogLine]) -> u64 {
    let mut hasher = DefaultHasher::new();
    logs.len().hash(&mut hasher);
    for line in logs {
        line.run_id.hash(&mut hasher);
        (line.kind as u8).hash(&mut hasher);
        line.elapsed.hash(&mut hasher);
        line.text.hash(&mut hasher);
    }
    hasher.finish()
}

/// The visible console rows for the current filters, search text and
/// language, memoized in `ctx`'s memory behind [`LinesCacheKey`].
pub(super) fn rendered_visible_lines(
    ctx: &egui::Context,
    state: &AppState,
) -> Arc<Vec<RenderedLine>> {
    let key = LinesCacheKey {
        fingerprint: logs_fingerprint(&state.logs),
        show_info: state.run_log_show_info,
        show_warn: state.run_log_show_warn,
        show_error: state.run_log_show_error,
        search: state.run_log_search.clone(),
        language: alas_i18n::get_language(),
    };
    let id = egui::Id::new("run_log_rendered_lines_cache");
    if let Some(cached) = ctx.data(|data| data.get_temp::<RenderedLinesCache>(id)) {
        if cached.key == key {
            return cached.lines;
        }
    }
    let lines = Arc::new(compute_visible_lines(state, &key.search));
    ctx.data_mut(|data| {
        data.insert_temp(
            id,
            RenderedLinesCache {
                key,
                lines: lines.clone(),
            },
        )
    });
    lines
}

pub(super) fn compute_visible_lines(state: &AppState, raw_search: &str) -> Vec<RenderedLine> {
    let query = raw_search.trim().to_lowercase();
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

#[cfg(test)]
mod cache_tests {
    use super::rendered_visible_lines;
    use crate::state::{AppState, LogKind, LogLine};
    use std::sync::Arc;

    fn push(state: &mut AppState, text: &str) {
        state.logs.push(LogLine {
            text: text.to_owned(),
            kind: LogKind::Info,
            elapsed: None,
            run_id: 0,
        });
    }

    #[test]
    fn a_repeat_call_with_unchanged_logs_reuses_the_cached_rows() {
        let ctx = egui::Context::default();
        let mut state = AppState::default();
        push(&mut state, "Ready.");

        let first = rendered_visible_lines(&ctx, &state);
        let second = rendered_visible_lines(&ctx, &state);

        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn a_new_log_line_invalidates_the_cache() {
        let ctx = egui::Context::default();
        let mut state = AppState::default();
        push(&mut state, "Ready.");
        let first = rendered_visible_lines(&ctx, &state);

        push(&mut state, "Working.");
        let second = rendered_visible_lines(&ctx, &state);

        assert!(!Arc::ptr_eq(&first, &second));
        assert_eq!(second.len(), first.len() + 1);
    }

    #[test]
    fn a_search_text_change_invalidates_the_cache() {
        let ctx = egui::Context::default();
        let mut state = AppState::default();
        push(&mut state, "Ready.");
        push(&mut state, "Working.");
        let first = rendered_visible_lines(&ctx, &state);

        state.run_log_search = "working".to_owned();
        let second = rendered_visible_lines(&ctx, &state);

        assert!(!Arc::ptr_eq(&first, &second));
        assert_eq!(second.len(), 1);
    }
}
