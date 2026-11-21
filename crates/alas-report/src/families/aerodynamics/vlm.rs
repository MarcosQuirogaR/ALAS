// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (`figure_span_loading`,
// `figure_vlm_flow`)

//! Report figures that consume the per-panel VLM result, including the wake.

use alas_aero::operating_point::{AxisFrame, OperatingPoint};
use alas_aero::vlm;
use alas_atmo::Atmosphere;
use alas_config::AlasConfig;
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

/// The geometric body angle of the report's design point: the sweep point the
/// design point was read from, on the geometric axis (the compressible polar
/// carries the true angle on both axes; the relabelled one does not).
fn design_body_alpha_deg(report: &AnalysisReport) -> f64 {
    report
        .polar
        .alpha_deg
        .iter()
        .position(|&alpha| alpha == report.design_point.alpha_deg)
        .and_then(|index| report.polar.geometric_alpha_deg.get(index).copied())
        .unwrap_or(report.design_point.alpha_deg)
}

/// The wake figure's solve: the incompressible lattice at 100 m/s at the angle
/// that carries the design lift coefficient (two coarse probes fix the slope),
/// so the wake is that of the design loading rather than of the compressible
/// cruise angle flown on an incompressible lattice.
fn solve(report: &AnalysisReport) -> Result<(alas_aero::vlm::VlmResult, OperatingPoint), ()> {
    // One mesh and one factorization serve the three operating points.
    let system = vlm::VlmSystem::assemble(&report.airplane, 2, 2).map_err(|_| ())?;
    let at = |alpha_deg: f64| {
        let op = OperatingPoint::new(Atmosphere::new(0.0), 100.0, alpha_deg, 0.0, 0.0, 0.0, 0.0);
        system.solve(&op).map(|result| (result, op)).map_err(|_| ())
    };
    let design_cl = report
        .trimmed_design_point
        .map_or(report.design_point.cl, |trim| trim.cl);
    let (low, high) = (at(0.0)?, at(4.0)?);
    let slope = (high.0.cl_lift - low.0.cl_lift) / 4.0;
    if slope.is_finite() && slope.abs() > 1e-9 && design_cl.is_finite() {
        at((design_cl - low.0.cl_lift) / slope)
    } else {
        at(design_body_alpha_deg(report))
    }
}

/// The lift the span-loading figure integrates and the load it must carry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpanLoadingBalance {
    /// Lift of the main wing (strips integrated over both semi-spans), N.
    pub wing_lift_n: f64,
    /// Lift of every lifting surface of the solve, N.
    pub total_lift_n: f64,
    /// Aircraft weight the solve must balance (level flight, n = 1), N.
    pub weight_n: f64,
    /// Dynamic pressure of the cruise condition, Pa.
    pub dynamic_pressure_pa: f64,
}

/// Solve the cruise condition the span-loading figure plots: the report's
/// cruise altitude and Mach number at the trimmed body angle with the trimmed
/// stabilizer incidence (the un-trimmed design-point angle when the trim did
/// not converge). Returns the solve, its operating point and the weight (N)
/// the lift must balance: the loaded takeoff mass (payload plus the fuel the
/// tanks allow), the weight the trim solves its cruise CL for.
fn solve_cruise(
    report: &AnalysisReport,
    config: &AlasConfig,
) -> Option<(alas_aero::vlm::VlmResult, OperatingPoint, f64)> {
    let req = &config.requirements;
    let atmosphere = Atmosphere::new(req.cruise_altitude_m);
    let velocity = req.cruise_mach * atmosphere.speed_of_sound();
    if !(velocity.is_finite() && velocity > 0.0) {
        return None;
    }
    let mut plane = report.airplane.clone();
    let alpha_deg = match report.trimmed_design_point {
        Some(trim) if trim.geometric_body_alpha_deg.is_finite() && trim.trim_ih_deg.is_finite() => {
            for wing in plane
                .wings
                .iter_mut()
                .filter(|wing| wing.name == "Horizontal Stabilizer")
            {
                wing.twist_about_pitch_axis = true;
                for section in &mut wing.xsecs {
                    section.twist = trim.trim_ih_deg;
                }
            }
            trim.geometric_body_alpha_deg
        }
        _ => design_body_alpha_deg(report),
    };
    // With the compressible lattice on, the trim angle and the polar's design
    // point angle are those of the Goethert/Prandtl-Glauert stretched lattice
    // at the cruise Mach (alas-stab `trim::compressible`), whose lift equals
    // the compressible lift at the same dynamic pressure; the load is
    // therefore read on that lattice, in either branch. With it off, both
    // angles are the incompressible lattice's and the plane stays unstretched.
    // The report's own basis decides, not the configuration flag: a
    // reference-compatibility report keeps the default flag but carries a
    // relabelled incompressible polar (`compressible_mach` is `None`).
    if report.polar.compressible_mach.is_some() {
        let beta = alas_stab::neutral_point::beta_for_mach(req.cruise_mach);
        plane = alas_stab::neutral_point::goethert_stretch(&plane, beta);
    }
    let op = OperatingPoint::new(atmosphere, velocity, alpha_deg, 0.0, 0.0, 0.0, 0.0);
    let result = vlm::run(
        &plane,
        &op,
        config.analysis.fine_spanwise_resolution.max(1) as usize,
        config.analysis.fine_chordwise_resolution.max(1) as usize,
    )
    .ok()?;
    let weight_n = report.loaded_takeoff_mass_kg(req.mtow_kg) * req.gravity_m_s2;

    Some((result, op, weight_n))
}

