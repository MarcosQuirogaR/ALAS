// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Study tab: the resolved airfoil preview on top, then one single-column
//! card holding the database selector and the routine study controls, then
//! the sweep and action cards.

use super::drawing::label_with_help;
use super::drawing::paint_airfoil_outline;
use super::layout::{preview_height, show_groups, Group};
use crate::cfd::{AirfoilCfdState, CfdSweepVariable};
use crate::state::{AppState, LogKind};
use crate::views::{tr, tr_fields};
use alas_cfd::{
    CfdStudyConfig, FarFieldCondition, OperatingInput, RegimeAssessment, RegimeSeverity,
};
use egui::{vec2, ComboBox, DragValue, Grid, RichText, ScrollArea, Sense, Ui};

/// Routine controls, grouped: the operating point and the boundary state.
const ROUTINE_GROUPS: &[Group<CfdStudyConfig>] = &[
    ("Operating point", operating_point_group),
    ("Boundary conditions", boundary_group),
];

pub(crate) fn show_study_tab(state: &mut AppState, ui: &mut Ui) {
    ScrollArea::vertical()
        .id_salt("airfoil_cfd_study_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            show_preview_card(&state.cfd, ui);
            ui.add_space(8.0);
            show_study_card(state, ui);
            ui.add_space(8.0);
            show_sweep_card(state, ui);
            ui.add_space(8.0);
            show_study_actions(state, ui);
        });
}

/// The exact resolved section, full width at the top of the tab.
fn show_preview_card(cfd: &AirfoilCfdState, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr("Resolved outline")).strong().size(16.0));
            ui.label(RichText::new(cfd.selected_airfoil()).weak());
        });
        let width = ui.available_width().max(1.0);
        let (rect, _) = ui.allocate_exact_size(vec2(width, preview_height(width)), Sense::hover());
        paint_airfoil_outline(ui, rect.shrink(12.0), cfd.preview_coordinates.as_deref());
        if cfd.preview_coordinates.is_none() {
            ui.colored_label(
                ui.visuals().error_fg_color,
                tr("The selected database coordinates are unavailable; the solver will not substitute another section."),
            );
        }
    });
}

/// One card, one column: the database selector above the routine controls.
fn show_study_card(state: &mut AppState, ui: &mut Ui) {
    let before = state.cfd.config.clone();
    crate::theme::card_frame(ui).show(ui, |ui| {
        show_airfoil_selector(&mut state.cfd, ui);
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);
        show_routine_controls(&mut state.cfd.config, ui);
    });
    if state.cfd.config != before {
        state.cfd.mark_inputs_changed();
    }
}

fn show_airfoil_selector(cfd: &mut AirfoilCfdState, ui: &mut Ui) {
    ui.label(RichText::new(tr("Database airfoil")).strong().size(16.0));
    ui.label(
        RichText::new(tr(
            "Search and preview the exact library section used by this independent CFD study. Selecting it does not modify the aircraft configuration.",
        ))
        .weak()
        .small(),
    );
    let mut filter = cfd.airfoil_filter.clone();
    if ui
        .add(
            egui::TextEdit::singleline(&mut filter)
                .hint_text(tr("Filter airfoil names"))
                .desired_width(ui.available_width()),
        )
        .changed()
    {
        cfd.set_airfoil_filter(filter);
    }
    ui.add_space(4.0);
    let mut chosen = None;
    let row_height = ui
        .text_style_height(&egui::TextStyle::Button)
        .max(ui.spacing().interact_size.y);
    ScrollArea::vertical()
        .id_salt("airfoil_cfd_database_rows")
        .max_height(160.0)
        .show_rows(ui, row_height, cfd.filtered_airfoils.len(), |ui, range| {
            for index in range {
                let name = &cfd.filtered_airfoils[index];
                if ui
                    .selectable_label(cfd.selected_airfoil() == name, name)
                    .clicked()
                {
                    chosen = Some(name.clone());
                }
            }
        });
    if let Some(name) = chosen {
        cfd.select_airfoil(&name);
    }
    if cfd.filtered_airfoils.is_empty() {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            tr("No database airfoils match the filter."),
        );
    }
}

