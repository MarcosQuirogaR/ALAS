// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

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
    if let Some(scene) =
        super::super::technology::binding_error_scene(config, theme, (700.0, 500.0))
    {
        return scene;
    }
    if super::super::is_turboprop(config) {
        return super::super::technology::turboprop_power_speed_envelope(config, theme);
    }
    let eng = match super::super::turbofan_or_status(config, theme, (700.0, 500.0)) {
        Ok(eng) => eng,
        Err(scene) => return scene,
    };
    let pal = get_palette(theme);
    // Tall enough for the four-line masked-points footnote under the x label.
    let mut scene = Scene::new(700.0, 560.0, Some(Color::from_hex(pal.bg)));
    // The throttled cruise point, as the cycle summary shows it.
    let design = design_point(config, eng);
    scene.title = Some(format!(
        "On-Design Carpet Plot (BPR={:.1}, FPR={:.2})",
        eng.bypass_ratio, design.fan_pressure_ratio
    ));
    let title = scene.title.clone().unwrap_or_default();
    draw_title(&mut scene, &title, pal);
    scene.suppress_derived_title();

    let cyc_cfg = &config.propulsion_cycle;
    let req = &config.requirements;

    let pi_c_vec = linspace(15.0, 70.0, 12);
    let tit_vec = linspace(1300.0, 2000.0, 8);
    let carpet = compute_carpet_plot(
        &pi_c_vec,
        &tit_vec,
        req.cruise_mach,
        req.cruise_altitude_m,
        design.fan_pressure_ratio,
        eng.bypass_ratio,
        cyc_cfg,
    );
    let design_out = compute_turbofan_cycle(&design, cyc_cfg);

    // Mask the backward branch: along each fixed-T4t curve (OPR increasing), a
    // specific thrust more than 20 % below its running maximum is a collapsed cycle,
    // not a design option; that point and every later one on the curve are dropped.
    let (ok, masked_cells) = mask_backward_branch(&carpet);

    let mut sfn_min = f64::INFINITY;
    let mut sfn_max = f64::NEG_INFINITY;
    let mut tsfc_min = f64::INFINITY;
    let mut tsfc_max = f64::NEG_INFINITY;
    for (i, ok_row) in ok.iter().enumerate() {
        for (j, &cell) in ok_row.iter().enumerate() {
            if cell {
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
        // Axis rule: specific thrust is a non-negative magnitude with a
        // physical zero (no thrust), so its axis starts at 0 and the carpet
        // is read against it. TSFC stays autoscaled: zero fuel use is not a
        // reachable limit and would flatten the carpet into a sliver.
        (0.0, sfn_max + sfn_pad),
        (tsfc_min - tsfc_pad, tsfc_max + tsfc_pad),
    );
    axes.draw_frame(&mut scene, pal);

    let mut curves_px: Vec<Vec<[f64; 2]>> = Vec::new();
    // Family of curves at fixed TIT, across the OPR sweep.
    for (i, mask_row) in ok.iter().enumerate() {
        if mask_row.iter().filter(|&&ok| ok).count() < 2 {
            continue;
        }
        let pts: Vec<(f64, f64)> = (0..pi_c_vec.len())
            .filter(|&j| mask_row[j])
            .map(|j| (carpet.specific_thrust_ms[i][j], carpet.tsfc_mg_ns[i][j]))
            .collect();
        let color = plt_cmap_sample(Colormap::Plasma, i, tit_vec.len());
        curves_px.push(pts.iter().map(|&(x, y)| axes.map_point(x, y)).collect());
        axes.add_line_series(&mut scene, &pts, Stroke::new(color, 1.4));
    }

    // Family of curves at fixed OPR, across the TIT sweep.
    for j in 0..pi_c_vec.len() {
        let col_feasible: Vec<bool> = (0..tit_vec.len()).map(|i| ok[i][j]).collect();
        if col_feasible.iter().filter(|&&ok| ok).count() < 2 {
            continue;
        }
        let pts: Vec<(f64, f64)> = (0..tit_vec.len())
            .filter(|&i| col_feasible[i])
            .map(|i| (carpet.specific_thrust_ms[i][j], carpet.tsfc_mg_ns[i][j]))
            .collect();
        let color = plt_cmap_sample(Colormap::Viridis, j, pi_c_vec.len());
        curves_px.push(pts.iter().map(|&(x, y)| axes.map_point(x, y)).collect());
        axes.add_line_series(&mut scene, &pts, Stroke::dashed(color, 0.9, 4.0, 3.0));
    }

    // The masked points stay visible as faint hollow markers (those inside the
    // plotted range; the footnote counts the rest).
    let masked_xy: Vec<(f64, f64)> = masked_cells
        .iter()
        .map(|&(i, j)| (carpet.specific_thrust_ms[i][j], carpet.tsfc_mg_ns[i][j]))
        .collect();
    let inside = |&(x, y): &(f64, f64)| {
        x.is_finite()
            && y.is_finite()
            && x >= axes.x_min
            && x <= axes.x_max
            && y >= axes.y_min
            && y <= axes.y_max
    };
    let ink = Color::from_hex(pal.tick);
    for &(x, y) in masked_xy.iter().filter(|p| inside(p)) {
        scene.add(SceneElement::Circle {
            center: axes.map_point(x, y),
            radius: 2.6,
            fill: None,
            stroke: Some(Stroke::new(Color::rgba(ink.r, ink.g, ink.b, 110), 0.8)),
        });
    }
    let masked_outside = masked_xy.iter().filter(|p| !inside(p)).count();

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
    // The design point is always computed and plotted; when it lies outside the
    // swept OPR x T4t grid the legend says the carpet does not bracket it.
    let off_grid = design.overall_pressure_ratio < pi_c_vec[0]
        || design.overall_pressure_ratio > pi_c_vec[pi_c_vec.len() - 1]
        || design.turbine_inlet_temperature_k < tit_vec[0]
        || design.turbine_inlet_temperature_k > tit_vec[tit_vec.len() - 1];
    if design_out.cycle_feasible {
        legend_entries.push((
            format!(
                "Design point (SFn={:.0} m/s{})",
                design_out.specific_thrust_ms,
                if off_grid {
                    ", extrapolated: off the carpet grid"
                } else {
                    ""
                }
            ),
            LegendMarker::Circle(Color::from_hex("#f1c40f")),
        ));
    }
    let origin = super::super::support::least_cluttered_legend_origin(
        (70.0, 60.0, 580.0, 380.0),
        legend_entries.len(),
        7.2,
        if off_grid { 215.0 } else { 150.0 },
        &curves_px,
    );
    draw_legend(&mut scene, origin, &legend_entries, pal, 7.2);
    if !masked_cells.is_empty() {
        let (sfn_lo, sfn_hi) = masked_xy
            .iter()
            .map(|p| p.0)
            .filter(|v| v.is_finite())
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                (lo.min(v), hi.max(v))
            });
        let outside_sentence = if masked_outside > 0 {
            format!(" {masked_outside} of them fall outside the plotted range and are not drawn.")
        } else {
            String::new()
        };
        super::super::support::wrapped_footnote(
            &mut scene,
            pal,
            &format!(
                "{} grid point(s) masked (faint hollow markers): specific thrust fell more than 20 % below its running maximum along the OPR sweep (backward branch, not a valid operating point). SFn of the masked points: {sfn_lo:.0} to {sfn_hi:.0} m/s.{outside_sentence}",
                masked_cells.len()
            ),
            7.4,
            350.0,
            554.0,
            620.0,
        );
    }

    axis_labels(
        &mut scene,
        pal,
        (70.0, 60.0, 580.0, 380.0),
        "Specific thrust  SFn = F / \u{1e41}  [m/s]",
        "TSFC  [mg/(N\u{00b7}s)]",
    );

    scene
}

