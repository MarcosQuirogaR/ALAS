// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Study tab: a compact section card with the searchable picker and a preview
//! sized to the section's own aspect ratio, then one dense routine-controls
//! card whose third column carries the derived condition, then the sweep and
//! case-file cards.

use super::drawing::{outline_aspect, paint_airfoil_outline};
use super::layout::{
    field, preview_height, scientific_field, show_groups, Group, FIELD_WIDTH, PREVIEW_PADDING,
};
use super::widgets::{card, card_title};
use crate::cfd::{AirfoilCfdState, CfdSweepVariable};
use crate::state::{AppState, LogKind};
use crate::views::{tr, tr_fields};
use alas_cfd::{
    CfdStudyConfig, FarFieldCondition, FarFieldTurbulenceCondition, FarFieldVelocityCondition,
    OperatingInput, RegimeAssessment, RegimeSeverity,
};
use egui::{vec2, ComboBox, DragValue, Grid, RichText, ScrollArea, Sense, TextEdit, Ui};

/// Routine controls: the operating point, the boundary state, and the
/// resulting effective condition, so inputs and their consequence stay in one
/// row instead of one card each.
const ROUTINE_GROUPS: &[Group<CfdStudyConfig>] = &[
    (
        "Operating point",
        "The section frame is chord +x, normal +y, and extrusion +z. Positive angle rotates the freestream toward +y; drag is along the freestream and lift is normal to it.",
        operating_point_group,
    ),
    (
        "Boundary conditions",
        "Freestream turbulence state and the far-field patch treatment used to initialize and close the case.",
        boundary_group,
    ),
    (
        "Effective condition",
        "Derived from the entered operating point; these are the values the case builder writes, not independent inputs. Mach below 0.3 selects simpleFoam. Mach at or above 0.3 selects rhoSimpleFoam with perfect-gas thermodynamics and bounded shock-safe schemes; the transonic band is supported numerically and still needs physical validation against evidence.",
        effective_condition_group,
    ),
];

pub(crate) fn show_study_tab(state: &mut AppState, ui: &mut Ui) {
    ScrollArea::vertical()
        .id_salt("airfoil_cfd_study_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            show_section_card(&mut state.cfd, ui);
            ui.add_space(8.0);
            show_study_card(state, ui);
            ui.add_space(8.0);
            show_sweep_card(state, ui);
            ui.add_space(8.0);
            show_case_files_card(state, ui);
        });
}

/// The searchable picker and the resolved outline in one compact card: the
/// label, its control and the geometry it selects stay adjacent.
fn show_section_card(cfd: &mut AirfoilCfdState, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr("Database airfoil")).strong().size(15.0))
                .on_hover_text(tr(
                    "Search and preview the exact library section used by this independent CFD study. Selecting it does not modify the aircraft configuration.",
                ));
            show_airfoil_picker(cfd, ui);
            ui.label(
                RichText::new(tr_fields(
                    "{count} matching sections",
                    &[("count", cfd.filtered_airfoils.len().to_string())],
                ))
                .weak()
                .small(),
            );
        });
        let width = ui.available_width().max(1.0);
        let aspect = outline_aspect(cfd.preview_coordinates.as_deref());
        let (rect, _) =
            ui.allocate_exact_size(vec2(width, preview_height(width, aspect)), Sense::hover());
        paint_airfoil_outline(ui, rect.shrink(PREVIEW_PADDING), cfd.preview_coordinates.as_deref());
        if cfd.preview_coordinates.is_none() {
            ui.colored_label(
                ui.visuals().error_fg_color,
                tr("The selected database coordinates are unavailable; the solver will not substitute another section."),
            );
        }
    });
}

/// A filter-in-place popover instead of a permanently open list: the full
/// database stays one click away without reserving a tall empty rectangle.
fn show_airfoil_picker(cfd: &mut AirfoilCfdState, ui: &mut Ui) {
    let selected = cfd.selected_airfoil().to_owned();
    let mut chosen = None;
    let row_height = ui
        .text_style_height(&egui::TextStyle::Button)
        .max(ui.spacing().interact_size.y);
    ComboBox::from_id_salt("airfoil_cfd_database_picker")
        .width(200.0)
        .selected_text(RichText::new(&selected).strong())
        .show_ui(ui, |ui| {
            ui.set_min_width(240.0);
            let mut filter = cfd.airfoil_filter.clone();
            if ui
                .add(
                    TextEdit::singleline(&mut filter)
                        .hint_text(tr("Filter airfoil names"))
                        .desired_width(228.0),
                )
                .changed()
            {
                cfd.set_airfoil_filter(filter);
            }
            ui.separator();
            if cfd.filtered_airfoils.is_empty() {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    tr("No database airfoils match the filter."),
                );
                return;
            }
            ScrollArea::vertical()
                .id_salt("airfoil_cfd_database_rows")
                .max_height(260.0)
                .show_rows(ui, row_height, cfd.filtered_airfoils.len(), |ui, range| {
                    for index in range {
                        let name = &cfd.filtered_airfoils[index];
                        if ui.selectable_label(selected == *name, name).clicked() {
                            chosen = Some(name.clone());
                        }
                    }
                });
        });
    if let Some(name) = chosen {
        cfd.select_airfoil(&name);
    }
}

