// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Card renderers for the external-tools page.

use super::*;

mod openfoam;

pub(super) use openfoam::openfoam_card;

pub(super) fn routing_card(state: &mut AppState, ui: &mut Ui) {
    card(
        ui,
        "Mission routing",
        "https://www.simbrief.com/",
        &[],
        |ui| split_body(ui, state, simbrief_rows, local_route_rows),
    );
}

fn simbrief_rows(state: &mut AppState, ui: &mut Ui) {
    sub_heading(ui, "SimBrief flight plan").on_hover_text(tr(
        "Optionally use your most recent filed plan before ALAS falls back to local navigation data or a great-circle route.",
    ));
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
    ui.label(RichText::new(tr("Fetch timeout")).weak());
    if ui
        .add_sized(
            [ui.available_width(), ui.spacing().interact_size.y],
            egui::DragValue::new(&mut timeout)
                .range(1.0..=120.0)
                .suffix(" s"),
        )
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
}

fn local_route_rows(state: &mut AppState, ui: &mut Ui) {
    sub_heading(ui, "Local route data").on_hover_text(format!(
        "{}\n\n{}",
        tr("Navigation data and imported route files are kept in locations you choose. They are saved per user and never copied beside the application executable."),
        tr("The NASA Blue Marble route texture is bundled inside ALAS as a compressed PNG; it has no user path to configure."),
    ));
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
        state.note_parameter_modified(tr("Navigation-data directory"), navdata.clone());
    }
    ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !state.navdata_download_in_progress,
                        egui::Button::new(if state.navdata_download_in_progress {
                            tr("Downloading navigation data...")
                        } else {
                            tr("Download navigation data")
                        }),
                    )
                    .on_hover_text(tr(
                        "Download missing or truncated navigation-data files into the configured directory.",
                    ))
                    .clicked()
                {
                    state.start_navdata_download(&navdata);
                }
                if state.navdata_download_in_progress
                    && ui
                        .button(tr("Cancel"))
                        .on_hover_text(tr(
                            "Stop after the file currently transferring; any files already installed are kept.",
                        ))
                        .clicked()
                {
                    state.cancel_navdata_download();
                }
            });
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
}

pub(super) fn nastran_card(state: &mut AppState, ui: &mut Ui) {
    card(
        ui,
        "NASTRAN",
        "https://nexus.hexagon.com/home/product/msc-nastran/",
        &[],
        |ui| {
            run_toggle(
                state,
                ui,
                "structures",
                "run_nastran",
                false,
                "Run NASTRAN solve",
                "Otherwise the analytical wingbox is used.",
            );
            split_body(ui, state, msc_nastran_rows, nastran95_rows);
        },
    );
}

fn msc_nastran_rows(state: &mut AppState, ui: &mut Ui) {
    sub_heading(ui, "MSC Nastran");
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
}

fn nastran95_rows(state: &mut AppState, ui: &mut Ui) {
    sub_heading(ui, "Local NASA NASTRAN-95 comparison");
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
}

/// The card's "use this tool in runs" switch, shown first under its title.
fn run_toggle(
    state: &mut AppState,
    ui: &mut Ui,
    group: &str,
    name: &str,
    default: bool,
    label: &str,
    help: &str,
) {
    let mut run = state
        .config_values
        .get(group)
        .and_then(|g| g.get(name))
        .and_then(Value::as_bool)
        .unwrap_or(default);
    if ui
        .checkbox(&mut run, tr(label))
        .on_hover_text(tr(help))
        .changed()
    {
        set_bool(&mut state.config_values, group, name, run);
        state.on_config_modified();
    }
    ui.add_space(4.0);
}

pub(super) fn patran_card(state: &mut AppState, ui: &mut Ui) {
    card(
        ui,
        "Patran",
        "https://nexus.hexagon.com/home/product/patran/",
        &[],
        |ui| {
            run_toggle(
                state,
                ui,
                "structures",
                "run_patran_export",
                false,
                "Export/run Patran",
                "Requires NASTRAN above.",
            );
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
        },
    );
}

pub(super) fn mses_card(state: &mut AppState, ui: &mut Ui) {
    card(
        ui,
        "MSES",
        "https://web.mit.edu/drela/Public/web/mses/",
        &[],
        |ui| {
            run_toggle(
                state,
                ui,
                "mses",
                "enabled",
                true,
                "Run MSES 2-D airfoil analysis",
                "Runs on the optimized design's root section for Model Comparison.",
            );
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
        },
    );
}

