// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Actual wall-surface Cp/Cf plotting and branch-order diagnostics.

use super::super::drawing::{
    paint_multi_line_plot_with, plot_height, plot_legend, series_color, PlotSeries,
};
use super::super::widgets::card_title;
use super::{tr, tr_fields};
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
    let (upper_cp, lower_cp, upper_cf, lower_cf) = surface_branch_points(&surface.samples, chord_m);
    let count = surface.samples.len();
    let closures = surface
        .samples
        .iter()
        .filter(|face| is_vertical_closure(face))
        .count();
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
                    "{count} wall faces; branches classified by wall normal and plotted in increasing x/c ({order}).",
                    &[
                        ("count", count.to_string()),
                        ("order", format!("{:?}", surface.order)),
                    ],
                ))
                .weak()
                .small(),
            );
        });
        if closures > 0 {
            ui.label(RichText::new(format!("{closures} vertical closure face(s) omitted from branch curves; retained in integrated loads.")).weak().small());
        }
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

/// Upper and lower `(x/c, value)` point series for one surface quantity, in
/// increasing x/c order per branch: `(upper_cp, lower_cp, upper_cf, lower_cf)`.
type SurfaceBranchPoints = (
    Vec<(f64, f64)>,
    Vec<(f64, f64)>,
    Vec<(f64, f64)>,
    Vec<(f64, f64)>,
);

fn surface_branch_points(
    samples: &[alas_cfd::surface::SurfaceSample],
    chord_m: f64,
) -> SurfaceBranchPoints {
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
    // traversal before splitting branches. Sort only within each classified
    // branch afterwards, so opposite surfaces never become connected.
    let mut ordered_samples = samples.iter().collect::<Vec<_>>();
    if ordered_samples
        .iter()
        .all(|sample| sample.arc_length_m.is_finite())
    {
        ordered_samples.sort_by(|left, right| left.arc_length_m.total_cmp(&right.arc_length_m));
    }
    for sample in ordered_samples {
        // A vertical blunt-edge base has no tangent toward increasing x/c.
        // It is a separate physical face, not part of either surface curve.
        if is_vertical_closure(sample) {
            continue;
        }
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
    // Sort only after branch classification. Native face numbering may wrap
    // at any location; connecting that wrap produces a false diagonal.
    for branch in [&mut upper_cp, &mut lower_cp, &mut upper_cf, &mut lower_cf] {
        branch.sort_by(|a, b| a.0.total_cmp(&b.0));
    }
    (upper_cp, lower_cp, upper_cf, lower_cf)
}

fn is_vertical_closure(sample: &alas_cfd::surface::SurfaceSample) -> bool {
    sample.face_area_m2 > 0.0 && sample.area_vector_m2[1].abs() <= 1e-8 * sample.face_area_m2
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
    // Shared axes make upper/lower pressure loading directly comparable.
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(
            RichText::new(tr(if invert_y {
                "Pressure coefficient"
            } else {
                "Skin-friction coefficient"
            }))
            .strong(),
        );
        let series = [
            PlotSeries {
                name: upper_title.to_owned(),
                points: upper.to_vec(),
                color: series_color(ui, 0),
                dashed: false,
            },
            PlotSeries {
                name: lower_title.to_owned(),
                points: lower.to_vec(),
                color: series_color(ui, 1),
                dashed: true,
            },
        ];
        let width = ui.available_width();
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(width, plot_height(width)), egui::Sense::hover());
        paint_multi_line_plot_with(ui, rect, &series, invert_y);
        plot_legend(ui, &series);
        super::plot_footer(ui, "x/c [-]", y_label, upper.len() + lower.len());
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_cfd::surface::SurfaceSample;

    fn face(x: f64, y: f64, normal_y: f64, rank: f64) -> SurfaceSample {
        SurfaceSample {
            patch_face_index: 0,
            global_face_index: 0,
            center_m: [x, y, 0.0],
            area_vector_m2: [0.0, normal_y, 0.0],
            face_area_m2: 1.0,
            surface_length_m: 1.0,
            arc_length_m: rank,
            tangent_plus_chord: [1.0, 0.0, 0.0],
            p_kinematic_m2_s2: 0.0,
            pressure_pa: 0.0,
            wall_shear_kinematic_m2_s2: [0.0; 3],
            wall_shear_stress_pa: [0.0; 3],
            wall_shear_magnitude_pa: 0.0,
            cf: normal_y,
            cf_magnitude: 1.0,
            cp: normal_y,
        }
    }

    #[test]
    fn wrapped_native_order_does_not_connect_opposite_surfaces() {
        // Cambered upper TE lies below y=0; lower midchord lies above y=0.
        let faces = [
            face(0.9, -0.01, -1.0, 0.0),
            face(0.1, 0.02, -1.0, 1.0),
            face(0.8, 0.01, 1.0, 2.0),
            face(0.2, 0.02, 1.0, 3.0),
        ];
        let (upper, lower, _, _) = surface_branch_points(&faces, 1.0);
        assert_eq!(upper, vec![(0.1, -1.0), (0.9, -1.0)]);
        assert_eq!(lower, vec![(0.2, 1.0), (0.8, 1.0)]);
    }

    #[test]
    fn vertical_base_is_not_a_branch_but_reversed_leading_edge_flow_is() {
        let mut base = face(1.0, -0.01, 0.0, 0.0);
        base.area_vector_m2 = [-1.0, 0.0, 0.0];
        let mut leading_edge = face(0.0005, -0.0027, 0.18, 1.0);
        leading_edge.cf = -0.00624;
        let (upper, lower, _, lower_cf) = surface_branch_points(&[base, leading_edge], 1.0);
        assert!(upper.is_empty());
        assert_eq!(lower.len(), 1);
        assert_eq!(lower_cf, vec![(0.0005, -0.00624)]);
    }
}