fn show_routine_controls(config: &mut CfdStudyConfig, ui: &mut Ui) {
    ui.label(
        RichText::new(tr("Routine study controls"))
            .strong()
            .size(16.0),
    );
    ui.label(
        RichText::new(tr(
            "The section frame is chord +x, normal +y, and extrusion +z. Positive angle rotates the freestream toward +y; drag is along the freestream and lift is normal to it.",
        ))
        .weak()
        .small(),
    );
    ui.add_space(4.0);
    show_groups(ui, config, ROUTINE_GROUPS);
    ui.add_space(4.0);
    ui.separator();
    ui.label(RichText::new(tr("Derived effective condition")).strong());
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("U = {:.4} m/s", config.effective_speed_m_s()));
        ui.separator();
        ui.label(format!("Re = {:.4e}", config.effective_reynolds()));
        ui.separator();
        ui.label(format!("T = {:.2} K", config.freestream_temperature_k));
        ui.separator();
        ui.label(format!("a = {:.3} m/s", config.speed_of_sound_m_s()));
        ui.separator();
        ui.label(format!("Mach = {:.4}", config.mach_number()));
    });
    show_regime_flags(&alas_cfd::assess_regime(config), ui);
    ui.label(
        RichText::new(tr(
            "The initial steady incompressible SST template is intended for attached or mildly separated flow in its supported regime. Transition-sensitive, laminar, compressible, stall and unsteady cases are rejected or require a validated model.",
        ))
        .weak()
        .small(),
    );
}

fn operating_point_group(ui: &mut Ui, config: &mut CfdStudyConfig) {
    Grid::new("airfoil_cfd_operating_grid")
        .num_columns(2)
        .spacing([12.0, 5.0])
        .show(ui, |ui| {
            label_with_help(ui, "Chord [m]", "Reference chord used for geometry, Reynolds number and coefficients.");
            ui.add(DragValue::new(&mut config.chord_m).speed(0.01).range(0.01..=100.0));
            ui.end_row();
            label_with_help(ui, "Angle of attack [deg]", "Geometric angle used to rotate the freestream and wind-axis coefficient directions.");
            ui.add(DragValue::new(&mut config.angle_of_attack_deg).speed(0.1).range(-30.0..=30.0));
            ui.end_row();
            label_with_help(ui, "Independent operating input", "Choose whether speed or chord Reynolds number is entered directly; the other is derived using Re = rho U c / mu.");
            ComboBox::from_id_salt("airfoil_cfd_operating_input")
                .selected_text(match config.operating_input {
                    OperatingInput::Speed => tr("Speed"),
                    OperatingInput::Reynolds => tr("Reynolds number"),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut config.operating_input, OperatingInput::Speed, tr("Speed"));
                    ui.selectable_value(&mut config.operating_input, OperatingInput::Reynolds, tr("Reynolds number"));
                });
            ui.end_row();
            match config.operating_input {
                OperatingInput::Speed => {
                    label_with_help(ui, "Freestream speed [m/s]", "Direct input. Reynolds number is derived from density, speed, chord and dynamic viscosity.");
                    ui.add(DragValue::new(&mut config.speed_m_s).speed(0.5).range(0.1..=100.0));
                }
                OperatingInput::Reynolds => {
                    label_with_help(ui, "Reynolds number [-]", "Direct chord-based Reynolds number. Speed is derived from Re = rho U c / mu.");
                    ui.add(DragValue::new(&mut config.reynolds).speed(1.0e4).range(1.0e3..=1.0e9));
                }
            }
            ui.end_row();
            label_with_help(ui, "Density [kg/m\u{00b3}]", "Freestream density used for dynamic pressure and force coefficient normalization.");
            ui.add(DragValue::new(&mut config.density_kg_m3).speed(0.001).range(0.001..=20.0));
            ui.end_row();
            label_with_help(ui, "Dynamic viscosity [Pa\u{00b7}s]", "Dynamic viscosity used to derive kinematic viscosity and Reynolds number.");
            ui.add(DragValue::new(&mut config.dynamic_viscosity_pa_s).speed(1.0e-7).range(1.0e-8..=1.0));
            ui.end_row();
            label_with_help(ui, "Static temperature [K]", "Used only for the Mach diagnostic: a = sqrt(gamma R T), M = U/a with dry-air gamma = 1.4 and R = 287.05287 J/(kg K). The present solver remains incompressible.");
            ui.add(DragValue::new(&mut config.freestream_temperature_k).speed(0.5).range(100.0..=2_000.0));
            ui.end_row();
        });
}

