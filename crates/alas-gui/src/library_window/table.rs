// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Library table rows and their Load, Export, Delete and Rename actions.

use super::*;

pub(super) fn table(state: &mut AppState, ui: &mut Ui, tab: LibraryTab, rows: &[Row]) {
    egui::Grid::new("library_table")
        .num_columns(7)
        .striped(true)
        .spacing(crate::theme::space::GRID_SPACING)
        .show(ui, |ui| {
            for row in rows {
                row_cells(state, ui, tab, row);
                ui.end_row();
            }
        });
}

pub(super) fn row_cells(state: &mut AppState, ui: &mut Ui, tab: LibraryTab, row: &Row) {
    let renaming = state
        .files
        .library
        .rename
        .as_ref()
        .is_some_and(|(key, _)| *key == row.key);
    if renaming {
        let mut apply = false;
        let mut cancel = false;
        if let Some((_, text)) = state.files.library.rename.as_mut() {
            ui.add(egui::TextEdit::singleline(text).desired_width(200.0));
        }
        ui.label("");
        apply |= ui.button(tr("Apply")).clicked();
        cancel |= ui.button(tr("Cancel")).clicked();
        for _ in 0..3 {
            ui.label("");
        }
        if apply {
            if let Some((_, text)) = state.files.library.rename.clone() {
                if rename(state, tab, row, &text) {
                    state.files.library.rename = None;
                    state.files.library.invalidate();
                }
            }
        }
        if cancel {
            state.files.library.rename = None;
        }
        return;
    }
    ui.label(RichText::new(&row.name).strong());
    ui.label(RichText::new(&row.detail).weak().small());
    load_cell(state, ui, tab, row);
    if ui.button(tr("Rename")).clicked() {
        state.files.library.rename = Some((row.key.clone(), row.name.clone()));
        state.files.library.confirm_delete = None;
    }
    let exists = row.path.exists();
    if ui
        .add_enabled(exists, egui::Button::new(tr("Show in folder")))
        .on_hover_text(tr("Open the folder with this file selected."))
        .clicked()
    {
        reveal(state, &row.path.display().to_string(), false);
    }
    export_cell(state, ui, tab, row);
    delete_cell(state, ui, tab, row);
}

pub(super) fn load_cell(state: &mut AppState, ui: &mut Ui, tab: LibraryTab, row: &Row) {
    let enabled = matches!(tab, LibraryTab::Aircraft | LibraryTab::Airports);
    let hover = match tab {
        LibraryTab::Aircraft | LibraryTab::Airports => "",
        LibraryTab::Engines | LibraryTab::Cabins | LibraryTab::Airfoils => {
            "Engines and cabins are loaded from their selectors in the aircraft form."
        }
    };
    if ui
        .add_enabled(enabled, egui::Button::new(tr("Load")))
        .on_disabled_hover_text(tr(hover))
        .clicked()
    {
        match tab {
            LibraryTab::Aircraft => {
                state.guarded(crate::shell_confirm::Pending::UserAircraft(
                    row.path.clone(),
                ));
            }
            LibraryTab::Airports => {
                if let Some(airport) = alas_config::airport_io::registered_custom_airports()
                    .into_iter()
                    .find(|airport| airport.icao == row.key)
                {
                    crate::native_viewport::raise(ui.ctx(), "custom_airport");
                    state.custom_airport_draft =
                        crate::airport_editor::CustomAirportDraft::from_airport(&airport);
                    state.custom_airport_window.open = true;
                    state.custom_airport_status = None;
                }
            }
            LibraryTab::Engines | LibraryTab::Cabins | LibraryTab::Airfoils => {}
        }
    }
}

pub(super) fn export_cell(state: &mut AppState, ui: &mut Ui, tab: LibraryTab, row: &Row) {
    let target = match tab {
        LibraryTab::Aircraft => DialogTarget::AircraftExport(row.path.clone()),
        LibraryTab::Airports => DialogTarget::AirportExportOne(row.key.clone()),
        LibraryTab::Engines | LibraryTab::Cabins | LibraryTab::Airfoils => {
            DialogTarget::PlainFileExport(row.path.clone())
        }
    };
    let data = state.data_dir();
    let file_name = format!("{}.json", safe_stem(&row.name));
    if ui
        .add_enabled(
            !state.file_dialog_pending(&target),
            egui::Button::new(tr("Export")),
        )
        .on_hover_text(tr("Save a copy of this item to any location."))
        .clicked()
    {
        let dialog = spec(
            DialogKind::SaveFile,
            "Export",
            "",
            &data,
            Some(&file_name),
            vec![("ALAS files".to_owned(), vec!["json".to_owned()])],
        );
        state.begin_file_dialog(target, dialog);
    }
}

/// Delete asks for a second click in the same cell; the destructive buttons
/// are red, Keep is neutral.
pub(super) fn delete_cell(state: &mut AppState, ui: &mut Ui, tab: LibraryTab, row: &Row) {
    if state.files.library.confirm_delete.as_deref() != Some(row.key.as_str()) {
        if crate::theme::danger_button(ui, tr("Delete")).clicked() {
            state.files.library.confirm_delete = Some(row.key.clone());
        }
        return;
    }
    ui.horizontal(|ui| {
        if crate::theme::danger_button(ui, tr("Confirm delete")).clicked() {
            delete(state, tab, row);
            state.files.library.confirm_delete = None;
            state.files.library.invalidate();
        }
        if ui.button(tr("Keep")).clicked() {
            state.files.library.confirm_delete = None;
        }
    });
}

pub(super) fn delete(state: &mut AppState, tab: LibraryTab, row: &Row) {
    match tab {
        LibraryTab::Aircraft => state.delete_user_aircraft(&row.path),
        LibraryTab::Airports => state.delete_custom_airport(&row.key),
        LibraryTab::Engines | LibraryTab::Cabins | LibraryTab::Airfoils => {
            match std::fs::remove_file(&row.path) {
                Ok(()) => {
                    state.files.status = Some(tr_fields(
                        "Deleted file {path}.",
                        &[("path", row.path.display().to_string())],
                    ));
                }
                Err(error) => state.files.status = Some(error.to_string()),
            }
        }
    }
}

pub(super) fn rename(state: &mut AppState, tab: LibraryTab, row: &Row, text: &str) -> bool {
    match tab {
        LibraryTab::Aircraft => state.rename_user_aircraft(&row.path, text),
        LibraryTab::Airports => match state.rename_custom_airport(&row.key, text) {
            Ok(()) => true,
            Err(error) => {
                state.files.status = Some(error);
                false
            }
        },
        LibraryTab::Engines | LibraryTab::Cabins | LibraryTab::Airfoils => {
            let stem = safe_stem(text);
            let Some(parent) = row.path.parent() else {
                return false;
            };
            let target = parent.join(format!("{stem}.json"));
            if stem.is_empty() || (target.exists() && target != row.path) {
                state.files.status = Some(tr("Enter a name that is not already used."));
                return false;
            }
            match std::fs::rename(&row.path, &target) {
                Ok(()) => true,
                Err(error) => {
                    state.files.status = Some(error.to_string());
                    false
                }
            }
        }
    }
}
