// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The optional native-preview runtime row of the OpenVSP card.

use super::*;

/// The optional, app-local Python runtime used only for native OpenVSP
/// screenshots after export (see `docs/openvsp-preview.md`). Separate from
/// the install-directory row above: this never affects `vspscript` or
/// VSPAERO analysis, and staying user-initiated keeps a fresh install from
/// silently starting a multi-megabyte download.
pub(super) fn openvsp_preview_runtime_row(state: &mut AppState, ui: &mut Ui) {
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
    // The manifest read and the install-root walk are cached: they re-run only
    // when the configured directory changes or a setup run starts or ends.
    let status = {
        let key = egui::Id::new((
            "alas_openvsp_runtime_status",
            state.tool_preferences.openvsp_dir.clone(),
            state.openvsp_runtime_setup.running,
        ));
        if state.openvsp_runtime_setup.running {
            // The finished run changes the manifest: drop the idle entry so the
            // frame after it re-reads once.
            let idle = egui::Id::new((
                "alas_openvsp_runtime_status",
                state.tool_preferences.openvsp_dir.clone(),
                false,
            ));
            ui.ctx().data_mut(|data| {
                data.remove::<crate::openvsp_runtime_setup::PreviewRuntimeStatus>(idle)
            });
        }
        let cached = ui
            .ctx()
            .data(|data| data.get_temp::<crate::openvsp_runtime_setup::PreviewRuntimeStatus>(key));
        cached.unwrap_or_else(|| {
            let destination = crate::openvsp_runtime_setup::resolve_destination(
                state.tool_preferences.openvsp_dir.as_deref(),
            );
            let status = crate::openvsp_runtime_setup::runtime_status(destination.as_deref());
            ui.ctx()
                .data_mut(|data| data.insert_temp(key, status.clone()));
            status
        })
    };
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
