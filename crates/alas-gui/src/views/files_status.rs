// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The status bar of the Files explorer: item and selection counts, the
//! running operation, the last message and the storage total (measured on a
//! worker thread by `files_storage`).

use egui::{RichText, Ui};

use super::files_model::{format_size, Entry};
use super::{tr, tr_fields, AppState};

pub(super) fn show_status(state: &mut AppState, ui: &mut Ui, entries: &[Entry], order: &[usize]) {
    let lib = &state.shell.library;
    let selected = lib.selected(entries, order);
    let mut text = tr_fields("{count} items", &[("count", order.len().to_string())]);
    if !selected.is_empty() {
        let bytes: u64 = selected.iter().map(|entry| entry.size).sum();
        text.push_str("   ");
        text.push_str(&tr_fields(
            "{count} selected",
            &[("count", selected.len().to_string())],
        ));
        if bytes > 0 {
            text.push_str(", ");
            text.push_str(&format_size(bytes));
        }
    }
    let truncated =
        lib.searching_subfolders() && lib.search.results.as_ref().is_some_and(|(_, cut)| *cut);
    let busy = lib.jobs.busy_ops > 0;
    ui.separator();
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(RichText::new(text).small());
        if truncated {
            ui.label(
                RichText::new(tr("Only the first results are shown."))
                    .small()
                    .weak(),
            );
        }
        if busy {
            ui.spinner();
            ui.label(RichText::new(tr("Working\u{2026}")).small().weak());
        }
        if let Some(message) = state.files.status.as_deref() {
            let (shown, detail) = crate::status_text::present_status(message);
            let label = ui.label(RichText::new(shown).small().weak());
            if let Some(detail) = detail {
                label.on_hover_text(detail);
            }
        }
        // The storage total sits at the right edge like Explorer's status
        // bar; on a narrow window it is cut with an ellipsis, never wrapped.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            crate::files_storage::show_footer(state, ui);
        });
    });
}
