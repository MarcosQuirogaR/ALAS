// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Report-local comparison products for the VLM/AVL model-comparison figure.
//!
//! The pipeline owns whether AVL coefficients are physically admissible. This
//! module only pairs the already-admitted points, derives the VLM span
//! efficiency from its induced-drag term, and prepares deterministic chart
//! inputs. AVL total drag deliberately never enters these products.

use std::f64::consts::PI;

use alas_aero::analysis::PolarSweep;
use alas_aero::avl::AvlPolarPoint;
use alas_aero::fourier_lifting_line::{
    AircraftFourierLiftingLine, AircraftLiftingLineResult, FourierLiftingLineSurface,
};
use alas_aero::lifting_line::{helmbold_lift_slope_per_rad, LiftingLineModel, LiftingLinePoint};
use alas_pipeline::full_analysis::AnalysisReport;
use alas_pipeline::{AvlAnalysisResult, AvlAnalysisStatus};

use super::super::support::padded_range;
use super::{ALAS_VLM_COLOR, ATHENA_AVL_COLOR, MODEL_LINE_WIDTH};
use crate::scene::{Axes2D, Color, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::Palette;

const ALPHA_TOLERANCE_DEG: f64 = 1.0e-8;
const FOURIER_LIFTING_LINE_HARMONICS: usize = 12;

/// Return the main wing's planform-weighted geometric zero-lift incidence.
///
/// The report polar can cover only positive-lift cruise angles. Deriving this
/// quantity from wing camber and incidence keeps the classical model
/// independent of VLM output while evaluating it at the same alpha schedule.
fn classical_zero_lift_angle_rad(report: &AnalysisReport) -> Option<f64> {
    let main_wing = report
        .airplane
        .wings
        .iter()
        .filter(|wing| wing.symmetric)
        .filter_map(|wing| {
            wing.projected_area()
                .is_finite()
                .then_some((wing, wing.projected_area()))
        })
        .max_by(|(_, left), (_, right)| left.total_cmp(right))?
        .0;
    let surface = FourierLiftingLineSurface::from_wing(main_wing, 1).ok()?;

    let mut weighted_angle = 0.0;
    let mut area_weight = 0.0;
    for pair in surface.sections.windows(2) {
        let [inboard, outboard] = pair else {
            continue;
        };
        let span_fraction = outboard.span_fraction - inboard.span_fraction;
        let inboard_effective = inboard.zero_lift_angle_rad - inboard.twist_rad;
        let outboard_effective = outboard.zero_lift_angle_rad - outboard.twist_rad;
        let segment_weight = span_fraction * (inboard.chord_m + outboard.chord_m) / 2.0;
        let segment_angle = (inboard.chord_m * inboard_effective
            + outboard.chord_m * outboard_effective)
            / (inboard.chord_m + outboard.chord_m);
        if span_fraction.is_finite()
            && segment_weight.is_finite()
            && segment_angle.is_finite()
            && segment_weight > 0.0
        {
            weighted_angle += segment_weight * segment_angle;
            area_weight += segment_weight;
        }
    }
    (area_weight > 0.0).then_some(weighted_angle / area_weight)
}

pub(super) fn lifting_line_points(report: &AnalysisReport) -> Vec<LiftingLinePoint> {
    let Some(zero_lift_angle_rad) = classical_zero_lift_angle_rad(report) else {
        return Vec::new();
    };
    let model = LiftingLineModel {
        aspect_ratio: report.polar_fit.aspect_ratio,
        span_efficiency: report.polar_fit.oswald_e,
        section_lift_slope_per_rad: 2.0 * PI,
        zero_lift_angle_rad,
        parasite_drag_coefficient: report.polar_fit.cd0,
        moment_coefficient: 0.0,
    };
    let alpha_rad = report
        .polar
        .geometric_alpha_deg
        .iter()
        .map(|alpha| alpha.to_radians())
        .collect::<Vec<_>>();
    let Ok(points) = model.sweep(&alpha_rad) else {
        return Vec::new();
    };
    points
}

pub(super) fn fourier_lifting_line_points(
    report: &AnalysisReport,
) -> Vec<AircraftLiftingLineResult> {
    let Ok(model) =
        AircraftFourierLiftingLine::from_airplane(&report.airplane, FOURIER_LIFTING_LINE_HARMONICS)
    else {
        return Vec::new();
    };
    let alpha_rad = report
        .polar
        .geometric_alpha_deg
        .iter()
        .map(|alpha| alpha.to_radians())
        .collect::<Vec<_>>();
    model.sweep(&alpha_rad).unwrap_or_default()
}

pub(super) fn helmbold_points(report: &AnalysisReport) -> Vec<(f64, f64)> {
    let Some(zero_lift_angle_rad) = classical_zero_lift_angle_rad(report) else {
        return Vec::new();
    };
    let Ok(slope) = helmbold_lift_slope_per_rad(report.polar_fit.aspect_ratio) else {
        return Vec::new();
    };
    report
        .polar
        .geometric_alpha_deg
        .iter()
        .filter(|alpha| alpha.is_finite())
        .map(|&alpha| {
            let lift = slope * (alpha.to_radians() - zero_lift_angle_rad);
            (alpha, lift)
        })
        .collect()
}

/// Induced-drag and span-efficiency data that share the report's references.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct InducedComparison {
    /// `(alpha_deg, ALAS VLM CDi, AVL Trefftz CDi)` samples.
    pub(super) induced_drag_points: Vec<(f64, f64, f64)>,
    /// `(alpha_deg, ALAS VLM e, AVL Trefftz e)` samples.
    pub(super) span_efficiency_points: Vec<(f64, f64, f64)>,
}

