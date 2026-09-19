// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Result cards, actual solver histories, and field-artifact dispatch.

use super::super::*;
use super::drawing::{paint_line_plot_with, plot_height};
use super::widgets::{
    card, coefficient_value, count_value, details, optional_value, physical_value, value_table,
    value_table_with, WIDE_ROW_WIDTH,
};
use crate::state::{AppState, LogKind};
use crate::views::{tr, tr_fields};
use alas_cfd::CfdOutcome;
use egui::{ComboBox, Grid, RichText, ScrollArea, Sense, Ui};

mod contours;
mod diagnostics;
mod qualification;
mod surface;
mod sweep;

pub(crate) fn show_results_tab(state: &mut AppState, ui: &mut Ui) {
    ScrollArea::vertical()
        .id_salt("airfoil_cfd_results_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut pending_open_case = None;
            let mut pending_folder_error = None;
            if let Some(result) = state.cfd.result.as_ref() {
                show_result_status(result, ui);
                ui.add_space(8.0);
                show_coefficients(result, ui);
                ui.add_space(8.0);
                if ui.available_width() >= WIDE_ROW_WIDTH {
                    ui.columns(2, |columns| {
                        show_force_plot(result, &mut columns[0]);
                        diagnostics::show_residual_plot(result, &mut columns[1]);
                    });
                } else {
                    show_force_plot(result, ui);
                    ui.add_space(8.0);
                    diagnostics::show_residual_plot(result, ui);
                }
                ui.add_space(8.0);
                show_quality_and_balance(result, ui);
                ui.add_space(8.0);
                diagnostics::show_mesh_quality_plots(result, ui);
                ui.add_space(8.0);
                contours::show_contour_images(&mut state.cfd.contour_textures, result, ui);
                ui.add_space(8.0);
                surface::show_surface_distribution(result, ui);
                ui.add_space(8.0);
                (pending_open_case, pending_folder_error) = show_field_inspection(
                    result,
                    ui,
                    &mut state.cfd.selected_field,
                    state.cfd.paraview_executable.is_some(),
                );
                ui.add_space(8.0);
            }
            if let Some(error) = pending_folder_error {
                state.cfd.error = Some(error.clone());
                state.log(error, LogKind::Warn);
            }
            if let Some(case_dir) = pending_open_case {
                if let Err(error) = state.cfd.open_case_in_paraview(&case_dir) {
                    state.cfd.error = Some(error.clone());
                    state.log(error, LogKind::Error);
                }
            }
            if !state.cfd.sweep_results.is_empty() {
                sweep::show_sweep_results(state, ui);
                ui.add_space(8.0);
            } else if state.cfd.result.is_none() {
                ui.label(RichText::new(tr("No current CFD result. Run a study after the inputs and connection test are ready.")).weak());
                ui.add_space(8.0);
            }
            show_result_import(state, ui);
        });
}

fn show_result_import(state: &mut AppState, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr("Load persisted OpenFOAM result")).strong())
                .on_hover_text(tr("Load an existing results.json without rerunning the solver. The recorded convergence status, exact geometry snapshot and field paths remain unchanged."));
            ui.add(
                egui::TextEdit::singleline(&mut state.cfd.result_json_path)
                    .hint_text(tr("Path to results.json"))
                    .desired_width((ui.available_width() - 120.0).clamp(160.0, 560.0)),
            );
            if ui.small_button(tr("Load result")).clicked() {
                let path = std::path::PathBuf::from(state.cfd.result_json_path.trim());
                match state.cfd.load_result_json(&path) {
                    Ok(()) => state.log(
                        tr_fields("Loaded persisted CFD result from {path}.", &[("path", path.display().to_string())]),
                        LogKind::Info,
                    ),
                    Err(error) => state.log(error, LogKind::Error),
                }
            }
        });
    });
}

/// The numerical outcome, its detail and its provenance on one compact band.
/// An unconverged or failed run keeps its colour and its caution line here;
/// nothing about the status is folded away.
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
        ui.set_min_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr("CFD result")).strong().size(15.0));
            ui.colored_label(color, RichText::new(tr(label)).strong());
            ui.separator();
            ui.label(RichText::new(result.status_detail.as_str()).weak());
        });
        if result.outcome == CfdOutcome::Unconverged {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr("Finite native force, field, residual, and mesh diagnostics remain available for inspection; numerical qualification is still unconverged."),
            );
        }
        ui.label(
            RichText::new(tr_fields(
                "Airfoil {airfoil} \u{00b7} template {template} \u{00b7} case {case}",
                &[
                    ("airfoil", result.provenance.airfoil.name.clone()),
                    ("template", result.provenance.template_version.clone()),
                    ("case", result.case_dir.display().to_string()),
                ],
            ))
            .weak()
            .small(),
        );
    });
}

