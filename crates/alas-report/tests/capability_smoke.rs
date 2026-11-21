// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Capability smoke test, family `report`: a registered figure renders to a
//! scene, an SVG, and an exportable plot-data record whose JSON and CSV forms
//! read back.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_perf::performance::{Far25PositiveLoadFactorStatus, VnDiagramData};
use alas_report::families::performance;
use alas_report::registry::find_figure;
use alas_report::{capture_plot_data, PlotData};

#[test]
fn capability_smoke_report_figure_renders_and_exports_plot_data() {
    let vn = VnDiagramData {
        v_kt: vec![0.0, 100.0, 200.0, 300.0],
        n_stall_pos: vec![0.0, 0.8, 2.5, 2.5],
        n_stall_neg: vec![0.0, -0.3, -1.0, -1.0],
        n_lim_pos: 2.5,
        far25_positive_limit_load_factor_min: Some(2.5),
        far25_positive_load_factor_status: Far25PositiveLoadFactorStatus::MeetsMinimum,
        n_lim_neg: -1.0,
        n_ult_pos: 3.75,
        n_ult_neg: -1.5,
        v_s_kt: 110.0,
        v_a_kt: 180.0,
        v_c_kt: 280.0,
        v_d_kt: 340.0,
        v_cruise_op_kt: 250.0,
    };
    assert!(
        find_figure("vn_diagram").is_some(),
        "the figure is registered"
    );

    let scene = capture_plot_data(|| performance::figure_vn_diagram(&vn, None));
    let svg = alas_report::svg::render_svg(&scene);
    assert!(svg.contains("<svg"));
    let data = PlotData::from_scene(&scene, "vn_diagram", Some("smoke")).unwrap();
    assert!(data.series_count() > 0 && data.row_count() > 0);
    let from_json = PlotData::from_json(&data.to_json()).unwrap();
    assert_eq!(from_json.series_count(), data.series_count());
    assert!(PlotData::from_csv(&data.to_csv()).is_ok());
}