/// Data products used by the report without expanding the public pipeline API.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SolverComparison {
    /// `(alpha, ALAS VLM CL, AVL CL, ALAS VLM Cm, AVL Cm)` samples.
    pub(super) coefficient_points: Vec<(f64, f64, f64, f64, f64)>,
    /// Flight-phase label attached to the same-condition comparison.
    pub(super) phase: Option<String>,
    /// Mach number shared by the two solvers.
    pub(super) mach: Option<f64>,
    /// Altitude shared by the two solvers, in meters.
    pub(super) altitude_m: Option<f64>,
    /// Induced-drag comparison, if the physical data supports it.
    pub(super) induced: Option<InducedComparison>,
}

#[derive(Debug, Clone, Copy)]
struct PairedPoint {
    alpha_deg: f64,
    cl_vlm: f64,
    cl_avl: f64,
    cm_vlm: f64,
    cm_avl: f64,
    cdi_vlm: f64,
    cdi_avl: f64,
    e_avl: Option<f64>,
}

/// Build deterministic report data from the existing pipeline result.
pub(super) fn build_solver_comparison(
    report: &AnalysisReport,
    avl: Option<&AvlAnalysisResult>,
) -> SolverComparison {
    let avl_points = avl
        .filter(|result| result.status == AvlAnalysisStatus::CompletedComparable)
        .and_then(AvlAnalysisResult::comparable_polar)
        .map(|polar| polar.points.clone())
        .unwrap_or_default();
    let reference = avl.and_then(|result| result.comparison_reference.as_ref());
    let reference_polar = reference.map_or_else(
        || report.polar.clone(),
        |reference| {
            let mut polar = reference.vlm_polar.clone();
            if polar.geometric_alpha_deg.len() == reference.geometric_alpha_deg.len() {
                polar.alpha_deg = reference.geometric_alpha_deg.clone();
            }
            polar
        },
    );
    let paired = aligned_points(&reference_polar, &avl_points);
    let coefficient_points = paired
        .iter()
        .map(|point| {
            (
                point.alpha_deg,
                point.cl_vlm,
                point.cl_avl,
                point.cm_vlm,
                point.cm_avl,
            )
        })
        .collect();
    let induced = (!avl_points.is_empty())
        .then(|| induced_comparison(report, &reference_polar, &paired).ok())
        .flatten();
    SolverComparison {
        coefficient_points,
        phase: reference.map(|reference| reference.phase.clone()),
        mach: reference.map(|reference| reference.mach),
        altitude_m: reference.map(|reference| reference.altitude_m),
        induced,
    }
}

