// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Library window: one table per kind of user-saved object (aircraft,
//! airports, engines, cabins) with the same row actions everywhere: Load,
//! Rename, Show in folder, Export, Delete. The header names the real folder of
//! the tab. Aircraft and airports are loaded and saved elsewhere; engines and
//! cabins are listed here from their folders (their editors write the files).

use std::path::{Path, PathBuf};

use egui::{RichText, Ui};

use crate::file_dialog::{spec, DialogKind, DialogTarget};
use crate::native_viewport::show_native_viewport;
use crate::state::{AppState, LogKind};
use crate::user_files::format_utc;
use crate::views::{tr, tr_fields};

/// The tabs of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LibraryTab {
    /// Saved aircraft (`my-aircraft`).
    #[default]
    Aircraft,
    /// Custom airports (`library/my-airports`).
    Airports,
    /// Saved engines (`library/my-engines`).
    Engines,
    /// Saved cabin layouts (`library/my-cabins`).
    Cabins,
    /// Imported custom airfoils (kept in the workspace, not as files).
    Airfoils,
}

impl LibraryTab {
    const ALL: [Self; 5] = [
        Self::Aircraft,
        Self::Airports,
        Self::Engines,
        Self::Cabins,
        Self::Airfoils,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::Aircraft => "Aircraft",
            Self::Airports => "Airports",
            Self::Engines => "Engines",
            Self::Cabins => "Cabins",
            Self::Airfoils => "Airfoils",
        }
    }
}

/// One row of a tab.
#[derive(Debug, Clone)]
struct Row {
    /// Stable identity: the ICAO code for airports, the file path otherwise.
    key: String,
    name: String,
    detail: String,
    path: PathBuf,
}

/// State of the Library window.
#[derive(Debug, Default)]
pub struct LibraryWindowState {
    /// The tab shown.
    pub tab: LibraryTab,
    rows: Option<(LibraryTab, Vec<Row>)>,
    rename: Option<(String, String)>,
    confirm_delete: Option<String>,
}

impl LibraryWindowState {
    /// Re-read the listing on the next frame.
    pub fn invalidate(&mut self) {
        self.rows = None;
    }
}

/// The folder of a tab. The Airfoils tab has no folder of its own: it answers
/// with the library root, which holds the other tabs' folders.
pub fn tab_dir(state: &AppState, tab: LibraryTab) -> PathBuf {
    match tab {
        LibraryTab::Airfoils => state
            .tool_locator
            .my_airports_dir()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default(),
        LibraryTab::Aircraft => crate::user_files::aircraft_dir(&state.data_dir()),
        LibraryTab::Airports => state.tool_locator.my_airports_dir(),
        LibraryTab::Engines => state.tool_locator.my_engines_dir(),
        LibraryTab::Cabins => state.tool_locator.my_cabins_dir(),
    }
}

/// `path` written with the platform's separator only: folders built by
/// joining a relative `library/my-aircraft` string would otherwise read
/// `...\ALAS\library/my-aircraft` on Windows.
pub(crate) fn platform_path(path: &Path) -> String {
    let text = path.display().to_string();
    match std::path::MAIN_SEPARATOR {
        '\\' => text.replace('/', "\\"),
        _ => text,
    }
}

/// Open the window on `tab`, creating the library folders if needed.
pub fn open(state: &mut AppState, tab: LibraryTab) {
    for each in LibraryTab::ALL {
        let _ = std::fs::create_dir_all(tab_dir(state, each));
    }
    state.refresh_user_aircraft();
    state.files.library.tab = tab;
    state.files.library.invalidate();
    state.files.library.rename = None;
    state.files.library.confirm_delete = None;
    state.files.manager_open = true;
    state.files.status = None;
}

/// [`open`], first raising an already-open Library window (it may be minimised
/// or behind the one the request came from).
pub(crate) fn open_raised(state: &mut AppState, ctx: &egui::Context, tab: LibraryTab) {
    crate::native_viewport::raise(ctx, "my_aircraft");
    open(state, tab);
}

fn file_rows(dir: &Path) -> Vec<Row> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut rows: Vec<Row> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .map(|path| {
            let modified = std::fs::metadata(&path)
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|elapsed| format_utc(elapsed.as_secs()))
                .unwrap_or_default();
            Row {
                key: path.display().to_string(),
                name: path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                detail: modified,
                path,
            }
        })
        .collect();
    rows.sort_by_key(|row| row.name.to_lowercase());
    rows
}

