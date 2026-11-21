// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Widgets for "My aircraft": the selector group, the save window, and the
//! manager window (rename, delete, import, export), plus the data-folder row.

use std::path::PathBuf;

use egui::{Context, RichText, Ui};

use crate::file_dialog::{spec, DialogKind, DialogTarget};
use crate::native_viewport::show_compact_native_viewport;
use crate::state::{AppState, LogKind};
use crate::views::{tr, tr_label};

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

/// "Save as my aircraft...": asks for a name and stores the current design.
pub(crate) fn save_aircraft_button(state: &mut AppState, ui: &mut Ui) {
    if ui
        .button(tr_label("Save as my aircraft\u{2026}"))
        .on_hover_text(tr(
            "Store the current aircraft and mission configuration under a name of your choice. Several variants of one aircraft can be saved.",
        ))
        .clicked()
    {
        // An open save window may be minimised or behind this one.
        crate::native_viewport::raise(ui.ctx(), "save_my_aircraft");
        state.files.save_name = state.default_user_aircraft_name();
        state.files.save_open = true;
        state.files.status = None;
    }
}

/// "Library...": opens the Library on its aircraft tab.
pub(crate) fn library_button(state: &mut AppState, ui: &mut Ui) {
    if ui
        .button(tr_label("Library\u{2026}"))
        .on_hover_text(tr(
            "Open the Library: load, rename, show in folder, export or delete saved aircraft, airports, engines and cabins.",
        ))
        .clicked()
    {
        crate::library_window::open_raised(
            state,
            ui.ctx(),
            crate::library_window::LibraryTab::Aircraft,
        );
    }
}

/// "Open file...": browse for a saved aircraft or any ALAS file.
pub(crate) fn open_file_button(state: &mut AppState, ui: &mut Ui) {
    if ui
        .add_enabled(
            !state.file_dialog_pending(&DialogTarget::LibraryOpenPath),
            egui::Button::new(tr_label("Open file\u{2026}")),
        )
        .on_hover_text(tr_label("Open a saved aircraft or any ALAS file."))
        .clicked()
    {
        let data = state.data_dir();
        let dialog = spec(
            DialogKind::OpenFile,
            "Open File",
            "",
            &data,
            None,
            crate::file_dialog::config_filters(),
        );
        state.begin_file_dialog(DialogTarget::LibraryOpenPath, dialog);
    }
}

/// Draw the save and manager windows when open.
pub(crate) fn show_windows(state: &mut AppState, ctx: &Context) {
    show_save_window(state, ctx);
    show_manager_window(state, ctx);
}

/// Width of the save window; its height follows the content.
const SAVE_WINDOW_WIDTH: f32 = 400.0;

fn show_save_window(state: &mut AppState, ctx: &Context) {
    if !state.files.save_open {
        return;
    }
    let mut close = false;
    let title = tr("Save as my aircraft");
    let response = show_compact_native_viewport(
        ctx,
        "save_my_aircraft",
        title.clone(),
        egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size(egui::vec2(SAVE_WINDOW_WIDTH, 150.0))
            .with_resizable(false),
        |child, ui, _class| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.label(tr("Name"));
            let response = ui.add(
                egui::TextEdit::singleline(&mut state.files.save_name)
                    .desired_width(ui.available_width()),
            );
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
                let (shown, detail) = crate::status_text::present_status(status);
                let label = ui.label(RichText::new(shown).small());
                if let Some(detail) = detail {
                    label.on_hover_text(detail);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can_save = !state.files.save_name.trim().is_empty();
                if (ui
                    .add_enabled(can_save, egui::Button::new(tr("Save")))
                    .on_disabled_hover_text(tr("Enter a name to save the aircraft."))
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
            fit_height_to_content(child, ui);
        },
    );
    if response.close_requested || close {
        state.files.save_open = false;
    }
}

/// Resize the (fixed-width) window to what the content needs, so no space is
/// left unused below the buttons. Only a change of more than two points is sent.
fn fit_height_to_content(ctx: &Context, ui: &Ui) {
    let wanted = ui.min_rect().height() + 2.0 * ui.spacing().window_margin.top + 4.0;
    let id = egui::Id::new("save_window_fitted_height");
    let last: f32 = ctx.data(|data| data.get_temp(id)).unwrap_or(0.0);
    if (wanted - last).abs() > 2.0 {
        ctx.data_mut(|data| data.insert_temp(id, wanted));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
            SAVE_WINDOW_WIDTH,
            wanted,
        )));
    }
}

fn show_manager_window(state: &mut AppState, ctx: &Context) {
    crate::library_window::show(state, ctx);
}

/// Show the per-user data folder with an "Open folder" button. The button is
/// a full-width menu item, so its hover highlight spans the menu like every
/// other entry.
pub(crate) fn show_data_folder_row(state: &mut AppState, ui: &mut Ui) {
    let data = state.data_dir();
    ui.label(RichText::new(data.display().to_string()).weak().small());
    if ui.button(tr("Open folder")).clicked() {
        if let Err(error) = std::fs::create_dir_all(&data) {
            state.log(error.to_string(), LogKind::Warn);
        } else if let Err(error) =
            crate::views::tools_view::open_in_file_explorer(&data.display().to_string(), true)
        {
            state.log(error, LogKind::Warn);
        }
    }
}

/// The File-menu buttons that browse for a configuration to load or a
/// destination to save to, and the data-folder row beneath them.
pub(crate) fn show_config_file_buttons(state: &mut AppState, ui: &mut Ui) {
    let data = state.data_dir();
    {
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
    }
    show_data_folder_row(state, ui);
}
