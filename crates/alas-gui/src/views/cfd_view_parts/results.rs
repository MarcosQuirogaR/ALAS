// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Result cards, actual solver histories, and field-artifact dispatch.

use super::super::*;
use super::drawing::paint_line_plot;
use crate::cfd::CfdSweepPointStatus;
use crate::state::{AppState, LogKind};
use crate::views::{tr, tr_fields};
use alas_cfd::CfdOutcome;
use egui::{ComboBox, Grid, RichText, ScrollArea, Sense, Ui};

mod surface;

pub(crate) fn show_results_tab(state: &mut AppState, ui: &mut Ui) {
    ScrollArea::vertical()
        .id_salt("airfoil_cfd_results_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let result = state.cfd.result.clone();
            if let Some(result) = result.as_ref() {
                show_result_status(result, ui);
                ui.add_space(8.0);
                show_coefficients(result, ui);
                ui.add_space(8.0);
                if ui.available_width() >= 760.0 {
                    ui.columns(2, |columns| {
                        show_force_plot(result, &mut columns[0]);
                        show_residual_plot(result, &mut columns[1]);
                    });
                } else {
                    show_force_plot(result, ui);
                    ui.add_space(8.0);
                    show_residual_plot(result, ui);
                }
                ui.add_space(8.0);
                show_quality_and_balance(result, ui);
                ui.add_space(8.0);
                surface::show_surface_distribution(result, ui);
                ui.add_space(8.0);
                show_field_inspection(state, result, ui);
            }
            if !state.cfd.sweep_results.is_empty() {
                if result.is_some() {
                    ui.add_space(8.0);
                }
                show_sweep_results(state, ui);
            } else if result.is_none() {
                ui.label(RichText::new(tr("No current CFD result. Run a study after the inputs and connection test are ready.")).weak());
            }
        });
}

fn show_sweep_results(state: &mut AppState, ui: &mut Ui) {
    let variable = state.cfd.sweep_settings.variable;
    let total = state.cfd.sweep_results.len();
    let completed = state
        .cfd
        .sweep_results
        .iter()
        .filter(|point| {
            !matches!(
                point.status,
                CfdSweepPointStatus::Pending | CfdSweepPointStatus::Running
            )
        })
        .count();
    let cl_points = state
        .cfd
        .sweep_results
        .iter()
        .filter_map(|point| {
            point
                .result
                .as_ref()?
                .forces
                .last()
                .map(|force| (point.value, force.cl))
        })
        .collect::<Vec<_>>();
    let cd_points = state
        .cfd
        .sweep_results
        .iter()
        .filter_map(|point| {
            point
                .result
                .as_ref()?
                .forces
                .last()
                .map(|force| (point.value, force.cd))
        })
        .collect::<Vec<_>>();
    let mut open_case = None;
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(RichText::new(tr("Sweep results")).strong().size(16.0));
        ui.label(tr_fields(
            "{completed}/{total} points returned; every row retains its own case provenance.",
            &[
                ("completed", completed.to_string()),
                ("total", total.to_string()),
            ],
        ));
        ScrollArea::horizontal()
            .id_salt("airfoil_cfd_sweep_table_scroll")
            .show(ui, |ui| {
                Grid::new("airfoil_cfd_sweep_table")
                    .striped(true)
                    .num_columns(9)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        for label in [
                            "Point",
                            variable.label(),
                            "Status",
                            "U [m/s]",
                            "Re [-]",
                            "CL",
                            "CD",
                            "CM",
                            "Case",
                        ] {
                            ui.label(RichText::new(tr(label)).strong());
                        }
                        ui.end_row();
                        for point in &state.cfd.sweep_results {
                            ui.label((point.index + 1).to_string());
                            ui.label(format!("{:.6} {}", point.value, variable.unit()));
                            ui.label(tr(point.status.label()));
                            ui.label(format!("{:.5}", point.config.effective_speed_m_s()));
                            ui.label(format!("{:.5e}", point.config.effective_reynolds()));
                            if let Some(force) = point
                                .result
                                .as_ref()
                                .and_then(|result| result.forces.last())
                            {
                                ui.label(format!("{:.6}", force.cl));
                                ui.label(format!("{:.6}", force.cd));
                                ui.label(format!("{:.6}", force.cm));
                            } else {
                                for _ in 0..3 {
                                    ui.label(tr("Unavailable"));
                                }
                            }
                            if let Some(result) = point.result.as_ref() {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(result.case_dir.display().to_string());
                                    if state.cfd.paraview_executable.is_some()
                                        && ui.small_button(tr("Open")).clicked()
                                    {
                                        open_case = Some(result.case_dir.clone());
                                    }
                                });
                            } else if let Some(error) = point.error.as_ref() {
                                ui.label(RichText::new(error).weak());
                            } else {
                                ui.label(tr("Unavailable"));
                            }
                            ui.end_row();
                        }
                    });
            });
    });
    if !cl_points.is_empty() || !cd_points.is_empty() {
        ui.add_space(8.0);
        if ui.available_width() >= 760.0 {
            ui.columns(2, |columns| {
                show_line_plot(
                    &mut columns[0],
                    "Sweep CL distribution (actual points)",
                    variable.label(),
                    "CL",
                    &cl_points,
                );
                show_line_plot(
                    &mut columns[1],
                    "Sweep CD distribution (actual points)",
                    variable.label(),
                    "CD",
                    &cd_points,
                );
            });
        } else {
            show_line_plot(
                ui,
                "Sweep CL distribution (actual points)",
                variable.label(),
                "CL",
                &cl_points,
            );
            ui.add_space(8.0);
            show_line_plot(
                ui,
                "Sweep CD distribution (actual points)",
                variable.label(),
                "CD",
                &cd_points,
            );
        }
    }
    if let Some(case_dir) = open_case {
        if let Err(error) = state.cfd.open_case_in_paraview(&case_dir) {
            state.cfd.error = Some(error.clone());
            state.log(error, LogKind::Error);
        }
    }
}