/// One card, one dense responsive row of control groups.
fn show_study_card(state: &mut AppState, ui: &mut Ui) {
    let before = state.cfd.config.clone();
    card(
        ui,
        "Routine study controls",
        "The section frame is chord +x, normal +y, and extrusion +z. Positive angle rotates the freestream toward +y; drag is along the freestream and lift is normal to it.",
        |ui| {
            show_groups(ui, &mut state.cfd.config, ROUTINE_GROUPS);
            ui.add_space(4.0);
            show_regime_flags(&alas_cfd::assess_regime(&state.cfd.config), ui);
        },
    );
    if state.cfd.config != before {
        state.cfd.mark_inputs_changed();
    }
}

fn operating_point_group(ui: &mut Ui, config: &mut CfdStudyConfig) {
    Grid::new("airfoil_cfd_operating_grid")
        .num_columns(2)
        .spacing([10.0, 5.0])
        .show(ui, |ui| {
            row_label(ui, "Chord [m]", "Reference chord used for geometry, Reynolds number and coefficients.");
            field(ui, DragValue::new(&mut config.chord_m).speed(0.01).range(0.01..=100.0));
            ui.end_row();
            row_label(ui, "Angle of attack [deg]", "Geometric angle used to rotate the freestream and wind-axis coefficient directions.");
            field(ui, DragValue::new(&mut config.angle_of_attack_deg).speed(0.1).range(-30.0..=30.0));
            ui.end_row();
            row_label(ui, "Operating input", "Choose whether speed or chord Reynolds number is entered directly; the other is derived using Re = rho U c / mu.");
            ComboBox::from_id_salt("airfoil_cfd_operating_input")
                .width(FIELD_WIDTH)
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
                    row_label(ui, "Freestream speed [m/s]", "Direct input. Reynolds number is derived from density, speed, chord and dynamic viscosity.");
                    field(ui, DragValue::new(&mut config.speed_m_s).speed(0.5).range(0.1..=700.0));
                }
                OperatingInput::Reynolds => {
                    row_label(ui, "Reynolds number [-]", "Direct chord-based Reynolds number. Speed is derived from Re = rho U c / mu.");
                    field(ui, DragValue::new(&mut config.reynolds).speed(1.0e4).range(1.0e3..=1.0e9));
                }
            }
            ui.end_row();
            row_label(ui, "Density [kg/m\u{00b3}]", "Freestream density used for dynamic pressure and force coefficient normalization.");
            field(ui, DragValue::new(&mut config.density_kg_m3).speed(0.001).range(0.001..=20.0));
            ui.end_row();
            row_label(ui, "Dynamic viscosity [Pa\u{00b7}s]", "Dynamic viscosity used to derive kinematic viscosity and Reynolds number.");
            scientific_field(ui, DragValue::new(&mut config.dynamic_viscosity_pa_s).speed(1.0e-7).range(1.0e-8..=1.0));
            ui.end_row();
            row_label(ui, "Static temperature [K]", "Sets the sound speed a = sqrt(gamma R T) and therefore the Mach number M = U/a. Mach drives automatic solver and scheme selection; dry-air gamma = 1.4 and R = 287.05287 J/(kg K).");
            field(ui, DragValue::new(&mut config.freestream_temperature_k).speed(0.5).range(100.0..=2_000.0));
            ui.end_row();
        });
}

