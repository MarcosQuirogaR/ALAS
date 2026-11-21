// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Import and export of a whole cabin on the Cabin & Cargo page.
//!
//! The file is the payload-layout artifact the pipeline already writes
//! (`alas_pipeline::payload_layout_export`): the cabin is its
//! `effective_config.cabin`, so an exported cabin, or the artifact of any run,
//! is also an importable one. Cabins live in
//! `<user data root>/library/my-cabins/`. An import is checked field by field
//! before anything is applied; every failing field is listed with its path.

use std::path::{Path, PathBuf};

use alas_config::{
    validate, CabinConfig, CargoDeckConfig, PassengerCabinConfig, PayloadItemsConfig, Severity,
};
use alas_pipeline::payload_layout_export::{
    export_payload_layout_artifact, PAYLOAD_LAYOUT_SCHEMA_VERSION,
};
use egui::{Id, RichText, Ui};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use crate::state::{AppState, LogKind};
use crate::views::{tr_fields, tr_label};

/// A field path and what is wrong with it.
type FieldIssue = (String, String);

#[derive(Debug, Clone, Default)]
struct IoUi {
    export_open: bool,
    export_name: String,
    import_open: bool,
    import_path: String,
    errors: Vec<FieldIssue>,
    warnings: Vec<FieldIssue>,
    message: Option<(String, bool)>,
    /// The system file dialog while it is open.
    browse: Option<BrowseSlot>,
}

/// Where the dialog thread leaves its answer: `Some(None)` when the user
/// cancelled, `Some(Some(path))` when a file was chosen.
type BrowseSlot = std::sync::Arc<std::sync::Mutex<Option<Option<PathBuf>>>>;

/// Open the system file dialog on its own thread (the same helper every other
/// Browse button uses) so the window keeps drawing while it is open.
fn start_browse(state: &AppState, ctx: &egui::Context, typed: &str) -> BrowseSlot {
    let slot = BrowseSlot::default();
    let spec = crate::file_dialog::spec(
        crate::file_dialog::DialogKind::OpenFile,
        "Import cabin",
        typed,
        &cabins_dir(state),
        None,
        vec![("Cabin JSON".to_owned(), vec!["json".to_owned()])],
    );
    let (reply, ctx) = (slot.clone(), ctx.clone());
    std::thread::spawn(move || {
        let chosen = crate::file_dialog::run_blocking(&spec);
        if let Ok(mut guard) = reply.lock() {
            *guard = Some(chosen);
        }
        ctx.request_repaint();
    });
    slot
}

fn ui_id() -> Id {
    Id::new("cabin_io_state")
}

/// The folder cabins are kept in.
pub(super) fn cabins_dir(state: &AppState) -> PathBuf {
    state.data_dir().join("library").join("my-cabins")
}

/// A file stem made of letters, digits, `-`, `_` and spaces.
fn file_stem(name: &str) -> String {
    let stem: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if stem.trim().is_empty() {
        "cabin".to_owned()
    } else {
        stem
    }
}

/// The `Import cabin...` and `Export cabin...` buttons and their windows.
pub(super) fn show(state: &mut AppState, ui: &mut Ui) {
    let mut io: IoUi = ui
        .ctx()
        .data_mut(|data| data.get_temp(ui_id()))
        .unwrap_or_default();
    ui.horizontal(|ui| {
        if ui
            .button(tr_label("Import cabin\u{2026}"))
            .on_hover_text(tr_label("Load a cabin from a JSON file"))
            .clicked()
        {
            io.import_open = true;
            io.errors.clear();
            io.warnings.clear();
            io.message = None;
        }
        if ui
            .button(tr_label("Export cabin\u{2026}"))
            .on_hover_text(tr_label("Save this cabin as a JSON file in my-cabins"))
            .clicked()
        {
            io.export_open = true;
            io.message = None;
            if io.export_name.is_empty() {
                io.export_name = file_stem(&format!(
                    "{}-cabin",
                    if state.active_preset.is_empty() {
                        "my"
                    } else {
                        &state.active_preset
                    }
                ));
            }
        }
    });
    ui.add_space(4.0);
    export_window(state, ui.ctx(), &mut io);
    import_window(state, ui.ctx(), &mut io);
    ui.ctx().data_mut(|data| data.insert_temp(ui_id(), io));
}

fn export_window(state: &mut AppState, ctx: &egui::Context, io: &mut IoUi) {
    if !io.export_open {
        return;
    }
    let mut open = true;
    egui::Window::new(tr_label("Export cabin"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(RichText::new(cabins_dir(state).display().to_string()).weak());
            ui.horizontal(|ui| {
                ui.label(tr_label("File name"));
                ui.text_edit_singleline(&mut io.export_name);
                ui.label(".json");
            });
            if ui.button(tr_label("Save")).clicked() {
                let path = cabins_dir(state).join(format!("{}.json", file_stem(&io.export_name)));
                io.message = Some(match export(state, &path) {
                    Ok(()) => (
                        tr_fields(
                            "Cabin saved to {path}",
                            &[("path", path.display().to_string())],
                        ),
                        true,
                    ),
                    Err(reason) => (reason, false),
                });
            }
            message(ui, io);
        });
    io.export_open = open;
}

