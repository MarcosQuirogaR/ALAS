// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

/// Specific thrust and TSFC vs bypass ratio, at the current engine's
/// OPR/FPR/TIT and flight condition. Dual-axis: two [`Axes2D`] share the same
/// pixel rect and X range but different Y ranges (specific thrust on the
/// left, TSFC on the right), following the pattern `ax1.twinx()` has no
/// direct equivalent for here.
pub fn figure_propulsion_bpr_sensitivity(config: &AlasConfig, theme: Option<&str>) -> Scene {
    if let Some(scene) =
        super::super::technology::binding_error_scene(config, theme, (700.0, 500.0))
    {
        return scene;
    }
    if super::super::is_turboprop(config) {
        return super::super::technology::turboprop_rating_scene(config, theme);
    }
    let eng = match super::super::turbofan_or_status(config, theme, (700.0, 420.0)) {
        Ok(eng) => eng,
        Err(scene) => return scene,
    };
    let pal = get_palette(theme);
    let mut scene = Scene::new(700.0, 456.0, Some(Color::from_hex(pal.bg)));
    // The cruise (throttled) temperature, as the cycle summary shows it.
    let design = design_point(config, eng);
    let cruise_tit_k = design.turbine_inlet_temperature_k;
    scene.title = Some(format!(
        "Bypass-Ratio Sensitivity (OPR={:.0}, TIT={cruise_tit_k:.0} K)",
        design.overall_pressure_ratio
    ));
    let title = scene.title.clone().unwrap_or_default();
    draw_title(&mut scene, &title, pal);
    scene.suppress_derived_title();

    let cyc_cfg = &config.propulsion_cycle;
    let req = &config.requirements;

    let bpr_lo = (eng.bypass_ratio * 0.3).max(1.0);
    let bpr_hi = eng.bypass_ratio * 1.8 + 1.0;
    let bpr_vec = linspace(bpr_lo, bpr_hi, 40);
    let sens = compute_bpr_sensitivity(
        &bpr_vec,
        design.overall_pressure_ratio,
        cruise_tit_k,
        design.fan_pressure_ratio,
        req.cruise_mach,
        req.cruise_altitude_m,
        cyc_cfg,
    );
    let design_out = compute_turbofan_cycle(&design_point(config, eng), cyc_cfg);

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
    let tsfc_pts: Vec<(f64, f64)> = bpr_vec
        .iter()
        .zip(&sens.tsfc_mg_ns)
        .zip(&sens.feasible_mask)
        .filter_map(|((&x, &y), &ok)| ok.then_some((x, y)))
        .collect();
    // A corner in the specific-thrust curve is a solver branch or limit switch in the
    // cycle, and the TSFC hump that follows it is not a trend. The curves are drawn
    // solid up to the corner and dotted beyond it (never dropped), and the legend
    // and the footnote say so.
    let switch = super::bpr_helpers::branch_switch_index(&sfn_pts);
    let valid_len = switch.map_or(sfn_pts.len(), |k| k + 1);
    axes_sfn.add_line_series(
        &mut scene,
        &sfn_pts[..valid_len],
        Stroke::new(color_sfn, 2.0),
    );
    axes_tsfc.add_line_series(
        &mut scene,
        &tsfc_pts[..valid_len],
        Stroke::dashed(color_tsfc, 2.0, 6.0, 4.0),
    );
    let dotted = |c: Color| Stroke::dashed(Color::rgba(c.r, c.g, c.b, 120), 1.4, 1.5, 3.0);
    if switch.is_some() {
        axes_sfn.add_line_series(&mut scene, &sfn_pts[valid_len - 1..], dotted(color_sfn));
        axes_tsfc.add_line_series(&mut scene, &tsfc_pts[valid_len - 1..], dotted(color_tsfc));
    }
    let valid_sfn = &sfn_pts[..valid_len];
    let valid_tsfc = &tsfc_pts[..valid_len];

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

    let (rx, ry, rw, rh) = rect;
    let mut legend_rows = vec![
        (
            "SFn [m/s]".to_owned(),
            LegendMarker::Line(Stroke::new(color_sfn, 2.0)),
        ),
        (
            "TSFC [mg/(N\u{00b7}s)]".to_owned(),
            LegendMarker::Line(Stroke::dashed(color_tsfc, 2.0, 6.0, 4.0)),
        ),
    ];
    if switch.is_some() {
        legend_rows.push((
            "Past cycle limit".to_owned(),
            LegendMarker::Line(dotted(Color::from_hex(pal.tick))),
        ));
    }
    let curves_px: Vec<Vec<[f64; 2]>> = vec![
        sfn_pts
            .iter()
            .map(|&(x, y)| axes_sfn.map_point(x, y))
            .collect(),
        tsfc_pts
            .iter()
            .map(|&(x, y)| axes_tsfc.map_point(x, y))
            .collect(),
    ];
    let origin = super::super::support::least_cluttered_legend_origin(
        rect,
        legend_rows.len(),
        9.0,
        150.0,
        &curves_px,
    );
    draw_legend(&mut scene, origin, &legend_rows, pal, 9.0);

    // Right-axis (TSFC) ticks at round values, coloured like the curve. The left
    // axis keeps the frame's own ticks: a second set of labels at the range ends
    // collided with them.
    let (right_ticks, decimals) = super::bpr_helpers::nice_ticks(tsfc_lo, tsfc_hi, 5);
    for val in right_ticks {
        let py = ry + rh - (val - tsfc_lo) / (tsfc_hi - tsfc_lo) * rh;
        scene.add(SceneElement::Line {
            p1: [rx + rw, py],
            p2: [rx + rw + 4.0, py],
            stroke: Stroke::new(color_tsfc, 0.8),
        });
        scene.add(SceneElement::Text {
            text: format!("{val:.decimals$}"),
            pos: [rx + rw + 7.0, py],
            font_size: 8.0,
            color: color_tsfc,
            align: TextAlign::Left,
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

    // Footnotes: the cycle-limit flag, and the stated offset of the design markers
    // (the design cycle is solved on its own, not read off the 40-point sweep).
    let mut notes: Vec<String> = Vec::new();
    if let Some(k) = switch {
        notes.push(format!(
            "Dotted beyond BPR {:.1}: corner in SFn (cycle limit or solver branch switch), not a trend.",
            sfn_pts[k].0
        ));
    }
    if design_out.cycle_feasible {
        let off = |design: f64, curve: Option<f64>| {
            curve
                .map(|c| 100.0 * (design - c) / c)
                .filter(|v| v.is_finite())
        };
        let d_sfn = off(
            design_out.specific_thrust_ms,
            super::bpr_helpers::interpolate(valid_sfn, eng.bypass_ratio),
        );
        let d_tsfc = off(
            design_out.tsfc_mg_ns,
            super::bpr_helpers::interpolate(valid_tsfc, eng.bypass_ratio),
        );
        if let (Some(a), Some(b)) = (d_sfn, d_tsfc) {
            if a.abs().max(b.abs()) >= 0.5 {
                notes.push(format!(
                    "Design markers (cycle solved at the engine's own BPR) sit {a:+.1} % (SFn) and {b:+.1} % (TSFC) from the sweep curves at the same BPR."
                ));
            }
        }
    }
    if !notes.is_empty() {
        super::super::support::wrapped_footnote(
            &mut scene,
            pal,
            &notes.join(" "),
            7.4,
            350.0,
            450.0,
            620.0,
        );
    }

    scene
}