fn boundary_group(ui: &mut Ui, config: &mut CfdStudyConfig) {
    Grid::new("airfoil_cfd_boundary_state_grid")
        .num_columns(2)
        .spacing([10.0, 5.0])
        .show(ui, |ui| {
            row_label(ui, "Turbulence intensity [%]", "Freestream turbulence intensity used to initialize k and omega for the selected SST model.");
            let mut turbulence_percent = config.turbulence_intensity * 100.0;
            if field(ui, DragValue::new(&mut turbulence_percent).speed(0.1).range(0.0..=30.0)).changed() {
                config.turbulence_intensity = turbulence_percent / 100.0;
            }
            ui.end_row();
            row_label(ui, "Turbulent length scale [m]", "Length scale used to initialize the turbulence frequency.");
            field(ui, DragValue::new(&mut config.turbulence_length_m).speed(0.001).range(1.0e-5..=100.0));
            ui.end_row();
            row_label(ui, "Turbulence model", "The steady k-omega SST model is used in both paths; rhoSimpleFoam adds perfect-gas density and energy equations when Mach requires compressibility.");
            ui.label(RichText::new(&config.turbulence_model).monospace());
            ui.end_row();
            row_label(ui, "Far-field condition", "Boundary condition applied to the reusable far-field patch.");
            ComboBox::from_id_salt("airfoil_cfd_far_field")
                .width(FIELD_WIDTH)
                .selected_text(match config.boundaries.far_field {
                    FarFieldCondition::FixedValue => tr("Fixed velocity"),
                    FarFieldCondition::Freestream => tr("Freestream mixed"),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut config.boundaries.far_field, FarFieldCondition::FixedValue, tr("Fixed velocity"));
                    ui.selectable_value(&mut config.boundaries.far_field, FarFieldCondition::Freestream, tr("Freestream mixed"));
                });
            ui.end_row();
            // Both sub-conditions are documented as applying only while the
            // far field is `FixedValue`; disabling them there is what keeps the
            // form honest about when the choice has any effect.
            let sub_conditions_apply = config.boundaries.far_field == FarFieldCondition::FixedValue;
            row_label(ui, "Far-field turbulence", "Treatment of k and omega on the outer patch while the far field is Fixed velocity. inletOutlet imposes the freestream state on inflow and convects the interior value out on outflow; fixedValue clamps every outer face, including outflow.");
            ui.add_enabled_ui(sub_conditions_apply, |ui| {
                ComboBox::from_id_salt("airfoil_cfd_far_field_turbulence")
                    .width(FIELD_WIDTH)
                    .selected_text(tr(config.boundaries.far_field_turbulence.as_str()))
                    .show_ui(ui, |ui| {
                        for condition in [
                            FarFieldTurbulenceCondition::InletOutlet,
                            FarFieldTurbulenceCondition::FixedValue,
                        ] {
                            ui.selectable_value(
                                &mut config.boundaries.far_field_turbulence,
                                condition,
                                tr(condition.as_str()),
                            );
                        }
                    });
            });
            ui.end_row();
            row_label(ui, "Far-field velocity", "Treatment of U on the outer patch while the far field is Fixed velocity. freestream is inletOutlet for velocity: the freestream vector on inflow, zero gradient on outflow. The pressure treatment is untouched either way.");
            ui.add_enabled_ui(sub_conditions_apply, |ui| {
                ComboBox::from_id_salt("airfoil_cfd_far_field_velocity")
                    .width(FIELD_WIDTH)
                    .selected_text(tr(config.boundaries.far_field_velocity.as_str()))
                    .show_ui(ui, |ui| {
                        for condition in [
                            FarFieldVelocityCondition::FixedValue,
                            FarFieldVelocityCondition::Freestream,
                        ] {
                            ui.selectable_value(
                                &mut config.boundaries.far_field_velocity,
                                condition,
                                tr(condition.as_str()),
                            );
                        }
                    });
            });
            ui.end_row();
            row_label(ui, "Pressure reference [Pa]", "Positive values set the absolute static pressure for rhoSimpleFoam. Zero retains the incompressible gauge convention and derives a positive perfect-gas pressure from rho R T when the compressible path is selected.");
            field(ui, DragValue::new(&mut config.boundaries.pressure_reference_pa).speed(1.0).range(0.0..=1.0e6));
            ui.end_row();
        });
}

/// The resolved flow state for the entered inputs, read-only.
fn effective_condition_group(ui: &mut Ui, config: &mut CfdStudyConfig) {
    Grid::new("airfoil_cfd_derived_grid")
        .num_columns(2)
        .spacing([10.0, 5.0])
        .show(ui, |ui| {
            for (label, value) in [
                ("U [m/s]", format!("{:.4}", config.effective_speed_m_s())),
                ("Re [-]", format!("{:.4e}", config.effective_reynolds())),
                ("Mach [-]", format!("{:.4}", config.mach_number())),
                ("a [m/s]", format!("{:.2}", config.speed_of_sound_m_s())),
                ("T [K]", format!("{:.2}", config.freestream_temperature_k)),
                (
                    "Regime",
                    format!(
                        "{} / {}",
                        config.flow_regime().as_str(),
                        config.solver_kind().executable()
                    ),
                ),
                (
                    "Static pressure [Pa]",
                    format!("{:.2}", config.effective_static_pressure_pa()),
                ),
            ] {
                ui.label(RichText::new(tr(label)).weak());
                ui.monospace(value);
                ui.end_row();
            }
        });
}

