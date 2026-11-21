// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Card renderers for the external-tools page.

use super::*;

mod groups;
mod openfoam;
mod openvsp_runtime;

pub(super) use groups::{aero_card, cfd_card, flight_card, structural_card};
pub(super) use openfoam::openfoam_card;
use openvsp_runtime::openvsp_preview_runtime_row;

fn simbrief_rows(state: &mut AppState, ui: &mut Ui) {
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
    let edited = field_row::grid_row(ui, "Fetch timeout", None, |ui| {
        // The box has the width of the username field above it.
        let spacing = ui.spacing().item_spacing.x;
        let buttons = 2.0 * layout::picker_button_width(ui) + 2.0 * spacing;
        let width = field_row::field_width(ui, buttons, 8.0) + 8.0;
        ui.allocate_ui_with_layout(
            egui::vec2(width, crate::theme::control_height(ui)),
            egui::Layout::top_down_justified(egui::Align::Min),
            |ui| {
                ui.set_max_width(width);
                ui.add(
                    egui::DragValue::new(&mut timeout)
                        .range(1.0..=120.0)
                        .fixed_decimals(0)
                        .suffix(" s"),
                )
            },
        )
        .inner
        .changed()
    });
    if edited {
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
    tool_block(
        ui,
        Some("nastran"),
        "MSC Nastran",
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
            msc_nastran_rows(state, ui);
        },
    );
}

/// The NASA NASTRAN-95 comparison solver.
pub(super) fn nastran95_card(state: &mut AppState, ui: &mut Ui) {
    tool_block(
        ui,
        Some("nastran95"),
        "Local NASA NASTRAN-95 comparison",
        "https://github.com/nasa/NASTRAN-95",
        &[],
        |ui| nastran95_rows(state, ui),
    );
}

fn msc_nastran_rows(state: &mut AppState, ui: &mut Ui) {
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
    if crate::checkbox::checkbox(ui, &mut run, tr(label))
        .on_hover_text(tr(help))
        .changed()
    {
        set_bool(&mut state.config_values, group, name, run);
        state.on_config_modified();
    }
    ui.add_space(4.0);
}

pub(super) fn patran_card(state: &mut AppState, ui: &mut Ui) {
    tool_block(
        ui,
        Some("patran"),
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
    tool_block(
        ui,
        Some("mses"),
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
    tool_block(
        ui,
        Some("avl"),
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
    tool_block(ui, Some("openvsp"), "OpenVSP / VSPAERO", "https://openvsp.org/", &[
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

pub(super) fn flowunsteady_card(state: &mut AppState, ui: &mut Ui) {
    tool_block(
        ui,
        Some("flowunsteady"),
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

#[cfg(test)]
mod width_tests {
    use super::*;

    /// A lone value (the fetch timeout) is no wider than the grid's field
    /// box (the username field above it), whatever the card's width.
    #[test]
    fn the_fetch_timeout_is_no_wider_than_a_value_box() {
        let ctx = egui::Context::default();
        let mut state = AppState::default();
        let mut widest = 0.0_f32;
        for _ in 0..2 {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1800.0, 900.0),
                    )),
                    ..egui::RawInput::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        simbrief_rows(&mut state, ui);
                    });
                },
            );
            widest = output
                .shapes
                .iter()
                .filter_map(|clipped| match &clipped.shape {
                    // Value boxes only, not the panel behind them.
                    egui::Shape::Rect(rect) if rect.rect.height() < 60.0 => Some(rect.rect.width()),
                    _ => None,
                })
                .fold(0.0_f32, f32::max);
        }
        assert!(widest > 100.0, "the boxes were drawn: {widest}");
        assert!(
            widest <= super::field_row::MAX_FIELD_WIDTH + 24.0,
            "{widest}"
        );
    }
}
