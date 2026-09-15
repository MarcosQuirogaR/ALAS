// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use crate::theme::Palette;

#[path = "../scene/camera.rs"]
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
    /// Line width [CSS px].
    pub width: f64,
    /// Optional dash and gap lengths [CSS px].
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

/// Upper bound on the mean advance of one character of proportional
/// sans-serif text, in em, used by backends without glyph metrics.
///
/// Uppercase-heavy DejaVu Sans averages about 0.72 em per character and
/// Arial or the egui proportional face noticeably less, so a line budgeted
/// with this figure stays inside its box for anything short of a run of the
/// widest glyphs.
pub const CONSERVATIVE_ADVANCE_EM: f64 = 0.75;

/// Characters that fit in `width_px` at `font_size_pt` under
/// [`CONSERVATIVE_ADVANCE_EM`], never fewer than one.
pub fn conservative_char_budget(font_size_pt: f64, width_px: f64) -> usize {
    let advance = font_size_pt.max(0.1) * CSS_PIXELS_PER_POINT * CONSERVATIVE_ADVANCE_EM;
    (width_px / advance).floor().max(1.0) as usize
}

/// Wrap `text` so that no line exceeds the conservative character budget for
/// `width_px` at `font_size_pt`.
///
/// Words are packed greedily. A single word longer than the budget (a path,
/// an identifier, a solver token) is split across lines instead of being left
/// to overflow, so every character stays visible. Caller line breaks are kept
/// as paragraph breaks.
pub fn wrap_text_to_width(text: &str, font_size_pt: f64, width_px: f64) -> String {
    let budget = conservative_char_budget(font_size_pt, width_px);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let word_len = word.chars().count();
            if word_len > budget {
                if !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                }
                let mut remaining = word;
                while remaining.chars().count() > budget {
                    let split_at = remaining
                        .char_indices()
                        .nth(budget)
                        .map_or(remaining.len(), |(index, _)| index);
                    let (chunk, rest) = remaining.split_at(split_at);
                    lines.push(chunk.to_owned());
                    remaining = rest;
                }
                current.push_str(remaining);
                continue;
            }
            let candidate_len = if current.is_empty() {
                word_len
            } else {
                current.chars().count() + 1 + word_len
            };
            if candidate_len > budget && !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
        lines.push(current);
    }
    lines.join("\n")
}

/// Height in scene pixels that a [`SceneElement::TextBlock`] needs on a
/// metric-free backend: the conservative line count times the shared line
/// advance.
pub fn text_block_height(text: &str, font_size_pt: f64, width_px: f64) -> f64 {
    let lines = wrap_text_to_width(text, font_size_pt, width_px)
        .lines()
        .count()
        .max(1);
    lines as f64 * font_size_pt * CSS_PIXELS_PER_POINT * TEXT_LINE_HEIGHT_EM
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
    /// Left-aligned paragraph confined to a content box.
    ///
    /// Renderers wrap `text` to `width` before drawing it and split a
    /// whitespace-free token such as a file path when that token alone exceeds
    /// the box, so status prose never runs past the figure edge. A backend
    /// with real glyph metrics (the interactive egui view) wraps by
    /// measurement; metric-free backends (SVG, PDF) use
    /// [`wrap_text_to_width`]. Caller line breaks remain paragraph breaks.
    TextBlock {
        /// Content.
        text: String,
        /// Top-left anchor of the content box.
        pos: Point2D,
        /// Maximum line width in scene pixels.
        width: f64,
        /// Font size in points.
        font_size: f64,
        /// Text color.
        color: Color,
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
    /// Background color of the scene canvas, and the color renderers treat as
    /// this scene's true backdrop for contrast decisions (see
    /// [`Self::paint_background`] and [`visual_title`]) even on paths that
    /// must not paint an opaque rect here.
    pub background: Option<Color>,
    /// Whether renderers should paint an opaque [`Self::background`] rect.
    ///
    /// A scene composited over pre-drawn raster content (an embedded texture,
    /// a globe) must keep this canvas layer transparent there or the opaque
    /// rect would hide it, while `background` itself must keep carrying the
    /// real color: [`visual_title`] and other contrast decisions still need
    /// to know what is actually behind the scene. Use
    /// [`Self::hide_background_paint`] rather than clearing `background`.
    #[serde(default = "default_paint_background")]
    pub paint_background: bool,
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
            paint_background: true,
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

    /// Stop renderers from painting an opaque [`Self::background`] rect while
    /// keeping the color itself, so [`visual_title`] and other contrast
    /// decisions still see the scene's true backdrop.
    ///
    /// Compositing paths that pre-paint textured content beneath the vector
    /// layer (embedded rasters, the orbit globe) call this instead of
    /// clearing `background` to `None`, which would silently steer the
    /// automatic title onto its light-background fallback color regardless
    /// of the active theme.
    pub fn hide_background_paint(&mut self) {
        self.paint_background = false;
    }
}

const fn default_render_title() -> bool {
    true
}

const fn default_paint_background() -> bool {
    true
}

/// Return the visible heading associated with a scene's document title.
///
/// `Scene::title` began as export metadata, which meant that otherwise titled
/// figures had no heading in either the SVG or the raster viewport. Keeping
/// the heading derived at render time preserves the scene contract while
/// making the same title visible in every backend.
///
/// The contrast decision reads [`Scene::background`] directly, independent of
/// whether that color is actually painted (see [`Scene::paint_background`]).
/// A caller that needs a transparent canvas layer must use
/// [`Scene::hide_background_paint`] rather than clearing `background`, or
/// this heading silently falls back to its light-background color on every
/// theme.
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
    /// Optional fixed number of decimals for X-axis tick labels.
    ///
    /// Most axes use the automatic nice-step formatter.  Scientific figures
    /// with a deliberately small coefficient range can request a fixed
    /// precision so the labels remain comparable and do not collide with the
    /// axis title.
    pub x_tick_decimals: Option<usize>,
    /// Optional fixed number of decimals for Y-axis tick labels.
    pub y_tick_decimals: Option<usize>,
}
