// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Records kept on a scene: axes geometry, series, texts and legend rows.

use serde::{Deserialize, Serialize};

use super::is_active;
use crate::scene::{Axes2D, Scale};

/// Axis scaling recorded with a panel.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AxisScale {
    /// Linear axis.
    Linear,
    /// Base-10 logarithmic axis.
    Log10,
    /// Linear within `linthresh`, logarithmic beyond.
    SymLog {
        /// Half-width of the linear region.
        linthresh: f64,
    },
}

impl From<Scale> for AxisScale {
    fn from(scale: Scale) -> Self {
        match scale {
            Scale::Linear => Self::Linear,
            Scale::Log10 => Self::Log10,
            Scale::SymLog { linthresh } => Self::SymLog { linthresh },
        }
    }
}

impl From<AxisScale> for Scale {
    fn from(scale: AxisScale) -> Self {
        match scale {
            AxisScale::Linear => Self::Linear,
            AxisScale::Log10 => Self::Log10,
            AxisScale::SymLog { linthresh } => Self::SymLog { linthresh },
        }
    }
}

/// Geometry of one axes: canvas rectangle, data ranges and scales.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AxesRecord {
    /// Canvas left [px].
    pub left: f64,
    /// Canvas top [px].
    pub top: f64,
    /// Canvas width [px].
    pub width: f64,
    /// Canvas height [px].
    pub height: f64,
    /// Data range of the x axis, `[min, max]`.
    pub x_range: [f64; 2],
    /// Data range of the y axis, `[min, max]`.
    pub y_range: [f64; 2],
    /// X scale.
    pub x_scale: AxisScale,
    /// Y scale.
    pub y_scale: AxisScale,
}

impl From<&Axes2D> for AxesRecord {
    fn from(axes: &Axes2D) -> Self {
        Self {
            left: axes.left,
            top: axes.top,
            width: axes.width,
            height: axes.height,
            x_range: [axes.x_min, axes.x_max],
            y_range: [axes.y_min, axes.y_max],
            x_scale: axes.x_scale.into(),
            y_scale: axes.y_scale.into(),
        }
    }
}

impl AxesRecord {
    pub(super) fn same_axes(&self, other: &Self) -> bool {
        let bits = |value: &Self| {
            [
                value.left,
                value.top,
                value.width,
                value.height,
                value.x_range[0],
                value.x_range[1],
                value.y_range[0],
                value.y_range[1],
            ]
            .map(f64::to_bits)
        };
        bits(self) == bits(other) && self.x_scale == other.x_scale && self.y_scale == other.y_scale
    }

    /// Whether a canvas point lies inside the plot rectangle (half a pixel of slack).
    pub fn contains_pixel(&self, point: [f64; 2]) -> bool {
        point[0] >= self.left - 0.5
            && point[0] <= self.left + self.width + 0.5
            && point[1] >= self.top - 0.5
            && point[1] <= self.top + self.height + 0.5
    }

    /// Invert the axes mapping: canvas pixel to data coordinates.
    pub fn pixel_to_data(&self, pixel: [f64; 2]) -> [f64; 2] {
        let axis =
            |value: f64, origin: f64, extent: f64, range: [f64; 2], scale: AxisScale, flip| {
                let transform = |v: f64| Scale::from(scale).transform(v);
                let (lo, hi) = (transform(range[0]), transform(range[1]));
                let span = if (hi - lo).abs() < 1e-12 {
                    1e-12
                } else {
                    hi - lo
                };
                let mut u = (value - origin) / extent;
                if flip {
                    u = 1.0 - u;
                }
                invert_scale(scale, lo + u * span)
            };
        [
            axis(
                pixel[0],
                self.left,
                self.width,
                self.x_range,
                self.x_scale,
                false,
            ),
            axis(
                pixel[1],
                self.top,
                self.height,
                self.y_range,
                self.y_scale,
                true,
            ),
        ]
    }
}

fn invert_scale(scale: AxisScale, transformed: f64) -> f64 {
    match scale {
        AxisScale::Linear => transformed,
        AxisScale::Log10 => 10f64.powf(transformed),
        AxisScale::SymLog { linthresh } => {
            let lt = linthresh.max(1e-12);
            if transformed.abs() <= lt {
                transformed
            } else {
                transformed.signum() * lt * 10f64.powf(transformed.abs() / lt - 1.0)
            }
        }
    }
}

