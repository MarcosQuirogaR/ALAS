// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Setup > External Tools: the integration paths and credentials for the
//! optional external solvers, consolidated in one place rather than spread
//! across their own Advanced Settings pages.
//!
//! A port of the reference desktop app's `SetupScreen`. Navigation data is
//! downloaded through the shared windowless curl boundary in `alas-exec`, so
//! the setup page and headless CLI use the same HTTPS, retry, atomic-install,
//! and size-validation policy.

use alas_exec::ExecutableDiscovery;
use egui::{RichText, ScrollArea, TextEdit, Ui};
use serde_json::Value;

use crate::path_picker::{PathSelection, ToolPathTarget};
use crate::state::AppState;
use crate::views::{tr, tr_fields};

mod cards;
mod cfd;
mod status;

pub(crate) use cfd::save_cfd_environment_preferences;

/// Render the External Tools page.
pub fn show_tools_view(state: &mut AppState, ui: &mut Ui) {
    apply_completed_path_selection(state);
    ui.heading(tr("External Tools"));
    ui.label(
        RichText::new(tr(
            "Integration paths and credentials for the optional external tools ALAS can drive \
             during a run. Set once; saved in user preferences.",
        ))
        .weak(),
    );
    ui.add_space(6.0);

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            section_heading(ui, "Mission routing and data");
            cards::routing_card(state, ui);
            ui.add_space(8.0);

            section_heading(ui, "External solvers");
            if ui.available_width() >= 920.0 {
                ui.columns(2, |columns| {
                    let (left, right) = columns.split_at_mut(1);
                    cards::nastran_card(state, &mut left[0]);
                    cards::patran_card(state, &mut right[0]);
                });
                ui.add_space(8.0);
                ui.columns(2, |columns| {
                    let (left, right) = columns.split_at_mut(1);
                    cards::mses_card(state, &mut left[0]);
                    cards::openvsp_card(state, &mut right[0]);
                });
                ui.add_space(8.0);
                cards::openfoam_card(state, ui);
            } else {
                cards::nastran_card(state, ui);
                ui.add_space(8.0);
                cards::patran_card(state, ui);
                ui.add_space(8.0);
                cards::mses_card(state, ui);
                ui.add_space(8.0);
                cards::openvsp_card(state, ui);
                ui.add_space(8.0);
                cards::openfoam_card(state, ui);
            }
            ui.add_space(8.0);
            cards::avl_card(state, ui);
            ui.add_space(8.0);
            cards::flowunsteady_card(state, ui);
            ui.add_space(8.0);

            section_heading(ui, "Availability for the next run");
            status_card(state, ui);
        });
}

fn card(ui: &mut Ui, title: &str, link: &str, body: impl FnOnce(&mut Ui)) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(tr(title))
                    .strong()
                    .size(16.0)
                    .color(ui.visuals().hyperlink_color),
            );
            ui.hyperlink_to(tr("Documentation"), link);
        });
        ui.add_space(4.0);
        body(ui);
    });
}

fn section_heading(ui: &mut Ui, title: &str) {
    ui.label(
        RichText::new(tr(title))
            .strong()
            .size(16.0)
            .color(ui.visuals().hyperlink_color),
    );
    ui.add_space(4.0);
}

fn text_row(
    state: &mut AppState,
    ui: &mut Ui,
    label: &str,
    value: &mut String,
    directory: bool,
    picker_target: Option<ToolPathTarget>,
) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.add_sized([170.0, 18.0], egui::Label::new(tr(label)));
        changed = ui
            .add(TextEdit::singleline(value).desired_width(360.0))
            .changed();
        if let Some(target) = picker_target {
            let pending = state.path_picker_pending(target);
            if ui
                .add_enabled(!pending, egui::Button::new(tr("Browse...")).small())
                .on_hover_text(tr(
                    "Choose the directory or executable with the native file picker. The selected location is saved in this user's preferences.",
                ))
                .clicked()
            {
                state.begin_path_picker(target, directory);
            }
            if !value.trim().is_empty()
                && ui
                    .small_button(tr("Open folder"))
                    .on_hover_text(tr("Show this exact location in the system file explorer."))
                    .clicked()
            {
                if let Err(error) = open_in_file_explorer(value, directory) {
                    state.log(error, crate::state::LogKind::Warn);
                }
            }
        }
    });
    changed
}

