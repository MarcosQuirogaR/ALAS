// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py, `figure_aero_panel`
// (L224-257), `figure_polar_comparison`
// (L283-362) and `figure_drag_breakdown` (L759-861).
// Reference: alas @ rust-port-baseline.

use crate::chart_kit::{draw_legend, draw_title, LegendMarker};
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::{get_palette, Palette, BASELINE_COLOR, GHOST_COLOR, OPTIMIZED_COLOR};
use alas_pipeline::full_analysis::AnalysisReport;

use super::support::padded_range;

fn vline(axes: &Axes2D, scene: &mut Scene, x: f64, y_range: (f64, f64), stroke: Stroke) {
    axes.add_line_series(scene, &[(x, y_range.0), (x, y_range.1)], stroke);
}

fn hline(axes: &Axes2D, scene: &mut Scene, y: f64, x_range: (f64, f64), stroke: Stroke) {
    axes.add_line_series(scene, &[(x_range.0, y), (x_range.1, y)], stroke);
}

fn add_markers(axes: &Axes2D, scene: &mut Scene, points: &[(f64, f64)], color: Color) {
    for &(x, y) in points {
        scene.add(SceneElement::Circle {
            center: axes.map_point(x, y),
            radius: 2.8,
            fill: Some(Fill::new(color)),
            stroke: Some(Stroke::new(Color::from_hex("#ffffff"), 0.6)),
        });
    }
}

