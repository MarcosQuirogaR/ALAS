// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Setup > Analyses: one place to choose which analysis disciplines a Run
//! performs, mirroring the reference desktop app's `AnalysesScreen`. The
//! toggles write the same `mission.enabled` / `mses.enabled` /
//! `structures.enabled` flags the Advanced Settings pages own.

use egui::{Frame, RichText, ScrollArea, Ui};
use serde_json::Value;

use crate::state::AppState;
use crate::views::tr;

/// Render the Analyses page.
pub fn show_analyses_view(state: &mut AppState, ui: &mut Ui) {
    ui.heading(tr("Analyses"));
    ui.label(
        RichText::new(tr(
            "Choose which analysis disciplines a Run performs. Everything is on by default; \
             disabling an optional discipline skips its pipeline stage. Fine-grained settings \
             live under Advanced Settings; external-tool paths under Setup > External Tools.",
        ))
        .weak(),
    );
    ui.add_space(6.0);

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.label(RichText::new(tr("Core (every run)")).strong());
            locked_row(
                ui,
                "Aerodynamics -- VLM + drag build-up",
                "Native vortex-lattice analysis, drag polar, span loading, V-n envelope and \
             dynamic modes of the optimized design. The pipeline's backbone; cannot be skipped.",
            );
            locked_row(
                ui,
                "Weight & Balance / Stability",
                "Selected mass method (compatibility fractions or architecture-dependent NASA \
             FLOPS), CG solve and envelope, static margin, landing-gear placement and \
             cabin/payload layout.",
            );
            locked_row(
            ui,
            "Propulsion cycle",
            "On-design turbofan cycle analysis of the selected engine -- cycle summary, carpet \
             plot, efficiency decomposition and sweeps on the Propulsion results tab.",
        );
            locked_row(
            ui,
            "Field performance",
            "V-speeds, balanced field length and landing distances for the selected departure/ \
             arrival airports -- the Matching Chart and Landing & Take-Off results tabs.",
        );

            ui.add_space(6.0);
            ui.label(RichText::new(tr("Optional (toggle per run)")).strong());
            toggle_row(
                state,
                ui,
                "mission",
                "Mission analysis -- native",
                "Flies the full route natively: fuel burn, flight profile, aero coefficient \
             histories and the 3D route globe. Degrades gracefully when the environment is \
             missing.",
            );
            toggle_row(
                state,
                ui,
                "mses",
                "2-D airfoil analysis -- MSES",
                "High-fidelity viscous/transonic polar and pressure/Mach-contour analysis of the \
             optimized root section. Sweep settings under Advanced Settings > MSES Analysis.",
            );
            toggle_row(
                state,
                ui,
                "structures",
                "Structures -- wingbox FEM",
                "Sizes a generic wingbox from strength requirements -- always available \
             analytically; add a NASTRAN path under External Tools for a real solve.",
            );
        });
}

fn locked_row(ui: &mut Ui, title: &str, desc: &str) {
    Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            let mut checked = true;
            ui.add_enabled(false, egui::Checkbox::new(&mut checked, ""));
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr(title)).strong());
                    ui.label(RichText::new(tr("always runs")).weak().small());
                });
                ui.label(RichText::new(tr(desc)).weak().small());
            });
        });
    });
}

fn toggle_row(state: &mut AppState, ui: &mut Ui, group: &str, title: &str, desc: &str) {
    Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            let mut enabled = state
                .config_values
                .get(group)
                .and_then(|g| g.get("enabled"))
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let toggled = ui.checkbox(&mut enabled, "").changed();
            ui.vertical(|ui| {
                ui.label(RichText::new(tr(title)).strong());
                ui.label(RichText::new(tr(desc)).weak().small());
            });
            if toggled {
                if let Some(obj) = state
                    .config_values
                    .get_mut(group)
                    .and_then(Value::as_object_mut)
                {
                    obj.insert("enabled".to_owned(), Value::Bool(enabled));
                }
                state.on_config_modified();
            }
        });
    });
}