fn apply_completed_path_selection(state: &mut AppState) {
    let Some(PathSelection { target, result }) = state.take_path_selection() else {
        return;
    };
    let path = match result {
        Ok(Some(path)) => path,
        Ok(None) => return,
        Err(error) => {
            state.log(error, crate::state::LogKind::Error);
            return;
        }
    };

    if let Some(label) = cfd::apply_path_selection(state, target, &path) {
        state.on_config_modified();
        state.note_parameter_modified(tr(label), path);
        return;
    }

    let label = match target {
        ToolPathTarget::NastranExecutable => {
            set_str(
                &mut state.config_values,
                "structures",
                "nastran_exe_path",
                path.clone(),
            );
            state.save_tool_preferences();
            "Executable path"
        }
        ToolPathTarget::NastranSolver => {
            set_str(
                &mut state.config_values,
                "structures",
                "nastran_solver_path",
                path.clone(),
            );
            state.save_tool_preferences();
            "MSC solver override"
        }
        ToolPathTarget::Nastran95Directory => {
            set_str(
                &mut state.config_values,
                "structures",
                "nastran95_dir_path",
                path.clone(),
            );
            state.save_tool_preferences();
            "Local NASTRAN-95 directory"
        }
        ToolPathTarget::Nastran95RuntimeDirectory => {
            set_str(
                &mut state.config_values,
                "structures",
                "nastran95_runtime_path",
                path.clone(),
            );
            state.save_tool_preferences();
            "NASTRAN-95 runtime directory"
        }
        ToolPathTarget::Nastran95RfStageDirectory => {
            set_str(
                &mut state.config_values,
                "structures",
                "nastran95_rf_stage_path",
                path.clone(),
            );
            state.save_tool_preferences();
            "NASTRAN-95 RF staging directory"
        }
        ToolPathTarget::PatranExecutable => {
            set_str(
                &mut state.config_values,
                "structures",
                "patran_exe_path",
                path.clone(),
            );
            state.save_tool_preferences();
            "Executable path"
        }
        ToolPathTarget::MsesDirectory => {
            set_str(&mut state.config_values, "mses", "mses_dir", path.clone());
            state.save_tool_preferences();
            "Install directory"
        }
        ToolPathTarget::NavdataDirectory => {
            set_str(
                &mut state.config_values,
                "mission",
                "navdata_dir",
                path.clone(),
            );
            state.save_tool_preferences();
            "Navigation-data directory"
        }
        ToolPathTarget::RoutesDirectory => {
            set_str(
                &mut state.config_values,
                "mission",
                "routes_dir",
                path.clone(),
            );
            state.save_tool_preferences();
            "Saved-routes directory"
        }
        ToolPathTarget::AvlExecutable => {
            state.tool_preferences.avl_exe = Some(path.clone());
            save_direct_tool_preferences(state);
            "Executable path"
        }
        ToolPathTarget::OpenVspDirectory => {
            state.tool_preferences.openvsp_dir = Some(path.clone());
            save_direct_tool_preferences(state);
            "Install directory"
        }
        ToolPathTarget::FlowUnsteadyExecutable => {
            state.tool_preferences.flowunsteady_exe = Some(path.clone());
            save_direct_tool_preferences(state);
            "Executable path"
        }
        _ => return,
    };
    state.on_config_modified();
    state.note_parameter_modified(tr(label), path);
}

fn save_direct_tool_preferences(state: &mut AppState) {
    if let Err(error) = state.tool_locator.save_preferences(&state.tool_preferences) {
        state.log(
            tr_fields(
                "Tool preferences not saved: {error}",
                &[("error", error.to_string())],
            ),
            crate::state::LogKind::Warn,
        );
    }
}

