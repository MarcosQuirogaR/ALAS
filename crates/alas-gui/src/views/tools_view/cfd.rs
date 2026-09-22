// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! CFD path selection, persistence, and availability rows for External Tools.

use super::*;
use crate::path_picker::ToolPathTarget;

pub(super) fn apply_path_selection(
    state: &mut AppState,
    target: ToolPathTarget,
    path: &str,
) -> Option<&'static str> {
    let label = match target {
        ToolPathTarget::OpenFoamNativeBinDirectory => {
            state.cfd.openfoam_preferences.native_bin_dir = Some(path.to_owned());
            "OpenFOAM native bin directory"
        }
        ToolPathTarget::OpenFoamNativeProjectDirectory => {
            state.cfd.openfoam_preferences.native_project_dir = Some(path.to_owned());
            "OpenFOAM native project directory"
        }
        ToolPathTarget::GmshExecutable => {
            state.cfd.gmsh_executable = Some(path.to_owned());
            state.cfd.openfoam_preferences.gmsh_executable = Some(path.to_owned());
            "Gmsh executable"
        }
        ToolPathTarget::ParaViewExecutable => {
            state.cfd.paraview_executable = Some(path.to_owned());
            "ParaView executable"
        }
        _ => return None,
    };
    save_cfd_environment_preferences(state);
    Some(label)
}

pub(crate) fn save_cfd_environment_preferences(state: &mut AppState) {
    state.cfd.mark_environment_changed();
    if let Err(error) = state.cfd.save_environment_preferences(&state.tool_locator) {
        state.log(
            tr_fields(
                "CFD environment settings not saved: {error}",
                &[("error", error)],
            ),
            crate::state::LogKind::Warn,
        );
    }
}

pub(super) fn status_rows(state: &AppState, ui: &mut Ui, parafoam: Option<&std::path::Path>) {
    status_row(
        ui,
        "OpenFOAM CFD",
        state.cfd.capabilities.as_ref().map_or_else(
            || tr("Connection not tested yet."),
            |capabilities| capabilities.summary(),
        ),
    );
    status_row(
        ui,
        "Gmsh",
        describe_optional_executable(state.cfd.openfoam_preferences.gmsh_executable.as_deref()),
    );
    status_row(
        ui,
        "ParaView",
        describe_optional_executable(state.cfd.paraview_executable.as_deref()),
    );
    status_row(
        ui,
        "paraFoam",
        parafoam.map_or_else(
            || tr("not found in the configured OpenFOAM project"),
            |path| path.display().to_string(),
        ),
    );
}

/// Locate the official OpenFOAM ParaView launcher shipped beside a native
/// project.  It is a shell script, so the GUI reports it separately from the
/// native solver executables and leaves execution to the documented MSYS2
/// wrapper when exporting contours.
pub(super) fn detect_parafoam(state: &AppState) -> Option<std::path::PathBuf> {
    if let Some(project) = state.cfd.openfoam_preferences.native_project_dir.as_deref() {
        let root = std::path::Path::new(project);
        for name in ["paraFoam", "paraFoam.exe"] {
            let candidate = root.join("bin").join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let mut ancestor = state
        .cfd
        .openfoam_preferences
        .native_bin_dir
        .as_deref()
        .map(std::path::PathBuf::from)?;
    for _ in 0..8 {
        if let Some(parent) = ancestor.parent() {
            ancestor = parent.to_path_buf();
        } else {
            break;
        }
        for name in ["paraFoam", "paraFoam.exe"] {
            let candidate = ancestor.join("bin").join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn describe_optional_executable(path: Option<&str>) -> String {
    let Some(path) = path.filter(|path| !path.trim().is_empty()) else {
        return tr("not configured");
    };
    if std::path::Path::new(path).is_file() {
        path.to_owned()
    } else {
        tr_fields("missing: {path}", &[("path", path.to_owned())])
    }
}
