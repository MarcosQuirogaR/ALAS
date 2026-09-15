// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! OpenFOAM, Gmsh, and ParaView environment card.

use super::*;
use alas_exec::openfoam::{OpenFoamBackend, OpenFoamSupportLevel, GMSH_COMMAND, REQUIRED_COMMANDS};
use egui::ComboBox;
/// OpenFOAM/Gmsh/ParaView environment card for the standalone Airfoil CFD
/// workflow.  Detection runs in the CFD worker; this card only edits and
/// persists explicit paths.
pub(crate) fn openfoam_card(state: &mut AppState, ui: &mut Ui) {
    card(ui, "OpenFOAM CFD", "https://www.openfoam.com/", |ui| {
        ui.label(
            RichText::new(tr(
                "Configure the OpenFOAM backend, native project/bin directories and optional Gmsh and ParaView viewers. Connection probing and CFD solves run outside the user-interface thread.",
            ))
            .weak()
            .small(),
        );

        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr("Backend")).strong());
            let mut backend = state.cfd.openfoam_preferences.backend;
            ComboBox::from_id_salt("openfoam_backend")
                .selected_text(tr(backend.display_name()))
                .show_ui(ui, |ui| {
                    for candidate in [
                        OpenFoamBackend::Auto,
                        OpenFoamBackend::Native,
                        OpenFoamBackend::Wsl2,
                    ] {
                        ui.selectable_value(&mut backend, candidate, tr(candidate.display_name()));
                    }
                });
            if backend != state.cfd.openfoam_preferences.backend {
                state.cfd.openfoam_preferences.backend = backend;
                save_cfd_environment_preferences(state);
            }
        });

        let backend = state.cfd.openfoam_preferences.backend;
        if backend != OpenFoamBackend::Wsl2 {
            show_native_rows(state, ui);
        }
        if backend != OpenFoamBackend::Native {
            show_wsl_rows(state, ui);
        }

        let mut gmsh = state.cfd.gmsh_executable.clone().unwrap_or_default();
        if text_row(
            state,
            ui,
            "Gmsh executable",
            &mut gmsh,
            false,
            Some(ToolPathTarget::GmshExecutable),
        ) {
            state.cfd.gmsh_executable = (!gmsh.trim().is_empty()).then_some(gmsh.clone());
            state.cfd.openfoam_preferences.gmsh_executable = state.cfd.gmsh_executable.clone();
            save_cfd_environment_preferences(state);
        }

        let mut paraview = state.cfd.paraview_executable.clone().unwrap_or_default();
        if text_row(
            state,
            ui,
            "ParaView executable",
            &mut paraview,
            false,
            Some(ToolPathTarget::ParaViewExecutable),
        ) {
            state.cfd.paraview_executable = (!paraview.trim().is_empty()).then_some(paraview);
            save_cfd_environment_preferences(state);
        }

        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!state.cfd.probing && !state.cfd.running, egui::Button::new(tr("Connection test")))
                .on_hover_text(tr("Probe the configured backend, OpenFOAM version and required utilities in a background worker."))
                .clicked()
            {
                state.cfd.start_probe();
            }
            if state.cfd.probing {
                ui.spinner();
            }
            ui.label(RichText::new(tr(&state.cfd.status)).weak());
        });

        show_capabilities(state, ui);
        ui.label(
            RichText::new(tr(
                "The initial study uses an incompressible steady k-omega SST template. Missing utilities or unsupported regimes stop before a solver result is presented.",
            ))
            .weak()
            .small(),
        );
    });
}

