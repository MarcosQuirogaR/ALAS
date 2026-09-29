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
    let pal = get_palette(theme);
    let mut scene = Scene::new(700.0, 420.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some(format!(
        "Bypass-Ratio Sensitivity (OPR={:.0}, TIT={:.0} K)",
        turbofan_spec(config).overall_pressure_ratio,
        turbofan_spec(config).turbine_inlet_temp_k
    ));
    let title = scene.title.clone().unwrap_or_default();
    draw_title(&mut scene, &title, pal);
    scene.suppress_derived_title();

    let eng = turbofan_spec(config);
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
    // family's dual-axis convention, see the module doc.
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
