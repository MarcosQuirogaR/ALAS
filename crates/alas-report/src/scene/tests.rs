// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

#[test]
fn linear_scale_maps_range_endpoints_to_canvas_corners() {
    let axes = Axes2D::new((0.0, 0.0, 100.0, 50.0), (0.0, 10.0), (0.0, 5.0));
    assert_eq!(axes.map_point(0.0, 0.0), [0.0, 50.0]);
    assert_eq!(axes.map_point(10.0, 5.0), [100.0, 0.0]);
}

#[test]
fn reversed_y_range_maps_negative_pressure_upward() {
    let axes = Axes2D::new((0.0, 0.0, 100.0, 50.0), (0.0, 1.0), (1.0, -1.0));
    assert_eq!(axes.map_point(0.5, -1.0), [50.0, 0.0]);
    assert_eq!(axes.map_point(0.5, 1.0), [50.0, 50.0]);
}

#[test]
fn named_plot_colors_do_not_fall_back_to_black() {
    assert_eq!(Color::from_hex("gray"), Color::rgb(128, 128, 128));
    assert_eq!(Color::from_hex("grey"), Color::rgb(128, 128, 128));
    assert_eq!(Color::from_hex("tab:purple"), Color::rgb(148, 103, 189));
}

#[test]
fn contrast_measurement_composites_transparent_foregrounds() {
    let black = Color::rgb(0, 0, 0);
    let white = Color::rgb(255, 255, 255);
    assert!((white.contrast_against(black) - 21.0).abs() < 1e-12);
    assert!(Color::rgba(255, 255, 255, 64).contrast_against(black) < 3.0);
}

#[test]
fn multiline_offsets_respect_the_requested_block_baseline() {
    assert_eq!(
        text_line_center_offsets(2, 12.0, TextBaseline::Top),
        vec![6.0, 18.0]
    );
    assert_eq!(
        text_line_center_offsets(2, 12.0, TextBaseline::Middle),
        vec![-6.0, 6.0]
    );
    assert_eq!(
        text_line_center_offsets(2, 12.0, TextBaseline::Bottom),
        vec![-18.0, -6.0]
    );
}

#[test]
fn log_scale_maps_decades_to_equal_canvas_spans() {
    let axes =
        Axes2D::new((0.0, 0.0, 300.0, 10.0), (1.0, 1000.0), (0.0, 1.0)).with_x_scale(Scale::Log10);
    let x1 = axes.map_point(1.0, 0.0)[0];
    let x10 = axes.map_point(10.0, 0.0)[0];
    let x100 = axes.map_point(100.0, 0.0)[0];
    let x1000 = axes.map_point(1000.0, 0.0)[0];
    assert!((x10 - x1 - (x100 - x10)).abs() < 1e-9);
    assert!((x100 - x10 - (x1000 - x100)).abs() < 1e-9);
}

#[test]
fn symlog_scale_is_linear_inside_threshold_and_odd_symmetric() {
    let scale = Scale::SymLog { linthresh: 1.0 };
    assert_eq!(scale.transform(0.5), 0.5);
    assert_eq!(scale.transform(-0.5), -0.5);
    assert!((scale.transform(10.0) + scale.transform(-10.0)).abs() < 1e-9);
    assert!(scale.transform(10.0) > scale.transform(1.0));
}

#[test]
fn heatmap_grid_emits_one_rect_per_cell_colored_by_value() {
    let axes = Axes2D::new((0.0, 0.0, 200.0, 100.0), (0.0, 2.0), (0.0, 1.0));
    let mut scene = Scene::new(200.0, 100.0, None);
    let x_edges = [0.0, 1.0, 2.0];
    let y_edges = [0.0, 1.0];
    let values = [0.0, 1.0];
    axes.add_heatmap_grid(
        &mut scene,
        &x_edges,
        &y_edges,
        &values,
        crate::colormap::Colormap::Viridis,
        0.0,
        1.0,
    );
    assert_eq!(scene.elements.len(), 2);
    for elem in &scene.elements {
        assert!(matches!(elem, SceneElement::Rect { .. }));
    }
}

#[test]
fn equal_aspect_expands_the_shorter_data_range() {
    let axes = Axes2D::new((0.0, 0.0, 200.0, 100.0), (0.0, 10.0), (0.0, 1.0)).with_equal_aspect();
    assert_eq!(axes.x_max - axes.x_min, 10.0);
    assert_eq!(axes.y_max - axes.y_min, 5.0);
    assert_eq!(axes.map_point(0.0, axes.y_min)[0], 0.0);
    assert_eq!(axes.map_point(10.0, axes.y_max)[0], 200.0);
}

#[test]
fn line_series_clips_segments_to_the_data_rectangle() {
    let axes = Axes2D::new((10.0, 10.0, 100.0, 80.0), (0.0, 1.0), (0.0, 1.0));
    let mut scene = Scene::new(120.0, 100.0, None);
    axes.add_line_series(
        &mut scene,
        &[(-1.0, 0.5), (2.0, 0.5)],
        Stroke::new(Color::rgb(0, 0, 0), 1.0),
    );
    let SceneElement::Polyline { points, .. } = &scene.elements[0] else {
        panic!("clipped line should remain a polyline");
    };
    assert_eq!(points.first().copied(), Some([10.0, 50.0]));
    assert_eq!(points.last().copied(), Some([110.0, 50.0]));
}
