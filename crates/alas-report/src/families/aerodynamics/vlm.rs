// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (`figure_span_loading`,
// `figure_vlm_flow`)
// Reference: alas @ rust-port-baseline.

//! Report figures that consume the per-panel VLM result, including the wake.

use alas_aero::operating_point::{AxisFrame, OperatingPoint};
use alas_aero::vlm;
use alas_atmo::Atmosphere;
use alas_pipeline::full_analysis::AnalysisReport;

use super::support::padded_range;
use crate::chart_kit::{draw_legend, draw_title, LegendMarker};
use crate::colormap::Colormap;
use crate::families::geometry::{draw_fuselage_wireframe, draw_wing_wireframe};
use crate::scene::{Axes2D, Camera3D, Color, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

fn draw_aircraft(
    scene: &mut Scene,
    report: &AnalysisReport,
    camera: &Camera3D,
    center: [f64; 3],
    span: f64,
    viewport: (f64, f64, f64, f64),
    color: Color,
) {
    for wing in &report.airplane.wings {
        draw_wing_wireframe(scene, camera, center, span, viewport, wing, color);
    }
    for fuselage in &report.airplane.fuselages {
        draw_fuselage_wireframe(scene, camera, center, span, viewport, fuselage, color);
    }
}

fn solve(report: &AnalysisReport) -> Result<(alas_aero::vlm::VlmResult, OperatingPoint), ()> {
    let op = OperatingPoint::new(
        Atmosphere::new(0.0),
        100.0,
        report.design_point.alpha_deg,
        0.0,
        0.0,
        0.0,
        0.0,
    );
    let result = vlm::run(&report.airplane, &op, 2, 2).map_err(|_| ())?;
    Ok((result, op))
}

fn status(theme: Option<&str>, text: &str) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(700.0, 400.0, Some(Color::from_hex(pal.bg)));
    scene.add(SceneElement::Text {
        text: text.to_owned(),
        pos: [350.0, 200.0],
        font_size: 12.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });
    scene
}

fn main_wing_span_loading(
    vlm: &alas_aero::vlm::VlmResult,
    op: &OperatingPoint,
) -> (Vec<(f64, f64)>, f64) {
    let mut points = Vec::new();
    let mut total_wing_lift = 0.0;
    let mut strip_y = 0.0;
    let mut strip_dy = 0.0;
    let mut strip_lift = 0.0;
    let mut strip_panels = 0_usize;

    for (panel, force) in vlm.panels.iter().zip(&vlm.panel_forces_geometry) {
        if panel.wing_index != 0 {
            continue;
        }
        let (_, _, force_wind_z) = op.convert_axes(
            force[0],
            force[1],
            force[2],
            AxisFrame::Geometry,
            AxisFrame::Wind,
        );
        let lift = -force_wind_z;
        let dy = (panel.front_right[1] - panel.front_left[1]).abs();
        total_wing_lift += lift;
        strip_y += panel.vortex_center[1];
        strip_dy += dy;
        strip_lift += lift;
        strip_panels += 1;

        if panel.is_trailing_edge {
            let count = strip_panels as f64;
            let y = strip_y / count;
            let mean_dy = strip_dy / count;
            if y >= 0.0 && mean_dy > 0.0 {
                points.push((y, strip_lift / mean_dy));
            }
            strip_y = 0.0;
            strip_dy = 0.0;
            strip_lift = 0.0;
            strip_panels = 0;
        }
    }
    points.sort_by(|a, b| a.0.total_cmp(&b.0));
    (points, total_wing_lift)
}

