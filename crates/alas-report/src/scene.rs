// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Backend-neutral 2D/3D scene description, coordinate transforms, and cameras.

// SceneElement's public fields are documented by their variant contracts; this keeps the graph under the repository's source-size limit.
#![allow(missing_docs)]

use crate::theme::Palette;

mod camera;
pub use camera::Camera3D;
use serde::{Deserialize, Serialize};

/// 2D point in canvas or data coordinates `[x, y]`.
pub type Point2D = [f64; 2];

/// 3D point in model coordinates `[x, y, z]`.
pub type Point3D = [f64; 3];

/// CSS reference-pixel conversion for typographic points at 96 dpi.
pub const CSS_PIXELS_PER_POINT: f64 = 96.0 / 72.0;

/// Line advance used by every scene text backend, expressed in em.
pub const TEXT_LINE_HEIGHT_EM: f64 = 1.2;

/// Color represented in RGBA channels with 8-bit components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha opacity channel.
    pub a: u8,
}

impl Color {
    /// Create a fully opaque RGB color.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Create an RGBA color with explicit alpha.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Parse a color from hexadecimal string (e.g. `#ffffff`, `#333`, `#2563eb80`) or standard named color.
    pub fn from_hex(s: &str) -> Self {
        let trimmed = s.trim();
        match trimmed {
            "tab:blue" => return Self::rgb(31, 119, 180),
            "tab:red" => return Self::rgb(214, 39, 40),
            "tab:orange" => return Self::rgb(255, 127, 14),
            "tab:green" => return Self::rgb(44, 160, 44),
            "tab:purple" => return Self::rgb(148, 103, 189),
            "gray" | "grey" => return Self::rgb(128, 128, 128),
            "lightgray" | "lightgrey" => return Self::rgb(211, 211, 211),
            _ => {}
        }
        let clean = trimmed.trim_start_matches('#');
        match clean.len() {
            3 => {
                let r = u8::from_str_radix(&clean[0..1].repeat(2), 16).unwrap_or(0);
                let g = u8::from_str_radix(&clean[1..2].repeat(2), 16).unwrap_or(0);
                let b = u8::from_str_radix(&clean[2..3].repeat(2), 16).unwrap_or(0);
                Self::rgb(r, g, b)
            }
            6 => {
                let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(0);
                let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(0);
                let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(0);
                Self::rgb(r, g, b)
            }
            8 => {
                let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(0);
                let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(0);
                let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(0);
                let a = u8::from_str_radix(&clean[6..8], 16).unwrap_or(255);
                Self::rgba(r, g, b, a)
            }
            _ => Self::rgb(0, 0, 0),
        }
    }

    /// Render color to `#rrggbb` hex string.
    pub fn to_hex_rgb(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Normalized alpha in range `[0.0, 1.0]`.
    pub fn alpha_f64(&self) -> f64 {
        (self.a as f64) / 255.0
    }

    /// WCAG relative luminance of the opaque RGB channels.
    pub fn relative_luminance(&self) -> f64 {
        fn linear(channel: u8) -> f64 {
            let value = f64::from(channel) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * linear(self.r) + 0.7152 * linear(self.g) + 0.0722 * linear(self.b)
    }

    /// Composite this color over an opaque background.
    pub fn over(&self, background: Self) -> Self {
        let alpha = self.alpha_f64();
        let channel = |foreground: u8, behind: u8| {
            (f64::from(foreground) * alpha + f64::from(behind) * (1.0 - alpha)).round() as u8
        };
        Self::rgb(
            channel(self.r, background.r),
            channel(self.g, background.g),
            channel(self.b, background.b),
        )
    }

    /// Contrast ratio after compositing this color over an opaque background.
    pub fn contrast_against(&self, background: Self) -> f64 {
        let foreground_luminance = self.over(background).relative_luminance();
        let background_luminance = background.relative_luminance();
        (foreground_luminance.max(background_luminance) + 0.05)
            / (foreground_luminance.min(background_luminance) + 0.05)
    }
}

/// Stroke styling for outline paths and polylines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    /// Outline color.
    pub color: Color,
    /// Line width.
    pub width: f64,
    /// Optional dash and gap lengths.
    pub dash_array: Option<Vec<f64>>,
}

impl Stroke {
    /// Solid stroke with given color and width.
    pub fn new(color: Color, width: f64) -> Self {
        Self {
            color,
            width,
            dash_array: None,
        }
    }

