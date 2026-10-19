// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;
use crate::scene::{Axes2D, Color, Fill, Scale, Scene, SceneElement, Stroke};
use crate::theme::get_palette;

type Pairs = Vec<(f64, f64)>;

fn sample_scene() -> (Scene, Pairs, Pairs) {
    let pal = get_palette(None);
    let mut scene = Scene::new(600.0, 400.0, None);
    scene.title = Some("Test plot".to_owned());
    let axes = Axes2D::new((60.0, 40.0, 480.0, 300.0), (0.0, 10.0), (-1.0, 1.0));
    axes.draw_frame_with_labels(&mut scene, pal, "Range [nm]", "Payload (t)");
    // Values chosen to be awkward in binary floating point.
    let curve: Vec<(f64, f64)> = (0..=20)
        .map(|i| {
            let x = f64::from(i) * 0.5 + 1.0e-13;
            (x, (x * 0.7).sin() / 3.0)
        })
        .collect();
    axes.add_line_series(
        &mut scene,
        &curve,
        Stroke::new(Color::from_hex("#2563eb"), 2.0),
    );
    let manual: Vec<(f64, f64)> = vec![(0.1, 0.2), (3.3, -0.4), (9.99, 0.75)];
    scene.add(SceneElement::Polyline {
        points: manual.iter().map(|&(x, y)| axes.map_point(x, y)).collect(),
        stroke: Stroke::new(Color::from_hex("#d62728"), 1.0),
    });
    for &(x, y) in &manual {
        scene.add(SceneElement::Circle {
            center: axes.map_point(x, y),
            radius: 3.0,
            fill: Some(Fill::new(Color::from_hex("#d62728"))),
            stroke: None,
        });
    }
    crate::chart_kit::draw_legend(
        &mut scene,
        [80.0, 60.0],
        &[
            (
                "Sine".to_owned(),
                crate::chart_kit::LegendMarker::Line(Stroke::new(Color::from_hex("#2563eb"), 2.0)),
            ),
            (
                "Manual".to_owned(),
                crate::chart_kit::LegendMarker::Line(Stroke::new(Color::from_hex("#d62728"), 1.0)),
            ),
        ],
        pal,
        9.0,
    );
    (scene, curve, manual)
}

#[test]
fn capture_is_off_by_default() {
    let (scene, _, _) = {
        // Same construction without a guard.
        sample_scene()
    };
    assert!(scene.data_trace.panels.is_empty());
    assert!(PlotData::from_scene(&scene, "x", None).is_none());
}

#[test]
fn plotted_values_are_recorded_bit_exactly_with_units_and_names() {
    let (scene, curve, manual) = with_capture(sample_scene);
    let data = PlotData::from_scene(&scene, "test_plot", Some("run-7")).unwrap();
    assert_eq!(data.panels.len(), 1);
    let panel = &data.panels[0];
    assert_eq!(panel.x_label.as_deref(), Some("Range [nm]"));
    assert_eq!(panel.x_unit.as_deref(), Some("nm"));
    assert_eq!(panel.y_unit.as_deref(), Some("t"));
    assert_eq!(panel.x_si.as_ref().unwrap().factor, 1852.0);
    assert_eq!(panel.x_range, [0.0, 10.0]);
    let names: Vec<&str> = panel.series.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names[0], "Sine");
    assert_eq!(names[1], "Manual");
    let sine = &panel.series[0];
    assert_eq!(sine.x, curve.iter().map(|p| p.0).collect::<Vec<_>>());
    assert_eq!(sine.y, curve.iter().map(|p| p.1).collect::<Vec<_>>());
    let manual_line = &panel.series[1];
    assert_eq!(
        manual_line.x,
        manual.iter().map(|p| p.0).collect::<Vec<_>>()
    );
    assert_eq!(
        manual_line.y,
        manual.iter().map(|p| p.1).collect::<Vec<_>>()
    );
    let markers = &panel.series[2];
    assert_eq!(markers.kind, SeriesKind::Points);
    assert_eq!(markers.x, manual_line.x);
    // Axis frame grid lines and legend swatches are chrome, not data.
    assert_eq!(panel.series.len(), 3);
    assert!(data.notes.is_empty());
}