fn show_result_status(result: &alas_cfd::CfdResults, ui: &mut Ui) {
    let (color, label) = match result.outcome {
        CfdOutcome::NumericallyConverged => (
            crate::theme::success_color(ui.visuals()),
            "Numerically converged",
        ),
        CfdOutcome::Unconverged => (ui.visuals().warn_fg_color, "Unconverged"),
        CfdOutcome::Cancelled => (ui.visuals().warn_fg_color, "Cancelled"),
        CfdOutcome::Failed => (ui.visuals().error_fg_color, "Failed"),
    };
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr("CFD result")).strong().size(16.0));
            ui.colored_label(color, tr(label));
        });
        ui.label(RichText::new(result.status_detail.as_str()).weak());
        ui.label(tr_fields(
            "Airfoil {airfoil} \u{00b7} template {template} \u{00b7} case {case}",
            &[
                ("airfoil", result.provenance.airfoil.name.clone()),
                ("template", result.provenance.template_version.clone()),
                ("case", result.case_dir.display().to_string()),
            ],
        ));
    });
}

fn show_coefficients(result: &alas_cfd::CfdResults, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(
            RichText::new(tr("Aerodynamic coefficients"))
                .strong()
                .size(16.0),
        );
        if let Some(last) = result.forces.last() {
            Grid::new("airfoil_cfd_coefficients")
                .num_columns(2)
                .spacing([18.0, 4.0])
                .show(ui, |ui| {
                    coefficient_row(ui, "CL", last.cl);
                    coefficient_row(ui, "CD", last.cd);
                    coefficient_row(ui, "CM (quarter chord)", last.cm);
                    coefficient_optional_row(ui, "CD pressure", last.cd_pressure);
                    coefficient_optional_row(ui, "CD viscous", last.cd_viscous);
                    coefficient_optional_row(ui, "CL pressure", last.cl_pressure);
                    coefficient_optional_row(ui, "CL viscous", last.cl_viscous);
                    if let Some(surface) = result.surface.as_ref() {
                        ui.separator();
                        ui.label(RichText::new(tr("Surface-integrated coefficients")).strong());
                        ui.end_row();
                        coefficient_row(ui, "CL (surface)", surface.forces.cl);
                        coefficient_row(ui, "CD (surface)", surface.forces.cd);
                        coefficient_row(ui, "CM (surface)", surface.forces.cm);
                        coefficient_row(ui, "CL pressure (surface)", surface.forces.cl_pressure);
                        coefficient_row(ui, "CL viscous (surface)", surface.forces.cl_viscous);
                        coefficient_row(ui, "CD pressure (surface)", surface.forces.cd_pressure);
                        coefficient_row(ui, "CD viscous (surface)", surface.forces.cd_viscous);
                    }
                });
        } else {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr("No force coefficients were parsed from the OpenFOAM output."),
            );
        }
    });
}