fn build_rows(state: &mut AppState, tab: LibraryTab) -> Vec<Row> {
    match tab {
        LibraryTab::Aircraft => state
            .user_aircraft()
            .iter()
            .map(|entry| Row {
                key: entry.path.display().to_string(),
                name: entry.header.name.clone(),
                detail: if entry.header.saved_unix > 0 {
                    format_utc(entry.header.saved_unix)
                } else {
                    String::new()
                },
                path: entry.path.clone(),
            })
            .collect(),
        LibraryTab::Airports => {
            let dir = state.tool_locator.my_airports_dir();
            let mut rows: Vec<Row> = alas_config::airport_io::registered_custom_airports()
                .into_iter()
                .map(|airport| Row {
                    key: airport.icao.clone(),
                    detail: format!(
                        "{} - {:.0} m - {}",
                        airport.icao,
                        airport.altitude_m,
                        tr(provenance_label(airport.provenance.kind))
                    ),
                    path: crate::airport_store::airport_path(&dir, &airport.icao),
                    name: airport.name,
                })
                .collect();
            rows.sort_by_key(|row| row.name.to_lowercase());
            rows
        }
        LibraryTab::Engines | LibraryTab::Cabins => file_rows(&tab_dir(state, tab)),
        LibraryTab::Airfoils => Vec::new(),
    }
}

/// The Airfoils tab: the airfoils imported in this workspace and the single
/// entry to import another one (Selig `.dat`).
fn airfoils_body(state: &mut AppState, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        if ui
            .button(tr("Import airfoil..."))
            .on_hover_text(tr(
                "Import a Selig .dat airfoil; it then appears in every airfoil selector.",
            ))
            .clicked()
        {
            crate::native_viewport::raise(ui.ctx(), "custom_airfoil_import");
            state.screening.custom_airfoil_import_open = true;
        }
    });
    ui.add_space(4.0);
    let records = alas_geom::airfoil_io::records();
    if records.is_empty() {
        ui.label(RichText::new(tr("Nothing saved here yet.")).weak());
    }
    for record in records {
        ui.label(tr_fields(
            "{name} - {count} points",
            &[
                ("name", record.name.clone()),
                ("count", record.coordinates.len().to_string()),
            ],
        ));
    }
}

/// Draw the window when open.
pub(crate) fn show(state: &mut AppState, ctx: &egui::Context) {
    if !state.files.manager_open {
        return;
    }
    let title = tr("Library");
    let response = show_native_viewport(
        ctx,
        "my_aircraft",
        title.clone(),
        egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size(egui::vec2(760.0, 420.0))
            .with_min_inner_size(egui::vec2(560.0, 260.0))
            .with_resizable(true),
        |ctx, ui, _class| {
            // Escape closes the window, unless a rename box has the keyboard
            // and uses the key itself. (`wants_keyboard_input` is true for any
            // focused widget, a clicked tab button included, so it is no test
            // for typing.)
            if state.files.library.rename.is_none()
                && ctx.input(|input| input.key_pressed(egui::Key::Escape))
            {
                state.files.manager_open = false;
                return;
            }
            body(state, ui);
        },
    );
    if response.close_requested {
        state.files.manager_open = false;
    }
}

fn body(state: &mut AppState, ui: &mut Ui) {
    ui.horizontal(|ui| {
        for tab in LibraryTab::ALL {
            if ui
                .selectable_label(state.files.library.tab == tab, tr(tab.title()))
                .clicked()
            {
                state.files.library.tab = tab;
                state.files.library.rename = None;
                state.files.library.confirm_delete = None;
                state.files.library.invalidate();
            }
        }
    });
    ui.separator();
    let tab = state.files.library.tab;
    if tab == LibraryTab::Airfoils {
        airfoils_body(state, ui);
        return;
    }
    folder_header(state, ui, tab);
    ui.add_space(4.0);
    let fresh = state
        .files
        .library
        .rows
        .as_ref()
        .is_some_and(|(cached, _)| *cached == tab);
    if !fresh {
        let rows = build_rows(state, tab);
        state.files.library.rows = Some((tab, rows));
    }
    let rows = state
        .files
        .library
        .rows
        .as_ref()
        .map(|(_, rows)| rows.clone())
        .unwrap_or_default();
    egui::ScrollArea::vertical()
        .id_salt("library_window_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if rows.is_empty() {
                // The empty state sits in the middle of the list area.
                ui.add_space((ui.available_height() * 0.5 - 24.0).max(8.0));
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(tr("Nothing saved here yet.")).weak());
                });
            } else {
                table(state, ui, tab, &rows);
            }
            if let Some(status) = state.files.status.as_deref() {
                ui.add_space(6.0);
                let (shown, detail) = crate::status_text::present_status(status);
                let label = ui.label(RichText::new(shown).small());
                if let Some(detail) = detail {
                    label.on_hover_text(detail);
                }
            }
        });
}

