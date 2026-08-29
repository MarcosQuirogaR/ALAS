// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The scrolling, colour-coded run log beneath the page content.
//!
//! A port of the reference desktop app's `RunLog`.

use egui::{RichText, ScrollArea, Ui};

use crate::state::{AppState, LogKind};
use crate::views::{tr, tr_fields};

/// Render the run log, pinned to its most recent line.
pub fn show_run_log(state: &AppState, ui: &mut Ui) {
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
    });
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
                for line in &state.logs {
                    let color = match line.kind {
                        LogKind::Info => None,
                        LogKind::Warn => Some(ui.visuals().warn_fg_color),
                        LogKind::Error => Some(ui.visuals().error_fg_color),
                    };
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
                    let rendered =
                        format!("[{context}] [{severity}] {}", localize_log_text(&line.text));
                    let mut text = RichText::new(rendered).monospace().size(12.0);
                    if let Some(c) = color {
                        text = text.color(c);
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