fn coefficient_row(ui: &mut Ui, label: &str, value: f64) {
    ui.label(RichText::new(tr(label)).strong());
    ui.label(format!("{value:.6}"));
    ui.end_row();
}

fn coefficient_optional_row(ui: &mut Ui, label: &str, value: Option<f64>) {
    ui.label(tr(label));
    ui.label(value.map_or_else(|| tr("Unavailable"), |value| format!("{value:.6}")));
    ui.end_row();
}

fn show_force_plot(result: &alas_cfd::CfdResults, ui: &mut Ui) {
    let points = result
        .forces
        .iter()
        .map(|sample| (sample.time, sample.cl))
        .collect::<Vec<_>>();
    show_line_plot(
        ui,
        "Lift coefficient history (actual samples)",
        "iteration/time",
        "CL",
        &points,
    );
}

fn show_residual_plot(result: &alas_cfd::CfdResults, ui: &mut Ui) {
    let points = result
        .residuals
        .iter()
        .filter(|sample| sample.final_residual.is_finite() && sample.final_residual > 0.0)
        .enumerate()
        .map(|(index, sample)| (index as f64, sample.final_residual.log10()))
        .collect::<Vec<_>>();
    show_line_plot(
        ui,
        "Residual history (log10 final residual)",
        "sample",
        "log10(residual)",
        &points,
    );
}

fn show_line_plot(ui: &mut Ui, title: &str, x_label: &str, y_label: &str, points: &[(f64, f64)]) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(RichText::new(tr(title)).strong());
        let width = ui.available_width().max(260.0);
        let (rect, _) = ui.allocate_exact_size(vec2(width, 220.0), Sense::hover());
        paint_line_plot(ui, rect, points);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr(x_label)).weak().small());
            ui.separator();
            ui.label(RichText::new(tr(y_label)).weak().small());
            ui.separator();
            ui.label(
                RichText::new(if points.is_empty() {
                    tr("Unavailable: no parsed samples")
                } else {
                    tr_fields(
                        "{count} actual samples",
                        &[("count", points.len().to_string())],
                    )
                })
                .weak()
                .small(),
            );
        });
    });
}

fn show_quality_and_balance(result: &alas_cfd::CfdResults, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(RichText::new(tr("Quality and conservation evidence")).strong().size(16.0));
        Grid::new("airfoil_cfd_quality_grid")
            .num_columns(2)
            .spacing([18.0, 4.0])
            .show(ui, |ui| {
                ui.label(RichText::new(tr("Mesh quality gate")).strong());
                ui.label(if result.mesh_quality.passed { tr("Passed") } else { tr("Failed or unavailable") });
                ui.end_row();
                quality_optional_row(ui, "Cells", result.mesh_quality.cells.map(|value| value as f64));
                quality_optional_row(ui, "Max non-orthogonality [deg]", result.mesh_quality.max_non_orthogonality_deg);
                quality_optional_row(ui, "Max skewness", result.mesh_quality.max_skewness);
                quality_optional_row(ui, "Minimum cell volume [m\u{00b3}]", result.mesh_quality.min_volume_m3);
                ui.label(RichText::new(tr("Continuity samples")).strong());
                ui.label(result.mass_balance.len().to_string());
                ui.end_row();
            });
        if result.mass_balance.is_empty() {
            ui.colored_label(ui.visuals().warn_fg_color, tr("Mass-balance evidence is unavailable; the result cannot be called converged on process exit alone."));
        }
    });
}

