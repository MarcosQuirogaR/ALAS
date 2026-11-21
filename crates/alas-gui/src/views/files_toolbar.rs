// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The title row (page title and actions for the current folder), the
//! autosave notice and the "Save current as" window of the Files page.

use egui::{RichText, Ui};

use super::files_state::ViewMode;
use super::{json_filters, spec, tr, tr_label, AppState, DialogKind, DialogTarget, ObjectKind};

/// The library kind whose folder is `dir`, when it is one.
fn kind_of_folder(state: &AppState, dir: &std::path::Path) -> Option<ObjectKind> {
    let data = state.data_dir();
    ObjectKind::ALL
        .into_iter()
        .find(|kind| crate::file_library::kind_dir(&data, *kind) == dir)
}

pub(super) fn show_title_row(state: &mut AppState, ui: &mut Ui) {
    let data = state.data_dir();
    ui.horizontal_wrapped(|ui| {
        ui.heading(tr_label("Files"));
        ui.add_space(12.0);
        if ui.button(tr("New folder")).clicked() {
            super::files_menu::new_folder(state);
        }
        if ui
            .button(tr("Save current as\u{2026}"))
            .on_hover_text(tr(
                "Save the open aircraft, mission or payload to the library.",
            ))
            .clicked()
        {
            open_save_window(state);
        }
        if ui.button(tr("Import\u{2026}")).clicked() {
            let dialog = spec(
                DialogKind::OpenFile,
                "Import",
                "",
                &data,
                None,
                json_filters(),
            );
            state.begin_file_dialog(DialogTarget::LibraryImport, dialog);
        }
        if ui.button(tr("Refresh")).clicked() {
            state.shell.library.invalidate();
            state.shell.library.storage.invalidate();
        }
        ui.separator();
        let lib = &mut state.shell.library;
        ui.scope(|ui| {
            crate::theme::segmented_style(ui);
            for (mode, label) in [
                (ViewMode::Details, "Details"),
                (ViewMode::Icons, "Large icons"),
            ] {
                if ui
                    .add(crate::theme::selectable_button(tr(label), lib.mode == mode))
                    .clicked()
                {
                    lib.mode = mode;
                    lib.scroll_to = Some(0.0);
                }
            }
        });
        ui.separator();
        ui.checkbox(&mut lib.show_preview, tr("Preview pane"));
    });
}

/// Open the save window on the kind of the folder being shown.
pub(super) fn open_save_window(state: &mut AppState) {
    let current = state.shell.library.current();
    if let Some(kind) = kind_of_folder(state, &current) {
        if state.shell.library.kind != kind {
            state.shell.library.kind = kind;
            state.shell.library.save_name.clear();
            state.shell.library.save_choice.clear();
        }
    }
    state.shell.library.save_open = true;
}

/// One thin line when an earlier session left autosaved work to restore.
pub(super) fn show_autosave_notice(state: &mut AppState, ui: &mut Ui) {
    let count = state
        .shell
        .library
        .autosaves
        .iter()
        .filter(|(slot, _)| state.slot_protected(slot))
        .count();
    if count == 0 || state.shell.library.notice_dismissed {
        return;
    }
    let mut review = false;
    let mut dismiss = false;
    crate::theme::card_frame(ui)
        .inner_margin(egui::Margin::symmetric(12.0, 4.0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(tr_label("Recovered work"));
                ui.label(
                    RichText::new(crate::views::tr_fields(
                        "{count} autosaved copies from the last session can be restored.",
                        &[("count", count.to_string())],
                    ))
                    .weak(),
                );
                if ui.button(tr("Review")).clicked() {
                    review = true;
                }
                if ui.button(tr("Dismiss")).clicked() {
                    dismiss = true;
                }
            });
        });
    if review {
        let folder = state.data_dir().join("autosave");
        state.shell.library.go(folder);
    }
    if review || dismiss {
        state.shell.library.notice_dismissed = true;
    }
}

/// The small window behind the toolbar's "Save current as" button.
pub(super) fn show_save_window(state: &mut AppState, ctx: &egui::Context) {
    if !state.shell.library.save_open {
        return;
    }
    let response = crate::native_viewport::show_compact_native_viewport(
        ctx,
        "files_save_current",
        tr("Save current as\u{2026}"),
        egui::ViewportBuilder::default()
            .with_title(tr("Save current as\u{2026}"))
            .with_inner_size(egui::vec2(560.0, 130.0))
            .with_resizable(true),
        |_ctx, ui, _class| {
            let kind = state.shell.library.kind;
            let choices = state.library_save_choices(kind);
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_id_salt("files_save_kind")
                    .icon(crate::combo_icon::down_arrow)
                    .selected_text(tr_label(kind.name()))
                    .show_ui(ui, |ui| {
                        for each in ObjectKind::ALL {
                            if ui
                                .selectable_label(kind == each, tr_label(each.name()))
                                .clicked()
                            {
                                state.shell.library.kind = each;
                                state.shell.library.save_name.clear();
                                state.shell.library.save_choice.clear();
                            }
                        }
                    });
                if matches!(kind, ObjectKind::Airport | ObjectKind::Airfoil) {
                    egui::ComboBox::from_id_salt("library_save_choice")
                        .icon(crate::combo_icon::down_arrow)
                        .selected_text(if state.shell.library.save_choice.is_empty() {
                            tr_label("Choose")
                        } else {
                            state.shell.library.save_choice.clone()
                        })
                        .show_ui(ui, |ui| {
                            for choice in &choices {
                                let on = state.shell.library.save_choice == *choice;
                                if ui.selectable_label(on, choice).clicked() {
                                    state.shell.library.save_choice = choice.clone();
                                    state.shell.library.save_name = choice.clone();
                                }
                            }
                        });
                }
                if state.shell.library.save_name.is_empty() {
                    state.shell.library.save_name = state.library_default_name(kind);
                }
                ui.add(
                    egui::TextEdit::singleline(&mut state.shell.library.save_name)
                        .hint_text(tr_label("Name"))
                        .desired_width(200.0),
                );
                if ui.button(tr_label("Save")).clicked() {
                    let name = state.shell.library.save_name.clone();
                    let choice = state.shell.library.save_choice.clone();
                    if state.library_save(kind, &name, &choice) {
                        if kind == ObjectKind::Aircraft {
                            state.refresh_user_aircraft();
                        }
                        state.shell.library.invalidate();
                        state.shell.library.save_open = false;
                    }
                }
            });
        },
    );
    if response.close_requested {
        state.shell.library.save_open = false;
    }
}
