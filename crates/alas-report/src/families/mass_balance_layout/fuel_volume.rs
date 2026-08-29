// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py:figure_fuel_volume_check (L3018-3087)
// Reference: alas @ rust-port-baseline.

use crate::chart_kit::{draw_horizontal_legend_columns, draw_title, LegendMarker};
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;
use alas_config::AlasConfig;
use alas_geom::aircraft::spacing::linspace;
use alas_geom::aircraft::wing::Wing;
use alas_mass::breakdown::FUEL;
use alas_pipeline::full_analysis::AnalysisReport;
/// `color` with its alpha channel replaced -- `Color` carries no builder for
/// this, so the two channels a translucent fill needs are set by hand.
pub(super) fn with_alpha(color: Color, a: u8) -> Color {
    Color::rgba(color.r, color.g, color.b, a)
}

/// Torenbeek geometric wing fuel-tank volume estimate [m^3] --
/// `physics.performance.wing_fuel_volume_m3`:
///
/// `V = 0.54 * (S^2 / b) * (t/c)_root * (1 + lambda + lambda^2) / (1 + lambda)^2`
pub(super) fn wing_fuel_volume_m3(wing: &Wing, usable_fraction: f64) -> f64 {
    // This is the product report path: the Torenbeek correlation uses the
    // same projected main-wing reference quantities as the rest of the
    // performance model. The unfolded geometry remains available only to
    // explicitly named compatibility callers.
    let s = wing.reference_area();
    let b = wing.reference_span();
    let taper = wing.taper_ratio();
    let sample = linspace(0.0, 1.0, 101);
    let t_over_c_root = wing.xsecs[0].airfoil.max_thickness(&sample);
    let term_taper = (1.0 + taper + taper * taper) / (1.0 + taper).powi(2);
    let v_geo = 0.54 * (s * s / b.max(1e-6)) * t_over_c_root * term_taper;
    v_geo * usable_fraction.clamp(0.0, 1.0)
}

/// Single-bar check: does the wing physically have room for the fuel the
/// design requires? -- `figure_fuel_volume_check`, deliberately minimal (a
/// yes/no engineering check, not a multi-panel figure). A green bar means
/// the Torenbeek usable tank volume, converted to mass at the configured
/// fuel density, covers `component_masses["Fuel"]` with margin; red means
/// it does not.
pub fn figure_fuel_volume_check(
    report: &AnalysisReport,
    config: &AlasConfig,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let wing = &report.airplane.wings[0];
    let mm = &config.mass_model;

    let tank_volume_m3 = wing_fuel_volume_m3(wing, mm.fuel_tank_usable_fraction);
    let tank_capacity_kg = tank_volume_m3 * mm.fuel_density_kg_m3;
    let required_fuel_kg = report
        .component_masses
        .get(FUEL)
        .copied()
        .unwrap_or(0.0)
        .max(0.0);
    let sufficient = tank_capacity_kg >= required_fuel_kg;

    let cap_t = tank_capacity_kg / 1000.0;
    let req_t = required_fuel_kg / 1000.0;
    let bar_max = if cap_t.max(req_t) > 0.0 {
        cap_t.max(req_t) * 1.25
    } else {
        1.0
    };
    let color = if sufficient { "#27ae60" } else { "#e74c3c" };

    let mut scene = Scene::new(700.0, 320.0, Some(Color::from_hex(pal.bg)));
    let title = "Wing Fuel-Volume Check";
    scene.title = Some(title.to_owned());
    draw_title(&mut scene, title, pal);
    scene.suppress_derived_title();
    let axes = Axes2D::new((145.0, 62.0, 485.0, 150.0), (-0.5, 1.5), (0.0, bar_max));
    draw_categorical_frame(&axes, &mut scene, pal, &[]);

    let category_width = 0.32;
    let capacity_x = 0.0;
    let required_x = 1.0;
    let p_cap0 = axes.map_point(capacity_x - category_width / 2.0, 0.0);
    let p_cap1 = axes.map_point(capacity_x + category_width / 2.0, cap_t);
    scene.add(SceneElement::Rect {
        x: p_cap0[0],
        y: p_cap1[1],
        width: (p_cap1[0] - p_cap0[0]).max(0.5),
        height: (p_cap0[1] - p_cap1[1]).max(0.5),
        rx: 0.0,
        fill: Some(Fill::new(with_alpha(Color::from_hex(color), 90))),
        stroke: Some(Stroke::new(Color::from_hex(color), 1.5)),
    });
    let p_req0 = axes.map_point(required_x - category_width / 2.0, 0.0);
    let p_req1 = axes.map_point(required_x + category_width / 2.0, req_t);
    scene.add(SceneElement::Rect {
        x: p_req0[0],
        y: p_req1[1],
        width: (p_req1[0] - p_req0[0]).max(0.5),
        height: (p_req0[1] - p_req1[1]).max(0.5),
        rx: 0.0,
        fill: Some(Fill::new(Color::from_hex(color))),
        stroke: None,
    });
    for (x, value) in [(capacity_x, cap_t), (required_x, req_t)] {
        let p = axes.map_point(x, value.clamp(axes.y_min, axes.y_max));
        scene.add(SceneElement::Text {
            text: format!("{value:.1} t"),
            pos: [p[0], p[1] - 6.0],
            font_size: 8.5,
            color: Color::from_hex(pal.title),
            align: TextAlign::Center,
            baseline: TextBaseline::Bottom,
            angle_deg: 0.0,
            bold: true,
        });
    }
    // Dashed capacity reference line stays inside the plot and above the
    // category labels, even when the required bar exceeds capacity.
    let p0 = axes.map_point(axes.x_min, cap_t.clamp(axes.y_min, axes.y_max));
    let p1 = axes.map_point(axes.x_max, cap_t.clamp(axes.y_min, axes.y_max));
    scene.add(SceneElement::Line {
        p1: p0,
        p2: p1,
        stroke: Stroke::dashed(Color::from_hex(color), 1.5, 5.0, 3.0),
    });

    let entries = vec![
        (
            "Tank capacity".to_owned(),
            LegendMarker::Patch(with_alpha(Color::from_hex(color), 90)),
        ),
        (
            "Required fuel".to_owned(),
            LegendMarker::Patch(Color::from_hex(color)),
        ),
    ];
    draw_horizontal_legend_columns(&mut scene, [118.0, 274.0], &entries, pal, 8.5);

    scene
}

