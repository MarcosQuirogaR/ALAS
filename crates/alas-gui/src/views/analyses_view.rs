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
    ui.horizontal_wrapped(|ui| {
        ui.heading(tr("Analyses"));
        if ui
            .button(tr("Open Airfoil CFD"))
            .on_hover_text(tr(
                "Open the standalone 2-D OpenFOAM study for any database airfoil.",
            ))
            .clicked()
        {
            state.cfd.window_open = true;
            state.cfd.tab = crate::cfd::CfdTab::Study;
        }
    });
    ui.label(
        RichText::new(tr(
            "Choose which analysis disciplines a Run performs. Everything is on by default; \
             disabling an optional discipline skips its pipeline stage. Fine-grained settings \
             live under Advanced Settings; external-tool paths under Advanced Settings > External Tools.",
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
                "Aerodynamics: VLM + drag build-up",
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
                "On-design turbofan cycle analysis of the selected engine: cycle summary, carpet \
             plot, efficiency decomposition and sweeps on the Propulsion results tab.",
            );
            locked_row(
            ui,
            "Field performance",
            "V-speeds, balanced field length and landing distances for the selected departure/ \
             arrival airports: the Matching Chart and Landing & Take-Off results tabs.",
        );

            ui.add_space(6.0);
            ui.label(RichText::new(tr("Optional (toggle per run)")).strong());
            toggle_row(
                state,
                ui,
                "mission",
                "Mission analysis (native)",
                "Flies the full route natively: fuel burn, flight profile, aero coefficient \
             histories and the 3D route globe. Degrades gracefully when the environment is \
             missing.",
            );
            toggle_row(
                state,
                ui,
                "mses",
                "2-D airfoil analysis (MSES)",
                "High-fidelity viscous/transonic polar and pressure/Mach-contour analysis of the \
             optimized root section. Sweep settings under Advanced Settings > MSES Analysis.",
            );
            toggle_row(
                state,
                ui,
                "structures",
                "Structures (wingbox FEM)",
                "Sizes a generic wingbox from strength requirements, always available \
             analytically; add a NASTRAN path under Advanced Settings > External Tools for a real solve.",
            );
            structures_case_rows(state, ui);
        });
}

/// Solver-case selection for the structural analysis, kept on Analyses next
/// to the discipline toggle; model settings stay under Modeling and Advanced
/// Settings > Structures.
fn structures_case_rows(state: &mut AppState, ui: &mut Ui) {
    let enabled = state
        .config_values
        .get("structures")
        .and_then(|g| g.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    ui.add_enabled_ui(enabled, |ui| {
        ui.indent("structures_cases", |ui| {
            ui.label(
                RichText::new(tr("Structural solver cases"))
                    .strong()
                    .small(),
            );
            for (name, label) in crate::views::form_page::placement::STRUCTURES_CASES {
                let mut checked = state
                    .config_values
                    .get("structures")
                    .and_then(|g| g.get(name))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if ui.checkbox(&mut checked, tr(label)).changed() {
                    if let Some(values) = state.config_values.get_mut("structures") {
                        crate::views::form_page::placement::toggle_bool(values, name, checked);
                    }
                    state.on_config_modified();
                }
            }
        });
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
