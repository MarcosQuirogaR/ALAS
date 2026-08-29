// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py
// (figure_propulsion_carpet_plot, figure_propulsion_efficiency_decomposition,
// figure_propulsion_bpr_sensitivity)
// Reference: alas @ rust-port-baseline.

//! Parametric trade-space figures: the OPR x TIT carpet plot, efficiency
//! decomposition vs OPR, and specific-thrust/TSFC sensitivity to bypass
//! ratio (dual-axis).

use super::support::axis_labels;
use super::{design_point, linspace};
use crate::chart_kit::{draw_legend, draw_title, LegendMarker};
use crate::colormap::Colormap;
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;
use alas_config::AlasConfig;
use alas_prop::cycle::compute_turbofan_cycle;
use alas_prop::cycle::sweeps::{
    compute_bpr_sensitivity, compute_carpet_plot, compute_efficiency_decomposition,
};

/// `matplotlib.pyplot.get_cmap(name)(np.linspace(0.1, 0.9, n))`: `n`
/// evenly-spaced colors from a named colormap, matching `_plt_cmap`'s own
/// `linspace(0.1, 0.9, ...)` sampling window (avoids each map's very
/// dark/very light extremes, which read poorly as line colors).
fn plt_cmap_sample(cmap: Colormap, index: usize, n: usize) -> Color {
    let denom = (n.max(2) - 1) as f64;
    let t = 0.1 + 0.8 * (index as f64) / denom;
    cmap.sample(t)
}