fn boundary_group(ui: &mut Ui, config: &mut CfdStudyConfig) {
    Grid::new("airfoil_cfd_boundary_state_grid")
        .num_columns(2)
        .spacing([12.0, 5.0])
        .show(ui, |ui| {
            label_with_help(ui, "Turbulence intensity [%]", "Freestream turbulence intensity used to initialize k and omega for the selected SST model.");
            let mut turbulence_percent = config.turbulence_intensity * 100.0;
            if ui.add(DragValue::new(&mut turbulence_percent).speed(0.1).range(0.0..=30.0)).changed() {
                config.turbulence_intensity = turbulence_percent / 100.0;
            }
            ui.end_row();
            label_with_help(ui, "Turbulent length scale [m]", "Length scale used to initialize the turbulence frequency.");
            ui.add(DragValue::new(&mut config.turbulence_length_m).speed(0.001).range(1.0e-5..=100.0));
            ui.end_row();
            label_with_help(ui, "Turbulence model", "The initial validated template supports incompressible steady k-omega SST only.");
            ui.label(RichText::new(&config.turbulence_model).weak());
            ui.end_row();
            label_with_help(ui, "Far-field condition", "Boundary condition applied to the reusable far-field patch.");
            ComboBox::from_id_salt("airfoil_cfd_far_field")
                .selected_text(match config.boundaries.far_field {
                    FarFieldCondition::FixedValue => tr("Fixed velocity"),
                    FarFieldCondition::Freestream => tr("Freestream mixed"),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut config.boundaries.far_field, FarFieldCondition::FixedValue, tr("Fixed velocity"));
                    ui.selectable_value(&mut config.boundaries.far_field, FarFieldCondition::Freestream, tr("Freestream mixed"));
                });
            ui.end_row();
            label_with_help(ui, "Pressure reference [Pa]", "Physical static pressure reference. The incompressible p field stores p/rho internally.");
            ui.add(DragValue::new(&mut config.boundaries.pressure_reference_pa).speed(1.0).range(-1.0e6..=1.0e6));
            ui.end_row();
        });
}

/// Blocking limits and advisory cautions for the effective regime.  Blocking
/// entries mirror `validate`; cautions never stop a launch.
pub(crate) fn show_regime_flags(regime: &RegimeAssessment, ui: &mut Ui) {
    ui.label(RichText::new(tr("Regime assessment")).strong());
    if regime.flags.is_empty() {
        ui.label(
            RichText::new(tr("No blocking limits or cautions for the current inputs."))
                .weak()
                .small(),
        );
        return;
    }
    for flag in &regime.flags {
        let (color, prefix) = match flag.severity {
            RegimeSeverity::Blocking => (ui.visuals().error_fg_color, tr("blocking")),
            RegimeSeverity::Caution => (ui.visuals().warn_fg_color, tr("caution")),
        };
        ui.colored_label(color, format!("{prefix}: {}", flag.message));
    }
}

fn show_study_actions(state: &mut AppState, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            let running = state.cfd.running;
            if ui
                .add_enabled(!running, egui::Button::new(tr("Run CFD study")))
                .on_hover_text(tr("Prepare an isolated case, generate/check the mesh, solve, and collect actual OpenFOAM results in the background."))
                .clicked()
            {
                match state.cfd.start_run() {
                    Ok(run_id) => state.log(format!("Airfoil CFD run #{run_id} started."), LogKind::Info),
                    Err(error) => state.log(error, LogKind::Error),
                }
            }
            if ui
                .add_enabled(running, egui::Button::new(tr("Cancel CFD")))
                .clicked()
            {
                state.cfd.cancel_run();
                state.log("Airfoil CFD cancellation requested.", LogKind::Warn);
            }
            if ui.button(tr("Save case settings")).clicked() {
                match state.cfd.save_study() {
                    Ok(path) => state.log(
                        tr_fields("CFD study settings saved to {path}.", &[("path", path.display().to_string())]),
                        LogKind::Info,
                    ),
                    Err(error) => state.log(error, LogKind::Error),
                }
            }
            if ui.button(tr("Reload case settings")).clicked() {
                match state.cfd.reload_study() {
                    Ok(path) => state.log(
                        tr_fields("CFD study settings reloaded from {path}.", &[("path", path.display().to_string())]),
                        LogKind::Info,
                    ),
                    Err(error) => state.log(error, LogKind::Error),
                }
            }
            if state.cfd.probing {
                ui.spinner();
            }
            ui.label(RichText::new(tr(&state.cfd.status)).weak());
        });
        if let Some(path) = state.cfd.last_case_dir.as_ref() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(tr("Last isolated case")).strong());
                ui.label(path.display().to_string());
                if ui.small_button(tr("Copy path")).clicked() {
                    ui.ctx().copy_text(path.display().to_string());
                }
            });
        }
    });
}

