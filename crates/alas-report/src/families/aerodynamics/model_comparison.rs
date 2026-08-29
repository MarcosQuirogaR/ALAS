// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (`figure_model_comparison`)
// Reference: alas @ rust-port-baseline.

//! Compare whole-aircraft aerodynamic data products without mixing them with
//! two-dimensional section coefficients.
//!
//! AVL lift and pitching moment are overlaid only after the pipeline's
//! comparability decision. Its Trefftz-plane induced drag receives dedicated
//! cross-check panels; AVL near-field total drag is never plotted against the
//! ALAS drag buildup. When AVL is absent, the original four-panel layout is
//! retained.

use alas_aero::mses::MsesPolarResult;
use alas_mission::solve::MissionResult;
use alas_pipeline::full_analysis::AnalysisReport;
use alas_pipeline::{AvlAnalysisResult, VspaeroAnalysisResult};

mod comparison_support;
mod optimized_comparison;
use super::support::padded_range;
use crate::chart_kit::{draw_horizontal_legend_columns, LegendMarker};
use crate::scene::{Axes2D, Color, Scene, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;
use comparison_support::{
    build_solver_comparison, draw_avl_condition_panels, fourier_lifting_line_points,
    helmbold_points, lifting_line_points,
};
pub use optimized_comparison::figure_optimized_aircraft_comparison;

/// Color-blind-safe series colors keep model identity legible without relying
/// on point markers or one-off line widths.
pub(super) const ALAS_VLM_COLOR: &str = "#0072b2";
const PRANDTL_LIFTING_LINE_COLOR: &str = "#cc79a7";
const FOURIER_LIFTING_LINE_COLOR: &str = "#009e73";
const HELMBOLD_COLOR: &str = "#e69f00";
const VSPAERO_COLOR: &str = "#d55e00";
pub(super) const ATHENA_AVL_COLOR: &str = "#56b4e9";
const MISSION_COLOR: &str = "#767676";
pub(super) const MODEL_LINE_WIDTH: f64 = 1.8;
/// Display name distinguishes the in-process model from the optional GPL AVL
/// executable. The Fourier model is available on every normal ALAS run.
const LOCAL_FOURIER_LIFTING_LINE_LABEL: &str = "ALAS local Fourier lifting-line";

fn title(scene: &mut Scene, axes: &Axes2D, text: &str, color: Color) {
    scene.add(crate::scene::SceneElement::Text {
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

fn solver_stroke(color: &str) -> Stroke {
    Stroke::new(Color::from_hex(color), MODEL_LINE_WIDTH)
}

fn analytical_stroke(color: &str) -> Stroke {
    Stroke::dashed(Color::from_hex(color), MODEL_LINE_WIDTH, 5.0, 3.0)
}

fn operating_point_stroke() -> Stroke {
    Stroke::dashed(Color::from_hex(MISSION_COLOR), MODEL_LINE_WIDTH, 2.0, 2.0)
}

/// Overlay the whole-aircraft report polar with whole-aircraft mission points.
///
/// MSES solves one airfoil section, so its coefficients cannot be overlaid on
/// the aircraft-reference-area coefficients produced by the VLM and mission
/// models. Its status is available from the Model Comparison GUI, while the
/// section curves remain in the dedicated MSES figures.
pub fn figure_model_comparison(
    report: &AnalysisReport,
    mission: Option<&MissionResult>,
    _mses: Option<&MsesPolarResult>,
    vspaero: Option<&VspaeroAnalysisResult>,
    avl: Option<&AvlAnalysisResult>,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let solver_comparison = build_solver_comparison(report, avl);
    let legend_y = if solver_comparison.induced.is_some() {
        1155.0
    } else if !solver_comparison.coefficient_points.is_empty() {
        895.0
    } else {
        650.0
    };
    let scene_height = legend_y + 55.0;
    let mut scene = Scene::new(900.0, scene_height, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Whole-Aircraft Model Comparison".to_owned());

    let p = &report.polar;
    let lifting_line = lifting_line_points(report);
    let fourier_lifting_line = fourier_lifting_line_points(report);
    let helmbold = helmbold_points(report);
    let vspaero_points = vspaero
        .and_then(VspaeroAnalysisResult::comparable_polar)
        .map(|polar| polar.points.as_slice())
        .unwrap_or_default();
    let mission_points: Vec<(f64, f64, f64, f64)> = mission
        .into_iter()
        .flat_map(|result| result.segments.iter())
        .filter(|segment| {
            matches!(
                segment.spec.kind,
                alas_mission::segments::SegmentKind::Cruise { .. }
            )
        })
        .flat_map(|segment| {
            let c = &segment.conditions;
            c.lift_coefficient
                .iter()
                .zip(&c.drag_coefficient)
                .zip(&c.angle_of_attack_rad)
                .map(|((&cl, &cd), &alpha)| {
                    (
                        cl,
                        cd,
                        alpha.to_degrees(),
                        if cd > 0.0 { cl / cd } else { 0.0 },
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();

    let x_drag = padded_range(
        p.cd.iter()
            .copied()
            .chain(mission_points.iter().map(|v| v.1))
            .chain(lifting_line.iter().map(|point| point.drag_coefficient)),
        0.08,
    );
    let y_cl = padded_range(
        p.cl.iter()
            .copied()
            .chain(mission_points.iter().map(|v| v.0))
            .chain(lifting_line.iter().map(|point| point.lift_coefficient))
            .chain(
                fourier_lifting_line
                    .iter()
                    .map(|point| point.lift_coefficient),
            )
            .chain(helmbold.iter().map(|point| point.1))
            .chain(vspaero_points.iter().map(|point| point.lift_coefficient)),
        0.08,
    );
    let x_alpha = padded_range(
        p.geometric_alpha_deg
            .iter()
            .copied()
            .chain(mission_points.iter().map(|v| v.2))
            .chain(
                lifting_line
                    .iter()
                    .map(|point| point.alpha_rad.to_degrees()),
            )
            .chain(
                fourier_lifting_line
                    .iter()
                    .map(|point| point.alpha_rad.to_degrees()),
            )
            .chain(helmbold.iter().map(|point| point.0))
            .chain(vspaero_points.iter().map(|point| point.alpha_deg)),
        0.08,
    );
    let y_cm = padded_range(
        p.cm.iter().copied().chain(
            vspaero_points
                .iter()
                .map(|point| point.pitching_moment_coefficient),
        ),
        0.08,
    );
    let y_ld = padded_range(
        p.l_over_d
            .iter()
            .copied()
            .chain(mission_points.iter().map(|v| v.3))
            .chain(lifting_line.iter().filter_map(|point| point.lift_to_drag)),
        0.08,
    );

    let axes = [
        Axes2D::new((60.0, 55.0, 370.0, 260.0), x_drag, y_cl),
        Axes2D::new((480.0, 55.0, 370.0, 260.0), x_alpha, y_cl),
        Axes2D::new((60.0, 365.0, 370.0, 230.0), x_alpha, y_cm),
        Axes2D::new((480.0, 365.0, 370.0, 230.0), x_alpha, y_ld),
    ];
    for (axis, (title_text, x_label, y_label)) in axes.iter().zip([
        ("Drag polar", "CD", "CL"),
        ("Lift curve", "\u{03b1} [deg]", "CL"),
        ("Pitching moment", "\u{03b1} [deg]", "Cm"),
        ("Efficiency", "\u{03b1} [deg]", "L/D"),
    ]) {
        axis.draw_frame_with_labels(&mut scene, pal, x_label, y_label);
        title(&mut scene, axis, title_text, Color::from_hex(pal.title));
    }
    if !solver_comparison.coefficient_points.is_empty() {
        let comparison_alpha = padded_range(
            solver_comparison
                .coefficient_points
                .iter()
                .map(|point| point.0),
            0.08,
        );
        draw_avl_condition_panels(&mut scene, pal, comparison_alpha, &solver_comparison);
    }

    let alas_vlm = solver_stroke(ALAS_VLM_COLOR);
    axes[0].add_line_series(
        &mut scene,
        &p.cd
            .iter()
            .copied()
            .zip(p.cl.iter().copied())
            .collect::<Vec<_>>(),
        alas_vlm.clone(),
    );
    axes[1].add_line_series(
        &mut scene,
        &p.geometric_alpha_deg
            .iter()
            .copied()
            .zip(p.cl.iter().copied())
            .collect::<Vec<_>>(),
        alas_vlm.clone(),
    );
    axes[2].add_line_series(
        &mut scene,
        &p.geometric_alpha_deg
            .iter()
            .copied()
            .zip(p.cm.iter().copied())
            .collect::<Vec<_>>(),
        alas_vlm.clone(),
    );
    axes[3].add_line_series(
        &mut scene,
        &p.geometric_alpha_deg
            .iter()
            .copied()
            .zip(p.l_over_d.iter().copied())
            .collect::<Vec<_>>(),
        alas_vlm.clone(),
    );

    if !helmbold.is_empty() {
        axes[1].add_line_series(&mut scene, &helmbold, analytical_stroke(HELMBOLD_COLOR));
    }

    if !fourier_lifting_line.is_empty() {
        axes[1].add_line_series(
            &mut scene,
            &fourier_lifting_line
                .iter()
                .map(|point| (point.alpha_rad.to_degrees(), point.lift_coefficient))
                .collect::<Vec<_>>(),
            analytical_stroke(FOURIER_LIFTING_LINE_COLOR),
        );
    }

    if !vspaero_points.is_empty() {
        let vspaero = solver_stroke(VSPAERO_COLOR);
        let lift = vspaero_points
            .iter()
            .map(|point| (point.alpha_deg, point.lift_coefficient))
            .collect::<Vec<_>>();
        let moment = vspaero_points
            .iter()
            .map(|point| (point.alpha_deg, point.pitching_moment_coefficient))
            .collect::<Vec<_>>();
        axes[1].add_line_series(&mut scene, &lift, vspaero.clone());
        axes[2].add_line_series(&mut scene, &moment, vspaero);
    }

    if !lifting_line.is_empty() {
        let prandtl = analytical_stroke(PRANDTL_LIFTING_LINE_COLOR);
        axes[0].add_line_series(
            &mut scene,
            &lifting_line
                .iter()
                .map(|point| (point.drag_coefficient, point.lift_coefficient))
                .collect::<Vec<_>>(),
            prandtl.clone(),
        );
        axes[1].add_line_series(
            &mut scene,
            &lifting_line
                .iter()
                .map(|point| (point.alpha_rad.to_degrees(), point.lift_coefficient))
                .collect::<Vec<_>>(),
            prandtl.clone(),
        );
        axes[3].add_line_series(
            &mut scene,
            &lifting_line
                .iter()
                .filter_map(|point| {
                    point
                        .lift_to_drag
                        .map(|ratio| (point.alpha_rad.to_degrees(), ratio))
                })
                .collect::<Vec<_>>(),
            prandtl,
        );
    }

    if !mission_points.is_empty() {
        let mission = operating_point_stroke();
        axes[0].add_line_series(
            &mut scene,
            &mission_points
                .iter()
                .map(|v| (v.1, v.0))
                .collect::<Vec<_>>(),
            mission.clone(),
        );
        axes[1].add_line_series(
            &mut scene,
            &mission_points
                .iter()
                .map(|v| (v.2, v.0))
                .collect::<Vec<_>>(),
            mission.clone(),
        );
        axes[3].add_line_series(
            &mut scene,
            &mission_points
                .iter()
                .map(|v| (v.2, v.3))
                .collect::<Vec<_>>(),
            mission,
        );
    }
    let mut entries = vec![("ALAS VLM".to_owned(), LegendMarker::Line(alas_vlm))];
    if !fourier_lifting_line.is_empty() {
        entries.push((
            LOCAL_FOURIER_LIFTING_LINE_LABEL.to_owned(),
            LegendMarker::Line(analytical_stroke(FOURIER_LIFTING_LINE_COLOR)),
        ));
    }
    if !lifting_line.is_empty() {
        entries.push((
            "Prandtl lifting-line".to_owned(),
            LegendMarker::Line(analytical_stroke(PRANDTL_LIFTING_LINE_COLOR)),
        ));
    }
    if !helmbold.is_empty() {
        entries.push((
            "Helmbold lift slope".to_owned(),
            LegendMarker::Line(analytical_stroke(HELMBOLD_COLOR)),
        ));
    }
    if !vspaero_points.is_empty() {
        entries.push((
            "VSPAERO VLM".to_owned(),
            LegendMarker::Line(solver_stroke(VSPAERO_COLOR)),
        ));
    }
    if !solver_comparison.coefficient_points.is_empty() {
        entries.push((
            "Athena AVL (cross-check)".to_owned(),
            LegendMarker::Line(solver_stroke(ATHENA_AVL_COLOR)),
        ));
    }
    if !mission_points.is_empty() {
        entries.push((
            "Mission operating conditions".to_owned(),
            LegendMarker::Line(operating_point_stroke()),
        ));
    }
    draw_horizontal_legend_columns(&mut scene, [60.0, legend_y], &entries, pal, 8.0);
    scene
}
