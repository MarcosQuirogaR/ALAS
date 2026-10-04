// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Setup > External Tools: the integration paths and credentials for the
//! optional external solvers, consolidated in one place rather than spread
//! across their own Advanced Settings pages.
//!
//! Navigation data is
//! downloaded through the shared windowless curl boundary in `alas-exec`, so
//! the setup page and headless CLI use the same HTTPS, retry, atomic-install,
//! and size-validation policy.

use alas_exec::ExecutableDiscovery;
use egui::{RichText, ScrollArea, Ui};
use serde_json::Value;

use crate::path_picker::{PathSelection, ToolPathTarget};
use crate::state::AppState;
use crate::views::external_tool_catalog::ExternalToolConfig;
use crate::views::{tr, tr_fields};

mod cards;
mod cfd;
mod layout;
mod status;

use layout::{card, card_row, section_heading, split_body, sub_heading, text_row};

pub(crate) use cfd::save_cfd_environment_preferences;

/// Render the External Tools page.
pub fn show_tools_view(state: &mut AppState, ui: &mut Ui) {
    apply_completed_path_selection(state);
    ui.heading(tr("External Tools")).on_hover_text(tr(
        "Integration paths and credentials for the optional external tools ALAS can drive \
         during a run. Set once; saved in user preferences.",
    ));
    ui.add_space(6.0);

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // Cards are grouped by discipline. Paired cards share one height
            // and wide cards split their own body, so a wide window is used
            // across its width instead of leaving ragged gaps.
            section_heading(ui, "Mission routing and data");
            cards::routing_card(state, ui);

            section_heading(ui, "Structural solvers");
            cards::nastran_card(state, ui);
            ui.add_space(8.0);
            cards::patran_card(state, ui);

            section_heading(ui, "Aerodynamic solvers");
            card_row(
                ui,
                "mses_openvsp",
                state,
                cards::mses_card,
                cards::openvsp_card,
            );
            ui.add_space(8.0);
            card_row(
                ui,
                "avl_flowunsteady",
                state,
                cards::avl_card,
                cards::flowunsteady_card,
            );

            section_heading(ui, "CFD");
            cards::openfoam_card(state, ui);

            section_heading(ui, "Availability for the next run");
            status_card(state, ui);
        });
}

/// Render the existing configuration card for one tool inside the detached
/// manager. Tools sharing an installation environment use the same card.
pub(crate) fn show_tool_configuration(state: &mut AppState, ui: &mut Ui, tool: ExternalToolConfig) {
    match tool {
        ExternalToolConfig::All => {
            show_tools_view(state, ui);
            return;
        }
        ExternalToolConfig::Mses => cards::mses_card(state, ui),
        ExternalToolConfig::MscNastran | ExternalToolConfig::Nastran95 => {
            cards::nastran_card(state, ui)
        }
        ExternalToolConfig::MscPatran => cards::patran_card(state, ui),
        ExternalToolConfig::OpenVsp => cards::openvsp_card(state, ui),
        ExternalToolConfig::OpenFoam | ExternalToolConfig::Gmsh | ExternalToolConfig::ParaView => {
            cards::openfoam_card(state, ui)
        }
        ExternalToolConfig::FlowUnsteady => cards::flowunsteady_card(state, ui),
    }
    ui.add_space(10.0);
    section_heading(ui, "Availability for the next run");
    status_card(state, ui);
}

pub(crate) fn apply_completed_path_selection(state: &mut AppState) {
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

    fn painted_text(shape: &egui::Shape, texts: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| painted_text(s, texts)),
            egui::Shape::Text(text) => texts.push((
                text.galley.text().to_owned(),
                egui::Rect::from_min_size(text.pos, text.galley.size()),
            )),
            _ => {}
        }
    }

    #[test]
    fn no_text_on_the_external_tools_page_overlaps_at_wide_or_narrow_widths() {
        for width in [1500.0, 700.0] {
            let mut state = crate::state::AppState::default();
            let ctx = egui::Context::default();
            let mut output = None;
            for _ in 0..4 {
                output = Some(ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 4000.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            super::show_tools_view(&mut state, ui);
                        });
                    },
                ));
            }
            let mut texts = Vec::new();
            for clipped in &output.expect("rendered page").shapes {
                let first = texts.len();
                painted_text(&clipped.shape, &mut texts);
                // Only the visible part counts: a long path is clipped to its box.
                for (_, rect) in &mut texts[first..] {
                    *rect = rect.intersect(clipped.clip_rect);
                }
            }
            assert!(texts.iter().any(|(text, _)| text == "Structural solvers"));
            for (i, (text_a, a)) in texts.iter().enumerate() {
                for (text_b, b) in &texts[i + 1..] {
                    let overlap = a.intersect(*b);
                    assert!(
                        overlap.width() <= 0.5 || overlap.height() <= 0.5,
                        "at {width} px {text_a:?} {a:?} overlaps {text_b:?} {b:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn invalid_directory_never_falls_back_to_an_unrelated_file_explorer_location() {
        let result = explorer_target("no/such/ALAS/location", true);
        assert!(result.is_err());
    }
}