/// One grouped table instead of a narrow card: every coefficient is shown for
/// the force-history and the surface-integrated route side by side, so the two
/// independent evaluations can be compared row by row.
fn show_coefficients(result: &alas_cfd::CfdResults, ui: &mut Ui) {
    card(
        ui,
        "Aerodynamic coefficients",
        "Left column: the solver's own force function object. Right column: coefficients re-integrated from the parsed wall samples. They are independent evaluations of the same solved field.",
        |ui| {
            let Some(last) = result.forces.last() else {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    tr("No force coefficients were parsed from the OpenFOAM output."),
                );
                return;
            };
            let surface = result.surface.as_ref().map(|surface| &surface.forces);
            let rows: [(&str, Option<f64>, Option<f64>); 7] = [
                ("CL", Some(last.cl), surface.map(|forces| forces.cl)),
                ("CD", Some(last.cd), surface.map(|forces| forces.cd)),
                ("CM (quarter chord)", Some(last.cm), surface.map(|forces| forces.cm)),
                ("CL pressure", last.cl_pressure, surface.map(|forces| forces.cl_pressure)),
                ("CL viscous", last.cl_viscous, surface.map(|forces| forces.cl_viscous)),
                ("CD pressure", last.cd_pressure, surface.map(|forces| forces.cd_pressure)),
                ("CD viscous", last.cd_viscous, surface.map(|forces| forces.cd_viscous)),
            ];
            Grid::new("airfoil_cfd_coefficients")
                .num_columns(3)
                .striped(true)
                .min_col_width((ui.available_width() / 3.0 - 12.0).clamp(90.0, 460.0))
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    ui.label(RichText::new(tr("Coefficient")).strong());
                    ui.label(RichText::new(tr("Force history")).strong());
                    ui.label(RichText::new(tr("Surface-integrated")).strong());
                    ui.end_row();
                    for (label, history, integrated) in rows {
                        ui.label(tr(label));
                        ui.monospace(optional_value(history, coefficient_value));
                        ui.monospace(optional_value(integrated, coefficient_value));
                        ui.end_row();
                    }
                });
        },
    );
}

fn show_force_plot(result: &alas_cfd::CfdResults, ui: &mut Ui) {
    let points = result
        .forces
        .iter()
        .map(|sample| (sample.time, sample.cl))
        .collect::<Vec<_>>();
    show_line_plot(
        ui,
        "Lift coefficient history",
        "iteration/time",
        "CL",
        &points,
    );
}

/// A titled plot that grows with its column and keeps its axis captions and
/// its honest sample count on one compact footer row.
fn show_line_plot(ui: &mut Ui, title: &str, x_label: &str, y_label: &str, points: &[(f64, f64)]) {
    show_line_plot_with(ui, title, x_label, y_label, points, false);
}

/// Same card, with the vertical axis optionally inverted for display.
///
/// Inversion changes only which way the axis grows; the plotted values and the
/// tick labels are the recorded ones. It is not a sign change.
pub(super) fn show_line_plot_with(
    ui: &mut Ui,
    title: &str,
    x_label: &str,
    y_label: &str,
    points: &[(f64, f64)],
    invert_y: bool,
) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr(title)).strong());
        let width = ui.available_width().max(200.0);
        let (rect, _) = ui.allocate_exact_size(vec2(width, plot_height(width)), Sense::hover());
        paint_line_plot_with(ui, rect, points, invert_y);
        plot_footer(ui, x_label, y_label, points.len());
    });
}

/// Axis captions plus the parsed-sample count, never the plot title again.
pub(super) fn plot_footer(ui: &mut Ui, x_label: &str, y_label: &str, samples: usize) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tr(x_label)).weak().small());
        ui.separator();
        ui.label(RichText::new(tr(y_label)).weak().small());
        ui.separator();
        ui.label(
            RichText::new(if samples == 0 {
                tr("Unavailable: no parsed samples")
            } else {
                tr_fields("{count} actual samples", &[("count", samples.to_string())])
            })
            .weak()
            .small(),
        );
    });
}

