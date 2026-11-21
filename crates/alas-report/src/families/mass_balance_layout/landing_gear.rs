// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py:figure_landing_gear_planform (L2836-3017)

use super::common::missing_datum_scene;
use super::landing_gear_tables::{draw_dimension_callouts, draw_horizontal_legend};
use super::mass_breakdown::AC_CHORD_FRACTION;
use crate::chart_kit::LegendMarker;
use crate::families::MAIN_GEAR_STATION_NOT_MEASURED;
use crate::scene::{Axes2D, Color, Fill, Point2D, Scene, SceneElement, Stroke};
use crate::theme::get_palette;
use alas_config::AlasConfig;
use alas_mass::breakdown::{FUEL, OEW_KEYS, PAYLOAD};
use alas_perf::landing_gear::size_landing_gear_at_design_state;
use alas_pipeline::full_analysis::AnalysisReport;
use std::f64::consts::PI;
// ---------------------------------------------------------------------------
// figure_landing_gear_planform
// ---------------------------------------------------------------------------

/// Wording of the tire check, from the loads over the tire ratings.
///
/// A real overload is a load above the rating: the nose tire above its rated
/// static load or its dynamic (braking) rating, or a main tire above its rated
/// load. A tire loaded to less than its rating but short of the design margin
/// (Raymer 6th ed. Sec. 11.3 p.345 adds a 7 % margin to the calculated wheel load for FAR 25
/// aircraft, so `margin` includes it) is reported separately.
#[cfg(test)]
pub(super) fn tire_status(
    ratios: &alas_perf::landing_gear::tires::TireLoadRatios,
    lacks_design_factor: bool,
) -> &'static str {
    if [ratios.nose_static, ratios.nose_dynamic, ratios.main_static]
        .iter()
        .any(|ratio| *ratio > 1.0)
    {
        "OVERLOADED"
    } else if lacks_design_factor {
        "BELOW DESIGN FACTOR"
    } else {
        "OK"
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
    figure_landing_gear_planform_sized(report, config, theme, (620.0, 880.0))
}