#[test]
fn line_series_keeps_points_outside_the_visible_range() {
    let data = with_capture(|| {
        let mut scene = Scene::new(300.0, 200.0, None);
        let axes = Axes2D::new((20.0, 20.0, 200.0, 100.0), (0.0, 1.0), (0.0, 1.0));
        axes.add_line_series(
            &mut scene,
            &[(-5.0, 0.5), (0.5, 0.5), (7.0, 2.0)],
            Stroke::new(Color::from_hex("#000000"), 1.0),
        );
        PlotData::from_scene(&scene, "clip", None).unwrap()
    });
    assert_eq!(data.panels[0].series[0].x, vec![-5.0, 0.5, 7.0]);
}

#[test]
fn log_axes_and_bars_are_recovered() {
    let data = with_capture(|| {
        let mut scene = Scene::new(300.0, 200.0, None);
        let axes = Axes2D::new((20.0, 20.0, 200.0, 100.0), (1.0, 1000.0), (0.0, 10.0))
            .with_x_scale(Scale::Log10);
        let a = axes.map_point(10.0, 2.0);
        let b = axes.map_point(100.0, 8.0);
        scene.add(SceneElement::Rect {
            x: a[0],
            y: b[1],
            width: b[0] - a[0],
            height: a[1] - b[1],
            rx: 0.0,
            fill: Some(Fill::new(Color::from_hex("#00ff00"))),
            stroke: None,
        });
        PlotData::from_scene(&scene, "bars", None).unwrap()
    });
    let series = &data.panels[0].series[0];
    assert_eq!(series.kind, SeriesKind::Bars);
    assert!(!series.exact);
    assert!((series.x[0] - 10.0).abs() < 1e-9 && (series.x2[0] - 100.0).abs() < 1e-8);
    assert!((series.y[0] - 2.0).abs() < 1e-9 && (series.y2[0] - 8.0).abs() < 1e-9);
    assert!(!data.notes.is_empty());
}

#[test]
fn heatmap_records_grid_values() {
    let data = with_capture(|| {
        let mut scene = Scene::new(300.0, 200.0, None);
        let axes = Axes2D::new((20.0, 20.0, 200.0, 100.0), (0.0, 2.0), (0.0, 2.0));
        let values = [1.0, 2.0, 3.0, 4.0];
        axes.add_heatmap_grid(
            &mut scene,
            &[0.0, 1.0, 2.0],
            &[0.0, 1.0, 2.0],
            &values,
            crate::Colormap::Viridis,
            1.0,
            4.0,
        );
        PlotData::from_scene(&scene, "heat", None).unwrap()
    });
    let series = &data.panels[0].series[0];
    assert_eq!(series.kind, SeriesKind::Heatmap);
    assert_eq!(series.z, vec![1.0, 2.0, 3.0, 4.0]);
    assert_eq!(series.x, vec![0.5, 1.5, 0.5, 1.5]);
}

fn awkward_plot() -> PlotData {
    let mut data = PlotData::new("round_trip", "Round, trip \"quoted\"", Some("run-1"));
    let mut panel = PlotPanel::new("Speed [kt]", "Altitude [ft]");
    panel.x_scale = AxisScale::Log10;
    let mut series = PlotSeries::from_xy(
        "a, b \"c\"",
        SeriesKind::Line,
        vec![0.1, 1.0 / 3.0, -2.5e-300, 1.7976931348623157e308, 0.0, -0.0],
        vec![
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            1.0e-7,
            123_456_789.123_456_79,
            5e-324,
        ],
    );
    series.z = series.x.clone();
    panel.series.push(series);
    panel
        .series
        .push(PlotSeries::line("second", &[[1.0, 2.0], [3.0, 4.0]]));
    data.panels.push(panel);
    data
}

fn same_bits(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(p, q)| p.to_bits() == q.to_bits() || (p.is_nan() && q.is_nan()))
}