/// How a recorded series was drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeriesKind {
    /// Connected polyline; points in drawing order.
    Line,
    /// Closed polygon outline or fill; vertices in drawing order.
    Polygon,
    /// Marker positions.
    Points,
    /// Independent segments; consecutive point pairs form one segment.
    Segments,
    /// Bars or shaded rectangles: `(x, y)` is one corner, `(x2, y2)` the opposite.
    Bars,
    /// Colour-mapped grid: cell centres in `x`/`y`, cell value in `z`.
    Heatmap,
}

impl SeriesKind {
    /// Stable lower-case name used in the exported files.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Polygon => "polygon",
            Self::Points => "points",
            Self::Segments => "segments",
            Self::Bars => "bars",
            Self::Heatmap => "heatmap",
        }
    }
}

/// One series recorded from a scene build.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceSeries {
    /// Drawing primitive.
    pub kind: SeriesKind,
    /// Stroke or fill colour as `#rrggbb`, used to match legend entries.
    pub color: Option<String>,
    /// X values, plotted data units.
    pub x: Vec<f64>,
    /// Y values, plotted data units.
    pub y: Vec<f64>,
    /// Z values (heatmap cell values).
    pub z: Vec<f64>,
    /// Second-corner x (bars).
    pub x2: Vec<f64>,
    /// Second-corner y (bars).
    pub y2: Vec<f64>,
    /// Heatmap x cell edges.
    pub x_edges: Vec<f64>,
    /// Heatmap y cell edges.
    pub y_edges: Vec<f64>,
    /// Explicit series name (figures that name their own series).
    pub name: Option<String>,
    /// Category label of each row, when the figure names its rows.
    pub labels: Vec<String>,
    /// True when the values are the exact numbers the figure builder passed in.
    pub exact: bool,
}

impl TraceSeries {
    pub(super) fn new(kind: SeriesKind, color: Option<String>, exact: bool) -> Self {
        Self {
            kind,
            color,
            x: Vec::new(),
            y: Vec::new(),
            z: Vec::new(),
            x2: Vec::new(),
            y2: Vec::new(),
            x_edges: Vec::new(),
            y_edges: Vec::new(),
            name: None,
            labels: Vec::new(),
            exact,
        }
    }

    pub(super) fn from_pairs(
        kind: SeriesKind,
        color: Option<String>,
        pairs: &[(f64, f64)],
    ) -> Self {
        let mut series = Self::new(kind, color, true);
        series.x = pairs.iter().map(|pair| pair.0).collect();
        series.y = pairs.iter().map(|pair| pair.1).collect();
        series
    }
}

/// Everything recorded for one axes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TracePanel {
    /// Identity of a panel a figure records itself (not drawn through `Axes2D`).
    pub key: Option<String>,
    /// Axes geometry.
    pub axes: AxesRecord,
    /// X label passed to the axes drawing, if any.
    pub x_label: Option<String>,
    /// Y label passed to the axes drawing, if any.
    pub y_label: Option<String>,
    /// Series in drawing order.
    pub series: Vec<TraceSeries>,
}

/// A free text element, kept to recover axis labels, panel titles and names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceText {
    /// Text content.
    pub text: String,
    /// Anchor in canvas pixels.
    pub pos: [f64; 2],
    /// Rotation, degrees clockwise.
    pub angle_deg: f64,
    /// Font size, points.
    pub font_size: f64,
}

/// A legend row: label and the colour of its swatch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceLegend {
    /// Label shown in the legend.
    pub label: String,
    /// Swatch colour `#rrggbb`.
    pub color: String,
}

/// The data record carried by a scene. Empty unless capture was active.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DataTrace {
    /// Axes with their series.
    pub panels: Vec<TracePanel>,
    /// Free text outside chart chrome.
    pub texts: Vec<TraceText>,
    /// Legend rows.
    pub legend: Vec<TraceLegend>,
}

impl DataTrace {
    /// Whether nothing was recorded.
    pub fn is_empty(&self) -> bool {
        self.panels.is_empty() && self.texts.is_empty() && self.legend.is_empty()
    }