fn message(ui: &mut Ui, io: &IoUi) {
    if let Some((text, ok)) = &io.message {
        let color = if *ok {
            crate::theme::success_color(ui.visuals())
        } else {
            ui.visuals().error_fg_color
        };
        ui.colored_label(color, text);
    }
}

fn import_window(state: &mut AppState, ctx: &egui::Context, io: &mut IoUi) {
    if !io.import_open {
        return;
    }
    let mut open = true;
    let mut chosen: Option<PathBuf> = None;
    let answer = io
        .browse
        .as_ref()
        .and_then(|slot| slot.lock().ok().and_then(|mut guard| guard.take()));
    if let Some(answer) = answer {
        io.browse = None;
        if let Some(path) = answer {
            io.import_path = path.display().to_string();
            chosen = Some(path);
        }
    }
    egui::Window::new(tr_label("Import cabin"))
        .open(&mut open)
        .collapsible(false)
        .default_width(460.0)
        .show(ctx, |ui| {
            let dir = cabins_dir(state);
            ui.label(RichText::new(dir.display().to_string()).weak());
            let files = list_cabins(&dir);
            if files.is_empty() {
                ui.label(RichText::new(tr_label("No Saved Cabins")).weak());
            }
            for file in &files {
                let name = file
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if ui.button(name).clicked() {
                    chosen = Some(file.clone());
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(tr_label("Path"));
                ui.text_edit_singleline(&mut io.import_path);
                let waiting = io.browse.is_some();
                if ui
                    .add_enabled(!waiting, egui::Button::new(tr_label("Browse...")))
                    .on_hover_text(tr_label(
                        "Choose the location with the system file dialog. You can also paste a path in the box.",
                    ))
                    .clicked()
                {
                    io.browse = Some(start_browse(state, ui.ctx(), &io.import_path));
                }
                if ui
                    .add_enabled(
                        !io.import_path.trim().is_empty(),
                        egui::Button::new(tr_label("Import")),
                    )
                    .clicked()
                {
                    chosen = Some(state.user_load_path(&io.import_path));
                }
            });
            issues(ui, io);
            message(ui, io);
        });
    io.import_open = open;
    if let Some(path) = chosen {
        io.errors.clear();
        io.warnings.clear();
        io.message = None;
        match read_cabin(state, &path) {
            Ok((cabin, warnings)) => {
                io.warnings = warnings;
                apply(state, &cabin);
                state.log(
                    tr_fields(
                        "Cabin imported from {path}",
                        &[("path", path.display().to_string())],
                    ),
                    LogKind::Info,
                );
                io.message = Some((tr_label("Cabin Imported"), true));
            }
            Err(errors) => io.errors = errors,
        }
    }
}

fn issues(ui: &mut Ui, io: &IoUi) {
    if !io.errors.is_empty() {
        ui.colored_label(
            ui.visuals().error_fg_color,
            tr_label("The file was not imported; fix these fields"),
        );
        issue_rows(ui, &io.errors);
    }
    if !io.warnings.is_empty() {
        ui.label(RichText::new(tr_label("Warnings")).strong());
        issue_rows(ui, &io.warnings);
    }
}

fn issue_rows(ui: &mut Ui, list: &[FieldIssue]) {
    egui::ScrollArea::vertical()
        .max_height(180.0)
        .id_salt(("cabin_io_issues", list.len()))
        .show(ui, |ui| {
            for (field, reason) in list {
                ui.horizontal_wrapped(|ui| {
                    ui.monospace(field);
                    ui.label(reason);
                });
            }
        });
}

fn list_cabins(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    files
}

/// The aircraft and cabin the live preview builds, as the layout figure sees
/// them (the named cabin preset materialised).
fn export(state: &AppState, path: &Path) -> Result<(), String> {
    let mut config = state
        .typed_config()
        .ok_or_else(|| tr_label("The configuration is not valid yet"))?;
    let design = state.current_design().unwrap_or_default();
    if config.requirements.cabin_preset != "Custom" {
        alas_payload::apply_cabin_preset(&mut config, Some(&design))
            .map_err(|error| error.to_string())?;
    }
    let airplane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), true)
        .map_err(|error| error.to_string())?;
    let layout = alas_payload::build::build_payload_layout(&airplane, &config, 0.0, 0.0)
        .map_err(|error| error.to_string())?;
    export_payload_layout_artifact(&config, design, &layout, path)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Every field of `T` that `imported` cannot fill, by path. Each key is tried
/// alone on top of the defaults, so one bad field does not hide the others.
fn check<T>(path: &str, imported: &Value) -> Vec<FieldIssue>
where
    T: Default + Serialize + DeserializeOwned,
{
    let Err(whole) = serde_json::from_value::<T>(imported.clone()) else {
        return Vec::new();
    };
    let base = serde_json::to_value(T::default()).unwrap_or_default();
    let (Some(base), Some(fields)) = (base.as_object(), imported.as_object()) else {
        return vec![(path.to_owned(), whole.to_string())];
    };
    let mut found: Vec<FieldIssue> = fields
        .iter()
        .filter_map(|(key, child)| {
            let mut probe = base.clone();
            probe.insert(key.clone(), child.clone());
            serde_json::from_value::<T>(Value::Object(probe))
                .err()
                .map(|error| (format!("{path}.{key}"), error.to_string()))
        })
        .collect();
    if found.is_empty() {
        found.push((path.to_owned(), whole.to_string()));
    }
    found
}