/// Four-panel polar set: lift curve, drag polar, efficiency, longitudinal
/// stability -- `figure_aero_panel`.
pub fn figure_aero_panel(report: &AnalysisReport, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(900.0, 700.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Aerodynamic Polar Set".to_owned());
    draw_title(&mut scene, "Aerodynamic Polar Set", pal);
    scene.suppress_derived_title();
    let p = &report.polar;

    let alpha_range = padded_range(p.alpha_deg.iter().copied(), 0.05);
    let cl_range = padded_range(p.cl.iter().copied(), 0.08);
    let cd_range = padded_range(
        p.cd.iter().copied().chain(p.cd_induced.iter().copied()),
        0.08,
    );
    let ld_range = padded_range(p.l_over_d.iter().copied(), 0.08);
    let cm_range = padded_range(p.cm.iter().copied().chain(std::iter::once(0.0)), 0.15);

    let blue = Stroke::new(Color::from_hex("tab:blue"), 1.8);

    let ax_lift = Axes2D::new((70.0, 60.0, 340.0, 260.0), alpha_range, cl_range);
    ax_lift.draw_frame_with_labels(&mut scene, pal, "\u{03b1} [deg]", "CL");
    let lift_pts: Vec<(f64, f64)> = p
        .alpha_deg
        .iter()
        .copied()
        .zip(p.cl.iter().copied())
        .collect();
    ax_lift.add_line_series(&mut scene, &lift_pts, blue.clone());
    add_markers(&ax_lift, &mut scene, &lift_pts, Color::from_hex("tab:blue"));
    panel_title(&mut scene, &ax_lift, "Lift curve (\u{03b1} vs CL)", pal);

    let ax_drag = Axes2D::new((500.0, 60.0, 340.0, 260.0), cd_range, cl_range);
    ax_drag.draw_frame_with_labels(&mut scene, pal, "CD", "CL");
    let total_pts: Vec<(f64, f64)> = p.cd.iter().copied().zip(p.cl.iter().copied()).collect();
    let induced_pts: Vec<(f64, f64)> = p
        .cd_induced
        .iter()
        .copied()
        .zip(p.cl.iter().copied())
        .collect();
    ax_drag.add_line_series(
        &mut scene,
        &total_pts,
        Stroke::new(Color::from_hex("tab:red"), 1.8),
    );
    add_markers(&ax_drag, &mut scene, &total_pts, Color::from_hex("tab:red"));
    ax_drag.add_line_series(
        &mut scene,
        &induced_pts,
        Stroke::dashed(Color::from_hex("tab:blue"), 1.4, 5.0, 3.0),
    );
    panel_title(&mut scene, &ax_drag, "Drag polar", pal);
    draw_legend(
        &mut scene,
        [ax_drag.left + 8.0, ax_drag.top + 8.0],
        &[
            (
                "Total (corrected)".to_owned(),
                LegendMarker::Line(Stroke::new(Color::from_hex("tab:red"), 1.8)),
            ),
            (
                "Induced (VLM)".to_owned(),
                LegendMarker::Line(Stroke::dashed(Color::from_hex("tab:blue"), 1.4, 5.0, 3.0)),
            ),
        ],
        pal,
        8.0,
    );

    // Keep the lower-panel axis labels inside the canvas at card resolution.
    let ax_eff = Axes2D::new((70.0, 390.0, 340.0, 250.0), cl_range, ld_range);
    ax_eff.draw_frame_with_labels(&mut scene, pal, "CL", "L/D");
    let eff_pts: Vec<(f64, f64)> =
        p.cl.iter()
            .copied()
            .zip(p.l_over_d.iter().copied())
            .collect();
    ax_eff.add_line_series(
        &mut scene,
        &eff_pts,
        Stroke::new(Color::from_hex("tab:purple"), 2.0),
    );
    add_markers(&ax_eff, &mut scene, &eff_pts, Color::from_hex("tab:purple"));
    vline(
        &ax_eff,
        &mut scene,
        report.design_point.cl,
        ld_range,
        Stroke::dashed(Color::from_hex("tab:red"), 1.2, 3.0, 3.0),
    );
    panel_title(&mut scene, &ax_eff, "Efficiency", pal);
    draw_legend(
        &mut scene,
        [ax_eff.left + 8.0, ax_eff.top + 8.0],
        &[(
            "Design CL".to_owned(),
            LegendMarker::Line(Stroke::dashed(Color::from_hex("tab:red"), 1.2, 3.0, 3.0)),
        )],
        pal,
        8.0,
    );

    let ax_cm = Axes2D::new((500.0, 390.0, 340.0, 250.0), alpha_range, cm_range);
    ax_cm.draw_frame_with_labels(&mut scene, pal, "\u{03b1} [deg]", "Cm");
    let cm_pts: Vec<(f64, f64)> = p
        .alpha_deg
        .iter()
        .copied()
        .zip(p.cm.iter().copied())
        .collect();
    ax_cm.add_line_series(&mut scene, &cm_pts, blue);
    hline(
        &ax_cm,
        &mut scene,
        0.0,
        alpha_range,
        Stroke::dashed(Color::from_hex("#d62728"), 1.2, 4.0, 4.0),
    );
    panel_title(&mut scene, &ax_cm, "Longitudinal stability", pal);

    scene
}

fn panel_title(scene: &mut Scene, axes: &Axes2D, text: &str, pal: &Palette) {
    scene.add(SceneElement::Text {
        text: text.to_owned(),
        pos: [axes.left, axes.top - 8.0],
        font_size: 11.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
}

/// Compare baseline and optimized drag polar and efficiency traces.
pub fn figure_polar_comparison(
    baseline: &AnalysisReport,
    optimized: &AnalysisReport,
    labels: (&str, &str),
    ghost_reports: Option<&[&AnalysisReport]>,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(900.0, 460.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Drag Polar Comparison".to_owned());
    draw_title(&mut scene, "Drag Polar Comparison", pal);
    scene.suppress_derived_title();

    let b = &baseline.polar;
    let o = &optimized.polar;
    let label_b = labels.0.to_owned();
    let label_o = labels.1.to_owned();

    let cd_range = padded_range(
        b.cd.iter()
            .copied()
            .chain(o.cd.iter().copied())
            .chain(ghost_iter(ghost_reports, |r| r.polar.cd.iter().copied())),
        0.08,
    );
    let cl_range = padded_range(
        b.cl.iter()
            .copied()
            .chain(o.cl.iter().copied())
            .chain(ghost_iter(ghost_reports, |r| r.polar.cl.iter().copied())),
        0.08,
    );
    let ld_range = padded_range(
        b.l_over_d
            .iter()
            .copied()
            .chain(o.l_over_d.iter().copied())
            .chain(ghost_iter(ghost_reports, |r| {
                r.polar.l_over_d.iter().copied()
            })),
        0.08,
    );

    let ax_polar = Axes2D::new((70.0, 60.0, 360.0, 340.0), cd_range, cl_range);
    let ax_eff = Axes2D::new((470.0, 60.0, 360.0, 340.0), cl_range, ld_range);
    ax_polar.draw_frame_with_labels(&mut scene, pal, "CD", "CL");
    ax_eff.draw_frame_with_labels(&mut scene, pal, "CL", "L/D");
    panel_title(&mut scene, &ax_polar, "Drag polar", pal);
    panel_title(&mut scene, &ax_eff, "Efficiency", pal);

    let mut legend_entries = Vec::new();
    if let Some(ghosts) = ghost_reports {
        let ghost_stroke = Stroke::dashed(Color::from_hex(GHOST_COLOR), 1.2, 4.0, 3.0);
        for (i, ghost) in ghosts.iter().enumerate() {
            let polar_pts: Vec<(f64, f64)> = ghost
                .polar
                .cd
                .iter()
                .copied()
                .zip(ghost.polar.cl.iter().copied())
                .collect();
            let eff_pts: Vec<(f64, f64)> = ghost
                .polar
                .cl
                .iter()
                .copied()
                .zip(ghost.polar.l_over_d.iter().copied())
                .collect();
            ax_polar.add_line_series(&mut scene, &polar_pts, ghost_stroke.clone());
            ax_eff.add_line_series(&mut scene, &eff_pts, ghost_stroke.clone());
            legend_entries.push((
                format!("Prior run {}", i + 1),
                LegendMarker::Line(ghost_stroke.clone()),
            ));
        }
    }

    let baseline_stroke = Stroke::dashed(Color::from_hex(BASELINE_COLOR), 1.8, 5.0, 3.0);
    let optimized_stroke = Stroke::new(Color::from_hex(OPTIMIZED_COLOR), 2.0);
    ax_polar.add_line_series(
        &mut scene,
        &b.cd
            .iter()
            .copied()
            .zip(b.cl.iter().copied())
            .collect::<Vec<_>>(),
        baseline_stroke.clone(),
    );
    add_markers(
        &ax_polar,
        &mut scene,
        &b.cd
            .iter()
            .copied()
            .zip(b.cl.iter().copied())
            .collect::<Vec<_>>(),
        Color::from_hex(BASELINE_COLOR),
    );
    ax_polar.add_line_series(
        &mut scene,
        &o.cd
            .iter()
            .copied()
            .zip(o.cl.iter().copied())
            .collect::<Vec<_>>(),
        optimized_stroke.clone(),
    );
    add_markers(
        &ax_polar,
        &mut scene,
        &o.cd
            .iter()
            .copied()
            .zip(o.cl.iter().copied())
            .collect::<Vec<_>>(),
        Color::from_hex(OPTIMIZED_COLOR),
    );
    ax_eff.add_line_series(
        &mut scene,
        &b.cl
            .iter()
            .copied()
            .zip(b.l_over_d.iter().copied())
            .collect::<Vec<_>>(),
        baseline_stroke.clone(),
    );
    ax_eff.add_line_series(
        &mut scene,
        &o.cl
            .iter()
            .copied()
            .zip(o.l_over_d.iter().copied())
            .collect::<Vec<_>>(),
        optimized_stroke.clone(),
    );
    add_markers(
        &ax_eff,
        &mut scene,
        &o.cl
            .iter()
            .copied()
            .zip(o.l_over_d.iter().copied())
            .collect::<Vec<_>>(),
        Color::from_hex(OPTIMIZED_COLOR),
    );
    legend_entries.push((label_b, LegendMarker::Line(baseline_stroke)));
    legend_entries.push((label_o, LegendMarker::Line(optimized_stroke)));

    draw_legend(
        &mut scene,
        [ax_polar.left + 8.0, ax_polar.top + 8.0],
        &legend_entries,
        pal,
        8.0,
    );
    draw_legend(
        &mut scene,
        [ax_eff.left + 8.0, ax_eff.top + 8.0],
        &legend_entries,
        pal,
        8.0,
    );

    scene
}

fn ghost_iter<'a, I: Iterator<Item = f64> + 'a>(
    ghost_reports: Option<&'a [&'a AnalysisReport]>,
    extract: impl Fn(&'a AnalysisReport) -> I + 'a,
) -> impl Iterator<Item = f64> + 'a {
    ghost_reports
        .into_iter()
        .flatten()
        .flat_map(move |&r| extract(r))
}

fn nearest_cl_index(cl: &[f64], target_cl: f64) -> Option<usize> {
    cl.iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - target_cl).abs().total_cmp(&(*b - target_cl).abs()))
        .map(|(i, _)| i)
}

/// Show parasite, induced, and wave drag at the design point.
pub fn figure_drag_breakdown(report: &AnalysisReport, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(900.0, 460.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Drag Breakdown at Design CL".to_owned());
    draw_title(&mut scene, "Drag Breakdown at Design CL", pal);
    scene.suppress_derived_title();

    let p = &report.polar;
    let dp = &report.design_point;
    let fit = &report.polar_fit;

    let (cd_p, cd_i, cd_w) = match nearest_cl_index(&p.cl, dp.cl) {
        Some(idx) => (p.cd_parasite[idx], p.cd_induced[idx], p.cd_wave[idx]),
        None => (fit.cd0, fit.k * dp.cl * dp.cl, 0.0),
    };
    let cd_total = (cd_p + cd_i + cd_w).max(1e-9);

    let ax_bar = Axes2D::new(
        (80.0, 60.0, 300.0, 340.0),
        (0.0, 1.0),
        (0.0, cd_total * 1.3),
    );
    ax_bar.draw_frame_with_labels(&mut scene, pal, "Design point", "CD");
    panel_title(&mut scene, &ax_bar, "Drag breakdown at design CL", pal);

    let bar_w = 90.0;
    let x_center = ax_bar.map_point(0.5, 0.0)[0];
    let mut y0 = 0.0;
    for (val, color, name) in [
        (cd_p, "tab:blue", "CD0"),
        (cd_i, "tab:orange", "CDi"),
        (cd_w, "tab:red", "CDwave"),
    ] {
        let p_top = ax_bar.map_point(0.5, y0 + val);
        let p_bot = ax_bar.map_point(0.5, y0);
        scene.add(SceneElement::Rect {
            x: x_center - bar_w * 0.5,
            y: p_top[1],
            width: bar_w,
            height: (p_bot[1] - p_top[1]).max(1.0),
            rx: 2.0,
            fill: Some(Fill::new(Color::from_hex(color))),
            stroke: Some(Stroke::new(Color::from_hex(pal.title), 1.0)),
        });
        let bar_height = (p_bot[1] - p_top[1]).max(1.0);
        if bar_height >= 22.0 {
            scene.add(SceneElement::Text {
                text: format!("{name}\n{val:.4}"),
                pos: [x_center, (p_top[1] + p_bot[1]) * 0.5],
                font_size: 8.0,
                color: Color::from_hex("#ffffff"),
                align: TextAlign::Center,
                baseline: TextBaseline::Middle,
                angle_deg: 0.0,
                bold: true,
            });
        } else {
            scene.add(SceneElement::Text {
                text: format!("{name} = {val:.4}"),
                pos: [x_center + bar_w * 0.5 + 8.0, p_bot[1] - 3.0],
                font_size: 8.0,
                color: Color::from_hex(pal.tick),
                align: TextAlign::Left,
                baseline: TextBaseline::Bottom,
                angle_deg: 0.0,
                bold: true,
            });
        }
        y0 += val;
    }

    let ax_eff = Axes2D::new(
        (470.0, 60.0, 360.0, 340.0),
        padded_range(p.cl.iter().copied().chain(std::iter::once(dp.cl)), 0.08),
        padded_range(
            p.l_over_d
                .iter()
                .copied()
                .chain(std::iter::once(dp.l_over_d)),
            0.08,
        ),
    );
    ax_eff.draw_frame_with_labels(&mut scene, pal, "CL", "L/D");
    panel_title(&mut scene, &ax_eff, "Efficiency curve", pal);
    let eff_pts: Vec<(f64, f64)> =
        p.cl.iter()
            .copied()
            .zip(p.l_over_d.iter().copied())
            .collect();
    ax_eff.add_line_series(
        &mut scene,
        &eff_pts,
        Stroke::new(Color::from_hex("tab:purple"), 2.0),
    );
    add_markers(&ax_eff, &mut scene, &eff_pts, Color::from_hex("tab:purple"));
    let y_range = padded_range(
        p.l_over_d
            .iter()
            .copied()
            .chain(std::iter::once(dp.l_over_d)),
        0.08,
    );
    let x_range = padded_range(p.cl.iter().copied().chain(std::iter::once(dp.cl)), 0.08);
    vline(
        &ax_eff,
        &mut scene,
        dp.cl,
        y_range,
        Stroke::dashed(Color::from_hex("tab:red"), 1.2, 3.0, 3.0),
    );
    hline(
        &ax_eff,
        &mut scene,
        dp.l_over_d,
        x_range,
        Stroke::dashed(Color::from_hex("#e6c200"), 1.2, 4.0, 4.0),
    );
    let design_pt = ax_eff.map_point(dp.cl, dp.l_over_d);
    scene.add(SceneElement::Circle {
        center: design_pt,
        radius: 4.5,
        fill: Some(Fill::new(Color::from_hex("#ff0000"))),
        stroke: None,
    });
    draw_legend(
        &mut scene,
        [
            ax_eff.left + ax_eff.width - 145.0,
            ax_eff.top + ax_eff.height - 58.0,
        ],
        &[
            (
                "Efficiency".to_owned(),
                LegendMarker::Line(Stroke::new(Color::from_hex("tab:purple"), 2.0)),
            ),
            (
                format!("Design CL = {:.3}", dp.cl),
                LegendMarker::Line(Stroke::dashed(Color::from_hex("tab:red"), 1.2, 3.0, 3.0)),
            ),
            (
                format!("L/D = {:.2}", dp.l_over_d),
                LegendMarker::Line(Stroke::dashed(Color::from_hex("#e6c200"), 1.2, 4.0, 4.0)),
            ),
        ],
        pal,
        8.0,
    );

    scene
}