pub(super) fn avl_card(state: &mut AppState, ui: &mut Ui) {
    card(
        ui,
        "Athena AVL",
        "https://web.mit.edu/drela/Public/web/avl/",
        &[
            "The Windows release includes the official AVL 3.52 executable beside ALAS. Development checkouts discover external tools/avl352.exe automatically; a configured path may override it. ALAS exports an inspectable SI deck even when AVL is unavailable, and only completed compatible CL and Cm results appear in Model Comparison.",
            "ALAS always runs its local Fourier lifting-line check in-process. AVL is an independent external comparison, not a requirement for local aerodynamic analysis; the bundled executable is launched as a separate GPL-licensed program.",
        ],
        |ui| {
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

pub(super) fn openvsp_card(state: &mut AppState, ui: &mut Ui) {
    card(ui, "OpenVSP / VSPAERO", "https://openvsp.org/", &[
        "Native geometry export uses vspscript; VSPAERO uses its separate solver in the same installation. Both must be ready for an independent 3-D comparison.",
    ], |ui| {
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
        ui.add_space(8.0);
        openvsp_preview_runtime_row(state, ui);
    });
}

/// The optional, app-local Python runtime used only for native OpenVSP
/// screenshots after export (see `docs/openvsp-preview.md`). Separate from
/// the install-directory row above: this never affects `vspscript` or
/// VSPAERO analysis, and staying user-initiated keeps a fresh install from
/// silently starting a multi-megabyte download.
fn openvsp_preview_runtime_row(state: &mut AppState, ui: &mut Ui) {
    sub_heading(ui, "Optional native-preview runtime")
        .on_hover_text(tr(
            "A separate, app-local Python runtime used only for native OpenVSP screenshots after export; it never affects vspscript or VSPAERO analysis. Setup downloads hash-pinned Python, OpenVSP Python bindings, and NumPy from their official sources and verifies each archive before extracting it. See docs/openvsp-preview.md for the exact pinned versions and licenses.",
        ));
    if !crate::openvsp_runtime_setup::install_supported() {
        ui.label(
            RichText::new(tr(
                "Not available: the preview runtime only supports 64-bit Windows.",
            ))
            .weak()
            .small(),
        );
        return;
    }
    let destination = crate::openvsp_runtime_setup::resolve_destination(
        state.tool_preferences.openvsp_dir.as_deref(),
    );
    let status = crate::openvsp_runtime_setup::runtime_status(destination.as_deref());
    ui.label(format!("{}: {}", tr("Preview runtime"), status.label()));
    let running = state.openvsp_runtime_setup.running;
    let installed = matches!(
        status,
        crate::openvsp_runtime_setup::PreviewRuntimeStatus::Installed { .. }
    );
    ui.horizontal(|ui| {
        let button_label = if running {
            tr("Setting up...")
        } else if installed {
            tr("Reinstall preview runtime")
        } else {
            tr("Install preview runtime")
        };
        if ui
            .add_enabled(!running, egui::Button::new(button_label))
            .on_hover_text(tr(
                "Downloads and verifies the pinned archives, then atomically replaces the runtime. Cancel any time before it finishes; nothing already installed is touched until the very last step.",
            ))
            .clicked()
        {
            state.start_openvsp_runtime_setup();
        }
        if running {
            let cancelling = state.openvsp_runtime_setup.is_cancelling();
            if ui
                .add_enabled(!cancelling, egui::Button::new(tr("Cancel")))
                .clicked()
            {
                state.cancel_openvsp_runtime_setup();
            }
            ui.label(
                RichText::new(state.openvsp_runtime_setup.stage.clone())
                    .weak()
                    .small(),
            );
        }
    });
    if let Some(error) = state.openvsp_runtime_setup.error.clone() {
        ui.label(
            RichText::new(error)
                .color(ui.visuals().error_fg_color)
                .small(),
        );
    }
}

pub(super) fn flowunsteady_card(state: &mut AppState, ui: &mut Ui) {
    card(
        ui,
        "FLOWUnsteady / Julia",
        "https://github.com/byuflowlab/FLOWUnsteady",
        &[
            "Optional lifting-surface unsteady analysis through a process boundary. FLOWUnsteady's licence is user-supplied and follows your selected external release; ALAS does not assume the upstream MIT notice covers one user's Julia environment or dependency closure, so the adapter remains user-supplied and ALAS never downloads it.",
            "This configured path overrides the ALAS_FLOWUNSTEADY_EXE environment variable, which remains usable for headless or CI launches.",
        ],
        |ui| {
            let mut path = state
                .tool_preferences
                .flowunsteady_exe
                .clone()
                .unwrap_or_default();
            if text_row(
                state,
                ui,
                "Executable path",
                &mut path,
                false,
                Some(ToolPathTarget::FlowUnsteadyExecutable),
            ) {
                state.tool_preferences.flowunsteady_exe =
                    (!path.trim().is_empty()).then_some(path.clone());
                save_direct_tool_preferences(state);
                state.note_parameter_modified(tr("Executable path"), path);
            }
        },
    );
}
