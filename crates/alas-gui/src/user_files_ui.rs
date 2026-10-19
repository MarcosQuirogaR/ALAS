// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Widgets for "My aircraft": the selector group, the save window, and the
//! manager window (rename, delete, import, export), plus the data-folder row.

use std::path::PathBuf;

use egui::{Context, RichText, Ui};

use crate::file_dialog::{spec, DialogKind, DialogTarget};
use crate::state::{AppState, LogKind};
use crate::user_files::format_utc;
use crate::views::tr;

/// Filters for aircraft files.
fn aircraft_filters() -> Vec<(String, Vec<String>)> {
    vec![(
        "ALAS aircraft or configuration".to_owned(),
        vec!["json".to_owned(), "yaml".to_owned(), "yml".to_owned()],
    )]
}

/// The "My aircraft" group at the end of the aircraft list. Entries are
/// filtered by `query` over name and base preset. Returns the file clicked.
pub(crate) fn my_aircraft_group(state: &mut AppState, ui: &mut Ui, query: &str) -> Option<PathBuf> {
    let active = state.files.active_aircraft.clone();
    let entries = state.user_aircraft().to_vec();
    let visible: Vec<_> = entries
        .iter()
        .filter(|entry| {
            crate::selector_search::matches(
                query,
                &[&entry.header.name, &entry.header.base_preset, "my aircraft"],
            )
        })
        .collect();
    if visible.is_empty() {
        return None;
    }
    ui.separator();
    ui.label(RichText::new(tr("My aircraft")).strong());
    let mut chosen = None;
    for entry in visible {
        let selected = active.as_deref() == Some(entry.header.name.as_str());
        let mut response = ui.selectable_label(selected, &entry.header.name);
        if !entry.header.base_preset.is_empty() {
            response =
                response.on_hover_text(format!("{} {}", tr("Based on"), entry.header.base_preset));
        }
        if response.clicked() {
            chosen = Some(entry.path.clone());
        }
    }
    chosen
}

/// The buttons under the aircraft selector.
pub(crate) fn show_my_aircraft_row(state: &mut AppState, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        if ui
            .button(tr("Save as my aircraft..."))
            .on_hover_text(tr(
                "Store the current aircraft and mission configuration under a name of your choice. Several variants of one aircraft can be saved.",
            ))
            .clicked()
        {
            state.files.save_name = state.default_user_aircraft_name();
            state.files.save_open = true;
            state.files.status = None;
        }
        if ui
            .button(tr("Manage my aircraft..."))
            .on_hover_text(tr("Rename, delete, import or export saved aircraft."))
            .clicked()
        {
            state.refresh_user_aircraft();
            state.files.manager_open = true;
            state.files.status = None;
        }
    });
}

/// Draw the save and manager windows when open.
pub(crate) fn show_windows(state: &mut AppState, ctx: &Context) {
    show_save_window(state, ctx);
    show_manager_window(state, ctx);
}

fn show_save_window(state: &mut AppState, ctx: &Context) {
    if !state.files.save_open {
        return;
    }
    let mut open = true;
    let mut close = false;
    egui::Window::new(tr("Save as my aircraft"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(tr("Name"));
            let response =
                ui.add(egui::TextEdit::singleline(&mut state.files.save_name).desired_width(320.0));
            let exists = state.files.aircraft.iter().any(|a| {
                a.header
                    .name
                    .eq_ignore_ascii_case(state.files.save_name.trim())
            });
            if exists {
                ui.label(
                    RichText::new(tr(
                        "An aircraft with this name exists and will be replaced.",
                    ))
                    .weak()
                    .small(),
                );
            }
            ui.label(
                RichText::new(format!(
                    "{} {}",
                    tr("Saved in"),
                    crate::user_files::aircraft_dir(&state.data_dir()).display()
                ))
                .weak()
                .small(),
            );
            let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if let Some(status) = &state.files.status {
                ui.label(RichText::new(status).small());
            }
            ui.horizontal(|ui| {
                let can_save = !state.files.save_name.trim().is_empty();
                if (ui
                    .add_enabled(can_save, egui::Button::new(tr("Save")))
                    .clicked()
                    || (enter && can_save))
                    && state.save_current_as_user_aircraft(&state.files.save_name.clone())
                {
                    close = true;
                }
                if ui.button(tr("Cancel")).clicked() {
                    close = true;
                }
            });
        });
    if !open || close {
        state.files.save_open = false;
    }
}

fn show_manager_window(state: &mut AppState, ctx: &Context) {
    if !state.files.manager_open {
        return;
    }
    let mut open = true;
    egui::Window::new(tr("My aircraft"))
        .open(&mut open)
        .default_width(560.0)
        .show(ctx, |ui| manager_body(state, ui));
    if !open {
        state.files.manager_open = false;
    }
}

