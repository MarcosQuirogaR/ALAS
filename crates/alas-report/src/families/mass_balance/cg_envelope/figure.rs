// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py:figure_cg_envelope (L2354-2835)
// Reference: alas @ rust-port-baseline.

use super::super::{no_data_scene, with_alpha};
use super::helpers::{annotate, draw_curve_with_label, draw_trajectory, draw_vline, linspace};
use crate::chart_kit::{draw_horizontal_legend, draw_title, LegendMarker};
use crate::scene::{
    Axes2D, Color, Fill, Point2D, Scene, SceneElement, Stroke, TextAlign, TextBaseline,
};
use crate::theme::get_palette;
use alas_config::AlasConfig;
use alas_mass::breakdown::{FUEL, OEW_KEYS, PAYLOAD};
use alas_perf::landing_gear::size_landing_gear;
use alas_pipeline::full_analysis::AnalysisReport;
/// Generate a model-derived CG loading-state check figure.
///
/// The figure uses aggregate OEW, payload, and fuel centroids plus modeled
/// aerodynamic and gear limits. It is not an AFM/WBM operational envelope or
/// evidence of certified loading-order, fuel-sequence, or mission coverage.
pub fn figure_cg_envelope(
    report: &AnalysisReport,
    config: &AlasConfig,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(700.0, 620.0, Some(Color::from_hex(pal.bg)));
    let title = "Weight & Balance / Model CG Loading-State Check";
    scene.title = Some(title.to_owned());
    draw_title(&mut scene, title, pal);
    scene.suppress_derived_title();
    let masses = &report.component_masses;
    let coords = &report.mass_coordinates;
    let plane = &report.airplane;

    if masses.is_empty() || coords.is_empty() {
        return no_data_scene(scene, pal, "No mass / coordinate data available");
    }
    let Some(wing) = plane
        .wings
        .iter()
        .find(|w| w.name == "Main Wing")
        .or_else(|| plane.wings.first())
    else {
        return no_data_scene(scene, pal, "No mass / coordinate data available");
    };
    let Some(fus) = plane.fuselages.first() else {
        return no_data_scene(scene, pal, "No mass / coordinate data available");
    };
    if wing.xsecs.len() < 2 || fus.xsecs.is_empty() {
        return no_data_scene(scene, pal, "No mass / coordinate data available");
    }

    let mac = plane.c_ref;
    let x_wing_ac = wing.aerodynamic_center(0.25)[0];
    let x_mac_le = x_wing_ac - 0.25 * mac; // Leading edge of the MAC.
    let to_pct = |x_m: f64| ((x_m - x_mac_le) / mac.max(0.001)) * 100.0;
    // --- Advanced gear/limit settings from config --------------------------
    let mm = &config.mass_model;
    let nlg_x_frac = mm.nlg_x_fraction;
    let mlg_x_frac_mac = mm.mlg_x_fraction_mac;
    let pct_nlg_min = mm.pct_load_nlg_min;
    let mlw_frac = mm.mlw_fraction_mtow;

    // --- Fuselage for NLG/MLG wheel positioning ----------------------------
    let fus_start_x = fus.xsecs[0].xyz_c[0];
    let fus_end_x = fus.xsecs[fus.xsecs.len() - 1].xyz_c[0];
    let fus_len = fus_end_x - fus_start_x;

    let x_nlg = fus_start_x + fus_len * nlg_x_frac;
    let x_mlg = x_mac_le + mlg_x_frac_mac * mac;
    let wheelbase = x_mlg - x_nlg;

    // --- Component groups ----------------------------------------------------
    let get_mass = |k: &str| masses.get(k).copied().unwrap_or(0.0);
    let oew_mass: f64 = OEW_KEYS.iter().map(|&k| get_mass(k)).sum();
    let payload = get_mass(PAYLOAD);
    let fuel = get_mass(FUEL);
    let mtow_mass = oew_mass + payload + fuel.max(0.0);
    let mlw_mass = mtow_mass * mlw_frac;
    let mzfw_mass = oew_mass + payload;

    let cg_of_subset = |keys: &[&str]| -> f64 {
        let m_tot: f64 = keys.iter().map(|&k| get_mass(k).max(0.0)).sum();
        if m_tot <= 0.0 {
            return to_pct(plane.xyz_ref[0]);
        }
        let x_mom: f64 = keys
            .iter()
            .map(|&k| {
                let m = get_mass(k).max(0.0);
                let x = coords.get(k).map(|c| c[0]).unwrap_or(plane.xyz_ref[0]);
                m * x
            })
            .sum();
        to_pct(x_mom / m_tot)
    };
    let oew_cg_mac = cg_of_subset(&OEW_KEYS);
    let payload_cg_x = coords
        .get(PAYLOAD)
        .map(|c| c[0])
        .unwrap_or(plane.xyz_ref[0]);
    let fuel_cg_x = coords.get(FUEL).map(|c| c[0]).unwrap_or(plane.xyz_ref[0]);
    let oew_cg_x = x_mac_le + oew_cg_mac / 100.0 * mac;

    let composite_cg = |m_oew: f64,
                        x_oew: f64,
                        m_payload: f64,
                        x_payload: f64,
                        m_fuel: f64,
                        x_fuel: f64|
     -> (f64, f64) {
        let m_tot = (m_oew + m_payload + m_fuel).max(1.0);
        let x_cg = (m_oew * x_oew + m_payload * x_payload + m_fuel * x_fuel) / m_tot;
        (m_tot, to_pct(x_cg))
    };

    // Sequence: load payload progressively (0 -> 100%), then fuel (0 -> 100%).
    let fracs = linspace(0.0, 1.0, 15);
    let mut pts_weight_a = Vec::with_capacity(30);
    let mut pts_cg_a = Vec::with_capacity(30);
    for &frac in &fracs {
        let (w, cg) = composite_cg(
            oew_mass,
            oew_cg_x,
            frac * payload,
            payload_cg_x,
            0.0,
            fuel_cg_x,
        );
        pts_weight_a.push(w);
        pts_cg_a.push(cg);
    }
    for &frac in &fracs {
        let (w, cg) = composite_cg(
            oew_mass,
            oew_cg_x,
            payload,
            payload_cg_x,
            frac * fuel.max(0.0),
            fuel_cg_x,
        );
        pts_weight_a.push(w);
        pts_cg_a.push(cg);
    }

    // --- Aerodynamic limits --------------------------------------------------
    let sm_val = if report.static_margin.is_nan() {
        0.10
    } else {
        report.static_margin
    };
    let x_np = plane.xyz_ref[0] + sm_val * mac;
    let np_pct = to_pct(x_np);

    let target_sm = config.requirements.target_static_margin;
    let cg_range = config.requirements.cg_range_pct_mac;
    let aft_limit_mac = np_pct - target_sm * 100.0;
    let fwd_limit_mac = aft_limit_mac - cg_range;
    let tip_over_pct = to_pct(x_mlg);

    // Gear strength limits: the same wheel/tire-derived values the optimizer's
    // CG check enforces, computed from the aerodynamic limits above -- so this
    // plot shows exactly the boundary a design is actually held to. Python
    // wraps this in a `try/except`, falling back to `MassModelConfig`'s fixed
    // fractions on any failure; `size_landing_gear` here is infallible given
    // valid geometry, so the fallback branch is unreached and not translated.
    let fus_diam_raw = config.geometry.fuselage.diameter_m;
    let fus_diam = if fus_diam_raw > 0.0 {
        fus_diam_raw
    } else {
        4.0
    };
    let aero_fwd_lim_x = x_mac_le + fwd_limit_mac / 100.0 * mac;
    let aero_aft_lim_x = x_mac_le + aft_limit_mac / 100.0 * mac;
    let gear = size_landing_gear(
        mtow_mass,
        x_nlg,
        x_mlg,
        aero_fwd_lim_x,
        aero_aft_lim_x,
        fus_diam,
        fus_diam * 1.1,
        &config.landing_gear,
    );
    let pct_nlg_max = gear.pct_load_nlg_max;
    let pct_mlg_max = gear.pct_load_mlg_max;

    // --- Curves over the weight range -----------------------------------------
    let w_calc = linspace(oew_mass * 0.5, mtow_mass * 1.3, 200);
    let load_nlg_max = mtow_mass * pct_nlg_max;
    let load_mlg_max = mtow_mass * pct_mlg_max;
    let load_nlg_min = mtow_mass * pct_nlg_min;

    let c_nlg_str: Vec<f64> = w_calc
        .iter()
        .map(|&w| to_pct(x_mlg - (load_nlg_max * wheelbase / w)))
        .collect();
    let c_mlg_str: Vec<f64> = w_calc
        .iter()
        .map(|&w| to_pct(x_nlg + (load_mlg_max * wheelbase / w)))
        .collect();
    let c_nlg_min: Vec<f64> = w_calc
        .iter()
        .map(|&w| to_pct(x_mlg - (load_nlg_min * wheelbase / w)))
        .collect();

    // --- Model loading-state bounds ---------------------------------------------
    let w_ops = linspace(oew_mass, mtow_mass, 150);
    let op_nlg_str: Vec<f64> = w_ops
        .iter()
        .map(|&w| to_pct(x_mlg - (load_nlg_max * wheelbase / w)))
        .collect();
    let op_mlg_str: Vec<f64> = w_ops
        .iter()
        .map(|&w| to_pct(x_nlg + (load_mlg_max * wheelbase / w)))
        .collect();
    let op_nlg_min: Vec<f64> = w_ops
        .iter()
        .map(|&w| to_pct(x_mlg - (load_nlg_min * wheelbase / w)))
        .collect();

    let poly_fwd: Vec<f64> = op_nlg_str.iter().map(|&v| fwd_limit_mac.max(v)).collect();
    let poly_aft: Vec<f64> = (0..w_ops.len())
        .map(|i| aft_limit_mac.min(op_mlg_str[i].min(op_nlg_min[i])))
        .collect();

    // A strength boundary is useful only while it actually closes the
    // model loading-state check. Omitting inactive curves keeps their labels from
    // floating outside the plot and makes the visible constraints truthful.
    let nlg_strength_limits = op_nlg_str
        .iter()
        .zip(&poly_fwd)
        .any(|(&curve, &bound)| curve > fwd_limit_mac + 1e-9 && (curve - bound).abs() < 1e-9);
    let mlg_strength_limits = op_mlg_str
        .iter()
        .zip(&op_nlg_min)
        .any(|(&curve, &nose)| curve <= aft_limit_mac + 1e-9 && curve <= nose + 1e-9);
    let nose_load_limits = op_nlg_min
        .iter()
        .zip(&op_mlg_str)
        .any(|(&curve, &main)| curve <= aft_limit_mac + 1e-9 && curve <= main + 1e-9);

    // --- Dynamic viewport bounds -------------------------------------------
    let x_min_poly = poly_fwd.iter().cloned().fold(f64::INFINITY, f64::min);
    let x_max_poly = poly_aft.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let width = x_max_poly - x_min_poly;
    let view_min = x_min_poly.min(fwd_limit_mac) - width * 0.4;
    let view_max = x_max_poly.max(tip_over_pct).max(np_pct) + width * 0.4;

    let y_min = oew_mass * 0.7 / 1000.0;
    let y_max = mtow_mass * 1.25 / 1000.0;
    let axes = Axes2D::new(
        (75.0, 55.0, 570.0, 380.0),
        (view_min, view_max),
        (y_min, y_max),
    );
    axes.draw_frame_with_labels(&mut scene, pal, "CG position [% MAC]", "Mass [t]");

    // --- Weight thresholds -----------------------------------------------------
    let x_label = view_min + (view_max - view_min) * 0.02;
    for &(w, label, color) in &[
        (mtow_mass, "MTOW", "#e74c3c"),
        (mlw_mass, "MLW", "#9b59b6"),
        (mzfw_mass, "MZFW", "#3498db"),
        (oew_mass, "OEW", pal.accent),
    ] {
        let stroke = Stroke::dashed(with_alpha(Color::from_hex(color), 0.4), 1.0, 1.5, 3.0);
        axes.add_line_series(
            &mut scene,
            &[(view_min, w / 1000.0), (view_max, w / 1000.0)],
            stroke,
        );
        let p = axes.map_point(x_label, w / 1000.0);
        scene.add(SceneElement::Text {
            text: label.to_owned(),
            pos: [p[0], p[1] - 3.0],
            font_size: 9.0,
            color: Color::from_hex(color),
            align: TextAlign::Left,
            baseline: TextBaseline::Bottom,
            angle_deg: 0.0,
            bold: true,
        });
    }

    // --- NLG/MLG max-strength and min-nose-load curves. Dash-dot approximated
    // with a dash/gap ratio distinct from the plain-dashed curves below (see
    // module doc) --------------------------------------------------------------
    if nlg_strength_limits {
        draw_curve_with_label(
            &mut scene,
            &axes,
            &w_calc,
            &c_nlg_str,
            "#e74c3c",
            6.0,
            2.5,
            mtow_mass * 1.1,
            "",
            "#c0392b",
            -14.0,
        );
    }
    if mlg_strength_limits {
        draw_curve_with_label(
            &mut scene,
            &axes,
            &w_calc,
            &c_mlg_str,
            "#2980b9",
            6.0,
            2.5,
            mtow_mass * 0.95,
            "",
            "#2980b9",
            14.0,
        );
    }
    if nose_load_limits {
        draw_curve_with_label(
            &mut scene,
            &axes,
            &w_calc,
            &c_nlg_min,
            "#d35400",
            6.0,
            4.0,
            oew_mass * 1.1,
            "",
            "#d35400",
            16.0,
        );
    }

    // --- Vertical aerodynamic/gear limit lines ----------------------------
    draw_vline(
        &mut scene,
        &axes,
        fwd_limit_mac,
        pal.title,
        1.5,
        1.5,
        3.0,
        0.7,
        "  Fwd Aero Limit",
        0.0,
    );
    draw_vline(
        &mut scene,
        &axes,
        aft_limit_mac,
        "#f39c12",
        2.0,
        7.0,
        3.0,
        0.7,
        &format!("  Stability Limit (NP-{}%)", (target_sm * 100.0) as i64),
        16.0,
    );
    draw_vline(
        &mut scene,
        &axes,
        np_pct,
        "#3498db",
        1.5,
        6.0,
        2.5,
        0.5,
        "  Neutral Point (NP)",
        32.0,
    );
    draw_vline(
        &mut scene,
        &axes,
        tip_over_pct,
        "#c0392b",
        2.0,
        0.0,
        0.0,
        0.3,
        "  TIP-OVER (MLG)",
        48.0,
    );

    // --- Model loading-state fill and bold outline ---------------------------
    let mut poly_pts: Vec<Point2D> = Vec::with_capacity(w_ops.len() * 2);
    for i in 0..w_ops.len() {
        poly_pts.push(axes.map_point(
            poly_fwd[i].clamp(axes.x_min, axes.x_max),
            (w_ops[i] / 1000.0).clamp(axes.y_min, axes.y_max),
        ));
    }
    for i in (0..w_ops.len()).rev() {
        poly_pts.push(axes.map_point(
            poly_aft[i].clamp(axes.x_min, axes.x_max),
            (w_ops[i] / 1000.0).clamp(axes.y_min, axes.y_max),
        ));
    }
    scene.add(SceneElement::Polygon {
        points: poly_pts,
        fill: Some(Fill::new(with_alpha(Color::from_hex("#2ecc71"), 0.2))),
        stroke: None,
    });

    let outline_stroke = Stroke::new(Color::from_hex("#27ae60"), 3.0);
    let fwd_series: Vec<(f64, f64)> = (0..w_ops.len())
        .map(|i| (poly_fwd[i], w_ops[i] / 1000.0))
        .collect();
    let aft_series: Vec<(f64, f64)> = (0..w_ops.len())
        .map(|i| (poly_aft[i], w_ops[i] / 1000.0))
        .collect();
    axes.add_line_series(&mut scene, &fwd_series, outline_stroke.clone());
    axes.add_line_series(&mut scene, &aft_series, outline_stroke.clone());
    axes.add_line_series(
        &mut scene,
        &[
            (poly_fwd[0], oew_mass / 1000.0),
            (poly_aft[0], oew_mass / 1000.0),
        ],
        outline_stroke.clone(),
    );
    let last = w_ops.len() - 1;
    axes.add_line_series(
        &mut scene,
        &[
            (poly_fwd[last], mtow_mass / 1000.0),
            (poly_aft[last], mtow_mass / 1000.0),
        ],
        outline_stroke,
    );

    // --- Loading trajectories -------------------------------------------------
    let payload_stroke = Stroke::new(Color::from_hex(pal.accent), 2.0);
    let fuel_stroke = Stroke::new(Color::from_hex("#e67e22"), 2.0);
    draw_trajectory(
        &mut scene,
        &axes,
        &pts_cg_a[0..15],
        &pts_weight_a[0..15],
        payload_stroke,
    );
    draw_trajectory(
        &mut scene,
        &axes,
        &pts_cg_a[14..30],
        &pts_weight_a[14..30],
        fuel_stroke,
    );

    // --- Key-point annotations -------------------------------------------------
    let (_, cg_mzfw_mac) = composite_cg(oew_mass, oew_cg_x, payload, payload_cg_x, 0.0, fuel_cg_x);
    let (_, cg_mtow_mac) = composite_cg(oew_mass, oew_cg_x, payload, payload_cg_x, fuel, fuel_cg_x);
    annotate(
        &mut scene,
        &axes,
        oew_cg_mac,
        oew_mass / 1000.0,
        "OEW",
        pal.accent,
        -18.0,
        12.0,
    );
    annotate(
        &mut scene,
        &axes,
        cg_mzfw_mac,
        mzfw_mass / 1000.0,
        "MZFW",
        "#3498db",
        8.0,
        0.0,
    );
    annotate(
        &mut scene,
        &axes,
        cg_mtow_mac,
        mtow_mass / 1000.0,
        "MTOW",
        "#e74c3c",
        8.0,
        -10.0,
    );

    // The framed axes already own the labels; a second hand-positioned pair
    // occupies the same pixels once either string is localized.
    let mut legend_entries = vec![
        (
            "Model state limits".to_owned(),
            LegendMarker::Line(Stroke::new(Color::from_hex("#27ae60"), 3.0)),
        ),
        (
            "Payload loading".to_owned(),
            LegendMarker::Line(Stroke::new(Color::from_hex(pal.accent), 2.0)),
        ),
        (
            "Fuel loading".to_owned(),
            LegendMarker::Line(Stroke::new(Color::from_hex("#e67e22"), 2.0)),
        ),
    ];
    if nlg_strength_limits {
        legend_entries.push((
            "NLG max strength".to_owned(),
            LegendMarker::Line(Stroke::dashed(Color::from_hex("#e74c3c"), 1.5, 6.0, 2.5)),
        ));
    }
    if mlg_strength_limits {
        legend_entries.push((
            "MLG max strength".to_owned(),
            LegendMarker::Line(Stroke::dashed(Color::from_hex("#2980b9"), 1.5, 6.0, 2.5)),
        ));
    }
    if nose_load_limits {
        legend_entries.push((
            "Min nose load (steering)".to_owned(),
            LegendMarker::Line(Stroke::dashed(Color::from_hex("#d35400"), 1.5, 6.0, 4.0)),
        ));
    }
    draw_horizontal_legend(
        &mut scene,
        [axes.left, axes.top + axes.height + 52.0],
        &legend_entries,
        pal,
        8.0,
    );

    scene.add(SceneElement::Text {
        text: "MODEL-DERIVED CG CHECK ONLY; NOT AN AFM/WBM OPERATIONAL ENVELOPE".to_owned(),
        pos: [scene.width * 0.5, 607.0],
        font_size: 7.5,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: false,
    });

    scene
}
