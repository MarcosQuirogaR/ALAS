// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the Files explorer does with items: the context-menu entries, running
//! an action on the selection, rename, new folder, and the confirm dialogs for
//! delete and restore. Slow work (delete, copy) goes to the worker threads.

use std::path::{Path, PathBuf};

use egui::{Context, Ui};

use super::files_jobs::recycle_bin_available;
use super::files_model::{autosave_slot, Action, Entry};
use super::files_state::RenameState;
use super::{tr, tr_fields, AppState, LogKind};
use crate::file_dialog::{spec, DialogKind, DialogTarget};
use crate::shell_confirm::{confirm_dialog, ConfirmAction, ConfirmAnswer, Pending};

/// The menu text of an action (English; passed through the catalogue).
pub(super) const fn action_label(action: Action) -> &'static str {
    match action {
        Action::Open => "Open",
        Action::LoadIntoAlas => "Load into ALAS",
        Action::Restore => "Restore\u{2026}",
        Action::ShowInExplorer => "Show in Windows Explorer",
        Action::CopyPath => "Copy path",
        Action::Rename => "Rename",
        Action::Duplicate => "Duplicate",
        Action::ExportTo => "Export to\u{2026}",
        Action::Delete => "Delete",
    }
}

/// Widen a context menu to its longest entry plus the room the button and the
/// menu frame need, so no entry touches the menu's edge.
pub(super) fn fit_menu_width<'a>(ui: &mut Ui, labels: impl IntoIterator<Item = &'a str>) {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let color = ui.visuals().text_color();
    let longest = labels
        .into_iter()
        .map(|label| {
            ui.painter()
                .layout_no_wrap(label.to_owned(), font.clone(), color)
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
    let padding = 2.0 * ui.spacing().button_padding.x + 2.0 * ui.spacing().menu_margin.left + 12.0;
    ui.set_min_width(longest + padding);
}

/// One entry of a context menu; true when clicked.
pub(super) fn menu_item(ui: &mut Ui, action: Action) -> bool {
    let text = tr(action_label(action));
    let text = if action == Action::Delete {
        ui.separator();
        egui::RichText::new(text).color(egui::Color32::from_rgb(255, 154, 147))
    } else {
        egui::RichText::new(text)
    };
    ui.add(egui::Button::new(text).frame(false)).clicked()
}

/// Open a file with the system's default program.
pub(super) fn open_default(path: &Path) -> Result<(), String> {
    use alas_exec::process::NoConsoleWindow;
    #[cfg(target_os = "windows")]
    let mut command = std::process::Command::new("explorer.exe");
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    command
        .no_window()
        .arg(path)
        .spawn()
        .map(alas_exec::process::reap_in_background)
        .map_err(|error| error.to_string())
}

pub(super) fn reveal(state: &mut AppState, path: &Path, directory: bool) {
    if let Err(error) =
        crate::views::tools_view::open_in_file_explorer(&path.display().to_string(), directory)
    {
        state.log(error, LogKind::Warn);
    }
}

fn warn(state: &mut AppState, message: String) {
    state.files.status = Some(message.clone());
    state.log(message, LogKind::Warn);
}

/// Carry out `action` on `targets` (the clicked item, or the selection).
pub(super) fn run_action(state: &mut AppState, ctx: &Context, action: Action, targets: &[Entry]) {
    let Some(first) = targets.first() else {
        return;
    };
    let data = state.data_dir();
    match action {
        Action::Open if first.is_folder() => state.shell.library.go(first.path.clone()),
        Action::Open => {
            for target in targets {
                if let Err(error) = open_default(&target.path) {
                    warn(state, error);
                }
            }
        }
        Action::LoadIntoAlas => state.guarded(Pending::Path(first.path.clone())),
        Action::Restore => {
            state.shell.library.restore_confirm = autosave_slot(&data, &first.path);
        }
        Action::ShowInExplorer => reveal(state, &first.path, first.is_folder()),
        Action::CopyPath => {
            let text: Vec<String> = targets
                .iter()
                .map(|target| target.path.display().to_string())
                .collect();
            ctx.copy_text(text.join("\n"));
        }
        Action::Rename => {
            state.shell.library.rename = Some(RenameState {
                path: first.path.clone(),
                original: first.name.clone(),
                text: first.name.clone(),
                fresh: true,
            });
        }
        Action::Duplicate => state
            .shell
            .library
            .jobs
            .duplicate(ctx, data, first.path.clone()),
        Action::ExportTo => export_to(state, &first.path),
        Action::Delete => {
            state.shell.library.delete_confirm =
                Some(targets.iter().map(|target| target.path.clone()).collect());
        }
    }
}

fn export_to(state: &mut AppState, path: &Path) {
    let target = DialogTarget::PlainFileExport(path.to_path_buf());
    if state.file_dialog_pending(&target) {
        return;
    }
    let data = state.data_dir();
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    let dialog = spec(
        DialogKind::SaveFile,
        "Export",
        "",
        &data,
        name.as_deref(),
        Vec::new(),
    );
    state.begin_file_dialog(target, dialog);
}

/// Rename keeps a library file's header in step with its name; any other
/// item is renamed on disk within its folder.
pub(super) fn rename_item(state: &AppState, path: &Path, text: &str) -> Result<PathBuf, String> {
    let text = text.trim();
    let small_json = path.extension().is_some_and(|ext| ext == "json")
        && std::fs::metadata(path).is_ok_and(|meta| meta.len() < 8 * 1024 * 1024);
    if small_json {
        if let Some(kind) = crate::file_library::file_kind(path) {
            return crate::file_library::rename(&state.data_dir(), kind, path, text);
        }
    }
    if text.is_empty() || text.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
        return Err(tr("Enter a plain file name."));
    }
    let target = path.with_file_name(text);
    if target == path {
        return Ok(target);
    }
    if target.exists() {
        return Err(tr("A file with this name already exists."));
    }
    std::fs::rename(path, &target).map_err(|error| error.to_string())?;
    Ok(target)
}

/// Apply a finished rename typed in the list.
pub(super) fn finish_rename(state: &mut AppState, text: Option<String>) {
    let Some(rename) = state.shell.library.rename.take() else {
        return;
    };
    let Some(text) = text.filter(|text| text.trim() != rename.original.trim()) else {
        return;
    };
    match rename_item(state, &rename.path, &text) {
        Ok(target) => {
            state.refresh_user_aircraft();
            let lib = &mut state.shell.library;
            lib.reveal = Some((target, false));
            lib.invalidate();
        }
        Err(error) => warn(state, error),
    }
}

/// Create a folder in the current one and start naming it.
pub(super) fn new_folder(state: &mut AppState) {
    let lib = &state.shell.library;
    if lib.searching_subfolders() {
        return;
    }
    let dir = lib.current();
    let base = tr("New folder");
    let mut target = dir.join(&base);
    let mut index = 2;
    while target.exists() {
        target = dir.join(format!("{base} ({index})"));
        index += 1;
    }
    match std::fs::create_dir(&target) {
        Ok(()) => {
            let lib = &mut state.shell.library;
            lib.reveal = Some((target, true));
            lib.invalidate();
        }
        Err(error) => warn(state, error.to_string()),
    }
}

/// Fold a finished file operation into the page.
pub(super) fn finish_op(state: &mut AppState, done: super::files_jobs::OpDone) {
    if done.ok {
        state.refresh_user_aircraft();
    } else if !done.message.is_empty() {
        warn(state, done.message);
    }
    let lib = &mut state.shell.library;
    if let Some(path) = done.select {
        lib.reveal = Some((path, false));
    }
    lib.invalidate();
}

/// The delete and restore prompts.
pub(super) fn show_dialogs(state: &mut AppState, ctx: &Context) {
    confirm_delete(state, ctx);
    confirm_restore(state, ctx);
}

fn confirm_delete(state: &mut AppState, ctx: &Context) {
    let Some(paths) = state.shell.library.delete_confirm.clone() else {
        return;
    };
    let data = state.data_dir();
    let slots: Vec<String> = paths
        .iter()
        .filter_map(|path| autosave_slot(&data, path))
        .collect();
    let name = paths
        .first()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let title = if paths.len() == 1 {
        tr_fields("Delete {name}?", &[("name", name)])
    } else {
        tr_fields(
            "Delete {count} items?",
            &[("count", paths.len().to_string())],
        )
    };
    let has_folder = paths.iter().any(|path| path.is_dir());
    let mut body = if !slots.is_empty() {
        tr("The autosaved copy is deleted and cannot be restored afterwards.")
    } else if recycle_bin_available() {
        tr("It will be moved to the Recycle Bin.")
    } else {
        tr("It will be deleted permanently and cannot be recovered.")
    };
    if has_folder && slots.is_empty() {
        body.push(' ');
        body.push_str(&tr("Everything inside a folder goes with it."));
    }
    match confirm_dialog(
        ctx,
        "alas_files_delete",
        &title,
        Some(&body),
        &[ConfirmAction::destructive(tr("Delete"))],
    ) {
        Some(ConfirmAnswer::Action(_)) => {
            state.shell.library.delete_confirm = None;
            state.shell.library.sel.clear();
            let (autosaves, others): (Vec<PathBuf>, Vec<PathBuf>) = paths
                .into_iter()
                .partition(|path| autosave_slot(&data, path).is_some());
            for path in autosaves {
                if let Some(slot) = autosave_slot(&data, &path) {
                    state.library_dismiss_autosave(&slot);
                }
            }
            if !others.is_empty() {
                state.shell.library.jobs.delete(ctx, others);
            }
            state.shell.library.invalidate();
        }
        Some(ConfirmAnswer::Cancel) => state.shell.library.delete_confirm = None,
        None => {}
    }
}

/// The restore prompt: what is replaced and, when the current case can be
/// compared, how many fields differ and the first few paths.
fn confirm_restore(state: &mut AppState, ctx: &Context) {
    use crate::file_library::ObjectKind;
    let Some(slot) = state.shell.library.restore_confirm.clone() else {
        return;
    };
    let title = state
        .shell
        .library
        .autosaves
        .iter()
        .find(|(each, _)| *each == slot)
        .map(|(_, title)| title.clone())
        .unwrap_or_else(|| slot.clone());
    let mut body = tr_fields(
        "Replace the current {kind} with its autosaved copy?",
        &[("kind", title.clone())],
    );
    if let Some(summary) = state.autosave_diff_summary(&slot) {
        body.push_str("\n\n");
        body.push_str(&summary);
    }
    let heading = tr_fields("Restore {kind}", &[("kind", title)]);
    match confirm_dialog(
        ctx,
        "alas_autosave_restore",
        &heading,
        Some(&body),
        &[ConfirmAction::primary(tr("Restore"))],
    ) {
        Some(ConfirmAnswer::Action(_)) => {
            let action = match ObjectKind::ALL.into_iter().find(|k| k.id() == slot) {
                Some(kind) => Pending::RestoreAutosave(kind),
                None => Pending::RestoreSandbox,
            };
            state.shell.library.restore_confirm = None;
            state.perform(action);
            state.shell.library.invalidate();
        }
        Some(ConfirmAnswer::Cancel) => state.shell.library.restore_confirm = None,
        None => {}
    }
}
