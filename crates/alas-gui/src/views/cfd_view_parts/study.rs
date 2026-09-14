// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Routine and advanced study controls.

use super::super::*;
use super::drawing::label_with_help;
use super::drawing::paint_airfoil_outline;
use crate::cfd::{AirfoilCfdState, CfdSweepVariable};
use crate::state::{AppState, LogKind};
use crate::views::{tr, tr_fields};
use alas_cfd::{CfdStudyConfig, FarFieldCondition, MeshPreset, OperatingInput};
use egui::{ComboBox, DragValue, Grid, RichText, ScrollArea, Sense, Ui};

pub(crate) fn show_study_tab(state: &mut AppState, ui: &mut Ui) {
    ScrollArea::vertical()
        .id_salt("airfoil_cfd_study_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if ui.available_width() >= 760.0 {
                ui.columns(2, |columns| {
                    show_airfoil_selector(&mut state.cfd, &mut columns[0]);
                    show_routine_card(state, &mut columns[1]);
                });
            } else {
                show_airfoil_selector(&mut state.cfd, ui);
                ui.add_space(8.0);
                show_routine_card(state, ui);
            }
            ui.add_space(8.0);
            show_sweep_card(state, ui);
            ui.add_space(8.0);
            show_study_actions(state, ui);
        });
}

fn show_airfoil_selector(cfd: &mut AirfoilCfdState, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
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
        let row_height = ui.text_style_height(&egui::TextStyle::Button)
            .max(ui.spacing().interact_size.y);
        ScrollArea::vertical()
            .id_salt("airfoil_cfd_database_rows")
            .max_height(180.0)
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
            ui.colored_label(ui.visuals().warn_fg_color, tr("No database airfoils match the filter."));
        }
        ui.add_space(6.0);
        ui.label(RichText::new(tr("Resolved outline")).strong());
        let width = ui.available_width().max(1.0);
        let (rect, _) = ui.allocate_exact_size(
            vec2(width, (width * 0.30).clamp(130.0, 250.0)),
            Sense::hover(),
        );
        paint_airfoil_outline(ui, rect.shrink(12.0), cfd.preview_coordinates.as_deref());
        if cfd.preview_coordinates.is_none() {
            ui.colored_label(
                ui.visuals().error_fg_color,
                tr("The selected database coordinates are unavailable; the solver will not substitute another section."),
            );
        }
    });
}