/// Open the user-entered location without writing to it. The path remains an
/// explicit editable preference: discovery never guesses an installation as a
/// successful solver run.
pub(crate) fn open_in_file_explorer(value: &str, directory: bool) -> Result<(), String> {
    let (target, select_file) = explorer_target(value, directory)?;
    #[cfg(target_os = "windows")]
    {
        let mut command = std::process::Command::new("explorer.exe");
        if select_file {
            command.arg(format!("/select,{}", target.display()));
        } else {
            command.arg(&target);
        }
        command
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    #[cfg(target_os = "macos")]
    {
        let _ = select_file;
        std::process::Command::new("open")
            .arg(&target)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = select_file;
        std::process::Command::new("xdg-open")
            .arg(&target)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

fn explorer_target(value: &str, directory: bool) -> Result<(std::path::PathBuf, bool), String> {
    let path = std::path::Path::new(value);
    let (target, select_file) = if directory {
        if path.is_dir() {
            (path.to_path_buf(), false)
        } else {
            return Err(tr_fields(
                "Cannot open folder: {path} is not an existing directory.",
                &[("path", path.display().to_string())],
            ));
        }
    } else if path.is_file() {
        (path.to_path_buf(), true)
    } else {
        let Some(parent) = path.parent().filter(|parent| parent.is_dir()) else {
            return Err(tr_fields(
                "Cannot open folder: {path} and its parent directory do not exist.",
                &[("path", path.display().to_string())],
            ));
        };
        (parent.to_path_buf(), false)
    };
    Ok((target, select_file))
}

fn str_field(config: &Value, group: &str, name: &str) -> String {
    config
        .get(group)
        .and_then(|g| g.get(name))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
}

fn set_str(config: &mut Value, group: &str, name: &str, value: String) {
    if let Some(obj) = config.get_mut(group).and_then(Value::as_object_mut) {
        obj.insert(name.to_owned(), Value::String(value));
    }
}

fn set_bool(config: &mut Value, group: &str, name: &str, value: bool) {
    if let Some(obj) = config.get_mut(group).and_then(Value::as_object_mut) {
        obj.insert(name.to_owned(), Value::Bool(value));
    }
}

fn status_card(state: &AppState, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        status::resolved_status(state, ui);
    });
}

fn status_row(ui: &mut Ui, label: &str, value: String) {
    ui.label(RichText::new(tr(label)).strong());
    ui.label(RichText::new(value).weak());
    ui.end_row();
}

fn describe_executable(discovery: &ExecutableDiscovery) -> String {
    match discovery {
        ExecutableDiscovery::Absent => tr("absent"),
        ExecutableDiscovery::Ready(path) => path.display().to_string(),
        ExecutableDiscovery::Incomplete { directory, missing } => tr_fields(
            "incomplete at {path} (missing {missing})",
            &[
                ("path", directory.display().to_string()),
                ("missing", missing.join(", ")),
            ],
        ),
    }
}

fn describe_flowunsteady(discovery: &ExecutableDiscovery) -> String {
    match discovery {
        ExecutableDiscovery::Absent => tr("not configured (ALAS_FLOWUNSTEADY_EXE not set)"),
        ExecutableDiscovery::Ready(path) => path.display().to_string(),
        ExecutableDiscovery::Incomplete { directory, .. } => tr_fields(
            "configured path not found: {path}",
            &[("path", directory.display().to_string())],
        ),
    }
}

fn describe_optional_file(configured: &str) -> String {
    if configured.trim().is_empty() {
        return tr("not configured (automatic launcher resolution)");
    }
    let path = std::path::Path::new(configured);
    if path.is_file() {
        path.display().to_string()
    } else {
        tr_fields(
            "invalid file: {path}",
            &[("path", path.display().to_string())],
        )
    }
}

fn describe_nastran_solver(configured: &str, resolved: Option<&std::path::Path>) -> String {
    if !configured.trim().is_empty() {
        return describe_optional_file(configured);
    }
    resolved.map_or_else(
        || tr("not found (automatic server-mode solver resolution)"),
        |path| format!("{} (automatic server-mode solver)", path.display()),
    )
}

fn describe_optional_directory(configured: &str) -> String {
    if configured.trim().is_empty() {
        return tr("not configured");
    }
    let path = std::path::Path::new(configured);
    if path.is_dir() {
        path.display().to_string()
    } else {
        tr_fields(
            "invalid directory: {path}",
            &[("path", path.display().to_string())],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::explorer_target;

    #[test]
    fn existing_executables_are_selected_and_draft_paths_open_their_real_parent() {
        let root = std::env::temp_dir().join(format!("alas-explorer-{}", std::process::id()));
        let executable = root.join("solver.exe");
        let _ = std::fs::create_dir_all(&root);
        let _ = std::fs::write(&executable, b"solver");

        let (selected, select_file) = explorer_target(&executable.display().to_string(), false)
            .expect("existing executable resolves");
        assert_eq!(selected, executable);
        assert!(select_file);

        let draft = root.join("not-yet-installed.exe");
        let (parent, select_file) =
            explorer_target(&draft.display().to_string(), false).expect("existing parent resolves");
        assert_eq!(parent, root);
        assert!(!select_file);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_directory_never_falls_back_to_an_unrelated_file_explorer_location() {
        let result = explorer_target("no/such/ALAS/location", true);
        assert!(result.is_err());
    }
}