fn show_native_rows(state: &mut AppState, ui: &mut Ui) {
    let mut native_bin = state
        .cfd
        .openfoam_preferences
        .native_bin_dir
        .clone()
        .unwrap_or_default();
    if text_row(
        state,
        ui,
        "Native OpenFOAM bin directory",
        &mut native_bin,
        true,
        Some(ToolPathTarget::OpenFoamNativeBinDirectory),
    ) {
        state.cfd.openfoam_preferences.native_bin_dir =
            (!native_bin.trim().is_empty()).then_some(native_bin.clone());
        save_cfd_environment_preferences(state);
    }

    let mut native_project = state
        .cfd
        .openfoam_preferences
        .native_project_dir
        .clone()
        .unwrap_or_default();
    if text_row(
        state,
        ui,
        "Native OpenFOAM project directory",
        &mut native_project,
        true,
        Some(ToolPathTarget::OpenFoamNativeProjectDirectory),
    ) {
        state.cfd.openfoam_preferences.native_project_dir =
            (!native_project.trim().is_empty()).then_some(native_project.clone());
        save_cfd_environment_preferences(state);
    }
}

fn show_wsl_rows(state: &mut AppState, ui: &mut Ui) {
    let mut distribution = state
        .cfd
        .openfoam_preferences
        .wsl_distribution
        .clone()
        .unwrap_or_default();
    if text_row(
        state,
        ui,
        "WSL distribution",
        &mut distribution,
        false,
        None,
    ) {
        state.cfd.openfoam_preferences.wsl_distribution =
            (!distribution.trim().is_empty()).then_some(distribution.clone());
        save_cfd_environment_preferences(state);
    }

    let mut launcher = state
        .cfd
        .openfoam_preferences
        .wsl_launcher
        .clone()
        .unwrap_or_default();
    if text_row(
        state,
        ui,
        "WSL environment launcher",
        &mut launcher,
        false,
        None,
    ) {
        state.cfd.openfoam_preferences.wsl_launcher =
            (!launcher.trim().is_empty()).then_some(launcher.clone());
        save_cfd_environment_preferences(state);
    }

    let mut bin_dir = state
        .cfd
        .openfoam_preferences
        .wsl_bin_dir
        .clone()
        .unwrap_or_default();
    if text_row(
        state,
        ui,
        "WSL OpenFOAM bin directory",
        &mut bin_dir,
        false,
        None,
    ) {
        state.cfd.openfoam_preferences.wsl_bin_dir =
            (!bin_dir.trim().is_empty()).then_some(bin_dir.clone());
        save_cfd_environment_preferences(state);
    }
    ui.label(
        RichText::new(tr(
            "The launcher receives each utility and its arguments inside WSL2, for example openfoam2306 from the openfoam.com Ubuntu packages. Without it, WSL runs the utilities without the OpenFOAM environment and a bin directory alone does not load their libraries.",
        ))
        .weak()
        .small(),
    );
}

fn show_capabilities(state: &AppState, ui: &mut Ui) {
    let Some(capabilities) = state.cfd.capabilities.as_ref() else {
        ui.label(RichText::new(tr("Connection not tested yet.")).weak());
        return;
    };
    ui.label(RichText::new(capabilities.summary()).strong());
    ui.label(RichText::new(capabilities.detail.as_str()).weak().small());
    let support_color = match capabilities.version_support.level {
        OpenFoamSupportLevel::Supported => crate::theme::success_color(ui.visuals()),
        OpenFoamSupportLevel::Untested => ui.visuals().warn_fg_color,
        OpenFoamSupportLevel::Unsupported => ui.visuals().error_fg_color,
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tr("Version support")).strong());
        ui.colored_label(support_color, capabilities.support_summary());
    });
    ui.label(
        RichText::new(capabilities.version_support.reason.as_str())
            .weak()
            .small(),
    );
    egui::Grid::new("openfoam_capabilities")
        .num_columns(2)
        .spacing([12.0, 3.0])
        .show(ui, |ui| {
            for command in REQUIRED_COMMANDS.iter().copied().chain([GMSH_COMMAND]) {
                ui.label(command);
                let ready = capabilities.commands.get(command).copied().unwrap_or(false);
                ui.colored_label(
                    if ready {
                        crate::theme::success_color(ui.visuals())
                    } else {
                        ui.visuals().error_fg_color
                    },
                    if ready {
                        tr("available")
                    } else {
                        tr("missing")
                    },
                );
                ui.end_row();
            }
        });
}