fn folder_header(state: &mut AppState, ui: &mut Ui, tab: LibraryTab) {
    let dir = tab_dir(state, tab);
    ui.horizontal_wrapped(|ui| {
        ui.label(tr("Folder:"));
        ui.label(RichText::new(platform_path(&dir)).monospace());
        if ui.button(tr("Open folder")).clicked() {
            let _ = std::fs::create_dir_all(&dir);
            reveal(state, &dir.display().to_string(), true);
        }
        let importing = match tab {
            LibraryTab::Aircraft => DialogTarget::AircraftImport,
            LibraryTab::Airports => DialogTarget::AirportImport,
            LibraryTab::Engines | LibraryTab::Cabins | LibraryTab::Airfoils => {
                DialogTarget::FolderImport(dir.clone())
            }
        };
        let data = state.data_dir();
        let filters = vec![(
            "ALAS files".to_owned(),
            vec![
                "json".to_owned(),
                "yaml".to_owned(),
                "yml".to_owned(),
                "dat".to_owned(),
            ],
        )];
        if ui
            .add_enabled(
                !state.file_dialog_pending(&importing),
                egui::Button::new(tr("Import...")),
            )
            .on_hover_text(tr("Import a file from any location into this folder."))
            .clicked()
        {
            let dialog = spec(DialogKind::OpenFile, "Import", "", &data, None, filters);
            state.begin_file_dialog(importing, dialog);
        }
        if tab == LibraryTab::Airports
            && ui
                .button(tr("New airport..."))
                .on_hover_text(tr("Enter a new custom airport in the airport editor."))
                .clicked()
        {
            crate::native_viewport::raise(ui.ctx(), "custom_airport");
            state.custom_airport_draft = crate::airport_editor::CustomAirportDraft::default();
            state.custom_airport_window.open = true;
            state.custom_airport_window.target = None;
            state.custom_airport_status = None;
        }
    });
}

fn reveal(state: &mut AppState, path: &str, directory: bool) {
    if let Err(error) = crate::views::tools_view::open_in_file_explorer(path, directory) {
        state.log(error, LogKind::Warn);
    }
}

fn provenance_label(kind: alas_config::airport_io::AirportProvenanceKind) -> &'static str {
    use alas_config::airport_io::AirportProvenanceKind as K;
    match kind {
        K::UserEntered => "user entered",
        K::DatImport => "DAT import",
        K::JsonImport => "JSON import",
        K::Workspace => "workspace",
    }
}

/// A file-system-safe stem for a typed name.
fn safe_stem(name: &str) -> String {
    let stem: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | ' ') {
                c
            } else {
                '-'
            }
        })
        .collect();
    stem.trim().to_owned()
}
mod table;
use table::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_path_is_shown_with_one_separator() {
        let mixed = Path::new("C:\\Users\\a\\ALAS\\library/my-aircraft");
        let shown = platform_path(mixed);
        if std::path::MAIN_SEPARATOR == '\\' {
            assert_eq!(shown, "C:\\Users\\a\\ALAS\\library\\my-aircraft");
        } else {
            assert_eq!(shown, mixed.display().to_string());
        }
    }

    /// Saving, listing through the Library, then loading leaves one entry.
    #[test]
    fn save_then_manage_then_load_leaves_one_aircraft() {
        let dir = std::env::temp_dir().join(format!("alas-library-b1-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut state = AppState {
            tool_locator: alas_exec::ToolLocator::new(dir.join("app"), dir.join("user")),
            ..AppState::default()
        };
        assert!(state.save_current_as_user_aircraft("Regression"));
        open(&mut state, LibraryTab::Aircraft);
        let rows = build_rows(&mut state, LibraryTab::Aircraft);
        assert_eq!(rows.len(), 1);
        state.load_user_aircraft(&rows[0].path);
        state.refresh_user_aircraft();
        assert_eq!(build_rows(&mut state, LibraryTab::Aircraft).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn engine_and_cabin_folders_are_created_and_listed() {
        let dir = std::env::temp_dir().join(format!("alas-library-f2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut state = AppState {
            tool_locator: alas_exec::ToolLocator::new(dir.join("app"), dir.join("user")),
            ..AppState::default()
        };
        open(&mut state, LibraryTab::Engines);
        let engines = tab_dir(&state, LibraryTab::Engines);
        assert!(engines.is_dir() && tab_dir(&state, LibraryTab::Cabins).is_dir());
        std::fs::write(engines.join("CFM56.json"), "{}").unwrap();
        let rows = build_rows(&mut state, LibraryTab::Engines);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "CFM56");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
