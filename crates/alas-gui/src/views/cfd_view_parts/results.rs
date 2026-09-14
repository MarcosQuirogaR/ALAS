// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Result cards, actual solver histories, and field-artifact dispatch.

use super::super::*;
use super::drawing::paint_line_plot;
use crate::state::{AppState, LogKind};
use crate::views::{tr, tr_fields};
use alas_cfd::CfdOutcome;
use egui::{ComboBox, Grid, RichText, ScrollArea, Sense, Ui};

mod diagnostics;
mod surface;
mod sweep;

pub(crate) fn show_results_tab(state: &mut AppState, ui: &mut Ui) {
    ScrollArea::vertical()
        .id_salt("airfoil_cfd_results_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            show_result_import(state, ui);
            ui.add_space(8.0);
            let mut pending_open_case = None;
            let mut pending_folder_error = None;
            if let Some(result) = state.cfd.result.as_ref() {
                show_result_status(result, ui);
                ui.add_space(8.0);
                show_coefficients(result, ui);
                ui.add_space(8.0);
                if ui.available_width() >= 760.0 {
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
                show_contour_images(&mut state.cfd.contour_textures, result, ui);
                ui.add_space(8.0);
                surface::show_surface_distribution(result, ui);
                ui.add_space(8.0);
                (pending_open_case, pending_folder_error) = show_field_inspection(
                    result,
                    ui,
                    &mut state.cfd.selected_field,
                    state.cfd.paraview_executable.is_some(),
                );
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
                if state.cfd.result.is_some() {
                    ui.add_space(8.0);
                }
                sweep::show_sweep_results(state, ui);
            } else if state.cfd.result.is_none() {
                ui.label(RichText::new(tr("No current CFD result. Run a study after the inputs and connection test are ready.")).weak());
            }
        });
}

fn show_result_import(state: &mut AppState, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(RichText::new(tr("Load persisted OpenFOAM result")).strong());
        ui.label(
            RichText::new(tr("Load an existing results.json without rerunning the solver. The recorded convergence status, exact geometry snapshot and field paths remain unchanged."))
                .weak()
                .small(),
        );
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut state.cfd.result_json_path)
                    .hint_text(tr("Path to results.json"))
                    .desired_width(ui.available_width().min(560.0)),
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
        if result.outcome == CfdOutcome::Unconverged {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr("Finite native force, field, residual, and mesh diagnostics remain available for inspection; numerical qualification is still unconverged."),
            );
        }
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
                ui.label(RichText::new(tr("Mesh quality check")).strong());
                ui.label(if result.mesh_quality.passed { tr("Passed") } else { tr("Failed or unavailable") });
                ui.end_row();
                quality_optional_row(ui, "Cells", result.mesh_quality.cells.map(|value| value as f64));
                quality_optional_row(ui, "Max non-orthogonality [deg]", result.mesh_quality.max_non_orthogonality_deg);
                quality_optional_row(ui, "Max skewness", result.mesh_quality.max_skewness);
                quality_optional_row(ui, "Minimum cell volume [m\u{00b3}]", result.mesh_quality.min_volume_m3);
                ui.label(RichText::new(tr("Native quality distributions")).strong());
                ui.label(result.mesh_quality.distributions.len().to_string());
                ui.end_row();
                ui.label(RichText::new(tr("Continuity samples")).strong());
                ui.label(result.mass_balance.len().to_string());
                ui.end_row();
            });
        if let Some(near_wall) = result.mesh_quality.near_wall.as_ref() {
            ui.label(tr_fields(
                "Solved wall y+ diagnostic: patch {patch}, time {time}, min {min}, average {average}, max {max}; target {target}",
                &[
                    ("patch", near_wall.patch_name.clone()),
                    ("time", format!("{:.4}", near_wall.time)),
                    ("min", format!("{:.4}", near_wall.min_y_plus)),
                    ("average", format!("{:.4}", near_wall.average_y_plus)),
                    ("max", format!("{:.4}", near_wall.max_y_plus)),
                    ("target", format!("{:.4}", near_wall.target_y_plus)),
                ],
            ));
        }
        if result.mass_balance.is_empty() {
            ui.colored_label(ui.visuals().warn_fg_color, tr("Mass-balance evidence is unavailable; numerical convergence remains unconfirmed."));
        }
    });
}

fn quality_optional_row(ui: &mut Ui, label: &str, value: Option<f64>) {
    ui.label(tr(label));
    ui.label(value.map_or_else(|| tr("Unavailable"), |value| format!("{value:.6}")));
    ui.end_row();
}

