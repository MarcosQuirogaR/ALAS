// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::super::with_alpha;
use super::helpers::{annotate, draw_curve_with_label, draw_trajectory, draw_vline};
use crate::chart_kit::{draw_horizontal_legend, LegendMarker};
use crate::scene::{
    Axes2D, Color, Fill, Point2D, Scene, SceneElement, Stroke, TextAlign, TextBaseline,
};
use crate::theme::Palette;

/// Inputs shared by the CG-envelope preparation and rendering phases.
pub(super) struct CgEnvelopeRenderData {
    pub(super) mac: f64,
    pub(super) x_mac_le: f64,
    pub(super) oew_mass: f64,
    pub(super) payload: f64,
    pub(super) fuel: f64,
    pub(super) mtow_mass: f64,
    pub(super) mlw_mass: f64,
    pub(super) mzfw_mass: f64,
    pub(super) oew_cg_mac: f64,
    pub(super) oew_cg_x: f64,
    pub(super) payload_cg_x: f64,
    pub(super) fuel_cg_x: f64,
    pub(super) target_sm: f64,
    pub(super) fwd_limit_mac: f64,
    pub(super) aft_limit_mac: f64,
    pub(super) tip_over_pct: f64,
    pub(super) np_pct: f64,
    pub(super) w_calc: Vec<f64>,
    pub(super) c_nlg_str: Vec<f64>,
    pub(super) c_mlg_str: Vec<f64>,
    pub(super) c_nlg_min: Vec<f64>,
    pub(super) pts_cg_a: Vec<f64>,
    pub(super) pts_weight_a: Vec<f64>,
    pub(super) w_ops: Vec<f64>,
    pub(super) poly_fwd: Vec<f64>,
    pub(super) poly_aft: Vec<f64>,
    pub(super) nlg_strength_limits: bool,
    pub(super) mlg_strength_limits: bool,
    pub(super) nose_load_limits: bool,
}