/// Mesh and conservation evidence packed across the card width, with counts
/// shown as integers and physical volumes in scientific notation so a tiny but
/// nonzero cell volume is never displayed as an exact zero.
fn show_quality_and_balance(result: &alas_cfd::CfdResults, ui: &mut Ui) {
    card(
        ui,
        "Quality and conservation evidence",
        "checkMesh scalars, the count of native quality distributions, and the number of parsed continuity samples for this case.",
        |ui| {
            let quality = &result.mesh_quality;
            let rows = [
                (
                    "checkMesh verdict".to_owned(),
                    if quality.passed { tr("Mesh OK") } else { tr("Failed or unavailable") },
                ),
                (
                    "Cells".to_owned(),
                    quality.cells.map_or_else(|| tr("Unavailable"), count_value),
                ),
                (
                    "Max non-orthogonality [deg]".to_owned(),
                    optional_value(quality.max_non_orthogonality_deg, physical_value),
                ),
                (
                    "Severely non-orthogonal faces".to_owned(),
                    // Prefer the qualification record: an archived result predates
                    // the MeshQuality field, and reading None there would
                    // contradict the warning below, which uses the record.
                    result
                        .mesh_qualification
                        .severely_non_orthogonal_faces
                        .or(quality.severely_non_orthogonal_faces)
                        .map_or_else(|| tr("None reported"), count_value),
                ),
                (
                    "Max skewness".to_owned(),
                    optional_value(quality.max_skewness, physical_value),
                ),
                (
                    "Minimum cell volume [m\u{00b3}]".to_owned(),
                    optional_value(quality.min_volume_m3, physical_value),
                ),
                (
                    "Native quality distributions".to_owned(),
                    count_value(quality.distributions.len() as u64),
                ),
                (
                    "Continuity samples".to_owned(),
                    count_value(result.mass_balance.len() as u64),
                ),
            ];
            value_table_with(ui, "airfoil_cfd_quality_grid", &rows, |ui, value| {
                ui.monospace(value);
            });
            qualification::show_qualification_notes(result, ui);
            qualification::show_field_updates(&result.field_updates, ui);
            qualification::show_plausibility(result, ui);
            if let Some(near_wall) = quality.near_wall.as_ref() {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(tr("Solved wall y+")).strong().small());
                    for (label, value) in [
                        ("Patch", near_wall.patch_name.clone()),
                        ("Time", format!("{:.4}", near_wall.time)),
                        ("Minimum", physical_value(near_wall.min_y_plus)),
                        ("Average", physical_value(near_wall.average_y_plus)),
                        ("Maximum", physical_value(near_wall.max_y_plus)),
                        ("Target", physical_value(near_wall.target_y_plus)),
                    ] {
                        ui.label(RichText::new(tr(label)).weak().small());
                        ui.label(RichText::new(value).monospace().small());
                    }
                });
            }
            if result.mass_balance.is_empty() {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    tr("Mass-balance evidence is unavailable; numerical convergence remains unconfirmed."),
                );
            }
        },
    );
}

/// Show the face-resolved pressure and signed skin-friction samples produced
/// by the native surface parser. The parser's face order is retained inside
/// each upper/lower branch; the GUI never globally sorts by x/c, which would
/// connect the two branches across a trailing edge.
fn show_field_inspection(
    result: &alas_cfd::CfdResults,
    ui: &mut Ui,
    selected_field_state: &mut Option<String>,
    has_paraview: bool,
) -> (Option<std::path::PathBuf>, Option<String>) {
    let mut open_case = None;
    let mut folder_error = None;
    card(
        ui,
        "Flow-field inspection and exports",
        "Inspect pressure and velocity fields, wall diagnostics and raw solver outputs.",
        |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(tr("Reproducible case folder")).weak());
                ui.label(RichText::new(result.case_dir.display().to_string()).monospace().small());
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
                        folder_error = Some(error);
                    }
                }
                if has_paraview {
                    if ui
                        .small_button(tr("Open case in ParaView"))
                        .on_hover_text(tr("Open the actual OpenFOAM case for pressure, velocity and streamline inspection."))
                        .clicked()
                    {
                        open_case = Some(result.case_dir.clone());
                    }
                } else {
                    ui.label(RichText::new(tr("Configure ParaView under External Tools for field and streamline inspection.")).weak().small());
                }
            });
            if result.surface.is_none() {
                ui.label(
                    RichText::new(tr(
                        "Parsed Cp/Cf distributions are unavailable for this case.",
                    ))
                    .weak()
                    .small(),
                );
                if let Some(error) = result.surface_error.as_deref() {
                    ui.label(RichText::new(error).weak().small());
                }
            }
            if result.fields.is_empty() {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    tr("No native or sampled field artifacts were found in this case."),
                );
                return;
            }
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(tr("Field artifact")).weak());
                let mut selected = selected_field_state.clone();
                let selected_text = selected
                    .as_deref()
                    .map_or_else(|| tr("Select field artifact"), str::to_owned);
                ComboBox::from_id_salt("airfoil_cfd_field_selector")
                    .width(ui.available_width().min(420.0))
                    .selected_text(selected_text)
                    .show_ui(ui, |ui| {
                        for artifact in &result.fields {
                            let label =
                                format!("{} \u{00b7} {}", artifact.name, artifact.relative_path);
                            if ui
                                .selectable_label(
                                    selected.as_deref() == Some(label.as_str()),
                                    label.clone(),
                                )
                                .clicked()
                            {
                                selected = Some(label);
                            }
                        }
                    });
                *selected_field_state = selected;
            });
            details(
                ui,
                "airfoil_cfd_artifact_list",
                "Available artifacts",
                |ui| {
                    let rows = result
                        .fields
                        .iter()
                        .map(|artifact| {
                            let time = artifact
                                .time
                                .map(|time| format!(" t={time:.4}"))
                                .unwrap_or_default();
                            (
                                artifact.name.clone(),
                                format!(
                                    "{} \u{00b7} {}{time}",
                                    artifact.relative_path, artifact.kind
                                ),
                            )
                        })
                        .collect::<Vec<_>>();
                    value_table(ui, "airfoil_cfd_artifact_table", &rows);
                },
            );
        },
    );
    (open_case, folder_error)
}
