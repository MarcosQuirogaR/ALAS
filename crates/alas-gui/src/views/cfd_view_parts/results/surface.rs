// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Actual wall-surface Cp/Cf plotting and branch-order diagnostics.

use super::super::widgets::card_title;
use super::{show_line_plot_with, tr, tr_fields};
use egui::{RichText, Ui};
pub(super) fn show_surface_distribution(result: &alas_cfd::CfdResults, ui: &mut Ui) {
    let Some(surface) = result.surface.as_ref() else {
        crate::theme::card_frame(ui).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            card_title(ui, "Cp and Cf distributions", "");
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr("Cp/Cf distributions unavailable: the solver did not emit parseable wall samples."),
            );
            if let Some(error) = result.surface_error.as_deref() {
                ui.label(RichText::new(error).weak().small());
            }
        });
        return;
    };
    let chord_m = result.provenance.config.chord_m;
    let (upper_cp, lower_cp, upper_cf, lower_cf) = surface_branch_points(surface, chord_m);
    let count = surface.samples.len();
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            card_title(
                ui,
                "Cp and Cf distributions",
                "Cp is based on dimensional pressure and signed Cf follows the local tangent toward increasing x/c.",
            );
            ui.label(
                RichText::new(tr_fields(
                    "{count} actual wall-face samples; upper/lower branches retain parser order ({order}).",
                    &[
                        ("count", count.to_string()),
                        ("order", format!("{:?}", surface.order)),
                    ],
                ))
                .weak()
                .small(),
            );
        });
    });
    ui.add_space(6.0);
    show_surface_plot_pair(
        ui,
        "Cp upper surface",
        "Cp lower surface",
        "Cp [-] (axis inverted, suction up)",
        &upper_cp,
        &lower_cp,
        true,
    );
    ui.add_space(6.0);
    show_surface_plot_pair(
        ui,
        "Cf upper surface",
        "Cf lower surface",
        "Cf [-] (signed toward +x/c)",
        &upper_cf,
        &lower_cf,
        false,
    );
}

fn surface_branch_points(
    surface: &alas_cfd::surface::SurfaceDistribution,
    chord_m: f64,
) -> (
    Vec<(f64, f64)>,
    Vec<(f64, f64)>,
    Vec<(f64, f64)>,
    Vec<(f64, f64)>,
) {
    if !chord_m.is_finite() || chord_m <= 0.0 {
        return (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    }
    let mut upper_cp = Vec::new();
    let mut lower_cp = Vec::new();
    let mut upper_cf = Vec::new();
    let mut lower_cf = Vec::new();
    let mut previous_branch_is_upper = true;
    // `SurfaceDistribution` retains the mesh's original face vector and
    // stores the parser's traversal rank in arc_length_m. Reconstruct that
    // traversal here before splitting branches; sorting by x/c would join
    // opposite branches at the trailing edge and is deliberately avoided.
    let mut ordered_samples = surface.samples.iter().collect::<Vec<_>>();
    if ordered_samples
        .iter()
        .all(|sample| sample.arc_length_m.is_finite())
    {
        ordered_samples.sort_by(|left, right| left.arc_length_m.total_cmp(&right.arc_length_m));
    }
    for sample in ordered_samples {
        let x_c = sample.center_m[0] / chord_m;
        if !x_c.is_finite() {
            continue;
        }
        let branch_is_upper = surface_sample_is_upper(sample, chord_m, previous_branch_is_upper);
        previous_branch_is_upper = branch_is_upper;
        if sample.cp.is_finite() {
            if branch_is_upper {
                upper_cp.push((x_c, sample.cp));
            } else {
                lower_cp.push((x_c, sample.cp));
            }
        }
        if sample.cf.is_finite() {
            if branch_is_upper {
                upper_cf.push((x_c, sample.cf));
            } else {
                lower_cf.push((x_c, sample.cf));
            }
        }
    }
    (upper_cp, lower_cp, upper_cf, lower_cf)
}

/// Classify a wall face using its outward fluid-facing normal. A cambered
/// lower surface can lie above the section x-axis, so center-y sign is not a
/// reliable branch discriminator. Near a vertical trailing-edge face the
/// y-normal is zero; retaining the previous branch prevents an artificial
/// branch switch at the closure.
fn surface_sample_is_upper(
    sample: &alas_cfd::surface::SurfaceSample,
    chord_m: f64,
    previous_branch_is_upper: bool,
) -> bool {
    let normal_y = sample.area_vector_m2[1];
    let normal_tolerance = (chord_m * chord_m).max(sample.face_area_m2.abs()) * 1.0e-12;
    if normal_y.is_finite() && normal_y.abs() > normal_tolerance {
        return normal_y < 0.0;
    }
    if sample.center_m[1].is_finite() && sample.center_m[1].abs() > 1.0e-12 * chord_m {
        return sample.center_m[1] > 0.0;
    }
    previous_branch_is_upper
}

fn show_surface_plot_pair(
    ui: &mut Ui,
    upper_title: &str,
    lower_title: &str,
    y_label: &str,
    upper: &[(f64, f64)],
    lower: &[(f64, f64)],
    invert_y: bool,
) {
    if ui.available_width() >= super::WIDE_ROW_WIDTH {
        ui.columns(2, |columns| {
            show_line_plot_with(
                &mut columns[0],
                upper_title,
                "x/c (parser order)",
                y_label,
                upper,
                invert_y,
            );
            show_line_plot_with(
                &mut columns[1],
                lower_title,
                "x/c (parser order)",
                y_label,
                lower,
                invert_y,
            );
        });
    } else {
        show_line_plot_with(
            ui,
            upper_title,
            "x/c (parser order)",
            y_label,
            upper,
            invert_y,
        );
        ui.add_space(6.0);
        show_line_plot_with(
            ui,
            lower_title,
            "x/c (parser order)",
            y_label,
            lower,
            invert_y,
        );
    }
}