/// Display contour artifacts rendered from the native OpenFOAM fields.  The
/// renderer writes these files outside the result parser (usually through the
/// bundled ParaView batch script), so a missing image stays visibly
/// unavailable rather than becoming a synthetic scalar plot.
fn show_contour_images(
    textures: &mut std::collections::BTreeMap<String, egui::TextureHandle>,
    result: &alas_cfd::CfdResults,
    ui: &mut Ui,
) {
    let figure_root = result
        .case_dir
        .join("postProcessing")
        .join("alas-field-figures");
    let figures = [
        ("Mach contour", "mach-contour.png", "Mach [-]"),
        (
            "Pressure contour",
            "pressure-contour.png",
            "Gauge pressure [Pa]",
        ),
    ]
    .into_iter()
    .filter_map(|(title, filename, unit)| {
        let path = figure_root.join(filename);
        path.is_file().then_some((title, path, unit))
    })
    .collect::<Vec<_>>();
    if figures.is_empty() {
        crate::theme::card_frame(ui).show(ui, |ui| {
            ui.label(
                RichText::new(tr("Mach and pressure contours"))
                    .strong()
                    .size(16.0),
            );
            ui.label(
                RichText::new(tr("Unavailable: no native ParaView contour artifacts were rendered for this case. The OpenFOAM p and U fields remain available in the field list and ParaView handoff."))
                    .weak()
                    .small(),
            );
        });
        return;
    }
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(
            RichText::new(tr("Native OpenFOAM contours"))
                .strong()
                .size(16.0),
        );
        ui.label(
            RichText::new(tr("Mach is the low-Mach diagnostic |U|/a at the recorded static temperature. Pressure is gauge rho*p for the incompressible kinematic p field. Both images retain the exact case folder and solved write time."))
                .weak()
                .small(),
        );
    });
    let mut show_figure = |ui: &mut Ui, title: &str, path: &std::path::Path, unit: &str| {
        crate::theme::card_frame(ui).show(ui, |ui| {
            ui.label(RichText::new(tr(title)).strong());
            let key = path.to_string_lossy().to_string();
            let texture = if let Some(texture) = textures.get(&key) {
                Some(texture.clone())
            } else {
                let bytes = std::fs::read(path).ok();
                let decoded = bytes
                    .as_deref()
                    .and_then(|bytes| eframe::icon_data::from_png_bytes(bytes).ok());
                decoded.map(|icon| {
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [icon.width as usize, icon.height as usize],
                        &icon.rgba,
                    );
                    let texture = ui.ctx().load_texture(
                        format!("airfoil-cfd-contour:{key}"),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    textures.insert(key.clone(), texture.clone());
                    texture
                })
            };
            if let Some(texture) = texture {
                let width = ui.available_width().max(260.0);
                let height = width * texture.size_vec2().y / texture.size_vec2().x;
                ui.add(
                    egui::Image::from_texture(&texture)
                        .fit_to_exact_size(egui::vec2(width, height.min(420.0))),
                );
                ui.label(RichText::new(tr(unit)).weak().small());
            } else {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    tr("Contour image could not be decoded from the recorded case artifact."),
                );
            }
            ui.label(RichText::new(path.display().to_string()).weak().small());
        });
    };
    if figures.len() == 2 && ui.available_width() >= 760.0 {
        ui.columns(2, |columns| {
            for (column, (title, path, unit)) in columns.iter_mut().zip(figures.iter()) {
                show_figure(column, title, path, unit);
            }
        });
    } else {
        for (title, path, unit) in figures {
            show_figure(ui, title, &path, unit);
            ui.add_space(8.0);
        }
    }
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
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(RichText::new(tr("Flow-field inspection and exports")).strong().size(16.0));
        ui.label(RichText::new(tr("Inspect pressure and velocity fields, wall diagnostics and raw solver outputs.")).weak().small());
        if result.fields.is_empty() {
            ui.colored_label(ui.visuals().warn_fg_color, tr("No native or sampled field artifacts were found in this case."));
        } else {
            let mut selected = selected_field_state.clone();
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
            *selected_field_state = selected;
            if let Some(field) = selected_field_state.as_ref() {
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
                    folder_error = Some(error);
                }
            }
            if has_paraview {
                if ui
                    .button(tr("Open case in ParaView"))
                    .on_hover_text(tr("Open the actual OpenFOAM case for pressure, velocity and streamline inspection."))
                    .clicked()
                {
                    open_case = Some(result.case_dir.clone());
                }
            } else {
                ui.label(RichText::new(tr("Configure ParaView under External Tools for field and streamline inspection.")).weak().small());
            }
        });
    });
    (open_case, folder_error)
}
