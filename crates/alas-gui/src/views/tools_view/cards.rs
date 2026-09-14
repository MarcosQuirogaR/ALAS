// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Card renderers for the external-tools page.

use super::*;

mod openfoam;

pub(super) use openfoam::openfoam_card;

pub(super) fn routing_card(state: &mut AppState, ui: &mut Ui) {
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
        ui.label(
            RichText::new(tr(
                "The NASA Blue Marble route texture is bundled inside ALAS as a compressed PNG; it has no user path to configure.",
            ))
            .weak()
            .small(),
        );
    });
}

pub(super) fn nastran_card(state: &mut AppState, ui: &mut Ui) {
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

pub(super) fn patran_card(state: &mut AppState, ui: &mut Ui) {
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

pub(super) fn mses_card(state: &mut AppState, ui: &mut Ui) {
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

pub(super) fn avl_card(state: &mut AppState, ui: &mut Ui) {
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

pub(super) fn openvsp_card(state: &mut AppState, ui: &mut Ui) {
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