    pub(super) fn panel_mut(&mut self, axes: &AxesRecord) -> &mut TracePanel {
        let index = self
            .panels
            .iter()
            .position(|panel| panel.key.is_none() && panel.axes.same_axes(axes))
            .unwrap_or_else(|| {
                self.panels.push(TracePanel {
                    key: None,
                    axes: *axes,
                    x_label: None,
                    y_label: None,
                    series: Vec::new(),
                });
                self.panels.len() - 1
            });
        &mut self.panels[index]
    }

    /// Record the axis labels passed to an axes frame.
    pub(crate) fn note_labels(&mut self, axes: &Axes2D, x: Option<&str>, y: Option<&str>) {
        if !is_active() {
            return;
        }
        let panel = self.panel_mut(&AxesRecord::from(axes));
        if let Some(label) = x {
            panel.x_label = Some(label.to_owned());
        }
        if let Some(label) = y {
            panel.y_label = Some(label.to_owned());
        }
    }

    /// Record a legend row.
    pub(crate) fn note_legend(&mut self, label: &str, color: &crate::scene::Color) {
        if is_active() {
            self.legend.push(TraceLegend {
                label: label.to_owned(),
                color: color.to_hex_rgb(),
            });
        }
    }

    /// Record a series a figure draws with its own geometry, keyed by panel.
    pub(crate) fn note_custom(&mut self, series: CustomSeries<'_>) {
        if !is_active() {
            return;
        }
        let index = self
            .panels
            .iter()
            .position(|panel| panel.key.as_deref() == Some(series.key))
            .unwrap_or_else(|| {
                self.panels.push(TracePanel {
                    key: Some(series.key.to_owned()),
                    axes: AxesRecord {
                        left: 0.0,
                        top: 0.0,
                        width: 0.0,
                        height: 0.0,
                        x_range: [f64::NAN; 2],
                        y_range: [f64::NAN; 2],
                        x_scale: AxisScale::Linear,
                        y_scale: AxisScale::Linear,
                    },
                    x_label: Some(series.x_label.to_owned()),
                    y_label: Some(series.y_label.to_owned()),
                    series: Vec::new(),
                });
                self.panels.len() - 1
            });
        let mut recorded = TraceSeries::new(series.kind, None, true);
        recorded.name = Some(series.name.to_owned());
        recorded.x = series.x;
        recorded.y = series.y;
        recorded.labels = series.labels;
        self.panels[index].series.push(recorded);
    }

    /// Record an unclipped line series from its input data.
    pub(crate) fn note_line_series(
        &mut self,
        axes: &Axes2D,
        pts: &[(f64, f64)],
        color: &crate::scene::Color,
    ) {
        if is_active() && !pts.is_empty() {
            let series = TraceSeries::from_pairs(SeriesKind::Line, Some(color.to_hex_rgb()), pts);
            self.panel_mut(&AxesRecord::from(axes)).series.push(series);
        }
    }

    /// Record a heatmap grid from its inputs.
    pub(crate) fn note_heatmap(
        &mut self,
        axes: &Axes2D,
        x_edges: &[f64],
        y_edges: &[f64],
        values: &[f64],
    ) {
        if !is_active() {
            return;
        }
        let nx = x_edges.len().saturating_sub(1);
        let ny = y_edges.len().saturating_sub(1);
        let mut series = TraceSeries::new(SeriesKind::Heatmap, None, true);
        series.x_edges = x_edges.to_vec();
        series.y_edges = y_edges.to_vec();
        for iy in 0..ny {
            for ix in 0..nx {
                series.x.push(0.5 * (x_edges[ix] + x_edges[ix + 1]));
                series.y.push(0.5 * (y_edges[iy] + y_edges[iy + 1]));
                series
                    .z
                    .push(values.get(iy * nx + ix).copied().unwrap_or(f64::NAN));
            }
        }
        self.panel_mut(&AxesRecord::from(axes)).series.push(series);
    }
}

/// A series a figure records itself because it is not drawn through `Axes2D`.
pub(crate) struct CustomSeries<'a> {
    /// Panel identity within the figure.
    pub key: &'a str,
    /// X axis label (with unit).
    pub x_label: &'a str,
    /// Y axis label (with unit).
    pub y_label: &'a str,
    /// Series name.
    pub name: &'a str,
    /// Drawing primitive.
    pub kind: SeriesKind,
    /// X values.
    pub x: Vec<f64>,
    /// Y values.
    pub y: Vec<f64>,
    /// Row labels, or empty.
    pub labels: Vec<String>,
}
