// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Setup > Analyses: one place to choose which analysis disciplines a Run
//! performs, mirroring the reference desktop app's `AnalysesScreen`. The
//! toggles write the same configuration flags the analysis stages own.

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
            structures_external_rows(state, ui);
            ui.add_space(6.0);
            ui.label(RichText::new(tr("External downstream tools")).strong().small());
            toggle_field_row(
                state,
                ui,
                "downstream",
                "openvsp",
                "OpenVSP geometry export",
                "Writes an inspectable OpenVSP script and, when OpenVSP is configured, materializes the \
             .vsp3 geometry and CAD preview. This export is required by VSPAERO.",
            );
            let openvsp_enabled = state
                .config_values
                .get("downstream")
                .and_then(|g| g.get("openvsp"))
                .and_then(Value::as_bool)
                .unwrap_or(true);
            ui.add_enabled_ui(openvsp_enabled, |ui| {
                toggle_field_row(
                    state,
                    ui,
                    "downstream",
                    "vspaero",
                    "VSPAERO analysis",
                    "Runs OpenVSP's independent 3-D vortex-lattice comparison from the exported geometry. \
                 Requires a configured VSPAERO executable.",
                );
            });
            toggle_field_row(
                state,
                ui,
                "downstream",
                "avl",
                "AVL comparison",
                "Runs Athena Vortex Lattice's take-off sweep and retains its SI deck for comparison. \
             The Aerodynamic results selector under Advanced Settings must include AVL.",
            );
            toggle_field_row(
                state,
                ui,
                "downstream",
                "flowunsteady",
                "FLOWUnsteady analysis",
                "Runs the FLOWUnsteady adapter and retains its request, result and solver logs. Set \
             ALAS_FLOWUNSTEADY_EXE before enabling a real external solve.",
            );
            ui.add_enabled_ui(state.run_options.optimize, |ui| {
                run_option_row(
                    state,
                    ui,
                    "Baseline comparison",
                    "Re-evaluates the baseline aircraft alongside the current design, adding a direct \
                 reference to the results without changing the optimizer's selected design.",
                );
            });
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

/// External structural stages whose prerequisites are selected alongside the
/// structural solver cases rather than hidden on the tool-path page.
fn structures_external_rows(state: &mut AppState, ui: &mut Ui) {
    let structures_enabled = state
        .config_values
        .get("structures")
        .and_then(|g| g.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    ui.add_enabled_ui(structures_enabled, |ui| {
        ui.indent("structures_external", |ui| {
            ui.label(
                RichText::new(tr("Structural external stages"))
                    .strong()
                    .small(),
            );
            let mut run_nastran = structure_bool(state, "run_nastran");
            if ui
                .checkbox(
                    &mut run_nastran,
                    tr("NASTRAN solve / NASTRAN-95 comparison"),
                )
                .changed()
            {
                set_structure_bool(state, "run_nastran", run_nastran);
            }
            let run_static = structure_bool(state, "run_sol_static");
            ui.add_enabled_ui(run_nastran && run_static, |ui| {
                let mut run_patran = structure_bool(state, "run_patran_export");
                if ui
                    .checkbox(&mut run_patran, tr("Patran deformation export"))
                    .changed()
                {
                    set_structure_bool(state, "run_patran_export", run_patran);
                }
            });
        });
    });
}

fn structure_bool(state: &AppState, name: &str) -> bool {
    state
        .config_values
        .get("structures")
        .and_then(|group| group.get(name))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn set_structure_bool(state: &mut AppState, name: &str, enabled: bool) {
    if let Some(values) = state.config_values.get_mut("structures") {
        crate::views::form_page::placement::toggle_bool(values, name, enabled);
    }
    state.on_config_modified();
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
    toggle_field_row(state, ui, group, "enabled", title, desc);
}

fn toggle_field_row(
    state: &mut AppState,
    ui: &mut Ui,
    group: &str,
    field: &str,
    title: &str,
    desc: &str,
) {
    Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            let mut enabled = state
                .config_values
                .get(group)
                .and_then(|g| g.get(field))
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
                    obj.insert(field.to_owned(), Value::Bool(enabled));
                }
                state.on_config_modified();
            }
        });
    });
}

/// A transient Run option that belongs alongside the optional downstream
/// disciplines but is not part of the persistent aircraft configuration.
fn run_option_row(state: &mut AppState, ui: &mut Ui, title: &str, desc: &str) {
    Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.checkbox(&mut state.run_options.compare_baseline, "");
            ui.vertical(|ui| {
                ui.label(RichText::new(tr(title)).strong());
                ui.label(RichText::new(tr(desc)).weak().small());
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::show_analyses_view;
    use crate::state::AppState;

    #[test]
    fn optional_panel_lists_every_external_downstream_stage() {
        let context = egui::Context::default();
        let mut state = AppState::default();
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 1_400.0),
                )),
                ..egui::RawInput::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    show_analyses_view(&mut state, ui);
                });
            },
        );
        let labels = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        for label in [
            "OpenVSP geometry export",
            "VSPAERO analysis",
            "AVL comparison",
            "FLOWUnsteady analysis",
            "Baseline comparison",
            "NASTRAN solve / NASTRAN-95 comparison",
            "Patran deformation export",
        ] {
            assert!(labels.contains(&label), "missing optional stage: {label}");
        }
    }
}