/// Render the prepared model CG loading-state figure.
pub(super) fn render(scene: &mut Scene, pal: &Palette, data: CgEnvelopeRenderData) {
    let CgEnvelopeRenderData {
        mac,
        x_mac_le,
        oew_mass,
        payload,
        fuel,
        mtow_mass,
        mlw_mass,
        mzfw_mass,
        oew_cg_mac,
        oew_cg_x,
        payload_cg_x,
        fuel_cg_x,
        target_sm,
        fwd_limit_mac,
        aft_limit_mac,
        tip_over_pct,
        np_pct,
        w_calc,
        c_nlg_str,
        c_mlg_str,
        c_nlg_min,
        pts_cg_a,
        pts_weight_a,
        w_ops,
        poly_fwd,
        poly_aft,
        nlg_strength_limits,
        mlg_strength_limits,
        nose_load_limits,
    } = data;
    let composite_cg = |m_oew: f64,
                        x_oew: f64,
                        m_payload: f64,
                        x_payload: f64,
                        m_fuel: f64,
                        x_fuel: f64|
     -> (f64, f64) {
        let m_tot = (m_oew + m_payload + m_fuel).max(1.0);
        let x_cg = (m_oew * x_oew + m_payload * x_payload + m_fuel * x_fuel) / m_tot;
        (m_tot, ((x_cg - x_mac_le) / mac.max(0.001)) * 100.0)
    };

    // --- Dynamic viewport bounds -------------------------------------------
    let x_min_poly = poly_fwd.iter().cloned().fold(f64::INFINITY, f64::min);
    let x_max_poly = poly_aft.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let width = x_max_poly - x_min_poly;
    let view_min = x_min_poly.min(fwd_limit_mac) - width * 0.4;
    let view_max = x_max_poly.max(tip_over_pct).max(np_pct) + width * 0.4;

    let y_min = oew_mass * 0.7 / 1000.0;
    let y_max = mtow_mass * 1.25 / 1000.0;
    let axes = Axes2D::new(
        (75.0, 55.0, 570.0, 380.0),
        (view_min, view_max),
        (y_min, y_max),
    );
    axes.draw_frame_with_labels(scene, pal, "CG position [% MAC]", "Mass [t]");

    // --- Weight thresholds -----------------------------------------------------
    let x_label = view_min + (view_max - view_min) * 0.02;
    for &(w, label, color) in &[
        (mtow_mass, "MTOW", "#e74c3c"),
        (mlw_mass, "MLW", "#9b59b6"),
        (mzfw_mass, "MZFW", "#3498db"),
        (oew_mass, "OEW", pal.accent),
    ] {
        let stroke = Stroke::dashed(with_alpha(Color::from_hex(color), 0.4), 1.0, 1.5, 3.0);
        axes.add_line_series(
            scene,
            &[(view_min, w / 1000.0), (view_max, w / 1000.0)],
            stroke,
        );
        let p = axes.map_point(x_label, w / 1000.0);
        scene.add(SceneElement::Text {
            text: label.to_owned(),
            pos: [p[0], p[1] - 3.0],
            font_size: 9.0,
            color: Color::from_hex(color),
            align: TextAlign::Left,
            baseline: TextBaseline::Bottom,
            angle_deg: 0.0,
            bold: true,
        });
    }

    // --- NLG/MLG max-strength and min-nose-load curves. Dash-dot approximated
    // with a dash/gap ratio distinct from the plain-dashed curves below (see
    // module doc) --------------------------------------------------------------
    if nlg_strength_limits {
        draw_curve_with_label(
            scene,
            &axes,
            &w_calc,
            &c_nlg_str,
            "#e74c3c",
            6.0,
            2.5,
            mtow_mass * 1.1,
            "",
            "#c0392b",
            -14.0,
        );
    }
    if mlg_strength_limits {
        draw_curve_with_label(
            scene,
            &axes,
            &w_calc,
            &c_mlg_str,
            "#2980b9",
            6.0,
            2.5,
            mtow_mass * 0.95,
            "",
            "#2980b9",
            14.0,
        );
    }
    if nose_load_limits {
        draw_curve_with_label(
            scene,
            &axes,
            &w_calc,
            &c_nlg_min,
            "#d35400",
            6.0,
            4.0,
            oew_mass * 1.1,
            "",
            "#d35400",
            16.0,
        );
    }

    // --- Vertical aerodynamic/gear limit lines ----------------------------
    draw_vline(
        scene,
        &axes,
        fwd_limit_mac,
        pal.title,
        1.5,
        1.5,
        3.0,
        0.7,
        "  Fwd Aero Limit",
        0.0,
    );
    draw_vline(
        scene,
        &axes,
        aft_limit_mac,
        "#f39c12",
        2.0,
        7.0,
        3.0,
        0.7,
        &format!("  Stability Limit (NP-{}%)", (target_sm * 100.0) as i64),
        16.0,
    );
    draw_vline(
        scene,
        &axes,
        np_pct,
        "#3498db",
        1.5,
        6.0,
        2.5,
        0.5,
        "  Neutral Point (NP)",
        32.0,
    );
    draw_vline(
        scene,
        &axes,
        tip_over_pct,
        "#c0392b",
        2.0,
        0.0,
        0.0,
        0.3,
        "  TIP-OVER (MLG)",
        48.0,
    );

    // --- Model loading-state fill and bold outline ---------------------------
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

    let outline_stroke = Stroke::new(Color::from_hex("#27ae60"), 3.0);
    let fwd_series: Vec<(f64, f64)> = (0..w_ops.len())
        .map(|i| (poly_fwd[i], w_ops[i] / 1000.0))
        .collect();
    let aft_series: Vec<(f64, f64)> = (0..w_ops.len())
        .map(|i| (poly_aft[i], w_ops[i] / 1000.0))
        .collect();
    axes.add_line_series(scene, &fwd_series, outline_stroke.clone());
    axes.add_line_series(scene, &aft_series, outline_stroke.clone());
    axes.add_line_series(
        scene,
        &[
            (poly_fwd[0], oew_mass / 1000.0),
            (poly_aft[0], oew_mass / 1000.0),
        ],
        outline_stroke.clone(),
    );
    let last = w_ops.len() - 1;
    axes.add_line_series(
        scene,
        &[
            (poly_fwd[last], mtow_mass / 1000.0),
            (poly_aft[last], mtow_mass / 1000.0),
        ],
        outline_stroke,
    );

    // --- Loading trajectories -------------------------------------------------
    let payload_stroke = Stroke::new(Color::from_hex(pal.accent), 2.0);
    let fuel_stroke = Stroke::new(Color::from_hex("#e67e22"), 2.0);
    draw_trajectory(
        scene,
        &axes,
        &pts_cg_a[0..15],
        &pts_weight_a[0..15],
        payload_stroke,
    );
    draw_trajectory(
        scene,
        &axes,
        &pts_cg_a[14..30],
        &pts_weight_a[14..30],
        fuel_stroke,
    );

    // --- Key-point annotations -------------------------------------------------
    let (_, cg_mzfw_mac) = composite_cg(oew_mass, oew_cg_x, payload, payload_cg_x, 0.0, fuel_cg_x);
    let (_, cg_mtow_mac) = composite_cg(oew_mass, oew_cg_x, payload, payload_cg_x, fuel, fuel_cg_x);
    annotate(
        scene,
        &axes,
        oew_cg_mac,
        oew_mass / 1000.0,
        "OEW",
        pal.accent,
        -18.0,
        12.0,
    );
    annotate(
        scene,
        &axes,
        cg_mzfw_mac,
        mzfw_mass / 1000.0,
        "MZFW",
        "#3498db",
        8.0,
        0.0,
    );
    annotate(
        scene,
        &axes,
        cg_mtow_mac,
        mtow_mass / 1000.0,
        "MTOW",
        "#e74c3c",
        8.0,
        -10.0,
    );

    // The framed axes already own the labels; a second hand-positioned pair
    // occupies the same pixels once either string is localized.
    let mut legend_entries = vec![
        (
            "Model state limits".to_owned(),
            LegendMarker::Line(Stroke::new(Color::from_hex("#27ae60"), 3.0)),
        ),
        (
            "Payload loading".to_owned(),
            LegendMarker::Line(Stroke::new(Color::from_hex(pal.accent), 2.0)),
        ),
        (
            "Fuel loading".to_owned(),
            LegendMarker::Line(Stroke::new(Color::from_hex("#e67e22"), 2.0)),
        ),
    ];
    if nlg_strength_limits {
        legend_entries.push((
            "NLG max strength".to_owned(),
            LegendMarker::Line(Stroke::dashed(Color::from_hex("#e74c3c"), 1.5, 6.0, 2.5)),
        ));
    }
    if mlg_strength_limits {
        legend_entries.push((
            "MLG max strength".to_owned(),
            LegendMarker::Line(Stroke::dashed(Color::from_hex("#2980b9"), 1.5, 6.0, 2.5)),
        ));
    }
    if nose_load_limits {
        legend_entries.push((
            "Min nose load (steering)".to_owned(),
            LegendMarker::Line(Stroke::dashed(Color::from_hex("#d35400"), 1.5, 6.0, 4.0)),
        ));
    }
    draw_horizontal_legend(
        scene,
        [axes.left, axes.top + axes.height + 52.0],
        &legend_entries,
        pal,
        8.0,
    );

    scene.add(SceneElement::Text {
        text: "MODEL-DERIVED CG CHECK ONLY; NOT AN AFM/WBM OPERATIONAL ENVELOPE".to_owned(),
        pos: [scene.width * 0.5, 607.0],
        font_size: 7.5,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: false,
    });
}
