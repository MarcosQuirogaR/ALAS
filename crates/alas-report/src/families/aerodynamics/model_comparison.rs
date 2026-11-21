// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (`figure_model_comparison`)

//! Compare whole-aircraft aerodynamic data products without mixing them with
//! two-dimensional section coefficients.
//!
//! AVL lift and pitching moment are overlaid only after the pipeline's
//! comparability decision. Its Trefftz-plane induced drag receives dedicated
//! cross-check panels; AVL near-field total drag is never plotted against the
//! ALAS drag buildup. When AVL is absent, the original four-panel layout is
//! retained.

use alas_mission::solve::MissionResult;
use alas_pipeline::full_analysis::AnalysisReport;
use alas_pipeline::{AvlAnalysisResult, VspaeroAnalysisResult};

mod comparison_support;
mod optimized_comparison;
use super::polar::polar_up_to_cl_max;
use super::support::{padded_range, ReportingAxis};
use crate::chart_kit::{draw_horizontal_legend_columns, LegendMarker};
use crate::scene::{Axes2D, Color, Scene, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;
use comparison_support::{
    build_solver_comparison, compressible_reference_map, draw_avl_condition_panels,
    fourier_lifting_line_points, helmbold_points, lifting_line_points,
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

/// Minimum vertical gap between one row's x-axis title (which extends about
/// 41 px below its frame: numeric ticks at +7, the axis title at +29 plus its
/// own line height) and the next row's panel headings (about 22 px above
/// their frame). A smaller gap let the shared "alpha [deg]" label collide
/// with the row below it.
const ROW_GAP: f64 = 70.0;
/// Row 1 (drag polar / lift curve). Also +10 px below the automatic figure
/// title compared to the original 55.0 top, on top of that title's own
/// existing ~28 px clearance.
pub(super) const ROW1_TOP: f64 = 65.0;
pub(super) const ROW1_HEIGHT: f64 = 260.0;
/// Row 2 (pitching moment / efficiency).
pub(super) const ROW2_TOP: f64 = ROW1_TOP + ROW1_HEIGHT + ROW_GAP;
pub(super) const ROW2_HEIGHT: f64 = 230.0;
/// Row 3 / row 4 (AVL cross-check panels), drawn by
/// `comparison_support::draw_avl_condition_panels`; shared here so both
/// modules stay in step instead of duplicating the same magic numbers.
pub(super) const ROW3_TOP: f64 = ROW2_TOP + ROW2_HEIGHT + ROW_GAP;
pub(super) const ROW4_TOP: f64 = ROW3_TOP + ROW_HEIGHT + ROW_GAP;
pub(super) const ROW_HEIGHT: f64 = 190.0;

fn title(scene: &mut Scene, axes: &Axes2D, text: &str, color: Color) {
    scene.add(crate::scene::SceneElement::Text {
        text: text.to_owned(),
        pos: [axes.left, axes.top - 10.0],
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
    vspaero: Option<&VspaeroAnalysisResult>,
    avl: Option<&AvlAnalysisResult>,
    theme: Option<&str>,
) -> Scene {
    figure_model_comparison_with_cl_max(report, None, mission, vspaero, avl, theme)
}

/// [`figure_model_comparison`] with the run's clean CL_max: the ALAS VLM curves stop at the last
/// sweep point at or below it and the limit is marked (the lattice is linear, with no stall, so
/// the sweep past CL_max is not a prediction and would squeeze every other curve).
pub fn figure_model_comparison_with_cl_max(
    report: &AnalysisReport,
    cl_max: Option<f64>,
    mission: Option<&MissionResult>,
    vspaero: Option<&VspaeroAnalysisResult>,
    avl: Option<&AvlAnalysisResult>,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let solver_comparison = build_solver_comparison(report, avl);
    // Row tops below must stay in step with `comparison_support::
    // draw_avl_condition_panels`'s ROW3_TOP/ROW4_TOP: both sides need at
    // least ROW_GAP between one row's x-axis title (which extends about 41 px
    // below its frame) and the next row's panel headings (about 22 px above
    // theirs), or the alpha-axis label collides with the row below it.
    let legend_y = if solver_comparison.induced.is_some() {
        ROW4_TOP + ROW_HEIGHT + ROW_GAP
    } else if !solver_comparison.coefficient_points.is_empty() {
        ROW3_TOP + ROW_HEIGHT + ROW_GAP
    } else {
        ROW2_TOP + ROW2_HEIGHT + ROW_GAP
    };
    // Two note lines under the legend when the AVL cross-check rows are drawn.
    let cross_check = !solver_comparison.coefficient_points.is_empty();
    let scene_height = legend_y + if cross_check { 70.0 } else { 55.0 };
    let mut scene = Scene::new(900.0, scene_height, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Whole-Aircraft Model Comparison".to_owned());

    let (polar_cut, cut) = polar_up_to_cl_max(&report.polar, cl_max);
    let p = &polar_cut;
    let lifting_line = lifting_line_points(report);
    let fourier_lifting_line = fourier_lifting_line_points(report);
    let axis = compressible_reference_map(report).map_or_else(
        || ReportingAxis::of(&report.polar),
        |(scale, offset)| ReportingAxis::affine(scale, offset),
    );
    let helmbold: Vec<(f64, f64)> = helmbold_points(report)
        .into_iter()
        .map(|(alpha, lift)| (axis.map(alpha), lift))
        .collect();
    let vspaero_points = vspaero
        .and_then(VspaeroAnalysisResult::comparable_polar)
        .map(|polar| polar.points.as_slice())
        .unwrap_or_default();
    // Cruise-kind segments only, and only at the cruise level: the highest
    // cruise altitude flown, within 150 m. Low-altitude cruise-kind legs
    // (reserve, diversion, hold) sit at other lift coefficients and are not
    // the operating point this comparison names.
    let cruise_segments: Vec<&_> = mission
        .into_iter()
        .flat_map(|result| result.segments.iter())
        .filter(|segment| {
            matches!(
                segment.spec.kind,
                alas_mission::segments::SegmentKind::Cruise { .. }
            )
        })
        .collect();
    let cruise_level_m = cruise_segments
        .iter()
        .flat_map(|segment| segment.conditions.altitude_m.iter().copied())
        .filter(|altitude| altitude.is_finite())
        .fold(f64::NEG_INFINITY, f64::max);
    let cruise_point_count: usize = cruise_segments
        .iter()
        .map(|segment| segment.conditions.altitude_m.len())
        .sum();
    let mission_points: Vec<(f64, f64, f64, f64)> = cruise_segments
        .into_iter()
        .flat_map(|segment| {
            let c = &segment.conditions;
            c.lift_coefficient
                .iter()
                .zip(&c.drag_coefficient)
                .zip(&c.angle_of_attack_rad)
                .zip(&c.altitude_m)
                .filter(|(_, &altitude)| altitude.is_finite() && altitude >= cruise_level_m - 150.0)
                .map(|(((&cl, &cd), &alpha), _)| {
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

    // One more note row when cruise points were dropped by the 150 m rule.
    let dropped_cruise_points = cruise_point_count.saturating_sub(mission_points.len());
    let scene_height = scene_height + if dropped_cruise_points > 0 { 14.0 } else { 0.0 };
    scene.height = scene_height;

    // One angle axis for every figure that draws this sweep: the reporting
    // angle (the true angle on the compressible lattice, the whole-wing
    // Prandtl-Glauert one when it is off), on which VSPAERO is flown and the analytical
    // references above are drawn (see `ReportingAxis`).
    let vlm_alpha_deg = &p.alpha_deg;
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
            .chain(cl_max.filter(|_| cut))
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
        vlm_alpha_deg
            .iter()
            .copied()
            .chain(mission_points.iter().map(|v| v.2))
            .chain(
                lifting_line
                    .iter()
                    .map(|point| axis.map(point.alpha_rad.to_degrees())),
            )
            .chain(
                fourier_lifting_line
                    .iter()
                    .map(|point| axis.map(point.alpha_rad.to_degrees())),
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
        Axes2D::new((60.0, ROW1_TOP, 370.0, ROW1_HEIGHT), x_drag, y_cl),
        Axes2D::new((480.0, ROW1_TOP, 370.0, ROW1_HEIGHT), x_alpha, y_cl),
        Axes2D::new((60.0, ROW2_TOP, 370.0, ROW2_HEIGHT), x_alpha, y_cm),
        Axes2D::new((480.0, ROW2_TOP, 370.0, ROW2_HEIGHT), x_alpha, y_ld),
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
        &vlm_alpha_deg
            .iter()
            .copied()
            .zip(p.cl.iter().copied())
            .collect::<Vec<_>>(),
        alas_vlm.clone(),
    );
    axes[2].add_line_series(
        &mut scene,
        &vlm_alpha_deg
            .iter()
            .copied()
            .zip(p.cm.iter().copied())
            .collect::<Vec<_>>(),
        alas_vlm.clone(),
    );
    axes[3].add_line_series(
        &mut scene,
        &vlm_alpha_deg
            .iter()
            .copied()
            .zip(p.l_over_d.iter().copied())
            .collect::<Vec<_>>(),
        alas_vlm.clone(),
    );

    if let Some(limit) = cl_max.filter(|_| cut) {
        let red = Stroke::dashed(Color::from_hex("#d62728"), 1.0, 4.0, 3.0);
        axes[0].add_line_series(
            &mut scene,
            &[(x_drag.0, limit), (x_drag.1, limit)],
            red.clone(),
        );
        axes[1].add_line_series(&mut scene, &[(x_alpha.0, limit), (x_alpha.1, limit)], red);
        let at = axes[1].map_point(x_alpha.0, limit);
        for (line, text) in [
            format!("Run CLmax {limit:.2}"),
            "ALAS VLM curve stops here: the lattice has no stall".to_owned(),
        ]
        .into_iter()
        .enumerate()
        {
            scene.add(crate::scene::SceneElement::Text {
                text,
                // Above the limit line, where only the left end of the curves is
                // (they rise to the right); 14 px between lines clears 8 pt text.
                pos: [at[0] + 6.0, at[1] - 4.0 - 14.0 * (1 - line) as f64],
                font_size: 8.0,
                color: Color::from_hex("#d62728"),
                align: TextAlign::Left,
                baseline: TextBaseline::Bottom,
                angle_deg: 0.0,
                bold: false,
            });
        }
    }

    if !helmbold.is_empty() {
        axes[1].add_line_series(&mut scene, &helmbold, analytical_stroke(HELMBOLD_COLOR));
    }

    if !fourier_lifting_line.is_empty() {
        axes[1].add_line_series(
            &mut scene,
            &fourier_lifting_line
                .iter()
                .map(|point| {
                    (
                        axis.map(point.alpha_rad.to_degrees()),
                        point.lift_coefficient,
                    )
                })
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
                .map(|point| {
                    (
                        axis.map(point.alpha_rad.to_degrees()),
                        point.lift_coefficient,
                    )
                })
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
                        .map(|ratio| (axis.map(point.alpha_rad.to_degrees()), ratio))
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
    super::support::draw_alpha_axis_note(&mut scene, 450.0, scene_height - 12.0, pal);
    // Which angle each family is drawn against: the references differ, so a
    // 1-2 deg offset between VLM and the analytical lines is read with this.
    scene.add(crate::scene::SceneElement::Text {
        text: "Angle references: VLM, VSPAERO on the reporting angle; lifting-line, \
               Helmbold on the geometric angle mapped onto it (alpha_0L from wing sections)"
            .to_owned(),
        pos: [450.0, scene_height - if cross_check { 40.0 } else { 26.0 }],
        font_size: 8.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });
    if dropped_cruise_points > 0 {
        scene.add(crate::scene::SceneElement::Text {
            text: format!(
                "{dropped_cruise_points} of {cruise_point_count} cruise points dropped: only points within 150 m of the highest cruise altitude flown are shown."
            ),
            pos: [450.0, scene_height - if cross_check { 54.0 } else { 40.0 }],
            font_size: 8.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Center,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }
    if cross_check {
        super::support::draw_cross_check_axis_note(&mut scene, 450.0, scene_height - 26.0, pal);
    }
    scene
}
