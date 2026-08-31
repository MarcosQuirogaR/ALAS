// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py
// (figure_propulsion_cycle_summary, figure_engine_designer_preview,
// _propulsion_cycle_summary_lines)
// Reference: alas @ rust-port-baseline.

//! On-design cruise cycle station temperatures and the engine designer's
//! nacelle-profile preview. Detailed cycle numbers are presented on the
//! Results Summary tab so the figure can keep its station chart uncluttered.

use super::design_point;
use crate::chart_kit::draw_title;
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::{get_palette, Palette};
use alas_config::{AlasConfig, EngineConfig};
use alas_prop::cycle::{
    anchor_mass_flow_kg_s, classify_engine_by_bpr, compute_turbofan_cycle, TurbofanCycleResult,
};

/// Station labels for the eight-bar temperature chart, in the order
/// `visualization.py`'s `stations`/`temps` lists build them.
const STATION_LABELS: [&str; 8] = [
    "T0 (static)",
    "Tt2 (fan face)",
    "Tt13 (fan exit)",
    "Tt25 (LPC exit)",
    "Tt3 (HPC exit)",
    "Tt4 (TIT)",
    "Tt45 (HPT exit)",
    "Tt5 (LPT exit)",
];

/// Per-station bar colors, copied verbatim from `figure_propulsion_cycle_summary`'s `colors` list.
const STATION_COLORS: [&str; 8] = [
    "#95a5a6", "#3498db", "#2ecc71", "#f1c40f", "#e67e22", "#e74c3c", "#9b59b6", "#8e44ad",
];

/// On-design cruise cycle: station stagnation temperatures (bar chart) plus a
/// text summary of specific thrust, TSFC, efficiencies, and a dimensional
/// cruise-thrust estimate anchored to the engine's rated static thrust.
pub fn figure_propulsion_cycle_summary(config: &AlasConfig, theme: Option<&str>) -> Scene {
    if let Some(scene) = super::technology::binding_error_scene(config, theme, (700.0, 460.0)) {
        return scene;
    }
    if super::is_turboprop(config) {
        return super::technology::turboprop_summary_scene(config, theme);
    }
    let pal = get_palette(theme);
    let mut scene = Scene::new(700.0, 460.0, Some(Color::from_hex(pal.bg)));
    let title = "On-Design Cruise Cycle Summary";
    scene.title = Some(title.to_owned());
    draw_title(&mut scene, title, pal);
    scene.suppress_derived_title();

    let out = compute_turbofan_cycle(&design_point(config), &config.propulsion_cycle);
    if !out.cycle_feasible {
        push_infeasible_text(&mut scene, &out.infeasibility_reason, 475.0, 240.0);
        return scene;
    }

    // Station 0 is the freestream static temperature, read off the same
    // fitted-atmosphere default used by the native atmosphere model
    // selects (see alas-atmo's own module doc); every other station comes
    // straight out of the cycle result.
    let t_static = alas_atmo::Atmosphere::new(config.requirements.cruise_altitude_m).temperature();
    let temps = [
        t_static,
        out.temperature_t0_k,
        out.temperature_t13_k,
        out.temperature_t25_k,
        out.temperature_t3_k,
        out.temperature_t4_k,
        out.temperature_t45_k,
        out.temperature_t5_k,
    ];

    let max_t = temps.iter().copied().fold(0.0f64, f64::max);
    let axes = Axes2D::new((70.0, 60.0, 580.0, 330.0), (0.0, 8.0), (0.0, max_t * 1.2));
    axes.draw_frame_with_labels(&mut scene, pal, "", "");
    scene.add(SceneElement::Text {
        text: "Stagnation temperature [K]".to_owned(),
        pos: [axes.left - 43.0, axes.top + axes.height * 0.5],
        font_size: 10.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: -90.0,
        bold: false,
    });

    for (i, (&t, color_hex)) in temps.iter().zip(STATION_COLORS.iter()).enumerate() {
        let p_top = axes.map_point(i as f64 + 0.5, t);
        let p_bot = axes.map_point(i as f64 + 0.5, 0.0);
        let bar_w = 44.0;
        scene.add(SceneElement::Rect {
            x: p_top[0] - bar_w * 0.5,
            y: p_top[1],
            width: bar_w,
            height: (p_bot[1] - p_top[1]).max(1.0),
            rx: 0.0,
            fill: Some(Fill::new(Color::from_hex(color_hex))),
            stroke: Some(Stroke::new(Color::rgb(0, 0, 0), 0.6)),
        });
        scene.add(SceneElement::Text {
            text: format!("{t:.0} K"),
            pos: [p_top[0], p_top[1] - 6.0],
            font_size: 7.0,
            color: Color::from_hex(pal.title),
            align: TextAlign::Center,
            baseline: TextBaseline::Bottom,
            angle_deg: 0.0,
            bold: true,
        });
        scene.add(SceneElement::Text {
            text: STATION_LABELS[i].to_owned(),
            pos: [p_top[0], p_bot[1] + 9.0],
            font_size: 7.5,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Right,
            baseline: TextBaseline::Top,
            angle_deg: -38.0,
            bold: false,
        });
    }

    scene
}

