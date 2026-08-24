// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Setup > External Tools: the integration paths and credentials for the
//! optional external solvers, consolidated in one place rather than spread
//! across their own Advanced Settings pages.
//!
//! A port of the reference desktop app's `SetupScreen`. The reference's
//! navdata download button calls out over HTTP to a Go-side downloader; that
//! transport has no counterpart here (see `docs/PORTING.md`'s "Dropped: the
//! HTTP sidecar"), so this shows the licensing note and a status line rather
//! than fabricating a network call this crate has no client for.

use alas_exec::ExecutableDiscovery;
use egui::{RichText, ScrollArea, TextEdit, Ui};
use serde_json::Value;

use crate::path_picker::{PathSelection, ToolPathTarget};
use crate::state::AppState;
use crate::views::{tr, tr_fields};

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
            routing_card(state, ui);
            ui.add_space(8.0);

            section_heading(ui, "External solvers");
            if ui.available_width() >= 920.0 {
                ui.columns(2, |columns| {
                    let (left, right) = columns.split_at_mut(1);
                    nastran_card(state, &mut left[0]);
                    patran_card(state, &mut right[0]);
                });
                ui.add_space(8.0);
                ui.columns(2, |columns| {
                    let (left, right) = columns.split_at_mut(1);
                    mses_card(state, &mut left[0]);
                    openvsp_card(state, &mut right[0]);
                });
            } else {
                nastran_card(state, ui);
                ui.add_space(8.0);
                patran_card(state, ui);
                ui.add_space(8.0);
                mses_card(state, ui);
                ui.add_space(8.0);
                openvsp_card(state, ui);
            }
            ui.add_space(8.0);
            avl_card(state, ui);
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
fn open_in_file_explorer(value: &str, directory: bool) -> Result<(), String> {
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

fn routing_card(state: &mut AppState, ui: &mut Ui) {
    card(ui, "Mission routing", "https://www.simbrief.com/", |ui| {
        ui.label(RichText::new(tr("SimBrief flight plan")).strong());
        ui.label(
            RichText::new(tr(
                "Optionally use your most recent filed plan before ALAS falls back to local navigation data or a great-circle route.",
            ))
            .weak()
            .small(),
        );
        let mut user = str_field(&state.config_values, "mission", "simbrief_username");
        if text_row(state, ui, "SimBrief username", &mut user, false, None) {
            set_str(
                &mut state.config_values,
                "mission",
                "simbrief_username",
                user.clone(),
            );
            state.on_config_modified();
            state.note_parameter_modified(tr("SimBrief username"), user);
        }
        let mut timeout = state
            .config_values
            .get("mission")
            .and_then(|m| m.get("simbrief_timeout_s"))
            .and_then(Value::as_f64)
            .unwrap_or(15.0);
        ui.horizontal(|ui| {
            ui.add_sized([170.0, 18.0], egui::Label::new(tr("Fetch timeout [s]")));
            if ui
                .add(egui::DragValue::new(&mut timeout).range(1.0..=120.0))
                .changed()
            {
                if let Some(obj) = state
                    .config_values
                    .get_mut("mission")
                    .and_then(Value::as_object_mut)
                {
                    obj.insert("simbrief_timeout_s".to_owned(), Value::from(timeout));
                }
                state.on_config_modified();
            }
        });

        ui.add_space(8.0);
        ui.label(RichText::new(tr("Local route data")).strong());
        ui.label(
            RichText::new(tr(
                "Navigation data and imported route files are kept in locations you choose. They are saved per user and never copied beside the application executable.",
            ))
            .weak()
            .small(),
        );
        let mut navdata = str_field(&state.config_values, "mission", "navdata_dir");
        if text_row(
            state,
            ui,
            "Navigation-data directory",
            &mut navdata,
            true,
            Some(ToolPathTarget::NavdataDirectory),
        ) {
            set_str(
                &mut state.config_values,
                "mission",
                "navdata_dir",
                navdata.clone(),
            );
            state.save_tool_preferences();
            state.on_config_modified();
            state.note_parameter_modified(tr("Navigation-data directory"), navdata);
        }
        let mut routes = str_field(&state.config_values, "mission", "routes_dir");
        if text_row(
            state,
            ui,
            "Saved-routes directory",
            &mut routes,
            true,
            Some(ToolPathTarget::RoutesDirectory),
        ) {
            set_str(
                &mut state.config_values,
                "mission",
                "routes_dir",
                routes.clone(),
            );
            state.save_tool_preferences();
            state.on_config_modified();
            state.note_parameter_modified(tr("Saved-routes directory"), routes);
        }
        ui.label(
            RichText::new(tr(
                "The NASA Blue Marble route texture is bundled inside ALAS as a compressed PNG; it has no user path to configure.",
            ))
            .weak()
            .small(),
        );
    });
}

fn nastran_card(state: &mut AppState, ui: &mut Ui) {
    card(ui, "NASTRAN", "https://hexagon.com/", |ui| {
        let mut path = str_field(&state.config_values, "structures", "nastran_exe_path");
        if text_row(
            state,
            ui,
            "Executable path",
            &mut path,
            false,
            Some(ToolPathTarget::NastranExecutable),
        ) {
            set_str(
                &mut state.config_values,
                "structures",
                "nastran_exe_path",
                path.clone(),
            );
            state.save_tool_preferences();
            state.on_config_modified();
            state.note_parameter_modified(tr("Executable path"), path);
        }
        let mut solver = str_field(&state.config_values, "structures", "nastran_solver_path");
        if text_row(
            state,
            ui,
            "MSC solver override",
            &mut solver,
            false,
            Some(ToolPathTarget::NastranSolver),
        ) {
            set_str(
                &mut state.config_values,
                "structures",
                "nastran_solver_path",
                solver.clone(),
            );
            state.save_tool_preferences();
            state.on_config_modified();
            state.note_parameter_modified(tr("MSC solver override"), solver);
        }
        ui.label(
            RichText::new(tr(
                "For the split MSC Student Edition, select Patran/.../analysis.exe here. Leave blank when the launcher finds its own solver.",
            ))
            .weak()
            .small(),
        );
        ui.add_space(6.0);
        ui.label(RichText::new(tr("Local NASA NASTRAN-95 comparison")).strong());
        let mut local_dir = str_field(&state.config_values, "structures", "nastran95_dir_path");
        if text_row(
            state,
            ui,
            "Local solver directory",
            &mut local_dir,
            true,
            Some(ToolPathTarget::Nastran95Directory),
        ) {
            set_str(
                &mut state.config_values,
                "structures",
                "nastran95_dir_path",
                local_dir.clone(),
            );
            state.save_tool_preferences();
            state.on_config_modified();
            state.note_parameter_modified(tr("Local NASTRAN-95 directory"), local_dir);
        }
        let mut runtime = str_field(&state.config_values, "structures", "nastran95_runtime_path");
        if text_row(
            state,
            ui,
            "Runtime DLL directory",
            &mut runtime,
            true,
            Some(ToolPathTarget::Nastran95RuntimeDirectory),
        ) {
            set_str(
                &mut state.config_values,
                "structures",
                "nastran95_runtime_path",
                runtime.clone(),
            );
            state.save_tool_preferences();
            state.on_config_modified();
            state.note_parameter_modified(tr("NASTRAN-95 runtime directory"), runtime);
        }
        let mut rf_stage = str_field(
            &state.config_values,
            "structures",
            "nastran95_rf_stage_path",
        );
        if text_row(
            state,
            ui,
            "Short RF staging directory",
            &mut rf_stage,
            true,
            Some(ToolPathTarget::Nastran95RfStageDirectory),
        ) {
            set_str(
                &mut state.config_values,
                "structures",
                "nastran95_rf_stage_path",
                rf_stage.clone(),
            );
            state.save_tool_preferences();
            state.on_config_modified();
            state.note_parameter_modified(tr("NASTRAN-95 RF staging directory"), rf_stage);
        }
        ui.label(
            RichText::new(tr(
                "Set these three paths once to run local SOL 101/SOL 103 beside MSC in every desktop launch. The RF directory must be an absolute path shorter than 38 bytes (for example C:/nas-rf).",
            ))
            .weak()
            .small(),
        );
        let mut open_core = str_field(
            &state.config_values,
            "structures",
            "nastran95_open_core_words",
        );
        if text_row(
            state,
            ui,
            "Open-core words (OCMEM)",
            &mut open_core,
            false,
            None,
        ) {
            set_str(
                &mut state.config_values,
                "structures",
                "nastran95_open_core_words",
                open_core.clone(),
            );
            state.save_tool_preferences();
            state.on_config_modified();
            state.note_parameter_modified(tr("NASTRAN-95 open-core words"), open_core);
        }
        ui.label(
            RichText::new(tr(
                "Leave OCMEM blank to use the capacity compiled into nastran.exe. This local build records its limit beside the executable; rebuild it with a larger open-core array only if the full mesh exceeds that recorded allocation.",
            ))
            .weak()
            .small(),
        );
        let mut run = state
            .config_values
            .get("structures")
            .and_then(|s| s.get("run_nastran"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if ui
            .checkbox(
                &mut run,
                tr("Run a real NASTRAN solve (otherwise the analytical wingbox is used)"),
            )
            .changed()
        {
            set_bool(&mut state.config_values, "structures", "run_nastran", run);
            state.on_config_modified();
        }
    });
}

fn patran_card(state: &mut AppState, ui: &mut Ui) {
    card(ui, "Patran", "https://hexagon.com/", |ui| {
        let mut path = str_field(&state.config_values, "structures", "patran_exe_path");
        if text_row(
            state,
            ui,
            "Executable path",
            &mut path,
            false,
            Some(ToolPathTarget::PatranExecutable),
        ) {
            set_str(
                &mut state.config_values,
                "structures",
                "patran_exe_path",
                path.clone(),
            );
            state.save_tool_preferences();
            state.on_config_modified();
            state.note_parameter_modified(tr("Executable path"), path);
        }
        let mut run = state
            .config_values
            .get("structures")
            .and_then(|s| s.get("run_patran_export"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if ui
            .checkbox(&mut run, tr("Export/run Patran (requires NASTRAN above)"))
            .changed()
        {
            set_bool(
                &mut state.config_values,
                "structures",
                "run_patran_export",
                run,
            );
            state.on_config_modified();
        }
    });
}

fn mses_card(state: &mut AppState, ui: &mut Ui) {
    card(
        ui,
        "MSES",
        "https://web.mit.edu/drela/Public/web/mses/",
        |ui| {
            let mut dir = str_field(&state.config_values, "mses", "mses_dir");
            if text_row(
                state,
                ui,
                "Install directory",
                &mut dir,
                true,
                Some(ToolPathTarget::MsesDirectory),
            ) {
                set_str(&mut state.config_values, "mses", "mses_dir", dir.clone());
                state.save_tool_preferences();
                state.on_config_modified();
                state.note_parameter_modified(tr("Install directory"), dir);
            }
            let mut enabled = state
                .config_values
                .get("mses")
                .and_then(|m| m.get("enabled"))
                .and_then(Value::as_bool)
                .unwrap_or(true);
            if ui
                .checkbox(&mut enabled, tr("Run MSES 2-D airfoil analysis"))
                .changed()
            {
                set_bool(&mut state.config_values, "mses", "enabled", enabled);
                state.on_config_modified();
            }
        },
    );
}

fn avl_card(state: &mut AppState, ui: &mut Ui) {
    card(
        ui,
        "Athena AVL",
        "https://web.mit.edu/drela/Public/web/avl/",
        |ui| {
            ui.label(
            RichText::new(tr(
                "The Windows release includes the official AVL 3.52 executable beside ALAS. Development checkouts discover external tools/avl352.exe automatically; a configured path may override it. ALAS exports an inspectable SI deck even when AVL is unavailable, and only completed compatible CL and Cm results appear in Model Comparison.",
            ))
            .weak()
            .small(),
        );
            ui.label(
                RichText::new(tr(
                    "ALAS always runs its local Fourier lifting-line check in-process. AVL is an independent external comparison, not a requirement for local aerodynamic analysis; the bundled executable is launched as a separate GPL-licensed program.",
                ))
                .weak()
                .small(),
            );
            let mut path = state.tool_preferences.avl_exe.clone().unwrap_or_default();
            if text_row(
                state,
                ui,
                "Executable path",
                &mut path,
                false,
                Some(ToolPathTarget::AvlExecutable),
            ) {
                state.tool_preferences.avl_exe = (!path.trim().is_empty()).then_some(path.clone());
                save_direct_tool_preferences(state);
                state.note_parameter_modified(tr("Executable path"), path);
            }
        },
    );
}

fn openvsp_card(state: &mut AppState, ui: &mut Ui) {
    card(ui, "OpenVSP / VSPAERO", "https://openvsp.org/", |ui| {
        ui.label(
            RichText::new(tr(
                "Native geometry export uses vspscript; VSPAERO uses its separate solver in the same installation. Both must be ready for an independent 3-D comparison.",
            ))
            .weak()
            .small(),
        );
        let mut path = state
            .tool_preferences
            .openvsp_dir
            .clone()
            .unwrap_or_default();
        if text_row(
            state,
            ui,
            "Install directory",
            &mut path,
            true,
            Some(ToolPathTarget::OpenVspDirectory),
        ) {
            state.tool_preferences.openvsp_dir = (!path.trim().is_empty()).then_some(path.clone());
            save_direct_tool_preferences(state);
            state.note_parameter_modified(tr("Install directory"), path);
        }
    });
}

fn status_card(state: &AppState, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        resolved_status(state, ui);
    });
}

fn resolved_status(state: &AppState, ui: &mut Ui) {
    let Some(config) = state.typed_config() else {
        return;
    };
    let environment = state.tool_locator.resolve_environment(
        std::path::Path::new(&config.mses.mses_dir),
        std::path::Path::new(&config.structures.nastran_exe_path),
        std::path::Path::new(&config.structures.patran_exe_path),
        std::path::Path::new(state.tool_preferences.openvsp_dir.as_deref().unwrap_or("")),
        std::path::Path::new(state.tool_preferences.avl_exe.as_deref().unwrap_or("")),
    );
    egui::Grid::new("external_tools_status")
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            status_row(
                ui,
                "MSES",
                environment
                    .mses_dir
                    .as_deref()
                    .map_or_else(|| tr("not found"), |path| path.display().to_string()),
            );
            status_row(
                ui,
                "Local navigation data",
                describe_optional_directory(&config.mission.navdata_dir),
            );
            status_row(
                ui,
                "Saved routes",
                describe_optional_directory(&config.mission.routes_dir),
            );
            status_row(
                ui,
                "NASTRAN",
                describe_executable(
                    &state.tool_locator.discover_nastran(std::path::Path::new(
                        &config.structures.nastran_exe_path,
                    )),
                ),
            );
            status_row(
                ui,
                "MSC solver override",
                describe_optional_file(&config.structures.nastran_solver_path),
            );
            status_row(
                ui,
                "Patran",
                describe_executable(
                    &state
                        .tool_locator
                        .discover_patran(std::path::Path::new(&config.structures.patran_exe_path)),
                ),
            );
            status_row(
                ui,
                "OpenVSP script runner",
                describe_executable(&state.tool_locator.discover_openvsp(std::path::Path::new(
                    state.tool_preferences.openvsp_dir.as_deref().unwrap_or(""),
                ))),
            );
            status_row(
                ui,
                "VSPAERO solver",
                describe_executable(&state.tool_locator.discover_vspaero(std::path::Path::new(
                    state.tool_preferences.openvsp_dir.as_deref().unwrap_or(""),
                ))),
            );
            status_row(
                ui,
                "Athena AVL",
                describe_executable(&state.tool_locator.discover_avl(std::path::Path::new(
                    state.tool_preferences.avl_exe.as_deref().unwrap_or(""),
                ))),
            );
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