fn show_routine_card(state: &mut AppState, ui: &mut Ui) {
    let before = state.cfd.config.clone();
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(RichText::new(tr("Routine study controls")).strong().size(16.0));
        ui.label(
            RichText::new(tr(
                "The section frame is chord +x, normal +y, and extrusion +z. Positive angle rotates the freestream toward +y; drag is along the freestream and lift is normal to it.",
            ))
            .weak()
            .small(),
        );
        let config = &mut state.cfd.config;
        Grid::new("airfoil_cfd_routine_grid")
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
        ui.separator();
        ui.label(RichText::new(tr("Derived effective condition")).strong());
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("U = {:.4} m/s", config.effective_speed_m_s()));
            ui.separator();
            ui.label(format!("Re = {:.4e}", config.effective_reynolds()));
            ui.separator();
            ui.label(format!("Mach \u{2248} {:.3}", config.approximate_mach()));
        });
        ui.label(
            RichText::new(tr(
                "The initial steady incompressible SST template is intended for attached or mildly separated flow in its supported regime. Transition-sensitive, laminar, compressible, stall and unsteady cases are rejected or require a validated model.",
            ))
            .weak()
            .small(),
        );
    });
    if state.cfd.config != before {
        state.cfd.mark_inputs_changed();
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

pub(crate) fn show_advanced_tab(state: &mut AppState, ui: &mut Ui) {
    let before = state.cfd.config.clone();
    ScrollArea::vertical()
        .id_salt("airfoil_cfd_advanced_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            crate::theme::card_frame(ui).show(ui, |ui| {
                ui.label(RichText::new(tr("Mesh settings")).strong().size(16.0));
                ui.label(
                    RichText::new(tr("The topology is versioned and regenerated when the section or compatible settings change. Boundary-layer extrusion remains disabled until its template is validated."))
                        .weak()
                        .small(),
                );
                let mesh = &mut state.cfd.config.mesh;
                Grid::new("airfoil_cfd_mesh_grid")
                    .num_columns(2)
                    .spacing([12.0, 5.0])
                    .show(ui, |ui| {
                        ui.label(tr("Mesh preset"));
                        ComboBox::from_id_salt("airfoil_cfd_mesh_preset")
                            .selected_text(match mesh.preset {
                                MeshPreset::Coarse => tr("Coarse"),
                                MeshPreset::Medium => tr("Medium"),
                                MeshPreset::Fine => tr("Fine"),
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut mesh.preset, MeshPreset::Coarse, tr("Coarse"));
                                ui.selectable_value(&mut mesh.preset, MeshPreset::Medium, tr("Medium"));
                                ui.selectable_value(&mut mesh.preset, MeshPreset::Fine, tr("Fine"));
                            });
                        ui.end_row();
                        ui.label(tr("Upstream extent [c]"));
                        ui.add(DragValue::new(&mut mesh.upstream_chords).speed(0.5).range(2.0..=100.0));
                        ui.end_row();
                        ui.label(tr("Downstream extent [c]"));
                        ui.add(DragValue::new(&mut mesh.downstream_chords).speed(0.5).range(5.0..=200.0));
                        ui.end_row();
                        ui.label(tr("Half-height [c]"));
                        ui.add(DragValue::new(&mut mesh.half_height_chords).speed(0.5).range(2.0..=100.0));
                        ui.end_row();
                        ui.label(tr("First layer height [m]"));
                        ui.add(DragValue::new(&mut mesh.first_layer_height_m).speed(1.0e-6).range(1.0e-9..=1.0));
                        ui.end_row();
                        ui.label(tr("Target y+"));
                        ui.add(DragValue::new(&mut mesh.target_y_plus).speed(1.0).range(1.0..=300.0));
                        ui.end_row();
                        ui.checkbox(&mut mesh.boundary_layers, tr("Enable boundary layers"));
                        ui.end_row();
                        ui.label(tr("Prism layers"));
                        ui.add_enabled(mesh.boundary_layers, DragValue::new(&mut mesh.n_layers).range(1..=30));
                        ui.end_row();
                        ui.label(tr("Wake refinement"));
                        ui.add(DragValue::new(&mut mesh.wake_refinement).range(0..=6));
                        ui.end_row();
                    });
                if mesh.boundary_layers {
                    ui.colored_label(ui.visuals().warn_fg_color, tr("Boundary layers are not supported by the initial validated template; disable them before running."));
                }
            });
            ui.add_space(8.0);
            crate::theme::card_frame(ui).show(ui, |ui| {
                ui.label(RichText::new(tr("Solver and resource settings")).strong().size(16.0));
                let solver = &mut state.cfd.config.solver;
                Grid::new("airfoil_cfd_solver_grid")
                    .num_columns(2)
                    .spacing([12.0, 5.0])
                    .show(ui, |ui| {
                        ui.label(tr("Maximum SIMPLE iterations"));
                        ui.add(DragValue::new(&mut solver.max_iterations).range(10..=100_000));
                        ui.end_row();
                        ui.label(tr("Residual tolerance"));
                        ui.add(DragValue::new(&mut solver.residual_tolerance).speed(1.0e-6).range(1.0e-12..=1.0e-1));
                        ui.end_row();
                        ui.label(tr("Force stabilization tolerance"));
                        ui.add(DragValue::new(&mut solver.force_tolerance).speed(0.001).range(1.0e-5..=1.0));
                        ui.end_row();
                        ui.label(tr("Mass-balance tolerance"));
                        ui.add(DragValue::new(&mut solver.mass_balance_tolerance).speed(1.0e-6).range(1.0e-10..=1.0));
                        ui.end_row();
                        ui.label(tr("Force-history window"));
                        ui.add(DragValue::new(&mut solver.force_window).range(3..=500));
                        ui.end_row();
                        ui.label(tr("Utility timeout [s]"));
                        ui.add(DragValue::new(&mut solver.timeout_seconds).range(1..=86_400));
                        ui.end_row();
                        ui.label(tr("Write interval"));
                        ui.add(DragValue::new(&mut solver.write_interval).range(1..=100_000));
                        ui.end_row();
                    });
                ui.label(RichText::new(tr("Convergence requires residuals, stabilized lift/drag/moment histories, and continuity evidence. A successful process exit alone is not presented as a converged result." )).weak().small());
            });
            ui.add_space(8.0);
            show_effective_configuration(&state.cfd.config, ui);
        });
    if state.cfd.config != before {
        state.cfd.mark_inputs_changed();
    }
}

fn show_effective_configuration(config: &CfdStudyConfig, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(RichText::new(tr("Effective configuration")).strong().size(16.0));
        ui.monospace(format!(
            "airfoil={}\nchord={:.8} m\nalpha={:.5} deg\nU={:.8} m/s\nRe={:.8e}\nrho={:.8} kg/m^3\nmu={:.8e} Pa s\nmodel={}\nfarField={:?}\nmesh={:?}\niterations={}",
            config.airfoil_name,
            config.chord_m,
            config.angle_of_attack_deg,
            config.effective_speed_m_s(),
            config.effective_reynolds(),
            config.density_kg_m3,
            config.dynamic_viscosity_pa_s,
            config.turbulence_model,
            config.boundaries.far_field,
            config.mesh.preset,
            config.solver.max_iterations,
        ));
    });
}
