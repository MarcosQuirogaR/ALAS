// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py:figure_cg_envelope (L2354-2835)

use super::super::with_alpha;
use super::helpers::{annotate, draw_curve_with_label, draw_trajectory, draw_vline};
use crate::chart_kit::{draw_horizontal_legend, LegendMarker};
use crate::scene::{Axes2D, Color, Fill, Point2D, Scene, SceneElement, Stroke, TextAlign};
use crate::theme::Palette;

/// One named loading state's mass and CG, read directly off
/// [`alas_opt::ModelCgEnvelopeAssessment::loading_states`].
pub(super) struct StatePoint {
    pub(super) mass_kg: f64,
    pub(super) cg_pct_mac: f64,
    pub(super) label: String,
}

/// The governing mechanism of each side of the physical limit at one
/// loading state, for labelling the governing lines.
pub(super) struct LimitMark {
    pub(super) mass_kg: f64,
    pub(super) fwd_pct_mac: f64,
    pub(super) fwd_label: &'static str,
    pub(super) aft_pct_mac: f64,
    pub(super) aft_label: &'static str,
}

/// Footnote on which mechanisms apply to which loading state.
pub(super) const SCOPE_NOTE: &str =
    "Limits are phase-scoped: OEW shows ground limits only (no flight limit); landing trim from ZFW, rotation at TOW";

/// Inputs shared by the CG-envelope preparation and rendering phases. Every
/// aft/forward series here is read from the gate assessment (or linearly
/// interpolated between its named states, see `figure::series_over`), not
/// recomputed from a local formula.
pub(super) struct CgEnvelopeRenderData {
    pub(super) mtow_mass: f64,
    pub(super) mlw_mass: f64,
    pub(super) mzfw_mass: f64,
    pub(super) mtow_label: &'static str,
    pub(super) mzfw_label: &'static str,
    pub(super) clean_np_pct_mac: f64,
    pub(super) w_ops: Vec<f64>,
    pub(super) poly_fwd: Vec<f64>,
    pub(super) poly_aft: Vec<f64>,
    pub(super) aero_aft: Vec<f64>,
    pub(super) ground_aft: Vec<f64>,
    pub(super) tip_aft: Vec<f64>,
    pub(super) aero_active: bool,
    pub(super) ground_active: bool,
    pub(super) tip_active: bool,
    pub(super) max_nose_fwd: Vec<f64>,
    pub(super) scissor_fwd: Vec<f64>,
    pub(super) max_nose_active: bool,
    pub(super) scissor_active: bool,
    pub(super) state_points: Vec<StatePoint>,
    pub(super) limit_marks: Vec<LimitMark>,
}

/// The plot's data-space viewport, derived from every finite `%MAC` series
/// and mass bound the figure draws. Exposed (not just inlined in
/// [`render`]) so a test can reconstruct the exact same [`Axes2D`] and map a
/// gate value forward the same way this function does, rather than
/// independently guessing the figure's padding.
pub(super) fn axes_view(data: &CgEnvelopeRenderData) -> Axes2D {
    let mut all_pct: Vec<f64> = Vec::new();
    all_pct.extend(&data.poly_fwd);
    all_pct.extend(&data.poly_aft);
    all_pct.extend(&data.aero_aft);
    all_pct.extend(&data.ground_aft);
    all_pct.extend(&data.tip_aft);
    all_pct.extend(&data.max_nose_fwd);
    all_pct.extend(&data.scissor_fwd);
    all_pct.extend(data.state_points.iter().map(|p| p.cg_pct_mac));
    all_pct.push(data.clean_np_pct_mac);
    all_pct.retain(|v| v.is_finite());
    let pct_min = all_pct.iter().cloned().fold(f64::INFINITY, f64::min);
    let pct_max = all_pct.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let (pct_min, pct_max) = if pct_min.is_finite() && pct_max.is_finite() && pct_max > pct_min {
        (pct_min, pct_max)
    } else {
        (0.0, 100.0)
    };
    let span = (pct_max - pct_min).max(1.0);
    let view_min = pct_min - span * 0.25;
    let view_max = pct_max + span * 0.25;

    let w_lo = data.w_ops.first().copied().unwrap_or(0.0);
    let w_hi = data.w_ops.last().copied().unwrap_or(data.mtow_mass);
    let y_min = w_lo.min(data.mtow_mass) * 0.85 / 1000.0;
    let y_max = w_hi.max(data.mtow_mass) * 1.2 / 1000.0;
    Axes2D::new(
        (75.0, 55.0, 570.0, 380.0),
        (view_min, view_max),
        (y_min, y_max),
    )
}