fn quality_optional_row(ui: &mut Ui, label: &str, value: Option<f64>) {
    ui.label(tr(label));
    ui.label(value.map_or_else(|| tr("Unavailable"), |value| format!("{value:.6}")));
    ui.end_row();
}

/// Show the face-resolved pressure and signed skin-friction samples produced
/// by the native surface parser. The parser's face order is retained inside
/// each upper/lower branch; the GUI never globally sorts by x/c, which would
/// connect the two branches across a trailing edge.
fn show_field_inspection(state: &mut AppState, result: &alas_cfd::CfdResults, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(RichText::new(tr("Flow-field inspection and exports")).strong().size(16.0));
        ui.label(RichText::new(tr("Inspect pressure and velocity fields, wall diagnostics and raw solver outputs.")).weak().small());
        if result.fields.is_empty() {
            ui.colored_label(ui.visuals().warn_fg_color, tr("No native or sampled field artifacts were found in this case."));
        } else {
            let mut selected = state.cfd.selected_field.clone();
            let selected_text = selected
                .as_deref()
                .map_or_else(|| tr("Select field artifact"), str::to_owned);
            ComboBox::from_id_salt("airfoil_cfd_field_selector")
                .width(ui.available_width().min(420.0))
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    for field in &result.fields {
                        let label = format!("{} \u{00b7} {}", field.name, field.relative_path);
                        if ui.selectable_label(selected.as_deref() == Some(label.as_str()), label.clone()).clicked() {
                            selected = Some(label);
                        }
                    }
                });
            state.cfd.selected_field = selected;
            if let Some(field) = state.cfd.selected_field.as_ref() {
                ui.label(RichText::new(field).weak());
            }
            ui.add_space(4.0);
            ui.label(tr("Available artifacts"));
            for artifact in &result.fields {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&artifact.name).strong());
                    ui.label(artifact.relative_path.as_str());
                    if let Some(time) = artifact.time {
                        ui.label(format!("t={time:.4}"));
                    }
                    ui.label(RichText::new(&artifact.kind).weak());
                });
            }
        }
        ui.separator();
        if result.surface.is_some() {
            ui.label(tr(
                "Parsed Cp/Cf distributions are available above from dimensional pressure and wall shear.",
            ));
        } else if let Some(error) = result.surface_error.as_deref() {
            ui.label(tr("Parsed Cp/Cf distributions are unavailable for this case."));
            ui.label(RichText::new(error).weak().small());
        } else {
            ui.label(tr("Parsed Cp/Cf distributions are unavailable for this case."));
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr("Reproducible case folder")).strong());
            ui.label(result.case_dir.display().to_string());
            if ui.small_button(tr("Copy path")).clicked() {
                ui.ctx().copy_text(result.case_dir.display().to_string());
            }
            if ui
                .small_button(tr("Open folder"))
                .on_hover_text(tr("Show this exact location in the system file explorer."))
                .clicked()
            {
                if let Err(error) = crate::views::tools_view::open_in_file_explorer(
                    &result.case_dir.to_string_lossy(),
                    true,
                ) {
                    state.cfd.error = Some(error.clone());
                    state.log(error, LogKind::Warn);
                }
            }
            if state.cfd.paraview_executable.is_some() {
                if ui
                    .button(tr("Open case in ParaView"))
                    .on_hover_text(tr("Open the actual OpenFOAM case for pressure, velocity and streamline inspection."))
                    .clicked()
                {
                    if let Err(error) = state.cfd.open_case_in_paraview(&result.case_dir) {
                        state.cfd.error = Some(error.clone());
                        state.log(error, LogKind::Error);
                    }
                }
            } else {
                ui.label(RichText::new(tr("Configure ParaView under External Tools for field and streamline inspection.")).weak().small());
            }
        });
    });
}