fn show_sweep_card(state: &mut AppState, ui: &mut Ui) {
    let before = state.cfd.sweep_settings.clone();
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(RichText::new(tr("Sequential AoA / Reynolds sweep")).strong().size(16.0));
        ui.label(
            RichText::new(tr(
                "Run one isolated OpenFOAM case per point after single-case verification. Each point keeps its own inputs, status, case path and numerical evidence.",
            ))
            .weak()
            .small(),
        );
        let sweep = &mut state.cfd.sweep_settings;
        Grid::new("airfoil_cfd_sweep_grid")
            .num_columns(2)
            .spacing([12.0, 5.0])
            .show(ui, |ui| {
                ui.label(tr("Sweep variable"));
                ComboBox::from_id_salt("airfoil_cfd_sweep_variable")
                    .selected_text(tr(sweep.variable.label()))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut sweep.variable,
                            CfdSweepVariable::AngleOfAttack,
                            tr(CfdSweepVariable::AngleOfAttack.label()),
                        );
                        ui.selectable_value(
                            &mut sweep.variable,
                            CfdSweepVariable::Reynolds,
                            tr(CfdSweepVariable::Reynolds.label()),
                        );
                    });
                ui.end_row();
                ui.label(tr_fields(
                    "Start [{unit}]",
                    &[("unit", sweep.variable.unit().to_owned())],
                ));
                ui.add(DragValue::new(&mut sweep.start).speed(if sweep.variable == CfdSweepVariable::Reynolds { 1.0e4 } else { 0.5 }));
                ui.end_row();
                ui.label(tr_fields(
                    "End [{unit}]",
                    &[("unit", sweep.variable.unit().to_owned())],
                ));
                ui.add(DragValue::new(&mut sweep.end).speed(if sweep.variable == CfdSweepVariable::Reynolds { 1.0e4 } else { 0.5 }));
                ui.end_row();
                ui.label(tr_fields(
                    "Step [{unit}]",
                    &[("unit", sweep.variable.unit().to_owned())],
                ));
                ui.add(DragValue::new(&mut sweep.step).speed(if sweep.variable == CfdSweepVariable::Reynolds { 1.0e4 } else { 0.5 }));
                ui.end_row();
            });
        match sweep.values() {
            Ok(values) => {
                ui.label(tr_fields(
                    "{count} sequential cases will be generated.",
                    &[("count", values.len().to_string())],
                ));
            }
            Err(error) => {
                ui.colored_label(ui.visuals().error_fg_color, tr(&error));
            }
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !state.cfd.running,
                    egui::Button::new(tr("Run sweep")),
                )
                .on_hover_text(tr("Run the configured points sequentially in isolated case directories."))
                .clicked()
            {
                match state.cfd.start_sweep() {
                    Ok(run_id) => state.log(
                        format!("Airfoil CFD sweep #{run_id} started."),
                        LogKind::Info,
                    ),
                    Err(error) => state.log(error, LogKind::Error),
                }
            }
            if ui
                .add_enabled(
                    state.cfd.running && state.cfd.sweep_running,
                    egui::Button::new(tr("Cancel sweep")),
                )
                .clicked()
            {
                state.cfd.cancel_run();
                state.log("Airfoil CFD sweep cancellation requested.", LogKind::Warn);
            }
            if state.cfd.sweep_running {
                ui.spinner();
            }
        });
    });
    if state.cfd.sweep_settings != before {
        state.cfd.mark_inputs_changed();
    }
}