fn row_label(ui: &mut Ui, label: &str, help: &str) {
    ui.label(tr(label)).on_hover_text(tr(help));
}

/// Blocking limits and advisory cautions for the effective regime.  Blocking
/// entries mirror `validate`; cautions never stop a launch.  Neither is ever
/// folded away to save space.
pub(crate) fn show_regime_flags(regime: &RegimeAssessment, ui: &mut Ui) {
    if regime.flags.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr("Regime assessment")).strong().small());
            ui.label(
                RichText::new(tr("No blocking limits or cautions for the current inputs."))
                    .weak()
                    .small(),
            );
        });
        return;
    }
    ui.label(RichText::new(tr("Regime assessment")).strong().small());
    for flag in &regime.flags {
        let (color, prefix) = match flag.severity {
            RegimeSeverity::Blocking => (ui.visuals().error_fg_color, tr("blocking")),
            RegimeSeverity::Caution => (ui.visuals().warn_fg_color, tr("caution")),
        };
        ui.colored_label(color, format!("{prefix}: {}", flag.message));
    }
}

/// Persisted study settings and the reproducible case folder, kept on one row.
fn show_case_files_card(state: &mut AppState, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr("Case settings")).strong().size(15.0));
            if ui.button(tr("Save case settings")).clicked() {
                match state.cfd.save_study() {
                    Ok(path) => state.log(
                        tr_fields(
                            "CFD study settings saved to {path}.",
                            &[("path", path.display().to_string())],
                        ),
                        LogKind::Info,
                    ),
                    Err(error) => state.log(error, LogKind::Error),
                }
            }
            if ui.button(tr("Reload case settings")).clicked() {
                match state.cfd.reload_study() {
                    Ok(path) => state.log(
                        tr_fields(
                            "CFD study settings reloaded from {path}.",
                            &[("path", path.display().to_string())],
                        ),
                        LogKind::Info,
                    ),
                    Err(error) => state.log(error, LogKind::Error),
                }
            }
            if let Some(path) = state.cfd.last_case_dir.clone() {
                ui.separator();
                ui.label(RichText::new(tr("Last isolated case")).weak().small());
                ui.label(
                    RichText::new(path.display().to_string())
                        .monospace()
                        .small(),
                )
                .on_hover_text(path.display().to_string());
                if ui.small_button(tr("Copy path")).clicked() {
                    ui.ctx().copy_text(path.display().to_string());
                }
            }
        });
    });
}

fn show_sweep_card(state: &mut AppState, ui: &mut Ui) {
    let before = state.cfd.sweep_settings.clone();
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        card_title(
            ui,
            "Sequential AoA / Reynolds sweep",
            "Run one isolated OpenFOAM case per point after single-case verification. Each point keeps its own inputs, status, case path and numerical evidence.",
        );
        let sweep = &mut state.cfd.sweep_settings;
        let speed = if sweep.variable == CfdSweepVariable::Reynolds { 1.0e4 } else { 0.5 };
        let unit = sweep.variable.unit().to_owned();
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr("Sweep variable")).weak());
            ComboBox::from_id_salt("airfoil_cfd_sweep_variable")
                .width(150.0)
                .selected_text(tr(sweep.variable.label()))
                .show_ui(ui, |ui| {
                    for variable in [CfdSweepVariable::AngleOfAttack, CfdSweepVariable::Reynolds] {
                        ui.selectable_value(&mut sweep.variable, variable, tr(variable.label()));
                    }
                });
            ui.separator();
            for (label, value) in [
                ("Start [{unit}]", &mut sweep.start),
                ("End [{unit}]", &mut sweep.end),
                ("Step [{unit}]", &mut sweep.step),
            ] {
                ui.label(RichText::new(tr_fields(label, &[("unit", unit.clone())])).weak());
                field(ui, DragValue::new(value).speed(speed));
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!state.cfd.running, egui::Button::new(tr("Run sweep")))
                .on_hover_text(tr("Run the configured points sequentially in isolated case directories."))
                .clicked()
            {
                match state.cfd.start_sweep() {
                    Ok(run_id) => state.log(format!("Airfoil CFD sweep #{run_id} started."), LogKind::Info),
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
            ui.separator();
            match state.cfd.sweep_settings.values() {
                Ok(values) => {
                    ui.label(
                        RichText::new(tr_fields(
                            "{count} sequential cases will be generated.",
                            &[("count", values.len().to_string())],
                        ))
                        .weak()
                        .small(),
                    );
                }
                Err(error) => {
                    ui.colored_label(ui.visuals().error_fg_color, tr(&error));
                }
            }
        });
    });
    if state.cfd.sweep_settings != before {
        state.cfd.mark_inputs_changed();
    }
}
