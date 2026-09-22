// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py:figure_landing_gear_planform (L2836-3017)
// Reference: alas @ rust-port-baseline.

use super::common::missing_datum_scene;
use super::mass_breakdown::AC_CHORD_FRACTION;
use crate::chart_kit::LegendMarker;
use crate::families::MAIN_GEAR_STATION_NOT_MEASURED;
use crate::scene::{
    Axes2D, Color, Fill, Point2D, Scene, SceneElement, Stroke, TextAlign, TextBaseline,
};
use crate::theme::get_palette;
use alas_config::AlasConfig;
use alas_mass::breakdown::{FUEL, OEW_KEYS, PAYLOAD};
use alas_perf::landing_gear::size_landing_gear_with_group_stations;
use alas_pipeline::full_analysis::AnalysisReport;
use std::collections::HashSet;
use std::f64::consts::PI;
// ---------------------------------------------------------------------------
// figure_landing_gear_planform
// ---------------------------------------------------------------------------

/// The fill color for one gear group: upstream's `group_colors.get(...,
/// "#9b59b6")`.
pub(super) fn gear_color(strut_label: &str) -> &'static str {
    match strut_label {
        "NLG" => "#2ecc71",
        "MLG-L" | "MLG-R" => "#e74c3c",
        "MLG-Body-L" | "MLG-Body-R" => "#f39c12",
        _ => "#9b59b6",
    }
}

/// A filled `n`-sided polygon approximating an ellipse in data coordinates,
/// centered at `(lateral, station)` with full extents `width` (lateral) and
/// `height` (longitudinal), projected to canvas space by `to_px`.
pub(super) fn ellipse_polygon(
    to_px: &impl Fn(f64, f64) -> Point2D,
    lateral: f64,
    station: f64,
    width: f64,
    height: f64,
    n: usize,
) -> Vec<Point2D> {
    (0..n)
        .map(|i| {
            let theta = 2.0 * PI * (i as f64) / (n as f64);
            to_px(
                lateral + width / 2.0 * theta.cos(),
                station + height / 2.0 * theta.sin(),
            )
        })
        .collect()
}