/// Draw a numeric Y frame with category names on X; numeric X ticks would
/// falsely imply that the two storage states have a metric distance.
fn draw_categorical_frame(
    axes: &Axes2D,
    scene: &mut Scene,
    pal: &crate::theme::Palette,
    labels: &[&str],
) {
    let spine = Color::from_hex(pal.spine);
    let tick = Color::from_hex(pal.tick);
    scene.add(SceneElement::Rect {
        x: axes.left,
        y: axes.top,
        width: axes.width,
        height: axes.height,
        rx: 0.0,
        fill: None,
        stroke: Some(Stroke::new(spine, 1.0)),
    });
    for i in 0..=4 {
        let value = axes.y_min + (axes.y_max - axes.y_min) * i as f64 / 4.0;
        let p = axes.map_point(axes.x_min, value);
        scene.add(SceneElement::Line {
            p1: [axes.left, p[1]],
            p2: [axes.left + axes.width, p[1]],
            stroke: Stroke::dashed(Color::rgba(spine.r, spine.g, spine.b, 105), 0.7, 2.0, 3.0),
        });
    }
    for (index, label) in labels.iter().enumerate() {
        let x = axes.left + axes.width * index as f64 / (labels.len() - 1).max(1) as f64;
        scene.add(SceneElement::Line {
            p1: [x, axes.top + axes.height],
            p2: [x, axes.top + axes.height + 4.0],
            stroke: Stroke::new(spine, 1.0),
        });
        scene.add(SceneElement::Text {
            text: (*label).to_owned(),
            pos: [x, axes.top + axes.height + 7.0],
            font_size: 8.5,
            color: tick,
            align: TextAlign::Center,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: false,
        });
    }
    scene.add(SceneElement::Text {
        text: "Fuel storage".to_owned(),
        pos: [axes.left + axes.width / 2.0, axes.top + axes.height + 29.0],
        font_size: 10.0,
        color: tick,
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
    scene.add(SceneElement::Text {
        text: "Fuel mass [t]".to_owned(),
        pos: [axes.left - 48.0, axes.top + axes.height / 2.0],
        font_size: 10.0,
        color: tick,
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: -90.0,
        bold: false,
    });
}