/// Feasible-and-not-collapsed mask of a carpet, and the (row, column) cells
/// the collapse rule removed. See the call site for the rule.
fn mask_backward_branch(
    carpet: &alas_prop::cycle::sweeps::CarpetPlotResult,
) -> (Vec<Vec<bool>>, Vec<(usize, usize)>) {
    let mut masked: Vec<(usize, usize)> = Vec::new();
    let ok = carpet
        .feasible_mask
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let mut running_max = f64::NEG_INFINITY;
            let mut collapsed = false;
            row.iter()
                .enumerate()
                .map(|(j, &feasible)| {
                    if !feasible {
                        return false;
                    }
                    let sfn = carpet.specific_thrust_ms[i][j];
                    running_max = running_max.max(sfn);
                    collapsed |= sfn < 0.8 * running_max;
                    if collapsed {
                        masked.push((i, j));
                    }
                    !collapsed
                })
                .collect()
        })
        .collect();
    (ok, masked)
}

/// Thermal / propulsive / overall efficiency vs overall pressure ratio, at
/// the current engine's BPR/FPR/TIT and flight condition.
pub fn figure_propulsion_efficiency_decomposition(
    config: &AlasConfig,
    theme: Option<&str>,
) -> Scene {
    if let Some(scene) =
        super::super::technology::binding_error_scene(config, theme, (700.0, 420.0))
    {
        return scene;
    }
    if super::super::is_turboprop(config) {
        return super::super::technology::turboprop_efficiency_scene(config, theme);
    }
    let eng = match super::super::turbofan_or_status(config, theme, (700.0, 420.0)) {
        Ok(eng) => eng,
        Err(scene) => return scene,
    };
    let pal = get_palette(theme);
    let mut scene = Scene::new(700.0, 420.0, Some(Color::from_hex(pal.bg)));
    // The cruise (throttled) temperature, as the cycle summary shows it.
    let design = design_point(config, eng);
    let cruise_tit_k = design.turbine_inlet_temperature_k;
    scene.title = Some(format!(
        "Efficiency Decomposition vs OPR (BPR={:.1}, TIT={cruise_tit_k:.0} K)",
        eng.bypass_ratio
    ));
    let title = scene.title.clone().unwrap_or_default();
    draw_title(&mut scene, &title, pal);
    scene.suppress_derived_title();

    let cyc_cfg = &config.propulsion_cycle;
    let req = &config.requirements;

    let pi_c_lo = 15.0;
    let pi_c_hi = 70.0;
    let pi_c_vec = linspace(pi_c_lo, pi_c_hi, 60);
    let dec = compute_efficiency_decomposition(
        &pi_c_vec,
        cruise_tit_k,
        eng.bypass_ratio,
        design.fan_pressure_ratio,
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

    let opr = design.overall_pressure_ratio;
    let mut legend_entries = vec![
        (
            "Thermal efficiency (\u{03b7}th)".to_owned(),
            LegendMarker::Line(Stroke::new(color_th, 2.0)),
        ),
        (
            "Propulsive efficiency (\u{03b7}p)".to_owned(),
            LegendMarker::Line(Stroke::new(color_p, 2.0)),
        ),
        (
            "Overall efficiency (\u{03b7}o = \u{03b7}th \u{00b7} \u{03b7}p)".to_owned(),
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
    let mut curves_px: Vec<Vec<[f64; 2]>> = [
        &dec.thermal_efficiency,
        &dec.propulsive_efficiency,
        &dec.overall_efficiency,
    ]
    .iter()
    .map(|values| {
        feasible_series(values)
            .iter()
            .map(|&(x, y)| axes.map_point(x, y))
            .collect()
    })
    .collect();
    if !opr.is_nan() {
        curves_px.push(vec![axes.map_point(opr, 1.0), axes.map_point(opr, 0.0)]);
    }
    let origin = super::super::support::least_cluttered_legend_origin(
        rect,
        legend_entries.len(),
        8.5,
        215.0,
        &curves_px,
    );
    draw_legend(&mut scene, origin, &legend_entries, pal, 8.5);

    axis_labels(
        &mut scene,
        pal,
        rect,
        "Overall (core) pressure ratio  OPR  [-]",
        "Efficiency  [-]",
    );

    scene
}