/// Lift integrated by the span-loading figure against the weight it carries.
///
/// `None` when the report has no wing or the cruise condition cannot be
/// solved.
pub fn span_loading_balance(
    report: &AnalysisReport,
    config: &AlasConfig,
) -> Option<SpanLoadingBalance> {
    if report.airplane.wings.is_empty() {
        return None;
    }
    let (result, op, weight_n) = solve_cruise(report, config)?;
    let (_, wing_lift_n) = main_wing_span_loading(&result, &op);
    Some(SpanLoadingBalance {
        wing_lift_n,
        total_lift_n: result.lift,
        weight_n,
        dynamic_pressure_pa: op.dynamic_pressure(),
    })
}

fn status(theme: Option<&str>, text: &str) -> Scene {
    crate::status_figure::figure_status_message(text, "", false, theme)
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

/// Plot panel lift per span and a total-lift-matched elliptic reference at
/// the trimmed cruise condition (cruise altitude, Mach and takeoff weight),
/// which the figure states under the plot.
pub fn figure_span_loading(
    report: &AnalysisReport,
    config: &AlasConfig,
    theme: Option<&str>,
) -> Scene {
    if report.airplane.wings.is_empty() {
        return status(theme, "VLM span loading unavailable");
    }
    let Some((vlm, op, weight_n)) = solve_cruise(report, config) else {
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
    let mut scene = Scene::new(760.0, 475.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Span Loading".to_owned());
    draw_title(&mut scene, "Span Loading", pal);
    scene.suppress_derived_title();
    let axes = Axes2D::new(
        (65.0, 55.0, 630.0, 330.0),
        // Root to tip: 0 to b/2 (never narrower than the last station).
        (0.0, semi.max(points.last().map_or(0.0, |p| p.0))),
        {
            let (lo, hi) = padded_range(
                points
                    .iter()
                    .map(|p| p.1)
                    .chain(elliptical.iter().map(|p| p.1)),
                0.08,
            );
            (lo.min(0.0), hi)
        },
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
    // Mark the tip (y = b/2); the curves are drawn only where there are strips.
    axes.add_line_series(
        &mut scene,
        &[(semi, axes.y_min), (semi, axes.y_max)],
        Stroke::dashed(Color::from_hex(pal.tick), 1.0, 2.0, 3.0),
    );
    scene.add(SceneElement::Text {
        text: format!("tip  b/2 = {semi:.1} m"),
        pos: [
            axes.map_point(semi, axes.y_max)[0] - 4.0,
            axes.top + axes.height - 6.0,
        ],
        font_size: 8.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Right,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: false,
    });
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
    scene.add(SceneElement::Text {
        text: format!(
            "Trimmed cruise: M {:.2}, {:.0} m, W {:.0} kN, wing lift {:.0} kN = {:.0} % W",
            config.requirements.cruise_mach,
            config.requirements.cruise_altitude_m,
            weight_n / 1000.0,
            total_wing_lift / 1000.0,
            100.0 * total_wing_lift / weight_n.max(1e-9),
        ),
        pos: [axes.left + axes.width * 0.5, axes.top + axes.height + 46.0],
        font_size: 8.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
    // Where the rest of the weight is carried: wing + tail/body split, so a
    // wing share far from 100 % reads as a split, not an unexplained number.
    scene.add(SceneElement::Text {
        text: format!(
            "Lift split: wing {:.0} % W, tail + body {:+.0} % W, total {:.0} % W",
            100.0 * total_wing_lift / weight_n.max(1e-9),
            100.0 * (vlm.lift - total_wing_lift) / weight_n.max(1e-9),
            100.0 * vlm.lift / weight_n.max(1e-9),
        ),
        pos: [axes.left + axes.width * 0.5, axes.top + axes.height + 59.0],
        font_size: 8.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
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
    // The legend names the root, mid and tip streamlines (distinct |Y|, hence
    // distinct colours), not the first three seeds, which share one station.
    let mut origins: Vec<f64> = lines
        .iter()
        .map(|line| line.first().copied().unwrap_or(center)[1].abs())
        .collect();
    origins.sort_by(f64::total_cmp);
    origins.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let mut legend_y: Vec<f64> = Vec::new();
    if let (Some(&first), Some(&last)) = (origins.first(), origins.last()) {
        legend_y.push(first);
        if origins.len() > 2 {
            legend_y.push(origins[origins.len() / 2]);
        }
        if origins.len() > 1 {
            legend_y.push(last);
        }
    }
    for (y, color) in legend_y.iter().map(|&y| {
        (
            y,
            Colormap::Plasma.sample((y / lateral_scale).clamp(0.0, 1.0)),
        )
    }) {
        legend.push((format!("Span |Y| = {y:.1} m"), LegendMarker::Circle(color)));
    }
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
