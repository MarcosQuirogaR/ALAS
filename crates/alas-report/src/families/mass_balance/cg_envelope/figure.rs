// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py:figure_cg_envelope (L2354-2835)
// Reference: alas @ rust-port-baseline.

use super::super::no_data_scene;
use super::helpers::linspace;
use super::render::{render, CgEnvelopeRenderData};
use crate::chart_kit::draw_title;
use crate::families::MAIN_GEAR_STATION_NOT_MEASURED;
use crate::scene::{Color, Scene};
use crate::theme::get_palette;
use alas_config::AlasConfig;
use alas_mass::breakdown::{FUEL, OEW_KEYS, PAYLOAD};
use alas_perf::landing_gear::size_landing_gear_with_group_stations;
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

    // --- Fuselage for NLG/MLG wheel positioning ----------------------------
    let fus_start_x = fus.xsecs[0].xyz_c[0];
    let fus_end_x = fus.xsecs[fus.xsecs.len() - 1].xyz_c[0];
    let fus_len = fus_end_x - fus_start_x;

    let fallback_x_nlg = fus_start_x + fus_len * nlg_x_frac;
    let fallback_x_mlg = x_mac_le + mlg_x_frac_mac * mac;
    // Resolved through the shared gate rather than from a fallback rebuilt
    // here. The gear-strength boundaries of this envelope are moments about
    // the main-gear station, so drawing them for an aircraft whose station
    // the mass model refuses would put a limit line where nothing was
    // measured. The figure says so instead.
    let Ok(gear_stations) = alas_pipeline::gear_stations::resolved_gear_stations(
        config,
        plane,
        fallback_x_nlg,
        fallback_x_mlg,
        fus_start_x,
        fus_len,
    ) else {
        return no_data_scene(scene, pal, MAIN_GEAR_STATION_NOT_MEASURED);
    };
    let x_nlg = gear_stations.x_nlg_m;
    let x_mlg = gear_stations.x_mlg_m;
    let wheelbase = x_mlg - x_nlg;

    // --- Component groups ----------------------------------------------------
    let get_mass = |k: &str| masses.get(k).copied().unwrap_or(0.0);
    let oew_mass: f64 = OEW_KEYS.iter().map(|&k| get_mass(k)).sum();
    let payload = get_mass(PAYLOAD);
    let fuel = get_mass(FUEL);
    let mtow_mass = oew_mass + payload + fuel.max(0.0);
    // The same resolved limit the `LandingMassLimitViolation` feasibility
    // check uses (declared reference MLW in BaselineSandbox/
    // ReferenceAdaptation; the mass-model fraction in CleanSheet), so the plot
    // and the feasibility finding never disagree on which line is the limit.
    let mlw_mass = config.landing_mass_limit_kg(mtow_mass);
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
    // CG check enforces, computed from the aerodynamic limits above, so this
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
    let gear = size_landing_gear_with_group_stations(
        mtow_mass,
        x_nlg,
        x_mlg,
        aero_fwd_lim_x,
        aero_aft_lim_x,
        fus_diam,
        fus_diam * 1.1,
        &gear_stations.main_gear_x_m,
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

    render(
        &mut scene,
        pal,
        CgEnvelopeRenderData {
            mac,
            x_mac_le,
            oew_mass,
            payload,
            fuel,
            mtow_mass,
            mlw_mass,
            mzfw_mass,
            oew_cg_mac,
            oew_cg_x,
            payload_cg_x,
            fuel_cg_x,
            target_sm,
            fwd_limit_mac,
            aft_limit_mac,
            tip_over_pct,
            np_pct,
            w_calc,
            c_nlg_str,
            c_mlg_str,
            c_nlg_min,
            pts_cg_a,
            pts_weight_a,
            w_ops,
            poly_fwd,
            poly_aft,
            nlg_strength_limits,
            mlg_strength_limits,
            nose_load_limits,
        },
    );
    scene
}
