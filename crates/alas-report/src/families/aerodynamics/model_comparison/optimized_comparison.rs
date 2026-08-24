// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (`figure_model_comparison`)
// Reference: alas @ rust-port-baseline.

//! Compare the VLM-optimized and AVL-optimized aircraft branches.
//!
//! Each optimized geometry keeps its own native VLM curve and optional AVL
//! cross-check. The figure never attributes a result from one geometry to the
//! other branch, and AVL total drag remains excluded from the comparison.

use alas_pipeline::full_analysis::AnalysisReport;
use alas_pipeline::AvlAnalysisResult;

use super::super::support::padded_range;
use crate::chart_kit::{draw_legend, LegendMarker};
use crate::scene::{Axes2D, Color, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

fn title(scene: &mut Scene, axes: &Axes2D, text: &str, color: Color) {
    scene.add(SceneElement::Text {
        text: text.to_owned(),
        pos: [axes.left, axes.top - 8.0],
        font_size: 10.0,
        color,
        align: TextAlign::Left,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
}

/// Compare the two independently optimized aircraft branches.
///
/// The existing [`super::super::model_comparison::figure_model_comparison`]
/// remains the single-aircraft VLM/AVL diagnostic. This product-level figure
/// is selected when both optimizer branches completed: each branch's native
/// VLM prediction is shown with its own AVL cross-check, so a curve from one
/// geometry can never be mislabeled as a result from the other geometry.
pub fn figure_optimized_aircraft_comparison(
    vlm_report: &AnalysisReport,
    avl_report: &AnalysisReport,
    vlm_avl: Option<&AvlAnalysisResult>,
    avl_avl: Option<&AvlAnalysisResult>,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let alpha_range = padded_range(
        vlm_report
            .polar
            .alpha_deg
            .iter()
            .copied()
            .chain(avl_report.polar.alpha_deg.iter().copied()),
        0.08,
    );
    let mut scene = Scene::new(900.0, 780.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("VLM vs AVL optimized aircraft".to_owned());

    let vlm_alpha_cl = vlm_report
        .polar
        .alpha_deg
        .iter()
        .copied()
        .zip(vlm_report.polar.cl.iter().copied())
        .collect::<Vec<_>>();
    let avl_alpha_cl = avl_report
        .polar
        .alpha_deg
        .iter()
        .copied()
        .zip(avl_report.polar.cl.iter().copied())
        .collect::<Vec<_>>();
    let vlm_alpha_cm = vlm_report
        .polar
        .alpha_deg
        .iter()
        .copied()
        .zip(vlm_report.polar.cm.iter().copied())
        .collect::<Vec<_>>();
    let avl_alpha_cm = avl_report
        .polar
        .alpha_deg
        .iter()
        .copied()
        .zip(avl_report.polar.cm.iter().copied())
        .collect::<Vec<_>>();
    let vlm_alpha_cdi = vlm_report
        .polar
        .alpha_deg
        .iter()
        .copied()
        .zip(vlm_report.polar.cd_induced.iter().copied())
        .collect::<Vec<_>>();
    let avl_alpha_cdi = avl_report
        .polar
        .alpha_deg
        .iter()
        .copied()
        .zip(avl_report.polar.cd_induced.iter().copied())
        .collect::<Vec<_>>();

    let vlm_color = Color::from_hex("tab:blue");
    let avl_color = Color::from_hex("#17becf");
    let vlm_avl_color = Color::from_hex("#1f77b4");
    let avl_avl_color = Color::from_hex("#bcbd22");

    let cl_range = padded_range(
        vlm_alpha_cl
            .iter()
            .chain(&avl_alpha_cl)
            .map(|(_, value)| *value)
            .chain(
                vlm_avl
                    .and_then(AvlAnalysisResult::comparable_polar)
                    .into_iter()
                    .flat_map(|polar| polar.points.iter().map(|point| point.lift_coefficient)),
            )
            .chain(
                avl_avl
                    .and_then(AvlAnalysisResult::comparable_polar)
                    .into_iter()
                    .flat_map(|polar| polar.points.iter().map(|point| point.lift_coefficient)),
            ),
        0.08,
    );
    let cm_range = padded_range(
        vlm_alpha_cm
            .iter()
            .chain(&avl_alpha_cm)
            .map(|(_, value)| *value)
            .chain(
                vlm_avl
                    .and_then(AvlAnalysisResult::comparable_polar)
                    .into_iter()
                    .flat_map(|polar| {
                        polar
                            .points
                            .iter()
                            .map(|point| point.pitching_moment_coefficient)
                    }),
            )
            .chain(
                avl_avl
                    .and_then(AvlAnalysisResult::comparable_polar)
                    .into_iter()
                    .flat_map(|polar| {
                        polar
                            .points
                            .iter()
                            .map(|point| point.pitching_moment_coefficient)
                    }),
            ),
        0.08,
    );
    let cdi_range = padded_range(
        vlm_alpha_cdi
            .iter()
            .chain(&avl_alpha_cdi)
            .map(|(_, value)| *value)
            .chain(
                vlm_avl
                    .and_then(AvlAnalysisResult::comparable_polar)
                    .into_iter()
                    .flat_map(|polar| {
                        polar
                            .points
                            .iter()
                            .map(|point| point.induced_drag_coefficient)
                    }),
            )
            .chain(
                avl_avl
                    .and_then(AvlAnalysisResult::comparable_polar)
                    .into_iter()
                    .flat_map(|polar| {
                        polar
                            .points
                            .iter()
                            .map(|point| point.induced_drag_coefficient)
                    }),
            ),
        0.08,
    );

    let cl_axes = Axes2D::new((55.0, 80.0, 380.0, 235.0), alpha_range, cl_range);
    let cm_axes = Axes2D::new((475.0, 80.0, 380.0, 235.0), alpha_range, cm_range);
    let cdi_axes = Axes2D::new((55.0, 390.0, 380.0, 235.0), alpha_range, cdi_range);
    title(
        &mut scene,
        &cl_axes,
        "Lift: both optimized geometries",
        vlm_color,
    );
    title(
        &mut scene,
        &cm_axes,
        "Pitching moment: both optimized geometries",
        vlm_color,
    );
    title(
        &mut scene,
        &cdi_axes,
        "Induced drag: both optimized geometries",
        vlm_color,
    );
    cl_axes.draw_frame_with_labels(&mut scene, pal, "alpha [deg]", "CL");
    cm_axes.draw_frame_with_labels(&mut scene, pal, "alpha [deg]", "Cm");
    cdi_axes.draw_frame_with_labels(&mut scene, pal, "alpha [deg]", "CDi");

    cl_axes.add_line_series(&mut scene, &vlm_alpha_cl, Stroke::new(vlm_color, 1.9));
    cl_axes.add_line_series(
        &mut scene,
        &avl_alpha_cl,
        Stroke::dashed(avl_color, 1.9, 6.0, 3.0),
    );
    cm_axes.add_line_series(&mut scene, &vlm_alpha_cm, Stroke::new(vlm_color, 1.9));
    cm_axes.add_line_series(
        &mut scene,
        &avl_alpha_cm,
        Stroke::dashed(avl_color, 1.9, 6.0, 3.0),
    );
    cdi_axes.add_line_series(&mut scene, &vlm_alpha_cdi, Stroke::new(vlm_color, 1.9));
    cdi_axes.add_line_series(
        &mut scene,
        &avl_alpha_cdi,
        Stroke::dashed(avl_color, 1.9, 6.0, 3.0),
    );

    if let Some(polar) = vlm_avl.and_then(AvlAnalysisResult::comparable_polar) {
        let points = polar
            .points
            .iter()
            .map(|point| (point.alpha_deg, point.lift_coefficient))
            .collect::<Vec<_>>();
        cl_axes.add_line_series(
            &mut scene,
            &points,
            Stroke::dashed(vlm_avl_color, 1.6, 2.0, 2.0),
        );
        let points = polar
            .points
            .iter()
            .map(|point| (point.alpha_deg, point.pitching_moment_coefficient))
            .collect::<Vec<_>>();
        cm_axes.add_line_series(
            &mut scene,
            &points,
            Stroke::dashed(vlm_avl_color, 1.6, 2.0, 2.0),
        );
        let points = polar
            .points
            .iter()
            .map(|point| (point.alpha_deg, point.induced_drag_coefficient))
            .collect::<Vec<_>>();
        cdi_axes.add_line_series(
            &mut scene,
            &points,
            Stroke::dashed(vlm_avl_color, 1.6, 2.0, 2.0),
        );
    }
    if let Some(polar) = avl_avl.and_then(AvlAnalysisResult::comparable_polar) {
        let points = polar
            .points
            .iter()
            .map(|point| (point.alpha_deg, point.lift_coefficient))
            .collect::<Vec<_>>();
        cl_axes.add_line_series(
            &mut scene,
            &points,
            Stroke::dashed(avl_avl_color, 1.6, 2.0, 2.0),
        );
        let points = polar
            .points
            .iter()
            .map(|point| (point.alpha_deg, point.pitching_moment_coefficient))
            .collect::<Vec<_>>();
        cm_axes.add_line_series(
            &mut scene,
            &points,
            Stroke::dashed(avl_avl_color, 1.6, 2.0, 2.0),
        );
        let points = polar
            .points
            .iter()
            .map(|point| (point.alpha_deg, point.induced_drag_coefficient))
            .collect::<Vec<_>>();
        cdi_axes.add_line_series(
            &mut scene,
            &points,
            Stroke::dashed(avl_avl_color, 1.6, 2.0, 2.0),
        );
    }

    let entries = vec![
        (
            "VLM-optimized / ALAS VLM".to_owned(),
            LegendMarker::Line(Stroke::new(vlm_color, 1.9)),
        ),
        (
            "AVL-optimized / ALAS VLM".to_owned(),
            LegendMarker::Line(Stroke::dashed(avl_color, 1.9, 6.0, 3.0)),
        ),
        (
            "VLM-optimized / Athena AVL".to_owned(),
            LegendMarker::Line(Stroke::dashed(vlm_avl_color, 1.6, 2.0, 2.0)),
        ),
        (
            "AVL-optimized / Athena AVL".to_owned(),
            LegendMarker::Line(Stroke::dashed(avl_avl_color, 1.6, 2.0, 2.0)),
        ),
    ];
    draw_legend(&mut scene, [55.0, 650.0], &entries, pal, 8.0);
    scene.add(SceneElement::Text {
        text: format!(
            "Independent optimization comparison. VLM branch: span {:.2} m, area {:.2} m^2; AVL branch: span {:.2} m, area {:.2} m^2. AVL total drag is intentionally excluded; induced drag is Trefftz-plane inviscid.",
            vlm_report.airplane.b_ref,
            vlm_report.airplane.s_ref,
            avl_report.airplane.b_ref,
            avl_report.airplane.s_ref,
        ),
        pos: [55.0, 735.0],
        font_size: 8.5,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Left,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: false,
    });
    scene
}