    /// Dashed stroke with given dash and gap lengths.
    pub fn dashed(color: Color, width: f64, dash: f64, gap: f64) -> Self {
        Self {
            color,
            width,
            dash_array: Some(vec![dash, gap]),
        }
    }
}

/// Fill styling for closed polygons and areas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fill {
    /// Fill color.
    pub color: Color,
}

impl Fill {
    /// Construct a solid fill from a color.
    pub const fn new(color: Color) -> Self {
        Self { color }
    }
}

/// Horizontal alignment for text rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlign {
    /// Start alignment.
    Left,
    /// Center alignment.
    Center,
    /// End alignment.
    Right,
}

/// Vertical baseline alignment for text rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextBaseline {
    /// Top alignment.
    Top,
    /// Middle alignment.
    Middle,
    /// Bottom alignment.
    Bottom,
}

/// Return each line-box center relative to a multiline text anchor.
///
/// Backends use the same centers so top, middle, and bottom alignment occupy
/// identical bounds in exported and interactive figures.
pub fn text_line_center_offsets(
    line_count: usize,
    line_height: f64,
    baseline: TextBaseline,
) -> Vec<f64> {
    let count = line_count.max(1);
    let block_height = count as f64 * line_height;
    let first = match baseline {
        TextBaseline::Top => line_height * 0.5,
        TextBaseline::Middle => -block_height * 0.5 + line_height * 0.5,
        TextBaseline::Bottom => -block_height + line_height * 0.5,
    };
    (0..count)
        .map(|index| first + index as f64 * line_height)
        .collect()
}

/// Individual vector element in a scene graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SceneElement {
    /// Straight line between two 2D points.
    Line {
        p1: Point2D,
        p2: Point2D,
        stroke: Stroke,
    },
    /// Connected sequence of line segments.
    Polyline {
        points: Vec<Point2D>,
        stroke: Stroke,
    },
    /// Filled or outlined closed polygon.
    Polygon {
        points: Vec<Point2D>,
        fill: Option<Fill>,
        stroke: Option<Stroke>,
    },
    /// Axis-aligned rectangle.
    Rect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        rx: f64,
        fill: Option<Fill>,
        stroke: Option<Stroke>,
    },
    /// Circle with given center and radius.
    Circle {
        center: Point2D,
        radius: f64,
        fill: Option<Fill>,
        stroke: Option<Stroke>,
    },
    /// External raster image placed in a rectangular canvas region.
    /// The source is intentionally a path rather than decoded pixels: Patran
    /// owns the PNG files and the reporting layer should not load or copy a
    /// potentially large solver artifact just to construct a scene.
    Image {
        source: String,
        x: f64,
        y: f64,
        /// Display width.
        width: f64,
        /// Display height.
        height: f64,
        /// Optional normalized source rectangle `[left, top, width, height]`.
        ///
        /// The values are fractions of the source image.  Keeping the crop in
        /// the scene rather than pre-rendering it lets an equirectangular map
        /// use the same geographic transform in PNG and SVG output.
        source_rect: Option<[f64; 4]>,
    },
    /// Equirectangular raster mapped onto an orthographic sphere.
    ///
    /// `source` uses the same address contract as [`Self::Image`]. The
    /// camera establishes the view direction; renderers use longitude for U
    /// and north-to-south latitude for V, so route and texture coordinates
    /// share a single geographic convention.
    SphericalImage {
        source: String,
        center: Point2D,
        radius: f64,
        camera: Camera3D,
        /// Whether the source longitude is reflected to match globe geometry
        /// that presents geographic east on the visual right.
        mirror_longitude: bool,
    },
    /// Text label with typographic alignment and rotation.
    Text {
        /// Content.
        text: String,
        /// Anchor.
        pos: Point2D,
        /// Font size in points.
        font_size: f64,
        /// Text color.
        color: Color,
        /// Horizontal alignment.
        align: TextAlign,
        /// Vertical baseline alignment.
        baseline: TextBaseline,
        /// Rotation angle in degrees clockwise.
        angle_deg: f64,
        /// Bold weight flag.
        bold: bool,
    },
}