fn manager_body(state: &mut AppState, ui: &mut Ui) {
    show_data_folder_row(state, ui);
    ui.horizontal(|ui| {
        let data = state.data_dir();
        crate::file_dialog::browse_button(state, ui, DialogTarget::AircraftImport, |_| {
            spec(
                DialogKind::OpenFile,
                "Import aircraft",
                "",
                &data,
                None,
                aircraft_filters(),
            )
        });
        ui.label(tr("Import an aircraft file from any location."));
    });
    ui.separator();
    let entries = state.user_aircraft().to_vec();
    if entries.is_empty() {
        ui.label(tr(
            "No saved aircraft yet. Use \"Save as my aircraft...\" on the Inputs page.",
        ));
    }
    for entry in entries {
        ui.horizontal_wrapped(|ui| {
            let renaming = state
                .files
                .rename
                .as_ref()
                .is_some_and(|(path, _)| *path == entry.path);
            if renaming {
                let mut commit = false;
                let mut cancel = false;
                if let Some((_, text)) = state.files.rename.as_mut() {
                    ui.add(egui::TextEdit::singleline(text).desired_width(220.0));
                }
                if ui.button(tr("Apply")).clicked() {
                    commit = true;
                }
                if ui.button(tr("Cancel")).clicked() {
                    cancel = true;
                }
                if commit {
                    if let Some((path, text)) = state.files.rename.clone() {
                        if state.rename_user_aircraft(&path, &text) {
                            state.files.rename = None;
                        }
                    }
                }
                if cancel {
                    state.files.rename = None;
                }
                return;
            }
            ui.label(RichText::new(&entry.header.name).strong());
            let when = if entry.header.saved_unix > 0 {
                format_utc(entry.header.saved_unix)
            } else {
                String::new()
            };
            ui.label(RichText::new(when).weak().small());
            if ui.button(tr("Load")).clicked() {
                state.load_user_aircraft(&entry.path);
            }
            if ui.button(tr("Rename")).clicked() {
                state.files.rename = Some((entry.path.clone(), entry.header.name.clone()));
            }
            let target = DialogTarget::AircraftExport(entry.path.clone());
            let data = state.data_dir();
            let file_name = format!("{}.json", entry.header.name.replace(['/', '\\', ':'], "-"));
            crate::file_dialog::browse_button(state, ui, target, |_| {
                spec(
                    DialogKind::SaveFile,
                    "Export aircraft",
                    "",
                    &data,
                    Some(&file_name),
                    aircraft_filters(),
                )
            })
            .on_hover_text(tr("Export this aircraft to any location."));
            if state.files.confirm_delete.as_ref() == Some(&entry.path) {
                if ui.button(tr("Confirm delete")).clicked() {
                    state.delete_user_aircraft(&entry.path);
                    state.files.confirm_delete = None;
                }
                if ui.button(tr("Keep")).clicked() {
                    state.files.confirm_delete = None;
                }
            } else if ui.button(tr("Delete")).clicked() {
                state.files.confirm_delete = Some(entry.path.clone());
            }
        });
    }
    if let Some(status) = state.files.status.clone() {
        ui.separator();
        ui.label(RichText::new(status).small());
    }
}

/// Show the per-user data folder with an "Open folder" button.
pub(crate) fn show_data_folder_row(state: &mut AppState, ui: &mut Ui) {
    let data = state.data_dir();
    ui.horizontal_wrapped(|ui| {
        ui.label(tr("Data folder:"));
        ui.label(RichText::new(data.display().to_string()).monospace());
        if ui
            .button(tr("Open folder"))
            .on_hover_text(tr(
                "Saved configurations, My aircraft, custom airports and tool preferences are stored here by default. Any other location can be chosen in each dialog.",
            ))
            .clicked()
        {
            if let Err(error) = std::fs::create_dir_all(&data) {
                state.log(error.to_string(), LogKind::Warn);
            } else if let Err(error) =
                crate::views::tools_view::open_in_file_explorer(&data.display().to_string(), true)
            {
                state.log(error, LogKind::Warn);
            }
        }
    });
}

/// The File-menu buttons that browse for a configuration to load or a
/// destination to save to, and the data-folder row beneath them.
pub(crate) fn show_config_file_buttons(state: &mut AppState, ui: &mut Ui) {
    let data = state.data_dir();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !state.file_dialog_pending(&DialogTarget::ConfigLoad),
                egui::Button::new(tr("Browse to load...")),
            )
            .clicked()
        {
            let dialog = spec(
                DialogKind::OpenFile,
                "Load configuration",
                &state.config_path,
                &data,
                None,
                crate::file_dialog::config_filters(),
            );
            state.begin_file_dialog(DialogTarget::ConfigLoad, dialog);
            ui.close_menu();
        }
        if ui
            .add_enabled(
                !state.file_dialog_pending(&DialogTarget::ConfigSave),
                egui::Button::new(tr("Save as...")),
            )
            .clicked()
        {
            let name = std::path::Path::new(&state.config_path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "alas-config.json".to_owned());
            let dialog = spec(
                DialogKind::SaveFile,
                "Save configuration",
                &state.config_path,
                &data,
                Some(&name),
                crate::file_dialog::config_filters(),
            );
            state.begin_file_dialog(DialogTarget::ConfigSave, dialog);
            ui.close_menu();
        }
    });
    show_data_folder_row(state, ui);
}