/// Render the prepared model CG loading-state figure.
pub(super) fn render(scene: &mut Scene, pal: &Palette, data: CgEnvelopeRenderData) {
    let axes = axes_view(&data);
    let CgEnvelopeRenderData {
        mtow_mass,
        mlw_mass,
        mzfw_mass,
        mtow_label,
        mzfw_label,
        clean_np_pct_mac,
        w_ops,
        poly_fwd,
        poly_aft,
        aero_aft,
        ground_aft,
        tip_aft,
        aero_active,
        ground_active,
        tip_active,
        max_nose_fwd,
        scissor_fwd,
        max_nose_active,
        scissor_active,
        state_points,
        limit_marks,
    } = data;
    axes.draw_frame_with_labels(scene, pal, "CG position [% MAC]", "Mass [t]");
    let view_min = axes.x_min;
    let view_max = axes.x_max;
    let w_lo = w_ops.first().copied().unwrap_or(0.0);
    let w_hi = w_ops.last().copied().unwrap_or(mtow_mass);

    // --- Weight thresholds. Labels follow this rule: a certified/structural
    // weight prints its usual name, otherwise this design's own working
    // value is labelled "design" so it is never read as a certified limit.
    let x_label = view_min + (view_max - view_min) * 0.02;
    for &(w, label, color) in &[
        (mtow_mass, mtow_label, "#e74c3c"),
        (mlw_mass, "MLW", "#9b59b6"),
        (mzfw_mass, mzfw_label, "#3498db"),
    ] {
        let stroke = Stroke::dashed(with_alpha(Color::from_hex(color), 0.4), 1.0, 1.5, 3.0);
        axes.add_line_series(
            scene,
            &[(view_min, w / 1000.0), (view_max, w / 1000.0)],
            stroke,
        );
        annotate(scene, &axes, x_label, w / 1000.0, label, color, 2.0, -6.0);
    }

    // --- Aft physical-limit components, interpolated between
    // the gate's named loading states. The governing line is always drawn;
    // each contributing mechanism is drawn only while it actually governs
    // some state.
    if aero_active {
        draw_curve_with_label(
            scene,
            &axes,
            &w_ops,
            &aero_aft,
            "#3498db",
            6.0,
            2.5,
            w_hi,
            "  Aerodynamic aft",
            "#3498db",
            10.0,
        );
    }
    if ground_active {
        draw_curve_with_label(
            scene,
            &axes,
            &w_ops,
            &ground_aft,
            "#d35400",
            6.0,
            4.0,
            w_hi * 0.85,
            "  Ground min. nose load",
            "#d35400",
            10.0,
        );
    }
    if tip_active {
        draw_curve_with_label(
            scene,
            &axes,
            &w_ops,
            &tip_aft,
            "#c0392b",
            2.0,
            2.0,
            w_hi * 0.7,
            "  Tip-back",
            "#c0392b",
            10.0,
        );
    }
    draw_curve_with_label(
        scene,
        &axes,
        &w_ops,
        &poly_aft,
        "#27ae60",
        0.0,
        0.0,
        w_lo,
        "  Governing aft limit (per state)",
        "#27ae60",
        10.0,
    );

    // --- Forward physical-limit components.
    if max_nose_active {
        draw_curve_with_label(
            scene,
            &axes,
            &w_ops,
            &max_nose_fwd,
            "#8e44ad",
            6.0,
            2.5,
            w_lo,
            "  Max nose load",
            "#8e44ad",
            -10.0,
        );
    }
    if scissor_active {
        draw_curve_with_label(
            scene,
            &axes,
            &w_ops,
            &scissor_fwd,
            "#16a085",
            2.0,
            4.0,
            w_lo * 1.1,
            "  Scissor-plot (estimate)",
            "#16a085",
            -10.0,
        );
    }
    draw_curve_with_label(
        scene,
        &axes,
        &w_ops,
        &poly_fwd,
        "#27ae60",
        0.0,
        0.0,
        w_hi,
        "  Governing fwd limit (per state)",
        "#27ae60",
        -10.0,
    );

    // --- Diagnostic-only clean neutral point: never a limit.
    if clean_np_pct_mac.is_finite() {
        draw_vline(
            scene,
            &axes,
            clean_np_pct_mac,
            "#95a5a6",
            1.0,
            4.0,
            3.0,
            0.6,
            "  Clean NP (diagnostic)",
            0.0,
        );
    }

    // --- Governing-envelope fill between the aft/forward curves.
    let mut poly_pts: Vec<Point2D> = Vec::with_capacity(w_ops.len() * 2);
    for i in 0..w_ops.len() {
        poly_pts.push(axes.map_point(
            poly_fwd[i].clamp(axes.x_min, axes.x_max),
            (w_ops[i] / 1000.0).clamp(axes.y_min, axes.y_max),
        ));
    }
    for i in (0..w_ops.len()).rev() {
        poly_pts.push(axes.map_point(
            poly_aft[i].clamp(axes.x_min, axes.x_max),
            (w_ops[i] / 1000.0).clamp(axes.y_min, axes.y_max),
        ));
    }
    scene.add(SceneElement::Polygon {
        points: poly_pts,
        fill: Some(Fill::new(with_alpha(Color::from_hex("#2ecc71"), 0.2))),
        stroke: None,
    });

    // --- Mechanism of each governing line, named where it changes.
    let mut previous: (&str, &str) = ("", "");
    for mark in &limit_marks {
        for (fwd, pct, label, prev) in [
            (true, mark.fwd_pct_mac, mark.fwd_label, previous.0),
            (false, mark.aft_pct_mac, mark.aft_label, previous.1),
        ] {
            if label == prev || !pct.is_finite() {
                continue;
            }
            let p = axes.map_point(
                pct.clamp(axes.x_min, axes.x_max),
                (mark.mass_kg / 1000.0).clamp(axes.y_min, axes.y_max),
            );
            let dx = if fwd { -4.0 } else { 4.0 };
            scene.add(SceneElement::Text {
                text: label.to_owned(),
                pos: [p[0] + dx, p[1] + if fwd { 9.0 } else { -9.0 }],
                font_size: 7.0,
                color: Color::from_hex("#27ae60"),
                align: if fwd {
                    TextAlign::Right
                } else {
                    TextAlign::Left
                },
                baseline: crate::scene::TextBaseline::Middle,
                angle_deg: 0.0,
                bold: false,
            });
        }
        previous = (mark.fwd_label, mark.aft_label);
    }

    // --- Loading states, connected in ascending-mass order.
    let state_cg: Vec<f64> = state_points.iter().map(|p| p.cg_pct_mac).collect();
    let state_mass: Vec<f64> = state_points.iter().map(|p| p.mass_kg).collect();
    draw_trajectory(
        scene,
        &axes,
        &state_cg,
        &state_mass,
        Stroke::new(Color::from_hex(pal.accent), 2.0),
    );
    for point in &state_points {
        annotate(
            scene,
            &axes,
            point.cg_pct_mac,
            point.mass_kg / 1000.0,
            &point.label,
            pal.accent,
            8.0,
            0.0,
        );
    }

    let legend_entries = [
        (
            "Governing physical limit".to_owned(),
            LegendMarker::Line(Stroke::new(Color::from_hex("#27ae60"), 3.0)),
        ),
        (
            "Loading states".to_owned(),
            LegendMarker::Line(Stroke::new(Color::from_hex(pal.accent), 2.0)),
        ),
    ];
    draw_horizontal_legend(
        scene,
        [axes.left, axes.top + axes.height + 52.0],
        &legend_entries,
        pal,
        8.0,
    );

    scene.add(SceneElement::Text {
        text: SCOPE_NOTE.to_owned(),
        pos: [scene.width * 0.5, 596.0],
        font_size: 7.5,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: crate::scene::TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: false,
    });
    scene.add(SceneElement::Text {
        text: "MODEL-DERIVED CG CHECK ONLY; NOT AN AFM/WBM OPERATIONAL ENVELOPE".to_owned(),
        pos: [scene.width * 0.5, 607.0],
        font_size: 7.5,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: crate::scene::TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: false,
    });
}
