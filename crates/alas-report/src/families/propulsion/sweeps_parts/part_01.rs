// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::support::axis_labels;
use super::{design_point, linspace, turbofan_spec};
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
    if let Some(scene) = super::technology::binding_error_scene(config, theme, (700.0, 500.0)) {
        return scene;
    }
    if super::is_turboprop(config) {
        return super::technology::turboprop_power_speed_envelope(config, theme);
    }
    let pal = get_palette(theme);
    let mut scene = Scene::new(700.0, 500.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some(format!(
        "On-Design Carpet Plot (BPR={:.1}, FPR={:.2})",
        turbofan_spec(config).bypass_ratio,
        turbofan_spec(config).fan_pressure_ratio
    ));
    let title = scene.title.clone().unwrap_or_default();
    draw_title(&mut scene, &title, pal);
    scene.suppress_derived_title();

    let eng = turbofan_spec(config);
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
    if let Some(scene) = super::technology::binding_error_scene(config, theme, (700.0, 420.0)) {
        return scene;
    }
    if super::is_turboprop(config) {
        return super::technology::turboprop_efficiency_scene(config, theme);
    }
    let pal = get_palette(theme);
    let mut scene = Scene::new(700.0, 420.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some(format!(
        "Efficiency Decomposition vs OPR (BPR={:.1}, TIT={:.0} K)",
        turbofan_spec(config).bypass_ratio,
        turbofan_spec(config).turbine_inlet_temp_k
    ));
    let title = scene.title.clone().unwrap_or_default();
    draw_title(&mut scene, &title, pal);
    scene.suppress_derived_title();

    let eng = turbofan_spec(config);
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
            format!("{} OPR = {opr:.0}", config.geometry.engine.engine_name),
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
