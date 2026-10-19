// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native open/save/folder dialogs for every path field of the desktop GUI.
//!
//! Each path field keeps its text box (so a path can still be pasted) and gains
//! a "Browse..." button that opens the platform dialog through the `rfd` crate
//! (Win32 common dialogs on Windows, the XDG desktop portal on Linux). The
//! dialog runs on a worker thread so the render loop never blocks; the result
//! is applied on the next frame by [`AppState::poll_file_dialog`].
//!
//! The dialogs start in the per-user data folder (see [`AppState::data_dir`])
//! unless the field already holds a usable location.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::thread;

use crate::state::{AppState, LogKind};
use crate::views::tr_fields;

/// The field a pending dialog fills, and what happens with its result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogTarget {
    /// File menu: choose a configuration and load it.
    ConfigLoad,
    /// File menu: choose a destination and save the configuration there.
    ConfigSave,
    /// Custom-airport window: choose an airport file and import it.
    AirportImport,
    /// Custom-airport window: choose a destination and export the registry.
    AirportExport,
    /// Custom-airfoil window: choose a Selig `.dat` file and import it.
    AirfoilImport,
    /// Run options: output directory.
    OutputDirectory,
    /// Airfoil CFD window: a persisted `results.json`.
    CfdResultJson,
    /// My aircraft manager: import a saved aircraft file.
    AircraftImport,
    /// My aircraft manager: export this stored aircraft file elsewhere.
    AircraftExport(PathBuf),
}

/// What kind of dialog to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    /// Pick an existing file.
    OpenFile,
    /// Pick a (possibly new) destination file.
    SaveFile,
    /// Pick an existing folder.
    PickFolder,
}

/// Everything the dialog worker needs; plain data so it can cross threads.
#[derive(Debug, Clone)]
pub struct DialogSpec {
    /// Open, save or folder.
    pub kind: DialogKind,
    /// Window title.
    pub title: String,
    /// Directory the dialog opens in, when it exists.
    pub start_dir: Option<PathBuf>,
    /// Suggested file name for a save dialog.
    pub file_name: Option<String>,
    /// `(label, extensions)` filters; extensions have no leading dot.
    pub filters: Vec<(String, Vec<String>)>,
}

/// The one dialog that may be open at a time.
#[derive(Debug, Default)]
pub struct FileDialogState {
    pending: Option<(DialogTarget, Receiver<Option<PathBuf>>)>,
}

/// Run the dialog on the calling thread and return the chosen path.
///
/// `None` means the user cancelled (or no dialog backend is available, which
/// the text fields cover).
pub fn run_blocking(spec: &DialogSpec) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new().set_title(spec.title.as_str());
    if let Some(dir) = spec.start_dir.as_deref().filter(|dir| dir.is_dir()) {
        dialog = dialog.set_directory(dir);
    }
    if let Some(name) = &spec.file_name {
        dialog = dialog.set_file_name(name.as_str());
    }
    for (label, extensions) in &spec.filters {
        dialog = dialog.add_filter(label.as_str(), extensions);
    }
    match spec.kind {
        DialogKind::OpenFile => dialog.pick_file(),
        DialogKind::SaveFile => dialog.save_file(),
        DialogKind::PickFolder => dialog.pick_folder(),
    }
}

/// The directory a dialog should open in for a field currently holding
/// `current`: the field's own folder when it exists, else `fallback`.
pub fn start_directory(current: &str, fallback: &Path) -> PathBuf {
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        let path = Path::new(trimmed);
        let candidate = if path.is_dir() {
            Some(path)
        } else {
            path.parent()
        };
        if let Some(dir) = candidate.filter(|dir| !dir.as_os_str().is_empty() && dir.is_dir()) {
            return dir.to_path_buf();
        }
    }
    fallback.to_path_buf()
}

impl AppState {
    /// The per-user data folder every save defaults to: the same folder that
    /// holds the tool preferences and the custom-airport store.
    pub fn data_dir(&self) -> PathBuf {
        self.tool_locator.user_data_root().to_path_buf()
    }

    /// Resolve a typed path for writing. A relative path lands in the data
    /// folder rather than in whatever the working directory happens to be.
    pub fn user_save_path(&self, typed: &str) -> PathBuf {
        let path = PathBuf::from(typed.trim());
        if path.is_absolute() {
            path
        } else {
            self.data_dir().join(path)
        }
    }

    /// Resolve a typed path for reading: a relative path that exists from the
    /// working directory keeps working, otherwise it is looked up in the data
    /// folder.
    pub fn user_load_path(&self, typed: &str) -> PathBuf {
        let path = PathBuf::from(typed.trim());
        if path.is_absolute() || path.exists() {
            path
        } else {
            self.data_dir().join(path)
        }
    }

    /// Start a non-blocking dialog for `target` unless one is already open.
    pub fn begin_file_dialog(&mut self, target: DialogTarget, spec: DialogSpec) {
        if self.files.dialog.pending.is_some() {
            return;
        }
        let (sender, receiver) = channel();
        thread::spawn(move || {
            let _ = sender.send(run_blocking(&spec));
        });
        self.files.dialog.pending = Some((target, receiver));
    }

    /// Whether the dialog for `target` is open.
    pub fn file_dialog_pending(&self, target: &DialogTarget) -> bool {
        self.files
            .dialog
            .pending
            .as_ref()
            .is_some_and(|(pending, _)| pending == target)
    }