/// The planform fitted in a `bounds` box of the caller's choosing (scene
/// units, width and height): the drawing keeps its true scale, so the scene
/// is as large as the box allows in one direction and smaller in the other.
/// The text keeps its size whatever the box.
pub fn figure_landing_gear_planform_sized(
    report: &AnalysisReport,
    config: &AlasConfig,
    theme: Option<&str>,
    bounds: (f64, f64),
) -> Scene {
    let pal = get_palette(theme);
    // The gear where this report's aircraft has it, whichever configuration
    // the caller holds.
    let config = &alas_pipeline::gear_stations::report_config(config, report);
    let plane = &report.airplane;
    let masses = &report.component_masses;
    let mac = plane.c_ref;
    let wing = plane
        .wings
        .iter()
        .find(|w| w.name == "Main Wing")
        .unwrap_or(&plane.wings[0]);
    let x_mac_le = plane.mac_frame().map_or_else(
        || wing.carried_box().aerodynamic_center(AC_CHORD_FRACTION)[0] - 0.25 * mac,
        |frame| frame.x_lemac_m,
    );

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

    // Governing physical CG limits at the heaviest evaluated loading state
    // (analyzed takeoff, or the heaviest state available), read from the
    // same gate the feasibility pipeline evaluates a design against:
    // the aerodynamic, ground-minimum-nose-load and
    // tip-back aft mechanisms and the max-nose-load/scissor-plot forward
    // ones, not this figure's own simplified `NP - target_sm` reconstruction
    // (that formula used the optimizer's *preferred* static margin, never a
    // hard physical floor, and ignored the ground/tip-back mechanisms
    // entirely). Falls back to that formula only when the gate itself
    // cannot be assessed (e.g. no neutral point yet), so a design still
    // being edited keeps a usable, clearly approximate estimate rather than
    // no gear at all.
    let gate_assessment = crate::families::model_cg_gate_assessment(report, config).ok();
    let gate_limits = gate_assessment
        .as_ref()
        .and_then(|assessment| {
            assessment
                .loading_states
                .iter()
                .max_by(|a, b| a.mass_kg.total_cmp(&b.mass_kg))
                .map(|state| {
                    (
                        state.physical_limits.fwd_limit_pct_mac,
                        state.physical_limits.aft_limit_pct_mac,
                    )
                })
        })
        .filter(|(fwd, aft)| fwd.is_finite() && aft.is_finite());
    let (fwd_limit_mac, aft_limit_mac) = gate_limits.unwrap_or_else(|| {
        let sm_val = if report.static_margin.is_nan() {
            0.10
        } else {
            report.static_margin
        };
        let x_np = plane.xyz_ref[0] + sm_val * mac;
        let target_sm = config.requirements.target_static_margin;
        let cg_range = config.requirements.cg_range_pct_mac;
        let np_pct = (x_np - x_mac_le) / mac.max(0.001) * 100.0;
        let aft = np_pct - target_sm * 100.0;
        (aft - cg_range, aft)
    });
    let aero_fwd_lim_x = x_mac_le + fwd_limit_mac / 100.0 * mac;
    let aero_aft_lim_x = x_mac_le + aft_limit_mac / 100.0 * mac;

    let oew_mass: f64 = OEW_KEYS
        .iter()
        .map(|&k| masses.get(k).copied().unwrap_or(0.0))
        .sum();
    let mtow_mass = oew_mass
        + masses.get(PAYLOAD).copied().unwrap_or(0.0)
        + masses.get(FUEL).copied().unwrap_or(0.0).max(0.0);

    // Loaded CG height above the ground plane: the mass model's own
    // global CG `z` (`report.physical_cg[2]`, positive up) above the
    // fuselage's lowest skin point, rather than a bare `1.1 * diameter`
    // guess unrelated to this design's actual mass distribution. The strut
    // and tire that carry the fuselage the rest of the way to the ground
    // are not sized yet at this call site (that is what this function
    // computes), so a fuselage-radius floor keeps the estimate from
    // collapsing to an implausibly small height for a design whose CG sits
    // near the belly; it remains an estimate, not a measured ground height.
    let z_ground_body = fus
        .xsecs
        .iter()
        .map(|section| section.xyz_c[2] - section.width / 2.0)
        .fold(f64::INFINITY, f64::min);
    let h_cg_estimate_m = if z_ground_body.is_finite() && report.physical_cg[2].is_finite() {
        (report.physical_cg[2] - z_ground_body).max(fus_diam * 0.5)
    } else {
        fus_diam * 1.1
    };

    let gear = size_landing_gear_at_design_state(
        mtow_mass,
        None,
        x_nlg,
        x_mlg,
        aero_fwd_lim_x,
        aero_aft_lim_x,
        Some(report.physical_cg[0]),
        super::tip_back::min_nose_fraction(config),
        super::tip_back::basis(report, config, gate_assessment.as_ref()),
        fus_diam,
        h_cg_estimate_m,
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
    let y_pad = (y_max - y_min).max(1.0) * 0.05;
    let x_pad = (x_max - x_min).max(1.0) * 0.05;
    y_min -= y_pad;
    y_max += y_pad;
    x_min -= x_pad;
    x_max += x_pad;

    // The canvas follows the drawing: the planform at true scale (equal
    // metres per pixel on both axes) plus two bands inside the frame, outside
    // the airframe, that carry the dimension lines (wheelbase on the left,
    // track under the tail), so no label sits on the aircraft. The figure is
    // as large as the drawing is inside the `bounds` box.
    let canvas_bound = bounds.0.max(320.0);
    let height_bound = bounds.1.max(360.0);
    const LEFT: f64 = 58.0;
    const TOP: f64 = 46.0;
    const RIGHT: f64 = 18.0;
    const BELOW: f64 = 92.0;
    const WHEELBASE_BAND: f64 = 54.0;
    const TRACK_BAND: f64 = 56.0;
    let (drawing_y, drawing_x) = (y_max - y_min, x_max - x_min);
    let scale = ((canvas_bound - LEFT - RIGHT - WHEELBASE_BAND) / drawing_y)
        .min((height_bound - TOP - BELOW - TRACK_BAND) / drawing_x);
    let plot_w = drawing_y * scale + WHEELBASE_BAND;
    let plot_h = drawing_x * scale + TRACK_BAND;
    let canvas_w = LEFT + plot_w + RIGHT;
    let canvas_h = TOP + plot_h + BELOW;
    let (left, top) = (LEFT, TOP);
    // The bands as data extents beyond the airframe.
    y_min -= WHEELBASE_BAND / scale;
    x_max += TRACK_BAND / scale;
    // Where the dimension lines stand: lateral position of the wheelbase
    // line and station of the track line, inside their bands.
    let wheelbase_line_y = y_min + 0.62 * WHEELBASE_BAND / scale;
    let track_line_x = x_max - 0.58 * TRACK_BAND / scale;

    // `-station` is plotted on the axes' Y so the nose (smallest station)
    // renders at the top: `Axes2D` requires an ascending range, and negating
    // reverses the sense without violating that.
    let axes = Axes2D::new(
        (left, top, plot_w, plot_h),
        (y_min, y_max),
        (-x_max, -x_min),
    )
    .with_negated_y_tick_labels();
    let to_px = |lateral: f64, station: f64| axes.map_point(lateral, -station);

    let mut scene = Scene::new(canvas_w, canvas_h, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Landing-Gear Planform".to_owned());
    axes.draw_frame_with_labels(&mut scene, pal, "Span Y [m]", "Longitudinal X [m]");

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
        // A centreline surface (a fin: every section on y = 0) is one
        // surface, drawn once, whether or not it is flagged symmetric. Mirroring
        // it would stack two identical outlines on the centreline.
        let on_centreline = w.xsecs.iter().all(|sec| sec.xyz_le[1].abs() < 1e-3);
        let sides: &[f64] = if w.symmetric && !on_centreline {
            &[1.0, -1.0]
        } else {
            &[1.0]
        };
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

    // --- Wheels: each drawn individually, each strut (NLG, MLG-L, MLG-R...)
    // one series of the shared categorical palette, in first-seen order.
    let mut series: Vec<(String, Color)> = Vec::new();
    for wheel in &gear.wheels {
        let color = crate::theme::series_entry(&mut series, &wheel.strut_label);
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
            fill: Some(Fill::new(color)),
            stroke: Some(Stroke::new(Color::from_hex(pal.spine), 1.0)),
        });
    }

    // --- Legend: one entry per strut label actually drawn, first-seen order.
    let entries: Vec<(String, LegendMarker)> = series
        .into_iter()
        .map(|(label, color)| (label, LegendMarker::Patch(color)))
        .collect();
    draw_horizontal_legend(
        &mut scene,
        [left, top + plot_h + 62.0],
        plot_w,
        &entries,
        pal,
        10.5,
    );

    draw_dimension_callouts(
        &mut scene,
        &gear,
        &to_px,
        (wheelbase_line_y, track_line_x),
        pal,
    );

    scene
}
