// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Actual aerodynamic curves assembled from isolated CFD sweep cases.

use super::super::widgets::{card_title, coefficient_value, WIDE_ROW_WIDTH};
use super::{show_line_plot, tr, tr_fields};
use crate::cfd::{CfdSweepPointResult, CfdSweepPointStatus};
use crate::state::{AppState, LogKind};
use alas_cfd::CfdOutcome;
use egui::{Grid, RichText, ScrollArea, Ui};

pub(super) fn show_sweep_results(state: &mut AppState, ui: &mut Ui) {
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
    let mut actual_points = state
        .cfd
        .sweep_results
        .iter()
        .filter(|point| sweep_point_is_plot_usable(point))
        .filter_map(|point| {
            point
                .result
                .as_ref()?
                .forces
                .last()
                .map(|force| (point.value, force.cl, force.cd))
        })
        .collect::<Vec<_>>();
    // A sequential worker usually returns points in requested order, but the
    // result list is persisted and may be restored from an older run.  Sort
    // every curve by its physical sweep coordinate before drawing it so the
    // line never connects unrelated AoA/Reynolds samples.
    actual_points.sort_by(|left, right| left.0.total_cmp(&right.0));
    let cl_points = actual_points
        .iter()
        .map(|(value, cl, _)| (*value, *cl))
        .collect::<Vec<_>>();
    let cd_points = actual_points
        .iter()
        .map(|(value, _, cd)| (*value, *cd))
        .collect::<Vec<_>>();
    let cl_vs_cd = actual_points
        .iter()
        .map(|(_, cl, cd)| (*cd, *cl))
        .collect::<Vec<_>>();
    let efficiency_points = if variable == crate::cfd::CfdSweepVariable::AngleOfAttack {
        actual_points
            .iter()
            .filter(|(_, _, cd)| cd.is_finite() && cd.abs() > f64::EPSILON)
            .map(|(alpha, cl, cd)| (*alpha, *cl / *cd))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let has_unconverged_actual_point = state.cfd.sweep_results.iter().any(|point| {
        sweep_point_is_plot_usable(point)
            && point.status == CfdSweepPointStatus::Finished(CfdOutcome::Unconverged)
    });
    let mut open_case = None;
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            card_title(
                ui,
                "Sweep results",
                "Each row is one isolated OpenFOAM case with its own inputs, numerical status and case folder.",
            );
            ui.label(
                RichText::new(tr_fields(
                    "{completed}/{total} points returned; every row retains its own case provenance.",
                    &[
                        ("completed", completed.to_string()),
                        ("total", total.to_string()),
                    ],
                ))
                .weak()
                .small(),
            );
        });
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
                            ui.monospace(format!("{:.4} {}", point.value, variable.unit()));
                            ui.label(tr(point.status.label()));
                            ui.monospace(format!("{:.4}", point.config.effective_speed_m_s()));
                            ui.monospace(format!("{:.4e}", point.config.effective_reynolds()));
                            if let Some(force) = point
                                .result
                                .as_ref()
                                .and_then(|result| result.forces.last())
                            {
                                ui.monospace(coefficient_value(force.cl));
                                ui.monospace(coefficient_value(force.cd));
                                ui.monospace(coefficient_value(force.cm));
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
        if ui.available_width() >= WIDE_ROW_WIDTH {
            ui.columns(2, |columns| {
                show_line_plot(
                    &mut columns[0],
                    "Sweep CL (actual points)",
                    variable.label(),
                    "CL",
                    &cl_points,
                );
                show_line_plot(
                    &mut columns[1],
                    "Sweep CD (actual points)",
                    variable.label(),
                    "CD",
                    &cd_points,
                );
            });
        } else {
            show_line_plot(
                ui,
                "Sweep CL (actual points)",
                variable.label(),
                "CL",
                &cl_points,
            );
            ui.add_space(8.0);
            show_line_plot(
                ui,
                "Sweep CD (actual points)",
                variable.label(),
                "CD",
                &cd_points,
            );
        }
    }
    if !cl_vs_cd.is_empty() {
        ui.add_space(8.0);
        if ui.available_width() >= WIDE_ROW_WIDTH && !efficiency_points.is_empty() {
            ui.columns(2, |columns| {
                show_line_plot(
                    &mut columns[0],
                    "CL versus CD (actual points)",
                    "CD",
                    "CL",
                    &cl_vs_cd,
                );
                show_line_plot(
                    &mut columns[1],
                    "CL/CD versus angle of attack",
                    "angle of attack [deg]",
                    "CL/CD [-]",
                    &efficiency_points,
                );
            });
        } else {
            show_line_plot(ui, "CL versus CD (actual points)", "CD", "CL", &cl_vs_cd);
        }
    }
    if !efficiency_points.is_empty() && ui.available_width() < 760.0 {
        ui.add_space(8.0);
        show_line_plot(
            ui,
            "CL/CD versus angle of attack",
            "angle of attack [deg]",
            "CL/CD [-]",
            &efficiency_points,
        );
    }
    if has_unconverged_actual_point {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            tr("Polar curves include finite native samples from unconverged points as provisional evidence. Their individual numerical status remains visible in the table and result artifacts."),
        );
    }
    if let Some(case_dir) = open_case {
        if let Err(error) = state.cfd.open_case_in_paraview(&case_dir) {
            state.cfd.error = Some(error.clone());
            state.log(error, LogKind::Error);
        }
    }
}

fn sweep_point_is_plot_usable(point: &CfdSweepPointResult) -> bool {
    if !matches!(
        point.status,
        CfdSweepPointStatus::Finished(CfdOutcome::NumericallyConverged)
            | CfdSweepPointStatus::Finished(CfdOutcome::Unconverged)
    ) {
        return false;
    }
    let Some(result) = point.result.as_ref() else {
        return false;
    };
    result.mesh_quality.passed
        && result.forces.last().is_some_and(|force| {
            force.time.is_finite()
                && force.cl.is_finite()
                && force.cd.is_finite()
                && force.cm.is_finite()
        })
}