    /// Whether any dialog is open (the caller keeps repainting meanwhile).
    pub fn any_file_dialog_pending(&self) -> bool {
        self.files.dialog.pending.is_some()
    }

    /// Apply a finished dialog once. Returns whether a dialog is still open.
    pub fn poll_file_dialog(&mut self) -> bool {
        let Some((_, receiver)) = self.files.dialog.pending.as_ref() else {
            return false;
        };
        let outcome = match receiver.try_recv() {
            Ok(path) => path,
            Err(TryRecvError::Empty) => return true,
            Err(TryRecvError::Disconnected) => None,
        };
        let Some((target, _)) = self.files.dialog.pending.take() else {
            return false;
        };
        if let Some(path) = outcome {
            self.apply_dialog_result(target, path);
        }
        false
    }

    /// Route a chosen path to the field (and action) the dialog belonged to.
    pub fn apply_dialog_result(&mut self, target: DialogTarget, path: PathBuf) {
        let text = path.display().to_string();
        match target {
            DialogTarget::ConfigLoad => {
                self.config_path = text;
                self.load_config();
            }
            DialogTarget::ConfigSave => {
                self.config_path = text;
                self.save_config();
            }
            DialogTarget::AirportImport => {
                self.custom_airport_file_path = text;
                self.import_custom_airports();
            }
            DialogTarget::AirportExport => {
                self.custom_airport_file_path = text;
                self.export_custom_airports();
            }
            DialogTarget::AirfoilImport => {
                self.custom_airfoil_file_path = text;
                self.import_custom_airfoil();
            }
            DialogTarget::OutputDirectory => {
                self.pipeline_options.output_dir = Some(path);
            }
            DialogTarget::CfdResultJson => {
                self.cfd.result_json_path = text;
            }
            DialogTarget::AircraftImport => self.import_user_aircraft(&path),
            DialogTarget::AircraftExport(source) => {
                match crate::user_files::export_aircraft_file(&source, &path) {
                    Ok(()) => self.log(
                        tr_fields("Exported aircraft to {path}.", &[("path", text)]),
                        LogKind::Info,
                    ),
                    Err(error) => self.log(
                        tr_fields("Aircraft export failed: {error}", &[("error", error)]),
                        LogKind::Error,
                    ),
                }
            }
        }
    }
}

/// JSON/YAML configuration filter shared by the File menu dialogs.
pub fn config_filters() -> Vec<(String, Vec<String>)> {
    vec![(
        "ALAS configuration".to_owned(),
        vec!["json".to_owned(), "yaml".to_owned(), "yml".to_owned()],
    )]
}

/// Convenience constructor for the common "open this kind of file" spec.
pub fn spec(
    kind: DialogKind,
    title: &str,
    current: &str,
    fallback: &Path,
    file_name: Option<&str>,
    filters: Vec<(String, Vec<String>)>,
) -> DialogSpec {
    DialogSpec {
        kind,
        title: title.to_owned(),
        start_dir: Some(start_directory(current, fallback)),
        file_name: file_name.map(str::to_owned),
        filters,
    }
}

/// A "Browse..." button for `target`; opens the dialog when clicked.
pub fn browse_button(
    state: &mut AppState,
    ui: &mut egui::Ui,
    target: DialogTarget,
    make_spec: impl FnOnce(&AppState) -> DialogSpec,
) -> egui::Response {
    let pending = state.file_dialog_pending(&target);
    let response = ui
        .add_enabled(
            !pending,
            egui::Button::new(crate::views::tr("Browse...")),
        )
        .on_hover_text(crate::views::tr(
            "Choose the location with the system file dialog. You can also paste a path in the box.",
        ));
    if response.clicked() {
        let spec = make_spec(state);
        state.begin_file_dialog(target, spec);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("alas-file-dialog-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn start_directory_prefers_the_field_folder_then_the_fallback() {
        let dir = temp_dir("start");
        let fallback = temp_dir("fallback");
        let file = dir.join("a.json");
        std::fs::write(&file, "{}").unwrap();
        assert_eq!(start_directory(&file.display().to_string(), &fallback), dir);
        assert_eq!(start_directory(&dir.display().to_string(), &fallback), dir);
        assert_eq!(start_directory("", &fallback), fallback);
        assert_eq!(start_directory("no-such-dir/x.json", &fallback), fallback);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&fallback);
    }

    #[test]
    fn relative_saves_go_to_the_data_folder_and_absolute_paths_are_kept() {
        let state = AppState::default();
        let data = state.data_dir();
        assert_eq!(state.user_save_path("case.json"), data.join("case.json"));
        let absolute = std::env::temp_dir().join("case.json");
        assert_eq!(
            state.user_save_path(&absolute.display().to_string()),
            absolute
        );
    }

    #[test]
    fn a_cancelled_or_dead_dialog_changes_nothing() {
        let mut state = AppState::default();
        let before = state.config_path.clone();
        let (sender, receiver) = channel();
        state.files.dialog.pending = Some((DialogTarget::ConfigLoad, receiver));
        assert!(state.poll_file_dialog(), "still waiting before a reply");
        sender.send(None).unwrap();
        assert!(!state.poll_file_dialog());
        assert!(!state.any_file_dialog_pending());
        assert_eq!(state.config_path, before);
    }

    #[test]
    fn the_output_directory_dialog_sets_the_run_output_directory() {
        let mut state = AppState::default();
        let dir = temp_dir("output");
        state.apply_dialog_result(DialogTarget::OutputDirectory, dir.clone());
        assert_eq!(state.pipeline_options.output_dir, Some(dir.clone()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
