// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Plot-data capture on real figure builders: the exported numbers are the
//! numbers the builder was given.

// A test asserts on values it constructed, so a failed unwrap is the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_aero::mses::{MsesPressureResult, MsesStatus};
use alas_config::AlasConfig;
use alas_perf::performance::VnDiagramData;
use alas_report::families::{aerodynamics, performance, propulsion};
use alas_report::plot_data::SeriesKind;
use alas_report::{capture_plot_data, PlotData};

fn vn() -> VnDiagramData {
    VnDiagramData {
        v_kt: vec![0.0, 100.0, 200.0, 300.0],
        n_stall_pos: vec![0.0, 0.8, 2.5, 2.5],
        n_stall_neg: vec![0.0, -0.3, -1.0, -1.0],
        n_lim_pos: 2.5,
        far25_positive_limit_load_factor_min: Some(2.5),
        far25_positive_load_factor_status:
            alas_perf::performance::Far25PositiveLoadFactorStatus::MeetsMinimum,
        n_lim_neg: -1.0,
        n_ult_pos: 3.75,
        n_ult_neg: -1.5,
        v_s_kt: 110.0,
        v_a_kt: 180.0,
        v_c_kt: 280.0,
        v_d_kt: 340.0,
        v_cruise_op_kt: 250.0,
    }
}

#[test]
fn vn_diagram_exports_its_design_speeds_and_load_factors_exactly() {
    let scene = capture_plot_data(|| performance::figure_vn_diagram(&vn(), None));
    let data = PlotData::from_scene(&scene, "vn_diagram", Some("run-x")).unwrap();
    let panel = &data.panels[0];
    assert_eq!(panel.x_unit.as_deref(), Some("kt"));
    assert_eq!(panel.y_unit.as_deref(), Some("-"));
    let factor = panel.x_si.as_ref().unwrap().factor;
    assert_eq!(factor, 1852.0 / 3600.0);
    let segments: Vec<_> = panel
        .series
        .iter()
        .filter(|s| s.kind == SeriesKind::Segments)
        .collect();
    let has_pair = |x: f64, y0: f64, y1: f64| {
        segments.iter().any(|s| {
            s.x.chunks(2)
                .zip(s.y.chunks(2))
                .any(|(xs, ys)| xs == [x, x] && ys == [y0, y1])
        })
    };
    // The VC and VD boundaries are drawn from these exact values.
    assert!(has_pair(280.0, -1.0, 2.5), "VC boundary");
    assert!(has_pair(340.0, -1.5, 3.75), "VD boundary");
    // The design speeds exported in SI are the same speeds in m/s.
    assert!((280.0 * factor - 144.0).abs() < 0.2);
}

#[test]
fn mses_pressure_curves_are_exported_with_their_samples() {
    let pressure = MsesPressureResult {
        status: MsesStatus::Ok,
        error: None,
        alpha_deg: 2.5,
        x_upper: vec![0.0, 0.5, 1.0],
        cp_upper: vec![1.0, -0.6, 0.2],
        mach_upper: vec![0.0, 0.78, 0.4],
        x_lower: vec![0.0, 0.5, 1.0],
        cp_lower: vec![1.0, 0.1, 0.2],
        mach_lower: vec![0.0, 0.65, 0.4],
        ..MsesPressureResult::default()
    };
    let scene =
        capture_plot_data(|| aerodynamics::figure_mses_pressure_distribution(&pressure, None));
    let data = PlotData::from_scene(&scene, "mses_pressure", None).unwrap();
    let has_curve = |x: &[f64], y: &[f64]| {
        data.panels
            .iter()
            .flat_map(|p| &p.series)
            .any(|s| s.x == x && s.y == y)
    };
    assert!(has_curve(&pressure.x_upper, &pressure.cp_upper));
    assert!(has_curve(&pressure.x_lower, &pressure.cp_lower));
}

#[test]
fn configuration_only_figures_export_without_a_run() {
    let config = AlasConfig::default();
    for scene in [
        capture_plot_data(|| propulsion::figure_propulsion_carpet_plot(&config, Some("light"))),
        capture_plot_data(|| propulsion::figure_propulsion_bpr_sensitivity(&config, Some("dark"))),
        capture_plot_data(|| propulsion::figure_propulsion_efficiency_decomposition(&config, None)),
    ] {
        let data = PlotData::from_scene(&scene, "propulsion", None).unwrap();
        assert!(data.series_count() > 0 && data.row_count() > 0);
        let back = PlotData::from_csv(&data.to_csv()).unwrap();
        assert_eq!(back.panels.len(), data.panels.len());
    }
}

#[test]
fn optimization_history_exports_the_recorded_evaluations() {
    use alas_config::design_variables::DesignVector;
    use alas_opt::history::OptimizationHistory;
    let mut history = OptimizationHistory::new();
    let design = DesignVector::default();
    for (cost, l_over_d, span_m) in [(1.45, 12.0, 28.0), (1.12, 15.0, 30.0), (0.85, 13.0, 29.0)] {
        history.record(design, true, cost, l_over_d, span_m, 0.0, 0.0, 0.0, "");
    }
    let scene = capture_plot_data(|| {
        alas_report::families::optimization::figure_optimization_history(&history, Some("grey"))
    });
    let data = PlotData::from_scene(&scene, "optimization_history", None).unwrap();
    assert!(data.row_count() >= 3, "{} rows", data.row_count());
}
