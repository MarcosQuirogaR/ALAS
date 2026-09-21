// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Resolved availability rows for the External Tools page.

use super::*;

use alas_exec::{ExecutableDiscovery, RunEnvironment};
const STATUS_REFRESH_SECONDS: f64 = 5.0;

#[derive(Clone, PartialEq)]
struct ExternalToolStatus {
    fingerprint: String,
    built_at: f64,
    environment: RunEnvironment,
    nastran: ExecutableDiscovery,
    patran: ExecutableDiscovery,
    openvsp: ExecutableDiscovery,
    vspaero: ExecutableDiscovery,
    avl: ExecutableDiscovery,
    parafoam: Option<std::path::PathBuf>,
}

pub(super) fn resolved_status(state: &AppState, ui: &mut Ui) {
    let Some(config) = state.typed_config() else {
        return;
    };
    // Tool discovery walks several conventional installation roots and may
    // inspect each candidate's contents. Keep that filesystem work out of
    // the egui paint loop: unchanged status is reused for five seconds, while
    // a changed path becomes visible on the next refresh interval at worst.
    let fingerprint = status_fingerprint(state, &config);
    let now = ui.ctx().input(|input| input.time);
    let cache_id = egui::Id::new("alas_external_tools_status");
    let cached = ui
        .ctx()
        .data(|data| data.get_temp::<ExternalToolStatus>(cache_id));
    let snapshot = cached
        .filter(|cached| {
            cached.fingerprint == fingerprint && now - cached.built_at < STATUS_REFRESH_SECONDS
        })
        .unwrap_or_else(|| {
            let snapshot = build_status_snapshot(state, &config, fingerprint, now);
            ui.ctx()
                .data_mut(|data| data.insert_temp(cache_id, snapshot.clone()));
            snapshot
        });
    let environment = &snapshot.environment;
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
            status_row(ui, "NASTRAN", describe_executable(&snapshot.nastran));
            status_row(
                ui,
                "MSC solver override",
                describe_nastran_solver(
                    &config.structures.nastran_solver_path,
                    environment.nastran_solver.as_deref(),
                ),
            );
            status_row(ui, "Patran", describe_executable(&snapshot.patran));
            status_row(
                ui,
                "OpenVSP script runner",
                describe_executable(&snapshot.openvsp),
            );
            status_row(ui, "VSPAERO solver", describe_executable(&snapshot.vspaero));
            status_row(ui, "Athena AVL", describe_executable(&snapshot.avl));
            cfd::status_rows(state, ui, snapshot.parafoam.as_deref());
        });
}

fn status_fingerprint(state: &AppState, config: &alas_config::AlasConfig) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        config.mses.mses_dir,
        config.structures.nastran_exe_path,
        config.structures.nastran_solver_path,
        config.structures.patran_exe_path,
        config.mission.navdata_dir,
        config.mission.routes_dir,
        state.tool_preferences.openvsp_dir.as_deref().unwrap_or(""),
        state.tool_preferences.avl_exe.as_deref().unwrap_or(""),
        state
            .cfd
            .openfoam_preferences
            .native_project_dir
            .as_deref()
            .unwrap_or(""),
        state
            .cfd
            .openfoam_preferences
            .native_bin_dir
            .as_deref()
            .unwrap_or(""),
    )
}

fn build_status_snapshot(
    state: &AppState,
    config: &alas_config::AlasConfig,
    fingerprint: String,
    built_at: f64,
) -> ExternalToolStatus {
    let mses_dir = std::path::Path::new(&config.mses.mses_dir);
    let nastran = std::path::Path::new(&config.structures.nastran_exe_path);
    let patran = std::path::Path::new(&config.structures.patran_exe_path);
    let openvsp = std::path::Path::new(state.tool_preferences.openvsp_dir.as_deref().unwrap_or(""));
    let avl = std::path::Path::new(state.tool_preferences.avl_exe.as_deref().unwrap_or(""));
    // `resolve_environment` performs the same discovery calls that the
    // status card needs.  Calling it and then discovering every executable a
    // second time made the page scan all conventional installation roots
    // twice per refresh.  Resolve each candidate once and derive both the
    // displayed discoveries and the run environment from those values.
    let nastran_status = state.tool_locator.discover_nastran(nastran);
    let nastran_solver_status = state.tool_locator.discover_nastran_solver(nastran);
    let patran_status = state.tool_locator.discover_patran(patran);
    let openvsp_status = state.tool_locator.discover_openvsp(openvsp);
    let vspaero_status = state.tool_locator.discover_vspaero(openvsp);
    let avl_status = state.tool_locator.discover_avl(avl);
    let ready_path = |discovery: &ExecutableDiscovery| match discovery {
        ExecutableDiscovery::Ready(path) => Some(path.clone()),
        ExecutableDiscovery::Absent | ExecutableDiscovery::Incomplete { .. } => None,
    };
    let environment = RunEnvironment {
        mses_dir: state.tool_locator.resolve_mses_dir(mses_dir),
        nastran_exe: ready_path(&nastran_status),
        nastran_solver: ready_path(&nastran_solver_status),
        patran_exe: ready_path(&patran_status),
        openvsp_exe: ready_path(&openvsp_status),
        vspaero_exe: ready_path(&vspaero_status),
        avl_exe: ready_path(&avl_status),
        flowunsteady_exe: std::env::var_os("ALAS_FLOWUNSTEADY_EXE")
            .map(std::path::PathBuf::from)
            .filter(|path| path.is_file()),
    };
    ExternalToolStatus {
        fingerprint,
        built_at,
        environment,
        nastran: nastran_status,
        patran: patran_status,
        openvsp: openvsp_status,
        vspaero: vspaero_status,
        avl: avl_status,
        parafoam: cfd::detect_parafoam(state),
    }
}

#[cfg(test)]
mod tests {
    use super::status_fingerprint;
    use crate::state::AppState;

    #[test]
    fn status_cache_fingerprint_tracks_configured_tool_locations() {
        let mut state = AppState::default();
        let config = state.typed_config().expect("default config");
        let initial = status_fingerprint(&state, &config);

        state.tool_preferences.openvsp_dir = Some("C:\\OpenVSP-test".to_owned());
        assert_ne!(
            status_fingerprint(&state, &config),
            initial,
            "changing an expensive discovery root must invalidate the cached status"
        );
    }
}
