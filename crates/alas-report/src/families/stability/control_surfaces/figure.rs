// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py:figure_control_surfaces (L1735-1991)
// Reference: alas @ rust-port-baseline.

use super::geometry::{
    cs_surface_area, cs_surface_patch, draw_planform_fill, draw_top_patch, fmt_volume_coef,
    planform_bounds, TopAxes,
};
use crate::chart_kit::{draw_title, LegendMarker};
use crate::scene::{
    Axes2D, Color, Fill, Point2D, Scene, SceneElement, Stroke, TextAlign, TextBaseline,
};
use crate::theme::{get_palette, Palette};
use alas_config::control_surfaces::ControlSurfacesConfig;
use alas_config::AlasConfig;
use alas_geom::aircraft::wing::Wing;
use alas_pipeline::full_analysis::AnalysisReport;
use alas_stab::trim::tail_volume_coefficients;

/// Generate the control-surface layout and tail-volume sizing figure.
pub fn figure_control_surfaces(
    report: &AnalysisReport,
    config: &AlasConfig,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let plane = &report.airplane;

    if plane.wings.is_empty() {
        return super::super::status_scene(
            "Control Surfaces & Tail Sizing",
            "No wing geometry available.",
            pal,
        );
    }

    let cs = &config.control_surfaces;
    let w_opt = &config.optimizer.weights;
    let wing = &plane.wings[0];
    let hstab = plane.wings.get(1);
    let vstab = plane.wings.get(2);

    let width = if vstab.is_some() { 1020.0 } else { 780.0 };
    let mut scene = Scene::new(width, 660.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Control Surfaces & Tail Sizing".to_owned());
    draw_title(&mut scene, "Control Surfaces & Tail Sizing", pal);
    scene.suppress_derived_title();

    let (y_min, y_max, x_min, x_max) = planform_bounds(plane);
    let top_w = if vstab.is_some() { 560.0 } else { 700.0 };
    let top_axes = TopAxes::new((60.0, 70.0, top_w, 460.0), (y_min, y_max), (x_min, x_max));

    draw_planform_fill(
        &mut scene,
        &top_axes,
        plane,
        Color::rgba(176, 190, 197, 128),
    );
    top_axes
        .0
        .draw_frame_with_labels(&mut scene, pal, "Span Y [m]", "Longitudinal X [m]");

    let mut legend: Vec<(String, Color)> = Vec::new();
    let wing_semi = wing.projected_span() / 2.0;
    draw_top_patch(
        &mut scene,
        &top_axes,
        &mut legend,
        &wing.xsecs,
        cs.slat_span_start_frac * wing_semi,
        cs.slat_span_end_frac * wing_semi,
        0.0,
        cs.slat_chord_fraction,
        "#e74c3c",
        "Slat",
        wing.symmetric,
    );
    draw_top_patch(
        &mut scene,
        &top_axes,
        &mut legend,
        &wing.xsecs,
        cs.flap_span_start_frac * wing_semi,
        cs.flap_span_end_frac * wing_semi,
        1.0 - cs.flap_chord_fraction,
        1.0,
        "#27ae60",
        "Flap",
        wing.symmetric,
    );
    draw_top_patch(
        &mut scene,
        &top_axes,
        &mut legend,
        &wing.xsecs,
        cs.aileron_span_start_frac * wing_semi,
        cs.aileron_span_end_frac * wing_semi,
        1.0 - cs.aileron_chord_fraction,
        1.0,
        "#2980b9",
        "Aileron",
        wing.symmetric,
    );
    let spoiler_hi = 1.0 - cs.flap_chord_fraction;
    draw_top_patch(
        &mut scene,
        &top_axes,
        &mut legend,
        &wing.xsecs,
        cs.spoiler_span_start_frac * wing_semi,
        cs.spoiler_span_end_frac * wing_semi,
        spoiler_hi - cs.spoiler_chord_fraction,
        spoiler_hi,
        "#9b59b6",
        "Spoiler",
        wing.symmetric,
    );
    if let Some(hstab) = hstab {
        let hstab_semi = hstab.projected_span() / 2.0;
        draw_top_patch(
            &mut scene,
            &top_axes,
            &mut legend,
            &hstab.xsecs,
            cs.elevator_span_start_frac * hstab_semi,
            cs.elevator_span_end_frac * hstab_semi,
            1.0 - cs.elevator_chord_fraction,
            1.0,
            "#f39c12",
            "Elevator",
            hstab.symmetric,
        );
    }

    scene.add(SceneElement::Text {
        text: "Top view".to_owned(),
        pos: [top_axes.0.left, top_axes.0.top - 10.0],
        font_size: 11.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
    if let Some(vstab) = vstab {
        draw_vstab_side_view(&mut scene, vstab, cs, &mut legend, top_w, pal);
    }

    let legend_entries = legend
        .iter()
        .map(|(n, c)| (n.clone(), LegendMarker::Patch(*c)))
        .collect::<Vec<_>>();
    draw_horizontal_legend(&mut scene, [60.0, 600.0], &legend_entries, pal, 8.5);

    let (vh, vv) = tail_volume_coefficients(plane);
    let info = [
        fmt_volume_coef(
            "Vh",
            vh,
            w_opt.min_hstab_volume_coef,
            w_opt.max_hstab_volume_coef,
        ),
        fmt_volume_coef(
            "Vv",
            vv,
            w_opt.min_vstab_volume_coef,
            w_opt.max_vstab_volume_coef,
        ),
    ];
    draw_info_text(&mut scene, &info, [60.0, 630.0], pal);

    scene
}

/// The v-stab side view (X horizontal, Z vertical, not inverted): its own
/// filled outline plus the rudder patch.
fn draw_vstab_side_view(
    scene: &mut Scene,
    vstab: &Wing,
    cs: &ControlSurfacesConfig,
    legend: &mut Vec<(String, Color)>,
    top_w: f64,
    pal: &'static Palette,
) {
    let z_vals: Vec<f64> = vstab.xsecs.iter().map(|xs| xs.xyz_le[2]).collect();
    let le: Vec<f64> = vstab.xsecs.iter().map(|xs| xs.xyz_le[0]).collect();
    let te: Vec<f64> = vstab
        .xsecs
        .iter()
        .map(|xs| xs.xyz_le[0] + xs.chord)
        .collect();

    let x_min = le
        .iter()
        .cloned()
        .fold(f64::INFINITY, f64::min)
        .min(te.iter().cloned().fold(f64::INFINITY, f64::min));
    let x_max = le
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max)
        .max(te.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
    let z_lo = z_vals.iter().cloned().fold(f64::INFINITY, f64::min);
    let z_hi = z_vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let axes = Axes2D::new(
        (top_w + 150.0, 70.0, 250.0, 460.0),
        (x_min - 1.0, x_max + 1.0),
        (z_lo - 1.0, z_hi + 1.0),
    )
    .with_equal_aspect();
    axes.draw_frame_with_labels(scene, pal, "Longitudinal X [m]", "Height Z [m]");

    let mut fill_pts: Vec<(f64, f64)> = le.iter().zip(&z_vals).map(|(&x, &z)| (x, z)).collect();
    fill_pts.extend(
        te.iter()
            .rev()
            .zip(z_vals.iter().rev())
            .map(|(&x, &z)| (x, z)),
    );
    scene.add(SceneElement::Polygon {
        points: fill_pts
            .iter()
            .map(|&(x, z)| axes.map_point(x, z))
            .collect(),
        fill: Some(Fill::new(Color::rgba(176, 190, 197, 128))),
        stroke: Some(Stroke::new(Color::from_hex("#616a6e"), 1.0)),
    });

    let rz0 = z_lo + cs.rudder_span_start_frac * (z_hi - z_lo);
    let rz1 = z_lo + cs.rudder_span_end_frac * (z_hi - z_lo);
    let poly = cs_surface_patch(
        &vstab.xsecs,
        2,
        rz0,
        rz1,
        1.0 - cs.rudder_chord_fraction,
        1.0,
    );
    let side_pts: Vec<Point2D> = poly.iter().map(|&(z, x)| axes.map_point(x, z)).collect();
    scene.add(SceneElement::Polygon {
        points: side_pts,
        fill: Some(Fill::new(Color::from_hex("#e67e22"))),
        stroke: Some(Stroke::new(Color::rgb(0, 0, 0), 0.6)),
    });
    if !legend.iter().any(|(n, _)| n == "Rudder") {
        legend.push(("Rudder".to_owned(), Color::from_hex("#e67e22")));
    }
    // Computed and discarded, matching upstream's own dead `rows.append` on
    // the rudder -- see the module doc.
    let _ = cs_surface_area(
        &vstab.xsecs,
        2,
        rz0,
        rz1,
        1.0 - cs.rudder_chord_fraction,
        1.0,
        vstab.symmetric,
    );

    scene.add(SceneElement::Text {
        text: "V-stab side view".to_owned(),
        pos: [axes.left, axes.top - 10.0],
        font_size: 11.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
}

/// Place the computed Vh/Vv values beside the surface legend without a
/// high-contrast box that competes with the geometry.
fn draw_info_text(scene: &mut Scene, lines: &[String], pos: [f64; 2], pal: &'static Palette) {
    let mut x = pos[0];
    for line in lines {
        scene.add(SceneElement::Text {
            text: line.clone(),
            pos: [x, pos[1]],
            font_size: 9.5,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
        x += 24.0 + line.chars().count() as f64 * 9.5 * 0.70;
    }
}

/// Keep the surface legend in one compact row, leaving a separate row below
/// it for the tail-volume coefficients and their status text.
fn draw_horizontal_legend(
    scene: &mut Scene,
    pos: [f64; 2],
    entries: &[(String, LegendMarker)],
    pal: &'static Palette,
    font_size: f64,
) {
    let mut x = pos[0];
    for (label, marker) in entries {
        let mid_y = pos[1] + font_size * 0.5;
        match marker {
            LegendMarker::Patch(color) => scene.add(SceneElement::Rect {
                x,
                y: pos[1],
                width: 14.0,
                height: font_size,
                rx: 1.0,
                fill: Some(Fill::new(*color)),
                stroke: None,
            }),
            LegendMarker::Circle(color) => scene.add(SceneElement::Circle {
                center: [x + 7.0, mid_y],
                radius: 5.0,
                fill: Some(Fill::new(*color)),
                stroke: None,
            }),
            LegendMarker::Line(stroke) => scene.add(SceneElement::Line {
                p1: [x, mid_y],
                p2: [x + 18.0, mid_y],
                stroke: stroke.clone(),
            }),
        }
        scene.add(SceneElement::Text {
            text: label.clone(),
            pos: [x + 20.0, mid_y],
            font_size,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
        x += 28.0 + label.chars().count() as f64 * font_size * 0.52;
    }
}
