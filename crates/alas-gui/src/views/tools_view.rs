// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Workspace > External Tools: the integration paths and credentials for the
//! optional external solvers, consolidated in one place rather than spread
//! across their own Advanced sections.
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
mod field_row;
mod installer_card;
mod layout;
mod status;

use layout::{block_gap, card, framed, group, masonry, sub_heading, text_row, tool_block, CardFn};

pub(crate) use cfd::save_cfd_environment_preferences;

/// Render the External Tools page.
pub fn show_tools_view(state: &mut AppState, ui: &mut Ui) {
    apply_completed_path_selection(state);
    ui.heading(tr("External Tools")).on_hover_text(tr(
        "Integration paths and credentials for the optional external tools ALAS can drive \
         during a run. Set once; saved in user preferences.",
    ));
    // The rule under the title is the scroll area's top edge: content that
    // scrolls up is cut at a visible line, never under the title text.
    ui.add_space(4.0);
    ui.separator();

    // A page that was not drawn on the previous pass has just been opened: it
    // starts at its top, not where the page last left its scroll offset.
    let seen_id = egui::Id::new("tools_view_last_pass");
    let pass = ui.ctx().cumulative_pass_nr();
    let reopened = ui
        .ctx()
        .data(|data| data.get_temp::<u64>(seen_id))
        .is_none_or(|last| pass > last + 2);
    ui.ctx().data_mut(|data| data.insert_temp(seen_id, pass));
    let mut area = ScrollArea::vertical().auto_shrink([false, false]);
    if reopened {
        area = area.vertical_scroll_offset(0.0);
    }
    area.show(ui, |ui| {
            // One overview of every tool, then the tool groups as cards in a
            // masonry: two columns on a wide page, each card as tall as its
            // own content, one item spacing apart like the cards of a form.
            overview_card(state, ui);
            let cards: [(&'static str, CardFn); 5] = [
                ("install", installer_card::installer_card),
                ("structural", cards::structural_card),
                ("aero", cards::aero_card),
                ("cfd", cards::cfd_card),
                ("flight", cards::flight_card),
            ];
            masonry(ui, state, &cards);
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
        ExternalToolConfig::Mses => framed(ui, |ui| cards::mses_card(state, ui)),
        ExternalToolConfig::MscNastran => framed(ui, |ui| cards::nastran_card(state, ui)),
        ExternalToolConfig::Nastran95 => framed(ui, |ui| cards::nastran95_card(state, ui)),
        ExternalToolConfig::MscPatran => framed(ui, |ui| cards::patran_card(state, ui)),
        ExternalToolConfig::OpenVsp => framed(ui, |ui| cards::openvsp_card(state, ui)),
        ExternalToolConfig::OpenFoam | ExternalToolConfig::Gmsh | ExternalToolConfig::ParaView => {
            framed(ui, |ui| cards::openfoam_card(state, ui));
        }
        ExternalToolConfig::FlowUnsteady => framed(ui, |ui| cards::flowunsteady_card(state, ui)),
    }
    overview_card(state, ui);
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
    use alas_exec::process::NoConsoleWindow;
    let (target, select_file) = explorer_target(value, directory)?;
    #[cfg(target_os = "windows")]
    {
        let mut command = std::process::Command::new("explorer.exe");
        command.no_window();
        if select_file {
            command.arg(format!("/select,{}", target.display()));
        } else {
            command.arg(&target);
        }
        command
            .spawn()
            .map(alas_exec::process::reap_in_background)
            .map_err(|error| error.to_string())
    }
    #[cfg(target_os = "macos")]
    {
        let _ = select_file;
        std::process::Command::new("open")
            .no_window()
            .arg(&target)
            .spawn()
            .map(alas_exec::process::reap_in_background)
            .map_err(|error| error.to_string())
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = select_file;
        std::process::Command::new("xdg-open")
            .no_window()
            .arg(&target)
            .spawn()
            .map(alas_exec::process::reap_in_background)
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

/// The status overview: every tool with its chip and resolved path.
fn overview_card(state: &AppState, ui: &mut Ui) {
    group(ui, "Tool status", |ui| status::overview(state, ui));
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
        ExecutableDiscovery::Absent => {
            tr("not configured: set the executable in the FLOWUnsteady card")
        }
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

fn describe_nastran95(
    config: &alas_config::structures::StructuresConfig,
) -> (status::Tone, String) {
    use self::status::Tone;
    use alas_struct::nastran95::Nastran95Solver;
    let directory = config.nastran95_dir_path.trim();
    let runtime = config.nastran95_runtime_path.trim();
    let rf_stage = config.nastran95_rf_stage_path.trim();
    let open_core = config.nastran95_open_core_words.trim();
    let solver = if directory.is_empty() {
        Nastran95Solver::from_env()
            .or_else(Nastran95Solver::from_adjacent_bundle)
            .or_else(Nastran95Solver::from_installed_tool)
    } else {
        Nastran95Solver::from_paths(
            std::path::Path::new(directory),
            (!runtime.is_empty()).then(|| std::path::Path::new(runtime)),
            (!rf_stage.is_empty()).then(|| std::path::Path::new(rf_stage)),
            (!open_core.is_empty()).then_some(open_core),
        )
    };
    match solver {
        Some(solver) => (
            Tone::Ready,
            tr_fields(
                "{path} (executable and rigid formats found)",
                &[("path", solver.exe.display().to_string())],
            ),
        ),
        None if directory.is_empty() => (Tone::Neutral, tr("not configured or discovered")),
        None => (
            Tone::Warn,
            tr_fields(
                "not found under {path} (need {executable} and rf/NASINFO)",
                &[
                    ("path", directory.to_owned()),
                    (
                        "executable",
                        Nastran95Solver::EXECUTABLE_RELATIVE.to_owned(),
                    ),
                ],
            ),
        ),
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
            assert!(texts
                .iter()
                .any(|(text, _)| *text == crate::views::tr_label("Structural solvers")));
            assert!(texts
                .iter()
                .any(|(text, _)| *text == crate::views::tr_label("Tool status")));
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