/// Draw the two dedicated inviscid cross-check panels.
pub(super) fn draw_avl_condition_panels(
    scene: &mut Scene,
    pal: &Palette,
    alpha_range: (f64, f64),
    comparison: &SolverComparison,
) {
    let condition = format!(
        "{} (M={:.3}, h={:.0} m)",
        comparison.phase.as_deref().unwrap_or("shared condition"),
        comparison.mach.unwrap_or(f64::NAN),
        comparison.altitude_m.unwrap_or(f64::NAN),
    );
    let lift_axes = Axes2D::new(
        (60.0, 650.0, 370.0, 190.0),
        alpha_range,
        padded_range(
            comparison
                .coefficient_points
                .iter()
                .flat_map(|(_, vlm, avl, _, _)| [*vlm, *avl]),
            0.08,
        ),
    );
    panel_title(
        scene,
        &lift_axes,
        &format!("Lift cross-check -- {condition}"),
        pal,
    );
    lift_axes.draw_frame_with_labels(scene, pal, "alpha [deg]", "CL");
    let vlm_lift = comparison
        .coefficient_points
        .iter()
        .map(|(alpha, vlm, _, _, _)| (*alpha, *vlm))
        .collect::<Vec<_>>();
    let avl_lift = comparison
        .coefficient_points
        .iter()
        .map(|(alpha, _, avl, _, _)| (*alpha, *avl))
        .collect::<Vec<_>>();
    lift_axes.add_line_series(
        scene,
        &vlm_lift,
        Stroke::new(Color::from_hex(ALAS_VLM_COLOR), MODEL_LINE_WIDTH),
    );
    lift_axes.add_line_series(
        scene,
        &avl_lift,
        Stroke::new(Color::from_hex(ATHENA_AVL_COLOR), MODEL_LINE_WIDTH),
    );

    let moment_axes = Axes2D::new(
        (480.0, 650.0, 370.0, 190.0),
        alpha_range,
        padded_range(
            comparison
                .coefficient_points
                .iter()
                .flat_map(|(_, _, _, vlm, avl)| [*vlm, *avl]),
            0.08,
        ),
    );
    panel_title(scene, &moment_axes, "Pitching-moment cross-check", pal);
    moment_axes.draw_frame_with_labels(scene, pal, "alpha [deg]", "Cm");
    let vlm_moment = comparison
        .coefficient_points
        .iter()
        .map(|(alpha, _, _, vlm, _)| (*alpha, *vlm))
        .collect::<Vec<_>>();
    let avl_moment = comparison
        .coefficient_points
        .iter()
        .map(|(alpha, _, _, _, avl)| (*alpha, *avl))
        .collect::<Vec<_>>();
    moment_axes.add_line_series(
        scene,
        &vlm_moment,
        Stroke::new(Color::from_hex(ALAS_VLM_COLOR), MODEL_LINE_WIDTH),
    );
    moment_axes.add_line_series(
        scene,
        &avl_moment,
        Stroke::new(Color::from_hex(ATHENA_AVL_COLOR), MODEL_LINE_WIDTH),
    );

    let Some(comparison) = comparison.induced.as_ref() else {
        return;
    };
    let induced_axes = Axes2D::new(
        (60.0, 910.0, 370.0, 190.0),
        alpha_range,
        padded_range(
            comparison
                .induced_drag_points
                .iter()
                .flat_map(|(_, vlm, avl)| [*vlm, *avl]),
            0.08,
        ),
    );
    panel_title(scene, &induced_axes, "Induced drag cross-check", pal);
    induced_axes.draw_frame_with_labels(scene, pal, "alpha [deg]", "CDi");
    let vlm = comparison
        .induced_drag_points
        .iter()
        .map(|(alpha, value, _)| (*alpha, *value))
        .collect::<Vec<_>>();
    let avl = comparison
        .induced_drag_points
        .iter()
        .map(|(alpha, _, value)| (*alpha, *value))
        .collect::<Vec<_>>();
    induced_axes.add_line_series(
        scene,
        &vlm,
        Stroke::new(Color::from_hex(ALAS_VLM_COLOR), MODEL_LINE_WIDTH),
    );
    induced_axes.add_line_series(
        scene,
        &avl,
        Stroke::new(Color::from_hex(ATHENA_AVL_COLOR), MODEL_LINE_WIDTH),
    );

    let efficiency_axes = Axes2D::new(
        (480.0, 910.0, 370.0, 190.0),
        alpha_range,
        padded_range(
            comparison
                .span_efficiency_points
                .iter()
                .flat_map(|(_, vlm, avl)| [*vlm, *avl]),
            0.08,
        ),
    );
    panel_title(scene, &efficiency_axes, "Span efficiency cross-check", pal);
    efficiency_axes.draw_frame_with_labels(scene, pal, "alpha [deg]", "e");
    if comparison.span_efficiency_points.is_empty() {
        scene.add(SceneElement::Text {
            text: "Span efficiency unavailable: no finite paired e values.".to_owned(),
            pos: [efficiency_axes.left + 12.0, efficiency_axes.top + 70.0],
            font_size: 9.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: false,
        });
    } else {
        let vlm = comparison
            .span_efficiency_points
            .iter()
            .map(|(alpha, value, _)| (*alpha, *value))
            .collect::<Vec<_>>();
        let avl = comparison
            .span_efficiency_points
            .iter()
            .map(|(alpha, _, value)| (*alpha, *value))
            .collect::<Vec<_>>();
        efficiency_axes.add_line_series(
            scene,
            &vlm,
            Stroke::new(Color::from_hex(ALAS_VLM_COLOR), MODEL_LINE_WIDTH),
        );
        efficiency_axes.add_line_series(
            scene,
            &avl,
            Stroke::new(Color::from_hex(ATHENA_AVL_COLOR), MODEL_LINE_WIDTH),
        );
    }
}

