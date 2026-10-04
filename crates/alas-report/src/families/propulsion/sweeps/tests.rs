// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

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
