// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


impl Axes2D {
    /// Construct a 2D axes mapping data coordinates to a canvas bounding box.
    pub fn new(rect: (f64, f64, f64, f64), x_range: (f64, f64), y_range: (f64, f64)) -> Self {
        let (left, top, width, height) = rect;
        let (x_min, x_max) = x_range;
        let (y_min, y_max) = y_range;
        Self {
            left,
            top,
            width,
            height,
            x_min,
            x_max,
            y_min,
            y_max,
            x_scale: Scale::Linear,
            y_scale: Scale::Linear,
        }
    }

    /// Set the X-axis scale (chainable).
    pub fn with_x_scale(mut self, scale: Scale) -> Self {
        self.x_scale = scale;
        self
    }

    /// Set the Y-axis scale (chainable).
    pub fn with_y_scale(mut self, scale: Scale) -> Self {
        self.y_scale = scale;
        self
    }

    /// Expand one data range so one data unit has the same pixel size on both axes.
    pub fn with_equal_aspect(mut self) -> Self {
        (self.x_min, self.x_max, self.y_min, self.y_max) =
            crate::chart_kit::equal_aspect_ranges(&self);
        self
    }

    /// Transform data coordinates `(x, y)` to canvas pixel coordinates `[px, py]`.
    pub fn map_point(&self, x: f64, y: f64) -> Point2D {
        let x_min_s = self.x_scale.transform(self.x_min);
        let x_max_s = self.x_scale.transform(self.x_max);
        let y_min_s = self.y_scale.transform(self.y_min);
        let y_max_s = self.y_scale.transform(self.y_max);
        let x_span = x_max_s - x_min_s;
        let y_span = y_max_s - y_min_s;
        let dx = if x_span.abs() < 1e-12 { 1e-12 } else { x_span };
        let dy = if y_span.abs() < 1e-12 { 1e-12 } else { y_span };

        let u = (self.x_scale.transform(x) - x_min_s) / dx;
        let v = (self.y_scale.transform(y) - y_min_s) / dy;

        let px = self.left + u * self.width;
        let py = self.top + (1.0 - v) * self.height;
        [px, py]
    }

    /// Approximate a filled contour (`contourf`/`tricontourf`) as a fine grid
    /// of colored cells. Exact marching-squares iso-banding is not worth the
    /// complexity for a chart-legibility SVG export at typical figure sizes;
    /// a fine enough regular grid reads the same at chart scale. `x_edges`/
    /// `y_edges` are cell boundaries (`nx+1`/`ny+1` long); `values` is
    /// `nx*ny`, row-major with row 0 at `y_edges[0]`.
    #[allow(clippy::too_many_arguments)]
    pub fn add_heatmap_grid(
        &self,
        scene: &mut Scene,
        x_edges: &[f64],
        y_edges: &[f64],
        values: &[f64],
        cmap: crate::colormap::Colormap,
        vmin: f64,
        vmax: f64,
    ) {
        let nx = x_edges.len().saturating_sub(1);
        let ny = y_edges.len().saturating_sub(1);
        let span = (vmax - vmin).max(1e-12);
        for iy in 0..ny {
            for ix in 0..nx {
                let v = values[iy * nx + ix];
                let t = ((v - vmin) / span).clamp(0.0, 1.0);
                let color = cmap.sample(t);
                let p0 = self.map_point(x_edges[ix], y_edges[iy]);
                let p1 = self.map_point(x_edges[ix + 1], y_edges[iy + 1]);
                let (x0, x1) = (p0[0].min(p1[0]), p0[0].max(p1[0]));
                let (y0, y1) = (p0[1].min(p1[1]), p0[1].max(p1[1]));
                scene.add(SceneElement::Rect {
                    x: x0,
                    y: y0,
                    width: (x1 - x0).max(0.5),
                    height: (y1 - y0).max(0.5),
                    rx: 0.0,
                    fill: Some(Fill::new(color)),
                    stroke: None,
                });
            }
        }
    }

    /// Draw axes frame, bounding spine, and grid lines into a scene.
    pub fn draw_frame(&self, scene: &mut Scene, theme: &Palette) {
        crate::chart_kit::draw_axes(self, scene, theme, None, None);
    }

    /// Draw the frame, numeric ticks, grid and explicit axis labels.
    ///
    /// [`Self::draw_frame`] remains the compact default for inset plots. This
    /// variant is intended for primary scientific figures where units and
    /// variable names are part of the result's meaning.
    pub fn draw_frame_with_labels(
        &self,
        scene: &mut Scene,
        theme: &Palette,
        x_label: &str,
        y_label: &str,
    ) {
        crate::chart_kit::draw_axes(self, scene, theme, Some(x_label), Some(y_label));
    }

    /// Add a 2D continuous line trace to the scene.
    pub fn add_line_series(&self, scene: &mut Scene, pts: &[(f64, f64)], stroke: Stroke) {
        let mut clipped = Vec::new();
        for pair in pts.windows(2) {
            let Some((start, end)) = clip_segment(self, pair[0], pair[1]) else {
                continue;
            };
            if clipped.last().copied() != Some(start) {
                clipped.push(start);
            }
            clipped.push(end);
        }
        if clipped.len() >= 2 {
            scene.add(SceneElement::Polyline {
                points: clipped
                    .into_iter()
                    .map(|(x, y)| self.map_point(x, y))
                    .collect(),
                stroke,
            });
        }
    }
}

fn clip_segment(axes: &Axes2D, a: (f64, f64), b: (f64, f64)) -> Option<((f64, f64), (f64, f64))> {
    let (xmin, xmax) = (axes.x_min.min(axes.x_max), axes.x_min.max(axes.x_max));
    let (ymin, ymax) = (axes.y_min.min(axes.y_max), axes.y_min.max(axes.y_max));
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 1.0;
    for (p, q) in [
        (-dx, a.0 - xmin),
        (dx, xmax - a.0),
        (-dy, a.1 - ymin),
        (dy, ymax - a.1),
    ] {
        if p.abs() < f64::EPSILON {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let ratio = q / p;
        if p < 0.0 {
            t0 = t0.max(ratio);
        } else {
            t1 = t1.min(ratio);
        }
        if t0 > t1 {
            return None;
        }
    }
    Some((
        (a.0 + t0 * dx, a.1 + t0 * dy),
        (a.0 + t1 * dx, a.1 + t1 * dy),
    ))
}

/// 3D Camera for orthographic/isometric wireframe projection onto 2D canvas.
#[cfg(test)]
mod tests {
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
        let axes = Axes2D::new((0.0, 0.0, 300.0, 10.0), (1.0, 1000.0), (0.0, 1.0))
            .with_x_scale(Scale::Log10);
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
        let axes =
            Axes2D::new((0.0, 0.0, 200.0, 100.0), (0.0, 10.0), (0.0, 1.0)).with_equal_aspect();
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
}