fn panel_title(scene: &mut Scene, axes: &Axes2D, text: &str, pal: &Palette) {
    scene.add(SceneElement::Text {
        text: text.to_owned(),
        pos: [axes.left, axes.top - 8.0],
        font_size: 10.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
}

fn aligned_points(polar: &PolarSweep, avl_points: &[AvlPolarPoint]) -> Vec<PairedPoint> {
    polar
        .alpha_deg
        .iter()
        .zip(&polar.cl)
        .zip(&polar.cm)
        .zip(&polar.cd_induced)
        .zip(avl_points)
        .filter_map(|((((&alpha_deg, &cl_vlm), &cm_vlm), &cdi_vlm), avl)| {
            let finite = [
                alpha_deg,
                cl_vlm,
                cm_vlm,
                cdi_vlm,
                avl.alpha_deg,
                avl.lift_coefficient,
                avl.pitching_moment_coefficient,
                avl.induced_drag_coefficient,
            ]
            .iter()
            .all(|value| value.is_finite());
            (finite && (alpha_deg - avl.alpha_deg).abs() <= ALPHA_TOLERANCE_DEG).then_some(
                PairedPoint {
                    alpha_deg,
                    cl_vlm,
                    cl_avl: avl.lift_coefficient,
                    cm_vlm,
                    cm_avl: avl.pitching_moment_coefficient,
                    cdi_vlm,
                    cdi_avl: avl.induced_drag_coefficient,
                    e_avl: avl
                        .span_efficiency
                        .filter(|value| value.is_finite() && *value > 0.0),
                },
            )
        })
        .collect()
}

fn induced_comparison(
    report: &AnalysisReport,
    polar: &PolarSweep,
    paired: &[PairedPoint],
) -> Result<InducedComparison, String> {
    if polar.alpha_deg.len() != paired.len() {
        return Err("the ALAS VLM and AVL alpha schedules are only partially aligned.".to_owned());
    }
    let aspect_ratio = report.polar_fit.aspect_ratio;
    if !aspect_ratio.is_finite() || aspect_ratio <= 0.0 {
        return Err(
            "ALAS VLM aspect ratio is unavailable for span-efficiency derivation.".to_owned(),
        );
    }
    let induced_drag_points = paired
        .iter()
        .filter(|point| point.cdi_vlm >= 0.0 && point.cdi_avl >= 0.0)
        .map(|point| (point.alpha_deg, point.cdi_vlm, point.cdi_avl))
        .collect::<Vec<_>>();
    if induced_drag_points.is_empty() {
        return Err(
            "no finite, non-negative paired induced-drag values were available.".to_owned(),
        );
    }
    let span_efficiency_points = paired
        .iter()
        .filter_map(|point| {
            let vlm = derived_span_efficiency(point.cl_vlm, point.cdi_vlm, aspect_ratio)?;
            let avl = point.e_avl?;
            Some((point.alpha_deg, vlm, avl))
        })
        .collect::<Vec<_>>();
    Ok(InducedComparison {
        induced_drag_points,
        span_efficiency_points,
    })
}

fn derived_span_efficiency(cl: f64, induced_drag: f64, aspect_ratio: f64) -> Option<f64> {
    (cl.is_finite() && induced_drag.is_finite() && induced_drag > f64::EPSILON)
        .then_some(cl.powi(2) / (PI * aspect_ratio * induced_drag))
        .filter(|value| value.is_finite() && *value > 0.0)
}