/// Carpet plot: specific thrust vs TSFC as overall pressure ratio and turbine
/// inlet temperature are swept around the current engine's design point
/// (BPR/FPR and flight condition held fixed at their current values).
pub fn figure_propulsion_carpet_plot(config: &AlasConfig, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(700.0, 500.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some(format!(
        "On-Design Carpet Plot (BPR={:.1}, FPR={:.2})",
        config.geometry.engine.bypass_ratio, config.geometry.engine.fan_pressure_ratio
    ));
    let title = scene.title.clone().unwrap_or_default();
    draw_title(&mut scene, &title, pal);
    scene.suppress_derived_title();

    let eng = &config.geometry.engine;
    let cyc_cfg = &config.propulsion_cycle;
    let req = &config.requirements;

    let pi_c_vec = linspace(15.0, 70.0, 12);
    let tit_vec = linspace(1300.0, 2000.0, 8);
    let carpet = compute_carpet_plot(
        &pi_c_vec,
        &tit_vec,
        req.cruise_mach,
        req.cruise_altitude_m,
        eng.fan_pressure_ratio,
        eng.bypass_ratio,
        cyc_cfg,
    );
    let design_out = compute_turbofan_cycle(&design_point(config), cyc_cfg);

    let mut sfn_min = f64::INFINITY;
    let mut sfn_max = f64::NEG_INFINITY;
    let mut tsfc_min = f64::INFINITY;
    let mut tsfc_max = f64::NEG_INFINITY;
    for i in 0..tit_vec.len() {
        for j in 0..pi_c_vec.len() {
            if carpet.feasible_mask[i][j] {
                sfn_min = sfn_min.min(carpet.specific_thrust_ms[i][j]);
                sfn_max = sfn_max.max(carpet.specific_thrust_ms[i][j]);
                tsfc_min = tsfc_min.min(carpet.tsfc_mg_ns[i][j]);
                tsfc_max = tsfc_max.max(carpet.tsfc_mg_ns[i][j]);
            }
        }
    }
    if design_out.cycle_feasible {
        sfn_min = sfn_min.min(design_out.specific_thrust_ms);
        sfn_max = sfn_max.max(design_out.specific_thrust_ms);
        tsfc_min = tsfc_min.min(design_out.tsfc_mg_ns);
        tsfc_max = tsfc_max.max(design_out.tsfc_mg_ns);
    }
    if !sfn_min.is_finite() {
        let axes = Axes2D::new((70.0, 60.0, 580.0, 380.0), (0.0, 1.0), (0.0, 1.0));
        axes.draw_frame(&mut scene, pal);
        return scene;
    }

    let sfn_pad = (sfn_max - sfn_min).max(1.0) * 0.08;
    let tsfc_pad = (tsfc_max - tsfc_min).max(0.1) * 0.08;
    let axes = Axes2D::new(
        (70.0, 60.0, 580.0, 380.0),
        (sfn_min - sfn_pad, sfn_max + sfn_pad),
        (tsfc_min - tsfc_pad, tsfc_max + tsfc_pad),
    );
    axes.draw_frame(&mut scene, pal);

    // Family of curves at fixed TIT, across the OPR sweep.
    for (i, mask_row) in carpet.feasible_mask.iter().enumerate() {
        if mask_row.iter().filter(|&&ok| ok).count() < 2 {
            continue;
        }
        let pts: Vec<(f64, f64)> = (0..pi_c_vec.len())
            .filter(|&j| mask_row[j])
            .map(|j| (carpet.specific_thrust_ms[i][j], carpet.tsfc_mg_ns[i][j]))
            .collect();
        let color = plt_cmap_sample(Colormap::Plasma, i, tit_vec.len());
        axes.add_line_series(&mut scene, &pts, Stroke::new(color, 1.4));
    }

    // Family of curves at fixed OPR, across the TIT sweep.
    for j in 0..pi_c_vec.len() {
        let col_feasible: Vec<bool> = (0..tit_vec.len())
            .map(|i| carpet.feasible_mask[i][j])
            .collect();
        if col_feasible.iter().filter(|&&ok| ok).count() < 2 {
            continue;
        }
        let pts: Vec<(f64, f64)> = (0..tit_vec.len())
            .filter(|&i| col_feasible[i])
            .map(|i| (carpet.specific_thrust_ms[i][j], carpet.tsfc_mg_ns[i][j]))
            .collect();
        let color = plt_cmap_sample(Colormap::Viridis, j, pi_c_vec.len());
        axes.add_line_series(&mut scene, &pts, Stroke::dashed(color, 0.9, 4.0, 3.0));
    }

    if design_out.cycle_feasible {
        let p = axes.map_point(design_out.specific_thrust_ms, design_out.tsfc_mg_ns);
        scene.add(SceneElement::Circle {
            center: p,
            radius: 8.0,
            fill: Some(Fill::new(Color::from_hex("#f1c40f"))),
            stroke: Some(Stroke::new(Color::rgb(0, 0, 0), 1.0)),
        });
    }

    let mut legend_entries = Vec::new();
    for &i in &[0usize, 2, 4, 6] {
        let color = plt_cmap_sample(Colormap::Plasma, i, tit_vec.len());
        legend_entries.push((
            format!("T4t = {:.0} K", tit_vec[i]),
            LegendMarker::Line(Stroke::new(color, 1.4)),
        ));
    }
    for &j in &[0usize, 3, 6, 9] {
        let color = plt_cmap_sample(Colormap::Viridis, j, pi_c_vec.len());
        legend_entries.push((
            format!("OPR = {:.0}", pi_c_vec[j]),
            LegendMarker::Line(Stroke::dashed(color, 0.9, 4.0, 3.0)),
        ));
    }
    if design_out.cycle_feasible {
        legend_entries.push((
            format!(
                "Design point (SFn={:.0} m/s)",
                design_out.specific_thrust_ms
            ),
            LegendMarker::Circle(Color::from_hex("#f1c40f")),
        ));
    }
    draw_legend(&mut scene, [82.0, 72.0], &legend_entries, pal, 7.2);

    axis_labels(
        &mut scene,
        pal,
        (70.0, 60.0, 580.0, 380.0),
        "Specific thrust  SFn = F / \u{1e41}  [m/s]",
        "TSFC  [mg/(N\u{00b7}s)]",
    );

    scene
}

/// Thermal / propulsive / overall efficiency vs overall pressure ratio, at
/// the current engine's BPR/FPR/TIT and flight condition.
pub fn figure_propulsion_efficiency_decomposition(
    config: &AlasConfig,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(700.0, 420.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some(format!(
        "Efficiency Decomposition vs OPR (BPR={:.1}, TIT={:.0} K)",
        config.geometry.engine.bypass_ratio, config.geometry.engine.turbine_inlet_temp_k
    ));
    let title = scene.title.clone().unwrap_or_default();
    draw_title(&mut scene, &title, pal);
    scene.suppress_derived_title();

    let eng = &config.geometry.engine;
    let cyc_cfg = &config.propulsion_cycle;
    let req = &config.requirements;

    let pi_c_lo = 15.0;
    let pi_c_hi = 70.0;
    let pi_c_vec = linspace(pi_c_lo, pi_c_hi, 60);
    let dec = compute_efficiency_decomposition(
        &pi_c_vec,
        eng.turbine_inlet_temp_k,
        eng.bypass_ratio,
        eng.fan_pressure_ratio,
        req.cruise_mach,
        req.cruise_altitude_m,
        cyc_cfg,
    );

    let rect = (70.0, 50.0, 590.0, 320.0);
    let axes = Axes2D::new(rect, (pi_c_lo, pi_c_hi), (0.0, 1.0));
    axes.draw_frame(&mut scene, pal);

    let feasible_series = |values: &[f64]| -> Vec<(f64, f64)> {
        pi_c_vec
            .iter()
            .zip(values)
            .zip(&dec.feasible_mask)
            .filter_map(|((&x, &y), &ok)| ok.then_some((x, y)))
            .collect()
    };

    let color_th = Color::from_hex("#27ae60");
    let color_p = Color::from_hex("#2980b9");
    let color_o = Color::from_hex("#e67e22");
    axes.add_line_series(
        &mut scene,
        &feasible_series(&dec.thermal_efficiency),
        Stroke::new(color_th, 2.0),
    );
    axes.add_line_series(
        &mut scene,
        &feasible_series(&dec.propulsive_efficiency),
        Stroke::new(color_p, 2.0),
    );
    axes.add_line_series(
        &mut scene,
        &feasible_series(&dec.overall_efficiency),
        Stroke::dashed(color_o, 2.2, 6.0, 4.0),
    );

    let opr = eng.overall_pressure_ratio;
    let mut legend_entries = vec![
        (
            "\u{03b7}\u{209c}\u{2095} (thermal)".to_owned(),
            LegendMarker::Line(Stroke::new(color_th, 2.0)),
        ),
        (
            "\u{03b7}\u{209a} (propulsive)".to_owned(),
            LegendMarker::Line(Stroke::new(color_p, 2.0)),
        ),
        (
            "\u{03b7}\u{2092} = \u{03b7}\u{209c}\u{2095} \u{00b7} \u{03b7}\u{209a} (overall)"
                .to_owned(),
            LegendMarker::Line(Stroke::new(color_o, 2.2)),
        ),
    ];
    if !opr.is_nan() {
        let top = axes.map_point(opr, 1.0);
        let bot = axes.map_point(opr, 0.0);
        scene.add(SceneElement::Line {
            p1: top,
            p2: bot,
            stroke: Stroke::dashed(Color::from_hex(pal.title), 1.3, 2.0, 2.0),
        });
        legend_entries.push((
            format!("{} OPR = {opr:.0}", eng.engine_name),
            LegendMarker::Line(Stroke::dashed(Color::from_hex(pal.title), 1.3, 2.0, 2.0)),
        ));
    }
    draw_legend(&mut scene, [82.0, 78.0], &legend_entries, pal, 8.5);

    axis_labels(
        &mut scene,
        pal,
        rect,
        "Overall (core) pressure ratio  OPR  [-]",
        "Efficiency  [-]",
    );

    scene
}

/// Specific thrust and TSFC vs bypass ratio, at the current engine's
/// OPR/FPR/TIT and flight condition. Dual-axis: two [`Axes2D`] share the same
/// pixel rect and X range but different Y ranges (specific thrust on the
/// left, TSFC on the right), following the pattern `ax1.twinx()` has no
/// direct equivalent for here.
pub fn figure_propulsion_bpr_sensitivity(config: &AlasConfig, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(700.0, 420.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some(format!(
        "Bypass-Ratio Sensitivity (OPR={:.0}, TIT={:.0} K)",
        config.geometry.engine.overall_pressure_ratio, config.geometry.engine.turbine_inlet_temp_k
    ));
    let title = scene.title.clone().unwrap_or_default();
    draw_title(&mut scene, &title, pal);
    scene.suppress_derived_title();

    let eng = &config.geometry.engine;
    let cyc_cfg = &config.propulsion_cycle;
    let req = &config.requirements;

    let bpr_lo = (eng.bypass_ratio * 0.3).max(1.0);
    let bpr_hi = eng.bypass_ratio * 1.8 + 1.0;
    let bpr_vec = linspace(bpr_lo, bpr_hi, 40);
    let sens = compute_bpr_sensitivity(
        &bpr_vec,
        eng.overall_pressure_ratio,
        eng.turbine_inlet_temp_k,
        eng.fan_pressure_ratio,
        req.cruise_mach,
        req.cruise_altitude_m,
        cyc_cfg,
    );
    let design_out = compute_turbofan_cycle(&design_point(config), cyc_cfg);

    let mut sfn_min = f64::INFINITY;
    let mut sfn_max = f64::NEG_INFINITY;
    let mut tsfc_min = f64::INFINITY;
    let mut tsfc_max = f64::NEG_INFINITY;
    for (i, &ok) in sens.feasible_mask.iter().enumerate() {
        if ok {
            sfn_min = sfn_min.min(sens.specific_thrust_ms[i]);
            sfn_max = sfn_max.max(sens.specific_thrust_ms[i]);
            tsfc_min = tsfc_min.min(sens.tsfc_mg_ns[i]);
            tsfc_max = tsfc_max.max(sens.tsfc_mg_ns[i]);
        }
    }
    if design_out.cycle_feasible {
        sfn_min = sfn_min.min(design_out.specific_thrust_ms);
        sfn_max = sfn_max.max(design_out.specific_thrust_ms);
        tsfc_min = tsfc_min.min(design_out.tsfc_mg_ns);
        tsfc_max = tsfc_max.max(design_out.tsfc_mg_ns);
    }
    let rect = (70.0, 50.0, 540.0, 320.0);
    if !sfn_min.is_finite() {
        let axes = Axes2D::new(rect, (bpr_lo, bpr_hi), (0.0, 1.0));
        axes.draw_frame(&mut scene, pal);
        return scene;
    }

    let color_sfn = Color::from_hex("#2980b9");
    let color_tsfc = Color::from_hex("#c0392b");
    let sfn_pad = (sfn_max - sfn_min).max(1.0) * 0.1;
    let tsfc_pad = (tsfc_max - tsfc_min).max(0.1) * 0.1;
    let sfn_lo = sfn_min - sfn_pad;
    let sfn_hi = sfn_max + sfn_pad;
    let tsfc_lo = tsfc_min - tsfc_pad;
    let tsfc_hi = tsfc_max + tsfc_pad;

    let axes_sfn = Axes2D::new(rect, (bpr_lo, bpr_hi), (sfn_lo, sfn_hi));
    let axes_tsfc = Axes2D::new(rect, (bpr_lo, bpr_hi), (tsfc_lo, tsfc_hi));
    axes_sfn.draw_frame(&mut scene, pal);

    let sfn_pts: Vec<(f64, f64)> = bpr_vec
        .iter()
        .zip(&sens.specific_thrust_ms)
        .zip(&sens.feasible_mask)
        .filter_map(|((&x, &y), &ok)| ok.then_some((x, y)))
        .collect();
    axes_sfn.add_line_series(&mut scene, &sfn_pts, Stroke::new(color_sfn, 2.0));

    let tsfc_pts: Vec<(f64, f64)> = bpr_vec
        .iter()
        .zip(&sens.tsfc_mg_ns)
        .zip(&sens.feasible_mask)
        .filter_map(|((&x, &y), &ok)| ok.then_some((x, y)))
        .collect();
    axes_tsfc.add_line_series(
        &mut scene,
        &tsfc_pts,
        Stroke::dashed(color_tsfc, 2.0, 6.0, 4.0),
    );

    if design_out.cycle_feasible {
        let p1 = axes_sfn.map_point(eng.bypass_ratio, design_out.specific_thrust_ms);
        scene.add(SceneElement::Circle {
            center: p1,
            radius: 5.0,
            fill: Some(Fill::new(color_sfn)),
            stroke: Some(Stroke::new(Color::rgb(0, 0, 0), 0.6)),
        });
        let p2 = axes_tsfc.map_point(eng.bypass_ratio, design_out.tsfc_mg_ns);
        scene.add(SceneElement::Rect {
            x: p2[0] - 4.0,
            y: p2[1] - 4.0,
            width: 8.0,
            height: 8.0,
            rx: 1.0,
            fill: Some(Fill::new(color_tsfc)),
            stroke: Some(Stroke::new(Color::rgb(0, 0, 0), 0.6)),
        });
        let vtop = axes_sfn.map_point(eng.bypass_ratio, sfn_hi);
        let vbot = axes_sfn.map_point(eng.bypass_ratio, sfn_lo);
        scene.add(SceneElement::Line {
            p1: vtop,
            p2: vbot,
            stroke: Stroke::dashed(Color::rgb(128, 128, 128), 1.3, 3.0, 3.0),
        });
    }

    draw_legend(
        &mut scene,
        [rect.0 + 345.0, rect.1 + 102.0],
        &[
            (
                "SFn [m/s]".to_owned(),
                LegendMarker::Line(Stroke::new(color_sfn, 2.0)),
            ),
            (
                "TSFC [mg/(N\u{00b7}s)]".to_owned(),
                LegendMarker::Line(Stroke::dashed(color_tsfc, 2.0, 6.0, 4.0)),
            ),
        ],
        pal,
        9.0,
    );

    // Right-axis (TSFC) tick labels, placed and colored manually per this
    // family's dual-axis convention -- see the module doc.
    let (rx, ry, rw, rh) = rect;
    for frac in [0.0, 0.5, 1.0] {
        let val = tsfc_lo + frac * (tsfc_hi - tsfc_lo);
        let py = ry + rh - frac * rh;
        scene.add(SceneElement::Text {
            text: format!("{val:.1}"),
            pos: [rx + rw + 6.0, py],
            font_size: 8.0,
            color: color_tsfc,
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }
    for frac in [0.0, 0.5, 1.0] {
        let val = sfn_lo + frac * (sfn_hi - sfn_lo);
        let py = ry + rh - frac * rh;
        scene.add(SceneElement::Text {
            text: format!("{val:.0}"),
            pos: [rx - 6.0, py],
            font_size: 8.0,
            color: color_sfn,
            align: TextAlign::Right,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }

    scene.add(SceneElement::Text {
        text: "Bypass ratio  BPR  [-]".to_owned(),
        pos: [rx + rw * 0.5, ry + rh + 24.0],
        font_size: 9.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
    scene.add(SceneElement::Text {
        text: "Specific thrust  SFn  [m/s]".to_owned(),
        pos: [rx - 45.0, ry + rh * 0.5],
        font_size: 9.0,
        color: color_sfn,
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: -90.0,
        bold: false,
    });
    scene.add(SceneElement::Text {
        text: "TSFC  [mg/(N\u{00b7}s)]".to_owned(),
        pos: [rx + rw + 45.0, ry + rh * 0.5],
        font_size: 9.0,
        color: color_tsfc,
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: -90.0,
        bold: false,
    });

    scene
}

// A test asserts on values it constructed or read off a fixed config here
// directly, so a failed unwrap or expect is the assertion failing, not a
// library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_carpet_plot_draws_a_line_per_tit_and_a_dashed_line_per_opr_plus_the_design_star() {
        let config = AlasConfig::default();
        let scene = figure_propulsion_carpet_plot(&config, None);
        let polylines = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Polyline { .. }))
            .count();
        // 8 TIT rows + 12 OPR columns, all feasible over this sweep for the
        // default engine.
        assert_eq!(polylines, 8 + 12);
        assert!(scene
            .elements
            .iter()
            .any(|e| matches!(e, SceneElement::Circle { .. })));
    }

    #[test]
    fn the_efficiency_decomposition_plots_three_feasible_curves_and_a_reference_line() {
        let config = AlasConfig::default();
        let scene = figure_propulsion_efficiency_decomposition(&config, None);
        let polylines = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Polyline { .. }))
            .count();
        assert_eq!(polylines, 3, "thermal, propulsive, overall");
        assert!(scene
            .elements
            .iter()
            .any(|e| matches!(e, SceneElement::Line { .. })));
    }

    #[test]
    fn efficiency_decomposition_values_agree_with_the_sweep_function_directly() {
        let config = AlasConfig::default();
        let eng = &config.geometry.engine;
        let pi_c_vec = linspace(15.0, 70.0, 60);
        let dec = compute_efficiency_decomposition(
            &pi_c_vec,
            eng.turbine_inlet_temp_k,
            eng.bypass_ratio,
            eng.fan_pressure_ratio,
            config.requirements.cruise_mach,
            config.requirements.cruise_altitude_m,
            &config.propulsion_cycle,
        );
        assert!(dec.feasible_mask.iter().any(|&ok| ok));
        for &eta in &dec.overall_efficiency {
            assert!(eta.is_nan() || (0.0..=1.0).contains(&eta));
        }
    }

    #[test]
    fn bpr_sensitivity_draws_both_series_and_two_design_point_markers() {
        let config = AlasConfig::default();
        let scene = figure_propulsion_bpr_sensitivity(&config, None);
        let polylines = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Polyline { .. }))
            .count();
        assert_eq!(polylines, 2, "specific thrust and TSFC");
        assert!(scene
            .elements
            .iter()
            .any(|e| matches!(e, SceneElement::Circle { .. })));
        assert!(scene
            .elements
            .iter()
            .any(|e| matches!(e, SceneElement::Rect { .. })));
    }

    #[test]
    fn bpr_sensitivity_sweeps_around_the_engines_own_bypass_ratio() {
        let config = AlasConfig::default();
        let bpr = config.geometry.engine.bypass_ratio;
        let bpr_lo = (bpr * 0.3).max(1.0);
        let bpr_hi = bpr * 1.8 + 1.0;
        assert!(bpr_lo < bpr);
        assert!(bpr_hi > bpr);
    }
}