/// Plot panel lift per span and a total-lift-matched elliptic reference.
pub fn figure_span_loading(report: &AnalysisReport, theme: Option<&str>) -> Scene {
    if report.airplane.wings.is_empty() {
        return status(theme, "VLM span loading unavailable");
    }
    let Ok((vlm, op)) = solve(report) else {
        return status(theme, "VLM span loading unavailable");
    };
    let (points, total_wing_lift) = main_wing_span_loading(&vlm, &op);
    if points.is_empty() {
        return status(theme, "VLM span loading unavailable");
    }
    let span = report.airplane.b_ref.max(1e-9);
    let semi = span * 0.5;
    let root = 4.0 * (total_wing_lift * 0.5) / (std::f64::consts::PI * semi);
    let elliptical = points
        .iter()
        .map(|&(y, _)| (y, root * (1.0 - (y / semi).powi(2)).max(0.0).sqrt()))
        .collect::<Vec<_>>();
    let pal = get_palette(theme);
    let mut scene = Scene::new(760.0, 450.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Span Loading".to_owned());
    draw_title(&mut scene, "Span Loading", pal);
    scene.suppress_derived_title();
    let axes = Axes2D::new(
        (65.0, 55.0, 630.0, 330.0),
        padded_range(points.iter().map(|p| p.0), 0.05),
        padded_range(
            points
                .iter()
                .map(|p| p.1)
                .chain(elliptical.iter().map(|p| p.1)),
            0.08,
        ),
    );
    axes.draw_frame_with_labels(&mut scene, pal, "Spanwise position Y [m]", "");
    scene.add(SceneElement::Text {
        text: "Lift per unit span L' [N/m]".to_owned(),
        pos: [15.0, axes.top + axes.height * 0.5],
        font_size: 10.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: -90.0,
        bold: false,
    });
    axes.add_line_series(
        &mut scene,
        &points,
        Stroke::new(Color::from_hex("tab:blue"), 1.8),
    );
    axes.add_line_series(
        &mut scene,
        &elliptical,
        Stroke::dashed(Color::from_hex("tab:red"), 1.4, 5.0, 3.0),
    );
    draw_legend(
        &mut scene,
        [axes.left + axes.width - 220.0, axes.top + 8.0],
        &[
            (
                "Calculated lift".to_owned(),
                LegendMarker::Line(Stroke::new(Color::from_hex("tab:blue"), 1.8)),
            ),
            (
                "Ideal elliptical loading".to_owned(),
                LegendMarker::Line(Stroke::dashed(Color::from_hex("tab:red"), 1.4, 5.0, 3.0)),
            ),
        ],
        pal,
        8.0,
    );
    scene
}

/// Project VLM trailing-edge streamlines into a compact 2D wake view.
pub fn figure_vlm_flow(report: &AnalysisReport, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    if report.airplane.wings.is_empty() {
        return status(theme, "VLM flow unavailable");
    }
    let Ok((vlm, op)) = solve(report) else {
        return status(theme, "VLM flow unavailable");
    };
    let seeds: Vec<[f64; 3]> = vlm
        .panels
        .iter()
        .filter(|p| p.is_trailing_edge)
        .map(|p| {
            [
                (p.back_left[0] + p.back_right[0]) * 0.5,
                (p.back_left[1] + p.back_right[1]) * 0.5,
                (p.back_left[2] + p.back_right[2]) * 0.5,
            ]
        })
        .collect();
    let lines =
        vlm::calculate_streamlines(&vlm.panels, &vlm.vortex_strengths, &op, &seeds, 30, 25.0);
    let mut scene = Scene::new(760.0, 450.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("VLM Wake Flow".to_owned());
    draw_title(&mut scene, "VLM Wake Flow", pal);
    scene.suppress_derived_title();
    let camera = Camera3D::default();
    let viewport = (20.0, 30.0, 700.0, 380.0);
    let mut bounds = [
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    let mut fit_points = Vec::new();
    let mut include = |point: [f64; 3]| {
        fit_points.push(point);
        bounds[0] = bounds[0].min(point[0]);
        bounds[1] = bounds[1].max(point[0]);
        bounds[2] = bounds[2].min(point[1]);
        bounds[3] = bounds[3].max(point[1]);
        bounds[4] = bounds[4].min(point[2]);
        bounds[5] = bounds[5].max(point[2]);
    };
    for panel in &vlm.panels {
        for point in [
            panel.front_left,
            panel.front_right,
            panel.back_left,
            panel.back_right,
        ] {
            include(point);
        }
    }
    for point in lines.iter().flatten() {
        include(*point);
    }
    for wing in &report.airplane.wings {
        for xsec in &wing.xsecs {
            for mirror in [false, true]
                .into_iter()
                .take(if wing.symmetric { 2 } else { 1 })
            {
                let side = if mirror { -1.0 } else { 1.0 };
                include([xsec.xyz_le[0], xsec.xyz_le[1] * side, xsec.xyz_le[2]]);
                include([
                    xsec.xyz_le[0] + xsec.chord,
                    xsec.xyz_le[1] * side,
                    xsec.xyz_le[2],
                ]);
            }
        }
    }
    for fuselage in &report.airplane.fuselages {
        for xsec in &fuselage.xsecs {
            include([
                xsec.xyz_c[0],
                xsec.xyz_c[1],
                xsec.xyz_c[2] + xsec.height * 0.5,
            ]);
            include([
                xsec.xyz_c[0],
                xsec.xyz_c[1],
                xsec.xyz_c[2] - xsec.height * 0.5,
            ]);
            include([
                xsec.xyz_c[0],
                xsec.xyz_c[1] + xsec.width * 0.5,
                xsec.xyz_c[2],
            ]);
            include([
                xsec.xyz_c[0],
                xsec.xyz_c[1] - xsec.width * 0.5,
                xsec.xyz_c[2],
            ]);
        }
    }
    let center = [
        (bounds[0] + bounds[1]) * 0.5,
        (bounds[2] + bounds[3]) * 0.5,
        (bounds[4] + bounds[5]) * 0.5,
    ];
    let max_span = camera.fit_span_to_points(&fit_points, viewport, 0.08);
    let lateral_scale = report.airplane.b_ref.max(1.0);
    draw_aircraft(
        &mut scene,
        report,
        &camera,
        center,
        max_span,
        viewport,
        Color::from_hex(pal.title),
    );
    let mut legend = Vec::new();
    for line in lines {
        let origin = line.first().copied().unwrap_or(center);
        let t = (origin[1].abs() / lateral_scale).clamp(0.0, 1.0);
        let color = Colormap::Plasma.sample(t);
        let pts = line
            .iter()
            .map(|&p| camera.project(p, center, max_span, viewport))
            .collect::<Vec<_>>();
        if pts.len() > 1 {
            scene.add(SceneElement::Polyline {
                points: pts,
                stroke: Stroke::new(color, 1.2),
            });
        }
        if legend.len() < 3 {
            legend.push((
                format!("Span |Y| = {:.1} m", origin[1].abs()),
                LegendMarker::Circle(color),
            ));
        }
    }
    legend.push((
        "Aircraft".to_owned(),
        LegendMarker::Line(Stroke::new(Color::from_hex(pal.title), 1.0)),
    ));
    draw_legend(&mut scene, [35.0, 40.0], &legend, pal, 8.0);
    scene
}

#[cfg(test)]
mod tests {
    // These tests intentionally panic if their constructed fixture violates its precondition.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::airplane::Airplane;
    use alas_geom::aircraft::wing::{Wing, WingXSec};

    #[test]
    fn span_loading_collapses_chordwise_panels_into_unique_main_wing_stations() {
        let airfoil = Airfoil::from_name("naca2412").expect("analytical airfoil");
        let main_wing = Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 2.0, 2.0, airfoil.clone()),
                WingXSec::new([0.5, 5.0, 0.0], 1.0, 0.0, airfoil.clone()),
            ],
            true,
        );
        let tail = Wing::new(
            "Horizontal Stabilizer",
            vec![
                WingXSec::new([4.0, 0.0, 0.0], 1.0, 0.0, airfoil.clone()),
                WingXSec::new([4.2, 2.0, 0.0], 0.5, 0.0, airfoil),
            ],
            true,
        );
        let airplane = Airplane {
            name: "span-loading regression".to_owned(),
            xyz_ref: [0.0, 0.0, 0.0],
            wings: vec![main_wing, tail],
            fuselages: Vec::new(),
            s_ref: 15.0,
            c_ref: 1.5,
            b_ref: 10.0,
        };
        let op = OperatingPoint::new(Atmosphere::new(0.0), 50.0, 4.0, 0.0, 0.0, 0.0, 0.0);
        let vlm = vlm::run(&airplane, &op, 4, 3).expect("well-formed VLM mesh");

        let (points, total_lift) = main_wing_span_loading(&vlm, &op);

        let main_wing_positive_strips = vlm
            .panels
            .iter()
            .filter(|panel| {
                panel.wing_index == 0 && panel.is_trailing_edge && panel.vortex_center[1] >= 0.0
            })
            .count();
        assert_eq!(points.len(), main_wing_positive_strips);
        assert!(points.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert!(points.iter().all(|(_, lift_per_span)| *lift_per_span > 0.0));
        assert!(total_lift > 0.0);
    }
}