/// Top-down planform view of the aircraft with the sized landing gear:
/// `figure_landing_gear_planform`. Every wheel is drawn individually (NLG
/// vs. MLG-L/R/Body distinguished by color) at its real position and true
/// tire size, over the wing/fuselage outline; nose at the top, matching
/// `figure_geometry`'s top-view convention.
pub fn figure_landing_gear_planform(
    report: &AnalysisReport,
    config: &AlasConfig,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let plane = &report.airplane;
    let masses = &report.component_masses;
    let mac = plane.c_ref;
    let wing = plane
        .wings
        .iter()
        .find(|w| w.name == "Main Wing")
        .unwrap_or(&plane.wings[0]);
    let x_wing_ac = wing.aerodynamic_center(AC_CHORD_FRACTION)[0];
    let x_mac_le = x_wing_ac - 0.25 * mac;

    let mm = &config.mass_model;
    let gear_cfg = &config.landing_gear;

    let fus = &plane.fuselages[0];
    let fus_start_x = fus.xsecs[0].xyz_c[0];
    let fus_end_x = fus.xsecs[fus.xsecs.len() - 1].xyz_c[0];
    let fus_len = fus_end_x - fus_start_x;
    let fallback_x_nlg = fus_start_x + fus_len * mm.nlg_x_fraction;
    let fallback_x_mlg = x_mac_le + mm.mlg_x_fraction_mac * mac;
    // Resolved through the shared gate rather than from a fallback rebuilt
    // here. This figure draws every wheel at its station; an aircraft whose
    // main-gear station the mass model refuses has none to draw, and a
    // planform with legs under the wing root would be the most convincing
    // possible statement of a datum nobody measured.
    let Ok(gear_stations) = alas_pipeline::gear_stations::resolved_gear_stations(
        config,
        plane,
        fallback_x_nlg,
        fallback_x_mlg,
        fus_start_x,
        fus_len,
    ) else {
        return missing_datum_scene(
            520.0,
            640.0,
            "Landing-Gear Planform",
            pal,
            MAIN_GEAR_STATION_NOT_MEASURED,
        );
    };
    let x_nlg = gear_stations.x_nlg_m;
    let x_mlg = gear_stations.x_mlg_m;
    let fus_diam = if config.geometry.fuselage.diameter_m > 0.0 {
        config.geometry.fuselage.diameter_m
    } else if fus.xsecs.is_empty() {
        4.0
    } else {
        fus.xsecs.iter().map(|s| s.width).fold(f64::MIN, f64::max)
    };

    // Aerodynamic (gear-independent) CG limits: the same worst-case loads
    // the optimizer's CG check sizes the gear against.
    let sm_val = if report.static_margin.is_nan() {
        0.10
    } else {
        report.static_margin
    };
    let x_np = plane.xyz_ref[0] + sm_val * mac;
    let target_sm = config.requirements.target_static_margin;
    let cg_range = config.requirements.cg_range_pct_mac;
    let np_pct = (x_np - x_mac_le) / mac.max(0.001) * 100.0;
    let aft_limit_mac = np_pct - target_sm * 100.0;
    let fwd_limit_mac = aft_limit_mac - cg_range;
    let aero_fwd_lim_x = x_mac_le + fwd_limit_mac / 100.0 * mac;
    let aero_aft_lim_x = x_mac_le + aft_limit_mac / 100.0 * mac;

    let oew_mass: f64 = OEW_KEYS
        .iter()
        .map(|&k| masses.get(k).copied().unwrap_or(0.0))
        .sum();
    let mtow_mass = oew_mass
        + masses.get(PAYLOAD).copied().unwrap_or(0.0)
        + masses.get(FUEL).copied().unwrap_or(0.0).max(0.0);

    let gear = size_landing_gear_with_group_stations(
        mtow_mass,
        x_nlg,
        x_mlg,
        aero_fwd_lim_x,
        aero_aft_lim_x,
        fus_diam,
        fus_diam * 1.1,
        &gear_stations.main_gear_x_m,
        gear_cfg,
    );

    // --- Data extents (lateral Y, longitudinal/station X), then a scale
    // that keeps both axes in true proportion (Python's `set_aspect("equal")`).
    let mut y_min = -fus_diam / 2.0;
    let mut y_max = fus_diam / 2.0;
    let mut x_min = fus_start_x;
    let mut x_max = fus_end_x;
    for w in &plane.wings {
        for sec in &w.xsecs {
            let ly = sec.xyz_le[1];
            let lx0 = sec.xyz_le[0];
            let lx1 = lx0 + sec.chord;
            x_min = x_min.min(lx0);
            x_max = x_max.max(lx1);
            let extent = if w.symmetric { ly.abs() } else { ly };
            y_min = y_min.min(-extent.abs());
            y_max = y_max.max(extent.abs());
        }
    }
    for wheel in &gear.wheels {
        y_min = y_min.min(wheel.y - wheel.width_m);
        y_max = y_max.max(wheel.y + wheel.width_m);
        x_min = x_min.min(wheel.x - wheel.diameter_m);
        x_max = x_max.max(wheel.x + wheel.diameter_m);
    }
    let y_pad = (y_max - y_min).max(1.0) * 0.08;
    let x_pad = (x_max - x_min).max(1.0) * 0.08;
    y_min -= y_pad;
    y_max += y_pad;
    x_min -= x_pad;
    x_max += x_pad;

    let canvas_w = 520.0;
    let canvas_h = 640.0;
    let avail = (55.0, 78.0, canvas_w - 55.0 - 20.0, canvas_h - 78.0 - 130.0);
    let (avail_left, avail_top, avail_w, avail_h) = avail;
    let scale = (avail_w / (y_max - y_min)).min(avail_h / (x_max - x_min));
    let plot_w = (y_max - y_min) * scale;
    let plot_h = (x_max - x_min) * scale;
    let left = avail_left + (avail_w - plot_w) / 2.0;
    let top = avail_top + (avail_h - plot_h) / 2.0;

    // `-station` is plotted on the axes' Y so the nose (smallest station)
    // renders at the top: `Axes2D` requires an ascending range, and negating
    // reverses the sense without violating that.
    let axes = Axes2D::new(
        (left, top, plot_w, plot_h),
        (y_min, y_max),
        (-x_max, -x_min),
    );
    let to_px = |lateral: f64, station: f64| axes.map_point(lateral, -station);

    let mut scene = Scene::new(canvas_w, canvas_h, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Landing-Gear Planform".to_owned());
    axes.draw_frame(&mut scene, pal);

    scene.add(SceneElement::Text {
        text: format!(
            "Strut: {}   |   Track: {:.2} m   |   Wheelbase: {:.2} m   |   Turnover: {:.0} deg ({})",
            gear.strut_material,
            gear.track_width_m,
            gear.wheelbase_m,
            gear.turnover_angle_deg,
            if gear.turnover_ok { "OK" } else { "EXCEEDS LIMIT" }
        ),
        pos: [canvas_w / 2.0, 46.0],
        font_size: 9.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });
    scene.add(SceneElement::Text {
        text: crate::families::mass_balance::mass_method_note(config).to_owned(),
        pos: [canvas_w / 2.0, 60.0],
        font_size: 7.6,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });

    // --- Wing planform outline(s).
    let wing_fill = Fill::new(Color::rgba(31, 119, 180, 64));
    let wing_stroke = Stroke::new(Color::from_hex(pal.spine), 0.7);
    for w in &plane.wings {
        let mut le: Vec<(f64, f64)> = Vec::new();
        let mut te: Vec<(f64, f64)> = Vec::new();
        for sec in &w.xsecs {
            le.push((sec.xyz_le[1], sec.xyz_le[0]));
            te.push((sec.xyz_le[1], sec.xyz_le[0] + sec.chord));
        }
        let sides: &[f64] = if w.symmetric { &[1.0, -1.0] } else { &[1.0] };
        for &side in sides {
            let mut pts: Vec<Point2D> = le.iter().map(|&(y, x)| to_px(y * side, x)).collect();
            pts.extend(te.iter().rev().map(|&(y, x)| to_px(y * side, x)));
            scene.add(SceneElement::Polygon {
                points: pts,
                fill: Some(wing_fill),
                stroke: Some(wing_stroke.clone()),
            });
        }
    }

    // --- Fuselage outline.
    let xc: Vec<f64> = fus.xsecs.iter().map(|s| s.xyz_c[0]).collect();
    let r: Vec<f64> = fus.xsecs.iter().map(|s| s.width / 2.0).collect();
    let mut fus_pts: Vec<Point2D> = xc.iter().zip(&r).map(|(&x, &ri)| to_px(ri, x)).collect();
    fus_pts.extend(xc.iter().zip(&r).rev().map(|(&x, &ri)| to_px(-ri, x)));
    scene.add(SceneElement::Polygon {
        points: fus_pts,
        fill: Some(Fill::new(Color::rgba(127, 127, 127, 90))),
        stroke: None,
    });

    // --- Wheels: each drawn individually, NLG vs. MLG-* color coded.
    for wheel in &gear.wheels {
        let points = ellipse_polygon(
            &to_px,
            wheel.y,
            wheel.x,
            wheel.width_m,
            wheel.diameter_m,
            28,
        );
        scene.add(SceneElement::Polygon {
            points,
            fill: Some(Fill::new(Color::from_hex(gear_color(&wheel.strut_label)))),
            stroke: Some(Stroke::new(Color::from_hex(pal.spine), 1.0)),
        });
    }

    // --- Axis labels.
    scene.add(SceneElement::Text {
        text: "Y [m]".to_owned(),
        pos: [left + plot_w / 2.0, top + plot_h + 20.0],
        font_size: 9.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
    scene.add(SceneElement::Text {
        text: "X [m] (fuselage station)".to_owned(),
        pos: [left - 34.0, top + plot_h / 2.0],
        font_size: 9.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: -90.0,
        bold: false,
    });

    // --- Legend: one entry per strut label actually drawn, first-seen order.
    let mut seen = HashSet::new();
    let mut entries: Vec<(String, LegendMarker)> = Vec::new();
    for wheel in &gear.wheels {
        if seen.insert(wheel.strut_label.clone()) {
            entries.push((
                wheel.strut_label.clone(),
                LegendMarker::Patch(Color::from_hex(gear_color(&wheel.strut_label))),
            ));
        }
    }
    draw_horizontal_legend(
        &mut scene,
        [left, top + plot_h + 42.0],
        plot_w,
        &entries,
        pal,
        8.5,
    );

    scene
}

/// Draw the gear legend in a compact horizontal grid so each strut category
/// remains visible without consuming the aircraft's longitudinal axis.
fn draw_horizontal_legend(
    scene: &mut Scene,
    pos: Point2D,
    width: f64,
    entries: &[(String, LegendMarker)],
    pal: &crate::theme::Palette,
    font_size: f64,
) {
    if entries.is_empty() {
        return;
    }
    let columns = entries.len().clamp(1, 3);
    let column_width = width / columns as f64;
    let row_height = font_size + 6.0;
    for (index, (label, marker)) in entries.iter().enumerate() {
        let column = index % columns;
        let row = index / columns;
        let x = pos[0] + column as f64 * column_width;
        let y = pos[1] + row as f64 * row_height;
        let mid_y = y + font_size * 0.5;
        match marker {
            LegendMarker::Line(stroke) => scene.add(SceneElement::Line {
                p1: [x, mid_y],
                p2: [x + 18.0, mid_y],
                stroke: stroke.clone(),
            }),
            LegendMarker::Patch(color) => scene.add(SceneElement::Rect {
                x,
                y,
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
        }
        scene.add(SceneElement::Text {
            text: label.clone(),
            pos: [x + 24.0, mid_y],
            font_size,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }
}
