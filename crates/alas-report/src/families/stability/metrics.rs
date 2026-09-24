// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (`figure_stability_metrics`)
// Reference: alas @ rust-port-baseline.

//! Stability number line and the report polar's Cm-versus-CL view.

use alas_config::AlasConfig;
use alas_pipeline::full_analysis::AnalysisReport;

use super::label_rows::assign_label_rows;
use super::scalars::stability_scalars;
use super::status_scene;
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

use crate::families::common::padded_range;

/// Render stability markers, the Cm polar, and the derived metrics table.
pub fn figure_stability_metrics(
    report: &AnalysisReport,
    config: Option<&AlasConfig>,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let Some(s) = stability_scalars(report) else {
        return status_scene("Stability metrics", "No wing data available", pal);
    };
    let mut scene = Scene::new(950.0, 540.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Stability Metrics".to_owned());
    let np_pct = s.pct(s.x_np);
    let target_sm = config
        .map(|c| c.requirements.target_static_margin)
        .unwrap_or(0.10);
    let cg_range = config
        .map(|c| c.requirements.cg_range_pct_mac)
        .unwrap_or(15.0);
    let aft = np_pct - target_sm * 100.0;
    let fwd = aft - cg_range;
    let min_p = (fwd.min(np_pct).min(s.pct(s.x_wing_ac)) - 10.0).max(-50.0);
    let max_p = (np_pct.max(aft).max(s.pct(s.x_wing_ac)) + 10.0).min(150.0);
    let ruler = Axes2D::new((55.0, 70.0, 400.0, 240.0), (min_p, max_p), (-1.0, 1.0));
    ruler.draw_frame(&mut scene, pal);
    ruler.add_line_series(
        &mut scene,
        &[(min_p, 0.0), (max_p, 0.0)],
        Stroke::new(Color::from_hex(pal.title), 2.0),
    );
    let band = [ruler.map_point(fwd, -0.18), ruler.map_point(aft, 0.18)];
    scene.add(SceneElement::Rect {
        x: band[0][0].min(band[1][0]),
        y: band[0][1].min(band[1][1]),
        width: (band[1][0] - band[0][0]).abs(),
        height: (band[1][1] - band[0][1]).abs(),
        rx: 0.0,
        fill: Some(Fill::new(Color::rgba(39, 174, 96, 45))),
        stroke: None,
    });
    let mut points = [
        ("Wing AC", s.pct(s.x_wing_ac), "#c0392b"),
        ("Phys CG", s.pct(s.x_cg_phys), "#e67e22"),
        ("Aero CG", s.pct(s.x_cg_aero), "#2980b9"),
        ("NP", np_pct, "#8e44ad"),
        ("Fwd lim", fwd, "#e74c3c"),
        ("Aft lim", aft, "#e74c3c"),
    ];
    points.sort_by(|a, b| a.1.total_cmp(&b.1));
    let rows = assign_label_rows(
        &points.iter().map(|p| p.1).collect::<Vec<_>>(),
        ((max_p - min_p) * 0.12).max(1.0),
    );
    for ((name, value, color), row) in points.iter().zip(rows) {
        ruler.add_line_series(
            &mut scene,
            &[(*value, -0.8), (*value, 0.8)],
            Stroke::new(Color::from_hex(color), 1.4),
        );
        let lane = match row {
            0 => 0.68,
            1 => -0.68,
            2 => 0.32,
            3 => -0.32,
            _ => 0.0,
        };
        let p = ruler.map_point(*value, lane);
        scene.add(SceneElement::Text {
            text: format!("{}\n{:.0}%", name, value),
            pos: p,
            font_size: 7.5,
            color: Color::from_hex(color),
            align: TextAlign::Center,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: true,
        });
    }
    scene.add(SceneElement::Text {
        text: "% MAC from LEMAC".to_owned(),
        pos: [
            ruler.left + ruler.width * 0.5,
            ruler.top + ruler.height + 25.0,
        ],
        font_size: 9.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });

    let cm_axes = Axes2D::new(
        (510.0, 70.0, 360.0, 240.0),
        padded_range(report.polar.cl.iter().copied(), 0.08),
        padded_range(report.polar.cm.iter().copied(), 0.08),
    );
    cm_axes.draw_frame(&mut scene, pal);
    cm_axes.add_line_series(
        &mut scene,
        &report
            .polar
            .cl
            .iter()
            .copied()
            .zip(report.polar.cm.iter().copied())
            .collect::<Vec<_>>(),
        Stroke::new(Color::from_hex(pal.title), 1.8),
    );
    cm_axes.add_line_series(
        &mut scene,
        &[
            (report.design_point.cl, cm_axes.y_min),
            (report.design_point.cl, cm_axes.y_max),
        ],
        Stroke::dashed(Color::from_hex("#3498db"), 1.0, 3.0, 3.0),
    );
    scene.add(SceneElement::Text {
        text: "Cm vs CL".to_owned(),
        pos: [cm_axes.left, cm_axes.top - 8.0],
        font_size: 10.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
    let metrics = format!("MAC       = {:.2} m\nWing AC   = {:+.1}% MAC\nAero CG   = {:+.1}% MAC\nPhys CG   = {:+.1}% MAC\nNP        = {:+.1}% MAC\nSM        = {:.2}%\nV_H       = {:.3}\nl_t       = {:.2} m\nS_t / S   = {:.4}", s.c_ref, s.pct(s.x_wing_ac), s.pct(s.x_cg_aero), s.pct(s.x_cg_phys), np_pct, s.sm * 100.0, s.v_h, s.l_t, s.s_tail / s.s_wing.max(1e-9));
    scene.add(SceneElement::Rect {
        x: 55.0,
        y: 355.0,
        width: 795.0,
        height: 160.0,
        rx: 4.0,
        fill: Some(Fill::new(Color::rgba(248, 249, 250, 230))),
        stroke: Some(Stroke::new(Color::from_hex("#bdc3c7"), 1.0)),
    });
    scene.add(SceneElement::Text {
        text: metrics,
        pos: [70.0, 370.0],
        font_size: 9.0,
        color: Color::from_hex("#222222"),
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
    scene
}