/// Return the detailed cycle numbers for the GUI Results Summary tab.
///
/// Keeping this beside the figure preserves one calculation and one wording
/// source while allowing the station chart to remain a focused visual.
pub fn propulsion_cycle_summary(config: &AlasConfig) -> Vec<String> {
    if let Err(error) = config.geometry.engine.active_model() {
        return vec![format!("Propulsion binding error: {error}")];
    }
    if super::is_turboprop(config) {
        return super::technology::turboprop_summary_lines(config);
    }
    let out = compute_turbofan_cycle(&design_point(config), &config.propulsion_cycle);
    if !out.cycle_feasible {
        return vec![
            "Cycle infeasible at this design point:".to_owned(),
            out.infeasibility_reason,
        ];
    }
    propulsion_cycle_summary_lines(config, &out, true)
}

/// Focused nacelle-profile preview for the Engine Designer tab. Detailed
/// cycle quantities belong in the Results Summary, where they can be read at
/// useful scale instead of competing with the editable geometry preview.
pub fn figure_engine_designer_preview(config: &AlasConfig, theme: Option<&str>) -> Scene {
    if let Some(scene) = super::technology::binding_error_scene(config, theme, (550.0, 390.0)) {
        return scene;
    }
    let pal = get_palette(theme);
    let mut scene = Scene::new(550.0, 390.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Engine Designer Preview".to_owned());
    let eng = &config.geometry.engine;

    panel_title(&mut scene, pal, "Nacelle profile silhouette", 30.0);
    if eng.nacelle_profile.is_empty() {
        scene.add(SceneElement::Text {
            text: "No nacelle profile defined".to_owned(),
            pos: [275.0, 210.0],
            font_size: 10.0,
            color: Color::from_hex(pal.title),
            align: TextAlign::Center,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    } else {
        draw_nacelle_silhouette(&mut scene, pal, eng);
    }
    scene
}

/// Text lines for the on-design cycle summary panel -- shared by
/// [`figure_propulsion_cycle_summary`] (wide, `verbose = true`) and
/// [`figure_engine_designer_preview`] (compact, `verbose = false`), matching
/// `_propulsion_cycle_summary_lines`'s reason for existing: the two can never
/// silently drift into showing different numbers for the same design.
///
/// Upstream's docstring says `verbose = False` also drops "the
/// per-efficiency-term breakout lines" -- but the function body only ever
/// guards the fuel-air-ratio line with `if verbose:`; the three efficiency
/// lines are unconditional in both callers. Reproduced as written, not as
/// documented.
fn propulsion_cycle_summary_lines(
    config: &AlasConfig,
    out: &TurbofanCycleResult,
    verbose: bool,
) -> Vec<String> {
    let engine = &config.geometry.engine;
    let eng = super::turbofan_spec(config);
    let cyc_cfg = &config.propulsion_cycle;

    let (mdot_total, _static) = anchor_mass_flow_kg_s(
        eng.rated_thrust_kn,
        eng.overall_pressure_ratio,
        eng.fan_pressure_ratio,
        eng.bypass_ratio,
        eng.turbine_inlet_temp_k,
        cyc_cfg,
    );
    let cruise_thrust_kn = if mdot_total.is_nan() {
        f64::NAN
    } else {
        out.specific_thrust_ms * mdot_total / 1000.0
    };
    let n_eng = engine.spanwise_positions_m.len();
    let cruise_line = if cruise_thrust_kn.is_nan() {
        "Per-engine thrust, this cruise pt : n/a".to_owned()
    } else {
        format!("Per-engine thrust, this cruise pt : {cruise_thrust_kn:7.1} kN")
    };
    let total_line = if cruise_thrust_kn.is_nan() {
        String::new()
    } else {
        let total_kn = cruise_thrust_kn * n_eng as f64;
        format!("Total installed thrust (x{n_eng})       : {total_kn:7.1} kN")
    };

    let engine_name = &engine.engine_name;
    let engine_class = classify_engine_by_bpr(eng.bypass_ratio);
    let cruise_mach = config.requirements.cruise_mach;
    let cruise_alt_km = config.requirements.cruise_altitude_m / 1000.0;
    let bpr = eng.bypass_ratio;
    let opr = eng.overall_pressure_ratio;
    let fpr = eng.fan_pressure_ratio;
    let tit = eng.turbine_inlet_temp_k;
    let sfn = out.specific_thrust_ms;
    let tsfc_computed = out.tsfc_mg_ns;
    let tsfc_reference = eng.cruise_tsfc_kg_kgf_hr / (9.81 * 3600.0) * 1.0e6;
    let thrust_static_kn = eng.rated_thrust_kn;

    let mut lines = vec![
        format!("Engine: {engine_name}  ({engine_class})"),
        format!("Design point: M{cruise_mach:.2} @ {cruise_alt_km:.1} km"),
        String::new(),
        format!("BPR = {bpr:.1}    OPR = {opr:.1}    FPR = {fpr:.2}    TIT = {tit:.0} K"),
        String::new(),
        format!("Specific thrust   SFn = {sfn:6.1} m/s"),
        format!("TSFC (computed)        = {tsfc_computed:6.2} mg/(N.s)"),
        format!("TSFC (reference)       = {tsfc_reference:6.2} mg/(N.s)"),
    ];
    if verbose {
        lines.push(format!(
            "Fuel-air ratio    f    = {:6.4}",
            out.fuel_air_ratio
        ));
    }
    lines.extend([
        String::new(),
        format!("Thermal efficiency (eta_t) = {:.3}", out.thermal_efficiency),
        format!(
            "Propulsive efficiency (eta_p) = {:.3}",
            out.propulsive_efficiency
        ),
        format!("Overall efficiency (eta_o) = {:.3}", out.overall_efficiency),
        String::new(),
        format!("Per-engine thrust, static (rated) : {thrust_static_kn:7.1} kN"),
        cruise_line,
        total_line,
    ]);
    lines
}

/// Nacelle silhouette: mirrored top/bottom edges through `nacelle_profile`'s
/// control points, filled between, with each control point marked and
/// annotated -- ported from `figure_engine_designer_preview`'s `ax_nacelle`
/// block. No `set_aspect("equal")` primitive exists here, so the axes rect is
/// sized to match the data's own aspect ratio instead, which reads the same.
fn draw_nacelle_silhouette(scene: &mut Scene, pal: &Palette, eng: &EngineConfig) {
    let xs: Vec<f64> = eng.nacelle_profile.iter().map(|&(x, _)| x).collect();
    let rs: Vec<f64> = eng
        .nacelle_profile
        .iter()
        .map(|&(_, frac)| frac * eng.radius_scale_m)
        .collect();

    let x_min = xs.iter().copied().fold(f64::INFINITY, f64::min);
    let x_max = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let r_max = rs.iter().copied().fold(0.0f64, f64::max).max(1e-6);

    let margin_x = (x_max - x_min).max(1e-6) * 0.1;
    let x_lo = x_min - margin_x;
    let x_hi = x_max + margin_x;
    let y_lo = -r_max * 1.15;
    let y_hi = r_max * 1.55; // extra headroom for the point annotations above

    let avail_w = 460.0;
    let avail_h = 300.0;
    let data_w = (x_hi - x_lo).max(1e-6);
    let data_h = (y_hi - y_lo).max(1e-6);
    let scale = (avail_w / data_w).min(avail_h / data_h);
    let plot_w = data_w * scale;
    let plot_h = data_h * scale;
    let left = 45.0 + (avail_w - plot_w) * 0.5;
    let top = 60.0 + (avail_h - plot_h) * 0.5;

    let axes = Axes2D::new((left, top, plot_w, plot_h), (x_lo, x_hi), (y_lo, y_hi));

    let top_pts: Vec<(f64, f64)> = xs.iter().zip(&rs).map(|(&x, &r)| (x, r)).collect();
    let bot_pts: Vec<(f64, f64)> = xs.iter().zip(&rs).map(|(&x, &r)| (x, -r)).collect();

    let mut poly_pts: Vec<[f64; 2]> = top_pts.iter().map(|&(x, y)| axes.map_point(x, y)).collect();
    poly_pts.extend(bot_pts.iter().rev().map(|&(x, y)| axes.map_point(x, y)));
    scene.add(SceneElement::Polygon {
        points: poly_pts,
        fill: Some(Fill::new(Color::rgba(31, 119, 180, 38))), // tab:blue @ alpha=0.15
        stroke: None,
    });

    axes.draw_frame(scene, pal);
    let line_stroke = Stroke::new(Color::from_hex("tab:blue"), 1.5);
    axes.add_line_series(scene, &top_pts, line_stroke.clone());
    axes.add_line_series(scene, &bot_pts, line_stroke);

    let zero_l = axes.map_point(x_lo, 0.0);
    let zero_r = axes.map_point(x_hi, 0.0);
    scene.add(SceneElement::Line {
        p1: zero_l,
        p2: zero_r,
        stroke: Stroke::dashed(Color::from_hex("#888888"), 0.6, 4.0, 3.0),
    });

    for (&(x, r), &(_, frac)) in top_pts.iter().zip(eng.nacelle_profile.iter()) {
        let p = axes.map_point(x, r);
        scene.add(SceneElement::Circle {
            center: p,
            radius: 3.0,
            fill: Some(Fill::new(Color::from_hex("tab:blue"))),
            stroke: None,
        });
        scene.add(SceneElement::Text {
            text: format!("({x:.1}, {frac:.2})"),
            pos: [p[0], p[1] - 6.0],
            font_size: 7.0,
            color: Color::from_hex(pal.title),
            align: TextAlign::Center,
            baseline: TextBaseline::Bottom,
            angle_deg: 0.0,
            bold: false,
        });
    }
    for &(x, r) in &bot_pts {
        let p = axes.map_point(x, r);
        scene.add(SceneElement::Circle {
            center: p,
            radius: 3.0,
            fill: Some(Fill::new(Color::from_hex("tab:blue"))),
            stroke: None,
        });
    }

    scene.add(SceneElement::Text {
        text: "x-station [m]".to_owned(),
        pos: [left + plot_w * 0.5, top + plot_h + 16.0],
        font_size: 8.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
    scene.add(SceneElement::Text {
        text: "radius [m]".to_owned(),
        pos: [left - 28.0, top + plot_h * 0.5],
        font_size: 8.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: -90.0,
        bold: false,
    });
}

/// Bold panel-title text at a fixed vertical position, matching the
/// convention `figure_threeview` already uses for per-axes titles (this
/// scene primitive set has no per-`Axes2D` title, only the whole-figure
/// `Scene::title`).
fn panel_title(scene: &mut Scene, pal: &Palette, text: &str, x: f64) {
    scene.add(SceneElement::Text {
        text: text.to_owned(),
        pos: [x, 40.0],
        font_size: 11.5,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: true,
    });
}

/// Centered two-line infeasibility message, matching
/// `f"Cycle infeasible at this design point:\n{out.infeasibility_reason}"`.
fn push_infeasible_text(scene: &mut Scene, reason: &str, cx: f64, cy: f64) {
    scene.add(SceneElement::Text {
        text: "Cycle infeasible at this design point:".to_owned(),
        pos: [cx, cy - 8.0],
        font_size: 11.0,
        color: Color::from_hex("#c0392b"),
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: false,
    });
    scene.add(SceneElement::Text {
        text: reason.to_owned(),
        pos: [cx, cy + 8.0],
        font_size: 11.0,
        color: Color::from_hex("#c0392b"),
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
}

// A test asserts on values it constructed or read off a fixed config here
// directly, so a failed unwrap or expect is the assertion failing, not a
// library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::svg::render_svg;

    #[test]
    fn the_default_config_cycle_is_feasible_and_renders_all_eight_bars() {
        let config = AlasConfig::default();
        let scene = figure_propulsion_cycle_summary(&config, None);
        let rects = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Rect { .. }))
            .count();
        // The scene background is also a rectangle.
        assert_eq!(rects, 9, "one background plus one bar per station");
        let svg = render_svg(&scene);
        assert!(svg.contains("<svg"));
    }

    #[test]
    fn station_labels_are_angled_below_the_bars() {
        let scene = figure_propulsion_cycle_summary(&AlasConfig::default(), Some("dark"));
        let labels: Vec<f64> = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text {
                    text, angle_deg, ..
                } if STATION_LABELS.contains(&text.as_str()) => Some(*angle_deg),
                _ => None,
            })
            .collect();
        assert_eq!(labels.len(), STATION_LABELS.len());
        assert!(labels.iter().all(|angle| (*angle + 38.0).abs() < 1e-12));
    }

    #[test]
    fn an_infeasible_cycle_renders_the_reason_and_no_bars() {
        let mut config = AlasConfig::default();
        // A turbine inlet at or below the compressor discharge temperature
        // cannot combust; this is the same infeasibility branch
        // `compute_turbofan_cycle`'s own unit tests exercise.
        config
            .geometry
            .engine
            .turbofan
            .as_mut()
            .unwrap()
            .turbine_inlet_temp_k = 100.0;
        let scene = figure_propulsion_cycle_summary(&config, None);
        assert!(!scene
            .elements
            .iter()
            .any(|e| matches!(e, SceneElement::Rect { .. })));
        let svg = render_svg(&scene);
        assert!(svg.contains("infeasible"));
    }

    #[test]
    fn the_text_panel_carries_the_real_specific_thrust_and_tsfc() {
        let config = AlasConfig::default();
        let out = compute_turbofan_cycle(&design_point(&config), &config.propulsion_cycle);
        assert!(out.cycle_feasible);
        let lines = propulsion_cycle_summary_lines(&config, &out, true);
        let sfn_line = lines
            .iter()
            .find(|l| l.contains("Specific thrust"))
            .expect("specific thrust line present");
        assert!(sfn_line.contains(&format!("{:.1}", out.specific_thrust_ms)));
        let tsfc_line = lines
            .iter()
            .find(|l| l.contains("TSFC (computed)"))
            .expect("tsfc line present");
        assert!(tsfc_line.contains(&format!("{:.2}", out.tsfc_mg_ns)));
    }

    #[test]
    fn compact_verbose_false_drops_only_the_fuel_air_ratio_line() {
        let config = AlasConfig::default();
        let out = compute_turbofan_cycle(&design_point(&config), &config.propulsion_cycle);
        let verbose = propulsion_cycle_summary_lines(&config, &out, true);
        let compact = propulsion_cycle_summary_lines(&config, &out, false);
        assert_eq!(verbose.len(), compact.len() + 1);
        assert!(verbose.iter().any(|l| l.contains("Fuel-air ratio")));
        assert!(!compact.iter().any(|l| l.contains("Fuel-air ratio")));
        // The efficiency lines are unconditional in both, per the upstream
        // function body (not its docstring) -- see this module's own note.
        for eff in [
            "Thermal efficiency",
            "Propulsive efficiency",
            "Overall efficiency",
        ] {
            assert!(compact.iter().any(|l| l.contains(eff)));
        }
    }

    #[test]
    fn the_engine_designer_preview_draws_one_marker_per_nacelle_control_point() {
        let config = AlasConfig::default();
        let scene = figure_engine_designer_preview(&config, Some("dark"));
        let n_points = config.geometry.engine.nacelle_profile.len();
        let circles = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Circle { .. }))
            .count();
        // One circle per point on the top edge and one on the (identical
        // radius) bottom edge.
        assert_eq!(circles, 2 * n_points);
    }

    #[test]
    fn engine_designer_preview_keeps_cycle_summary_text_out_of_the_editor_canvas() {
        let scene = figure_engine_designer_preview(&AlasConfig::default(), Some("dark"));
        let texts: Vec<&str> = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(!texts
            .iter()
            .any(|text| text.contains("Cruise Design-Point")));
        assert!(!texts.iter().any(|text| text.starts_with("TSFC")));
    }

    #[test]
    fn an_empty_nacelle_profile_renders_a_placeholder_and_no_markers() {
        let mut config = AlasConfig::default();
        config.geometry.engine.nacelle_profile.clear();
        let scene = figure_engine_designer_preview(&config, None);
        assert!(!scene
            .elements
            .iter()
            .any(|e| matches!(e, SceneElement::Circle { .. })));
        let svg = render_svg(&scene);
        assert!(svg.contains("No nacelle profile defined"));
    }

    #[test]
    fn nan_mass_flow_anchor_reports_thrust_as_not_available() {
        // anchor_mass_flow_kg_s evaluates a static (M0=0, sea level) cycle at
        // the engine's own TIT; driving that down below the static
        // compressor-discharge temperature makes the static cycle itself
        // infeasible, so the anchor returns NaN.
        let mut config = AlasConfig::default();
        config
            .geometry
            .engine
            .turbofan
            .as_mut()
            .unwrap()
            .turbine_inlet_temp_k = 10.0;
        let spec = config.geometry.engine.turbofan.as_ref().unwrap();
        let (mdot, _) = anchor_mass_flow_kg_s(
            spec.rated_thrust_kn,
            spec.overall_pressure_ratio,
            spec.fan_pressure_ratio,
            spec.bypass_ratio,
            spec.turbine_inlet_temp_k,
            &config.propulsion_cycle,
        );
        assert!(mdot.is_nan());
        let dummy_out = TurbofanCycleResult {
            cycle_feasible: true,
            infeasibility_reason: String::new(),
            specific_thrust_ms: 300.0,
            tsfc_mg_ns: 15.0,
            fuel_air_ratio: 0.02,
            thermal_efficiency: 0.5,
            propulsive_efficiency: 0.6,
            overall_efficiency: 0.3,
            temperature_t0_k: 300.0,
            temperature_t13_k: 350.0,
            temperature_t25_k: 400.0,
            temperature_t3_k: 700.0,
            temperature_t4_k: 1600.0,
            temperature_t45_k: 1300.0,
            temperature_t5_k: 900.0,
            temperature_t6_k: 900.0,
            exit_velocity_core_ms: 500.0,
            exit_velocity_fan_ms: 300.0,
        };
        let lines = propulsion_cycle_summary_lines(&config, &dummy_out, true);
        assert!(lines
            .iter()
            .any(|l| l.contains("Per-engine thrust, this cruise pt : n/a")));
        // The "n/a" branch also leaves the total-installed-thrust line blank.
        assert!(lines.iter().any(|l| l.is_empty()));
    }
}