/// Read a cabin file: the cabin to apply and the warnings, or the failing
/// fields.
fn read_cabin(
    state: &AppState,
    path: &Path,
) -> Result<(CabinConfig, Vec<FieldIssue>), Vec<FieldIssue>> {
    let file = |reason: String| vec![("file".to_owned(), reason)];
    let text = std::fs::read_to_string(path).map_err(|error| file(error.to_string()))?;
    let root: Value = serde_json::from_str(&text).map_err(|error| file(error.to_string()))?;
    let mut errors: Vec<FieldIssue> = Vec::new();
    match root.get("schema_version").and_then(Value::as_str) {
        Some(PAYLOAD_LAYOUT_SCHEMA_VERSION) => {}
        Some(other) => errors.push((
            "schema_version".to_owned(),
            format!("expected {PAYLOAD_LAYOUT_SCHEMA_VERSION}, found {other}"),
        )),
        None => errors.push((
            "schema_version".to_owned(),
            format!("missing; expected {PAYLOAD_LAYOUT_SCHEMA_VERSION}"),
        )),
    }
    let Some(cabin) = root
        .pointer("/effective_config/cabin")
        .filter(|v| v.is_object())
    else {
        errors.push((
            "effective_config.cabin".to_owned(),
            "missing; the cabin is read from here".to_owned(),
        ));
        return Err(errors);
    };
    let base = "effective_config.cabin";
    errors.extend(check::<CabinConfig>(base, cabin));
    // Name the field inside a section instead of the section as a whole.
    for (key, refine) in [
        (
            "passenger",
            check::<PassengerCabinConfig> as fn(&str, &Value) -> Vec<FieldIssue>,
        ),
        ("cargo", check::<CargoDeckConfig>),
        ("payload_items", check::<PayloadItemsConfig>),
    ] {
        let section = format!("{base}.{key}");
        let Some(at) = errors.iter().position(|(field, _)| *field == section) else {
            continue;
        };
        let finer = cabin.get(key).map(|child| refine(&section, child));
        if let Some(finer) = finer.filter(|finer| !finer.is_empty()) {
            errors.splice(at..=at, finer);
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let parsed: CabinConfig =
        serde_json::from_value(cabin.clone()).map_err(|error| file(error.to_string()))?;
    if let Err(reason) = parsed.payload_items.validate() {
        return Err(vec![(format!("{base}.payload_items"), reason)]);
    }
    // The rest of the configuration stays the user's: check the imported
    // cabin against it with the same rules a run is gated on.
    let mut candidate = state
        .typed_config()
        .ok_or_else(|| file(tr_label("The configuration is not valid yet")))?;
    candidate.cabin = parsed.clone();
    candidate.requirements.cabin_preset = "Custom".to_owned();
    let mut warnings = Vec::new();
    for issue in validate(&candidate)
        .into_iter()
        .filter(|issue| issue.field_path.starts_with("cabin"))
    {
        let entry = (issue.field_path, issue.message);
        match issue.severity {
            Severity::Error => errors.push(entry),
            Severity::Warning => warnings.push(entry),
        }
    }
    if errors.is_empty() {
        Ok((parsed, warnings))
    } else {
        Err(errors)
    }
}

/// Replace the case's cabin. The cabin preset becomes Custom, since a named
/// preset would rebuild the cabin over the imported one.
fn apply(state: &mut AppState, cabin: &CabinConfig) {
    let Ok(value) = serde_json::to_value(cabin) else {
        return;
    };
    if let Some(root) = state.config_values.as_object_mut() {
        root.insert("cabin".to_owned(), value);
    }
    if let Some(preset) = state
        .config_values
        .pointer_mut("/requirements/cabin_preset")
    {
        *preset = Value::from("Custom");
    }
    state.on_config_modified();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bad_field_is_named_with_its_path_and_the_others_are_still_checked() {
        let mut cabin = serde_json::to_value(CabinConfig::default()).unwrap();
        cabin["cargo"]["main_deck_uld"] = Value::from(12);
        cabin["cargo"]["use_main_deck"] = Value::from("yes");
        let issues = check::<CargoDeckConfig>("cabin.cargo", &cabin["cargo"]);
        let fields: Vec<&str> = issues.iter().map(|(field, _)| field.as_str()).collect();
        assert!(fields.contains(&"cabin.cargo.main_deck_uld"), "{fields:?}");
        assert!(fields.contains(&"cabin.cargo.use_main_deck"), "{fields:?}");
    }

    #[test]
    fn a_file_stem_keeps_letters_and_replaces_path_characters() {
        assert_eq!(file_stem("A320 cabin/v2"), "A320 cabin_v2");
        assert_eq!(file_stem("  "), "cabin");
    }
}
