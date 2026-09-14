// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Resolved availability rows for the External Tools page.

use super::*;

pub(super) fn resolved_status(state: &AppState, ui: &mut Ui) {
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
                describe_nastran_solver(
                    &config.structures.nastran_solver_path,
                    environment.nastran_solver.as_deref(),
                ),
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
            cfd::status_rows(state, ui);
        });
}