#[test]
fn json_round_trip_is_lossless() {
    let data = awkward_plot();
    let back = PlotData::from_json(&data.to_json()).unwrap();
    for (a, b) in data.panels[0].series.iter().zip(&back.panels[0].series) {
        assert_eq!(a.name, b.name);
        assert!(same_bits(&a.x, &b.x) && same_bits(&a.y, &b.y) && same_bits(&a.z, &b.z));
    }
    assert_eq!(back.panels[0].x_unit, data.panels[0].x_unit);
    assert_eq!(back.run_id, data.run_id);
    assert_eq!(back.panels[0].x_scale, AxisScale::Log10);
}

#[test]
fn csv_round_trip_is_lossless_and_carries_si_columns() {
    let data = awkward_plot();
    let csv = data.to_csv();
    assert!(csv
        .lines()
        .next()
        .unwrap()
        .starts_with("# schema: alas.plot-data/1"));
    let back = PlotData::from_csv(&csv).unwrap();
    assert_eq!(back.plot_id, "round_trip");
    assert_eq!(back.title, data.title);
    assert_eq!(back.run_id.as_deref(), Some("run-1"));
    assert_eq!(back.panels[0].x_unit.as_deref(), Some("kt"));
    for (a, b) in data.panels[0].series.iter().zip(&back.panels[0].series) {
        assert_eq!(a.name, b.name);
        assert!(same_bits(&a.x, &b.x), "{:?} vs {:?}", a.x, b.x);
        assert!(same_bits(&a.y, &b.y));
        assert!(same_bits(&a.z, &b.z));
    }
    // 1 kt = 1852/3600 m/s on the SI column of the second series' first row.
    let row = csv.lines().find(|l| l.starts_with("1,2,second")).unwrap();
    let fields: Vec<&str> = row.split(',').collect();
    assert_eq!(fields[12].parse::<f64>().unwrap(), 1.0 * (1852.0 / 3600.0));
    assert_eq!(fields[13].parse::<f64>().unwrap(), 2.0 * 0.3048);
}

#[test]
fn bundle_lists_every_plot_with_unique_names() {
    let a = awkward_plot();
    let mut b = awkward_plot();
    b.plot_id = "asb_threeview".to_owned();
    let files = bundle_files(&[a.clone(), a, b]);
    let names: Vec<&str> = files.iter().map(|f| f.0.as_str()).collect();
    for expected in [
        "round_trip.csv",
        "round_trip_2.json",
        "threeview.csv",
        "index.csv",
        "index.json",
        "README.txt",
    ] {
        assert!(names.contains(&expected), "{expected} missing in {names:?}");
    }
    assert!(names.is_ascii_names());
}

trait AsciiNames {
    fn is_ascii_names(&self) -> bool;
}

impl AsciiNames for Vec<&str> {
    fn is_ascii_names(&self) -> bool {
        self.iter().all(|n| n.is_ascii() && !n.contains('/'))
    }
}

#[test]
fn custom_series_carry_names_labels_and_fitted_ranges() {
    let data = with_capture(|| {
        let mut scene = Scene::new(100.0, 100.0, None);
        scene.title = Some("Custom".to_owned());
        for (name, y) in [("required", [1.0, 2.0]), ("available", [3.0, 4.0])] {
            scene.data_trace.note_custom(CustomSeries {
                key: "k",
                x_label: "Type",
                y_label: "Distance [m]",
                name,
                kind: SeriesKind::Points,
                x: vec![0.0, 1.0],
                y: y.to_vec(),
                labels: vec!["A".to_owned(), "B".to_owned()],
            });
        }
        PlotData::from_scene(&scene, "custom", None).unwrap()
    });
    assert_eq!(data.panels.len(), 1);
    let panel = &data.panels[0];
    assert_eq!(panel.y_unit.as_deref(), Some("m"));
    assert_eq!(panel.y_range, [1.0, 4.0]);
    assert_eq!(panel.series[1].name, "available");
    let back = PlotData::from_csv(&data.to_csv()).unwrap();
    assert_eq!(back.panels[0].series[1].labels, vec!["A", "B"]);
    assert_eq!(back.panels[0].series[1].y, vec![3.0, 4.0]);
}