/// Complete backend-neutral figure scene graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    /// Canvas width in display pixels.
    pub width: f64,
    /// Canvas height in display pixels.
    pub height: f64,
    /// Background color of the scene canvas.
    pub background: Option<Color>,
    /// Figure title.
    pub title: Option<String>,
    /// Whether renderers should synthesize a visible heading from [`Self::title`].
    ///
    /// Figure builders with a deliberately positioned title retain the title
    /// as SVG/PDF metadata while disabling this automatic second heading.
    #[serde(default = "default_render_title")]
    pub render_title: bool,
    /// Flat list of scene elements.
    pub elements: Vec<SceneElement>,
}

impl Scene {
    /// Construct an empty scene with given dimensions and background color.
    pub fn new(width: f64, height: f64, bg: Option<Color>) -> Self {
        Self {
            width,
            height,
            background: bg,
            title: None,
            render_title: true,
            elements: Vec::new(),
        }
    }

    /// Add a scene element to the scene.
    pub fn add(&mut self, elem: SceneElement) {
        self.elements.push(elem);
    }

    /// Keep `title` as document metadata without rendering an automatic heading.
    pub fn suppress_derived_title(&mut self) {
        self.render_title = false;
    }
}

const fn default_render_title() -> bool {
    true
}

/// Return the visible heading associated with a scene's document title.
///
/// `Scene::title` began as export metadata, which meant that otherwise titled
/// figures had no heading in either the SVG or the raster viewport. Keeping
/// the heading derived at render time preserves the scene contract while
/// making the same title visible in every backend.
pub fn visual_title(scene: &Scene) -> Option<SceneElement> {
    if !scene.render_title {
        return None;
    }
    let title = scene.title.as_ref()?.trim();
    if title.is_empty() {
        return None;
    }
    let color = match scene.background {
        Some(background) if background.relative_luminance() < 0.5 => Color::from_hex("#ffffff"),
        _ => Color::from_hex("#202124"),
    };
    Some(SceneElement::Text {
        text: title.to_owned(),
        pos: [scene.width * 0.5, 18.0],
        font_size: 15.0,
        color,
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: true,
    })
}

/// Axis coordinate scaling mode.
///
/// `SymLog` follows matplotlib's convention: linear within `[-linthresh,
/// linthresh]` around zero (so a pole or eigenvalue can sit exactly on an
/// axis without a log singularity), log-scaled beyond it. Used by the
/// dynamic-modes s-plane plot and any Reynolds-number sweep axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scale {
    /// Values map directly to the axis range.
    Linear,
    /// Base-10 logarithmic scaling. Values must be strictly positive.
    Log10,
    /// Linear near zero, logarithmic beyond `linthresh` in each direction.
    SymLog {
        /// Half-width of the linear region around zero.
        linthresh: f64,
    },
}

impl Scale {
    fn transform(self, v: f64) -> f64 {
        match self {
            Scale::Linear => v,
            Scale::Log10 => v.max(1e-300).log10(),
            Scale::SymLog { linthresh } => {
                let lt = linthresh.max(1e-12);
                if v.abs() <= lt {
                    v
                } else {
                    v.signum() * lt * (1.0 + (v.abs() / lt).log10())
                }
            }
        }
    }
}

/// 2D Cartesian chart area handling coordinate scaling, grids, and series.
#[derive(Debug, Clone)]
pub struct Axes2D {
    /// Canvas left boundary.
    pub left: f64,
    /// Canvas top boundary.
    pub top: f64,
    /// Canvas plot area width.
    pub width: f64,
    /// Canvas plot area height.
    pub height: f64,
    /// Minimum data X value.
    pub x_min: f64,
    /// Maximum data X value.
    pub x_max: f64,
    /// Minimum data Y value.
    pub y_min: f64,
    /// Maximum data Y value.
    pub y_max: f64,
    /// X-axis coordinate scaling mode.
    pub x_scale: Scale,
    /// Y-axis coordinate scaling mode.
    pub y_scale: Scale,
}

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
