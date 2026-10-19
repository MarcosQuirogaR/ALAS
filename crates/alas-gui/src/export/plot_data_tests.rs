// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Plot-data export on a real completed run: coverage, exactness, round trip.

use std::fmt::Write as _;

use alas_config::AlasConfig;
use alas_report::{PlotData, PlotSeries};

use super::*;

fn completed_state() -> AppState {
    let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
    config.optimizer.design_space.mode = alas_config::optimizer::DesignMode::BaselineSandbox;
    let options = alas_pipeline::PipelineOptions {
        optimize: false,
        compare_baseline: true,
        parallel: true,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };
    let result = alas_pipeline::DesignPipeline::new(config)
        .run(&options, &alas_pipeline::RunEnvironment::default())
        .unwrap();
    let mut state = AppState {
        config_values: serde_json::to_value(&result.config).unwrap(),
        ..Default::default()
    };
    state.set_completed_pipeline_result(result);
    state
}

fn bits_equal(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(p, q)| p.to_bits() == q.to_bits() || (p.is_nan() && q.is_nan()))
}

fn series_equal(a: &PlotSeries, b: &PlotSeries) -> bool {
    a.name == b.name
        && a.kind == b.kind
        && bits_equal(&a.x, &b.x)
        && bits_equal(&a.y, &b.y)
        && bits_equal(&a.z, &b.z)
        && bits_equal(&a.x2, &b.x2)
        && bits_equal(&a.y2, &b.y2)
}

fn assert_round_trips(plot: &PlotData) {
    let from_json = PlotData::from_json(&plot.to_json()).unwrap();
    let from_csv = PlotData::from_csv(&plot.to_csv()).unwrap();
    assert_eq!(from_json.plot_id, plot.plot_id);
    assert_eq!(from_csv.plot_id, plot.plot_id);
    assert_eq!(
        from_json.panels.len(),
        plot.panels.len(),
        "{}",
        plot.plot_id
    );
    assert_eq!(from_csv.panels.len(), plot.panels.len(), "{}", plot.plot_id);
    for (index, panel) in plot.panels.iter().enumerate() {
        assert_eq!(from_json.panels[index].x_unit, panel.x_unit);
        assert_eq!(from_json.panels[index].x_label, panel.x_label);
        for (i, series) in panel.series.iter().enumerate() {
            assert!(
                series_equal(series, &from_json.panels[index].series[i]),
                "JSON round trip of {} / {}",
                plot.plot_id,
                series.name
            );
            let csv_series = &from_csv.panels[index].series[i];
            assert!(
                bits_equal(&series.x, &csv_series.x)
                    && bits_equal(&series.y, &csv_series.y)
                    && bits_equal(&series.z, &csv_series.z),
                "CSV round trip of {} / {}",
                plot.plot_id,
                series.name
            );
        }
    }
}

#[test]
fn every_plot_of_a_run_exports_and_round_trips() {
    let state = completed_state();
    let plots = collect_plot_data(&state).unwrap();
    assert!(plots.len() >= 10, "only {} plots carry data", plots.len());
    for plot in &plots {
        assert!(
            plot.series_count() > 0 && plot.row_count() > 0,
            "{}",
            plot.plot_id
        );
        assert_round_trips(plot);
    }
}

#[test]
fn payload_range_export_is_the_plotted_corner_data() {
    let state = completed_state();
    let result = state.pipeline_result.as_ref().unwrap();
    let report = result.optimized_report.as_ref().unwrap();
    let plots = collect_plot_data(&state).unwrap();
    let plot = plots.iter().find(|p| p.plot_id == "payload_range").unwrap();
    let data = alas_report::families::performance::payload_range_data(report, &result.config)
        .expect("payload-range data for the A320-200 run");
    let panel = &plot.panels[0];
    assert_eq!(panel.x_unit.as_deref(), Some("nm"));
    assert_eq!(panel.x_si.as_ref().unwrap().factor, 1852.0);
    let line = panel
        .series
        .iter()
        .find(|s| s.kind == alas_report::plot_data::SeriesKind::Line)
        .expect("payload-range curve");
    let ranges: Vec<f64> = data.points.iter().map(|p| p.range_nm).collect();
    let payloads_t: Vec<f64> = data.points.iter().map(|p| p.payload_kg / 1000.0).collect();
    assert_eq!(line.x, ranges);
    assert_eq!(line.y, payloads_t);
}

#[test]
fn the_bundle_lists_every_plot_and_the_zip_is_written() {
    let state = completed_state();
    let directory =
        std::env::temp_dir().join(format!("alas-plot-data-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    let receipt = export_all_plot_data(&state, &directory).unwrap();
    assert!(receipt.figure_count >= 10);
    let index = std::fs::read_to_string(receipt.path.join("index.csv")).unwrap();
    assert_eq!(index.lines().count(), receipt.figure_count + 1);
    assert!(directory.join(PLOT_DATA_ARCHIVE_NAME).is_file());
    let one = export_plot_data(
        &collect_plot_data(&state).unwrap()[0],
        &directory.join("one"),
    )
    .unwrap();
    assert!(one.path.is_file());
    let _ = std::fs::remove_dir_all(&directory);
}

/// Writes the coverage inventory when `ALAS_PLOT_INVENTORY_OUT` names a file.
#[test]
fn registry_plot_inventory() {
    let Some(out) = std::env::var_os("ALAS_PLOT_INVENTORY_OUT") else {
        return;
    };
    let state = completed_state();
    let result = state.pipeline_result.as_ref().unwrap();
    let theme = "light";
    let mut table = String::from(
        "| id | category | status | panels | series | rows |\n|---|---|---|---|---|---|\n",
    );
    for descriptor in alas_report::RESULT_FIGURES {
        let (status, panels, series, rows) =
            match crate::scene::build_result_figure(&state, descriptor.id, &result.config, theme) {
                Some(Some(scene)) => match PlotData::from_scene(&scene, descriptor.id, None) {
                    Some(plot) => (
                        "data",
                        plot.panels.len(),
                        plot.series_count(),
                        plot.row_count(),
                    ),
                    None => ("no plotted series", 0, 0, 0),
                },
                _ => ("unavailable in this run", 0, 0, 0),
            };
        let _ = writeln!(
            table,
            "| {} | {} | {status} | {panels} | {series} | {rows} |",
            descriptor.id, descriptor.category
        );
    }
    std::fs::write(out, table).unwrap();
    if let Some(sample) = std::env::var_os("ALAS_PLOT_SAMPLE_OUT") {
        write_plot_bundle(
            &collect_plot_data(&state).unwrap(),
            std::path::Path::new(&sample),
        )
        .unwrap();
    }
}
