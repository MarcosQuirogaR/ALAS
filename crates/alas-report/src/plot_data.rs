// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Raw data behind every plot, exportable to CSV and JSON.
//!
//! Figures are built as [`Scene`]s, which keep pixels only. Building a figure
//! under [`with_capture`] additionally records the plotted values on the
//! scene ([`DataTrace`]); [`PlotData::from_scene`] turns that record into a
//! self-describing [`PlotData`] with series names, axis labels and units.
//! Plots drawn directly by the GUI build a [`PlotData`] from their own arrays.
//!
//! Values are the numbers plotted, in the unit the axis label states. Where
//! that unit is a known display unit, [`PlotPanel::x_si`]/[`PlotPanel::y_si`]
//! give the factor to SI, and the CSV carries `x_si`/`y_si` columns.

use serde::{Deserialize, Serialize};

use crate::scene::Scene;

mod files;
mod trace;
mod units;

pub use files::{bundle_files, csv_string, safe_file_stem, ParseError};
pub use trace::{
    is_active, with_capture, AxesRecord, AxisScale, CaptureGuard, DataTrace, SeriesKind,
    TraceLegend, TracePanel, TraceSeries, TraceText,
};
pub(crate) use trace::{note_map_point, observe, ChromeGuard, CustomSeries};
pub use units::{si_conversion, unit_from_label, SiConversion};

/// Schema identifier written into every exported file.
pub const SCHEMA: &str = "alas.plot-data/1";

/// Serialization of f64 arrays that keeps NaN and infinities (JSON has none).
pub(crate) mod f64_array {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    #[serde(untagged)]
    enum Value {
        Number(f64),
        Text(String),
    }

    pub fn serialize<S: Serializer>(values: &[f64], serializer: S) -> Result<S::Ok, S::Error> {
        let encoded: Vec<Value> = values
            .iter()
            .map(|v| {
                if v.is_finite() {
                    Value::Number(*v)
                } else {
                    Value::Text(format!("{v}"))
                }
            })
            .collect();
        encoded.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<f64>, D::Error> {
        let encoded = Vec::<Value>::deserialize(deserializer)?;
        encoded
            .into_iter()
            .map(|v| match v {
                Value::Number(n) => Ok(n),
                Value::Text(t) => t.parse::<f64>().map_err(serde::de::Error::custom),
            })
            .collect()
    }
}

/// Serialization of a `[min, max]` pair that keeps non-finite values.
pub(crate) mod f64_pair {
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(values: &[f64; 2], serializer: S) -> Result<S::Ok, S::Error> {
        super::f64_array::serialize(values, serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[f64; 2], D::Error> {
        let values = super::f64_array::deserialize(deserializer)?;
        <[f64; 2]>::try_from(values)
            .map_err(|_| serde::de::Error::custom("a range needs exactly two values"))
    }
}

/// One series of one panel. Arrays are in plotted units.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlotSeries {
    /// Series name (legend text when the figure has one).
    pub name: String,
    /// Drawing primitive.
    pub kind: SeriesKind,
    /// Stroke or fill colour `#rrggbb`, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// X values.
    #[serde(with = "f64_array")]
    pub x: Vec<f64>,
    /// Y values.
    #[serde(with = "f64_array")]
    pub y: Vec<f64>,
    /// Z values (heatmap cell values).
    #[serde(default, with = "f64_array", skip_serializing_if = "Vec::is_empty")]
    pub z: Vec<f64>,
    /// Opposite-corner x of bars.
    #[serde(default, with = "f64_array", skip_serializing_if = "Vec::is_empty")]
    pub x2: Vec<f64>,
    /// Opposite-corner y of bars.
    #[serde(default, with = "f64_array", skip_serializing_if = "Vec::is_empty")]
    pub y2: Vec<f64>,
    /// Heatmap x cell edges.
    #[serde(default, with = "f64_array", skip_serializing_if = "Vec::is_empty")]
    pub x_edges: Vec<f64>,
    /// Heatmap y cell edges.
    #[serde(default, with = "f64_array", skip_serializing_if = "Vec::is_empty")]
    pub y_edges: Vec<f64>,
    /// Category label of each row, when the figure names its rows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    /// True when the values are exactly those the builder plotted; false for
    /// bars, which are recovered by inverting the axes mapping.
    pub exact: bool,
}

impl PlotSeries {
    /// A polyline from `[x, y]` pairs.
    pub fn line(name: impl Into<String>, points: &[[f64; 2]]) -> Self {
        Self::from_points(name, SeriesKind::Line, points)
    }

    /// Series of a given kind from `[x, y]` pairs.
    pub fn from_points(name: impl Into<String>, kind: SeriesKind, points: &[[f64; 2]]) -> Self {
        Self::from_xy(
            name,
            kind,
            points.iter().map(|p| p[0]).collect(),
            points.iter().map(|p| p[1]).collect(),
        )
    }

    /// Series of a given kind from x and y arrays.
    pub fn from_xy(name: impl Into<String>, kind: SeriesKind, x: Vec<f64>, y: Vec<f64>) -> Self {
        Self {
            name: name.into(),
            kind,
            color: None,
            x,
            y,
            z: Vec::new(),
            x2: Vec::new(),
            y2: Vec::new(),
            x_edges: Vec::new(),
            y_edges: Vec::new(),
            labels: Vec::new(),
            exact: true,
        }
    }

    /// Number of data rows.
    pub fn len(&self) -> usize {
        self.x.len()
    }

    /// Whether the series has no rows.
    pub fn is_empty(&self) -> bool {
        self.x.is_empty()
    }
}

/// One set of axes with its labels, scales and series.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlotPanel {
    /// Panel title, when the figure draws one above the axes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// X axis label as drawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_label: Option<String>,
    /// Y axis label as drawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_label: Option<String>,
    /// X unit read from the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_unit: Option<String>,
    /// Y unit read from the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_unit: Option<String>,
    /// SI conversion of the x unit, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_si: Option<SiConversion>,
    /// SI conversion of the y unit, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_si: Option<SiConversion>,
    /// X axis scale as drawn.
    pub x_scale: AxisScale,
    /// Y axis scale as drawn.
    pub y_scale: AxisScale,
    /// Visible x range `[min, max]`.
    #[serde(with = "f64_pair")]
    pub x_range: [f64; 2],
    /// Visible y range `[min, max]`.
    #[serde(with = "f64_pair")]
    pub y_range: [f64; 2],
    /// Series in drawing order.
    pub series: Vec<PlotSeries>,
}

impl PlotPanel {
    /// Set the visible ranges to the finite extent of the series.
    pub fn fit_ranges(&mut self) {
        let extent = |values: Vec<f64>| {
            let finite = values.into_iter().filter(|v| v.is_finite());
            finite.fold(None, |acc: Option<[f64; 2]>, v| {
                Some(acc.map_or([v, v], |r| [r[0].min(v), r[1].max(v)]))
            })
        };
        let xs = self
            .series
            .iter()
            .flat_map(|s| s.x.iter().chain(&s.x2).copied())
            .collect();
        let ys = self
            .series
            .iter()
            .flat_map(|s| s.y.iter().chain(&s.y2).copied())
            .collect();
        if let Some(range) = extent(xs) {
            self.x_range = range;
        }
        if let Some(range) = extent(ys) {
            self.y_range = range;
        }
    }

    /// Panel with linear axes from labels (units read from the labels).
    pub fn new(x_label: &str, y_label: &str) -> Self {
        let x_unit = unit_from_label(x_label);
        let y_unit = unit_from_label(y_label);
        Self {
            title: None,
            x_label: Some(x_label.to_owned()),
            y_label: Some(y_label.to_owned()),
            x_si: x_unit.as_deref().and_then(si_conversion),
            y_si: y_unit.as_deref().and_then(si_conversion),
            x_unit,
            y_unit,
            x_scale: AxisScale::Linear,
            y_scale: AxisScale::Linear,
            x_range: [f64::NAN; 2],
            y_range: [f64::NAN; 2],
            series: Vec::new(),
        }
    }
}

/// All data behind one plot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlotData {
    /// Schema identifier, always [`SCHEMA`].
    pub schema: String,
    /// Stable plot identifier (registry figure id for report figures).
    pub plot_id: String,
    /// Plot title.
    pub title: String,
    /// Run the data belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// Producing application and version.
    pub generator: String,
    /// Caveats for readers of the files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// Axes of the plot.
    pub panels: Vec<PlotPanel>,
}

impl PlotData {
    /// Empty plot record.
    pub fn new(plot_id: &str, title: &str, run_id: Option<&str>) -> Self {
        Self {
            schema: SCHEMA.to_owned(),
            plot_id: plot_id.to_owned(),
            title: title.to_owned(),
            run_id: run_id.map(str::to_owned),
            generator: format!("ALAS {}", env!("CARGO_PKG_VERSION")),
            notes: Vec::new(),
            panels: Vec::new(),
        }
    }

    /// Total number of data rows over all series.
    pub fn row_count(&self) -> usize {
        self.panels
            .iter()
            .flat_map(|panel| &panel.series)
            .map(PlotSeries::len)
            .sum()
    }

    /// Number of series over all panels.
    pub fn series_count(&self) -> usize {
        self.panels.iter().map(|panel| panel.series.len()).sum()
    }

    /// JSON text with full fidelity (all metadata).
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Parse a file written by [`Self::to_json`].
    pub fn from_json(text: &str) -> Result<Self, ParseError> {
        serde_json::from_str(text).map_err(|e| ParseError(e.to_string()))
    }

    /// CSV text (tidy long format with `#` metadata header).
    pub fn to_csv(&self) -> String {
        csv_string(self)
    }

    /// Parse the series values of a file written by [`Self::to_csv`].
    pub fn from_csv(text: &str) -> Result<Self, ParseError> {
        files::parse_csv(text)
    }

    /// Build the data record from a scene built under [`with_capture`].
    ///
    /// Returns `None` when the scene recorded no series: a status card or a
    /// purely schematic drawing has no plotted data.
    pub fn from_scene(scene: &Scene, plot_id: &str, run_id: Option<&str>) -> Option<Self> {
        let trace = &scene.data_trace;
        let mut data = Self::new(plot_id, scene.title.as_deref().unwrap_or(plot_id), run_id);
        let single_panel = trace.panels.iter().filter(|p| !p.series.is_empty()).count() == 1;
        for panel in trace.panels.iter().filter(|p| !p.series.is_empty()) {
            data.panels.push(build_panel(scene, panel, single_panel));
        }
        name_series(&mut data, trace);
        if data.panels.is_empty() {
            return None;
        }
        if data.panels.iter().flat_map(|p| &p.series).any(|s| !s.exact) {
            data.notes.push(
                "Series of kind 'bars' were recovered from the drawn rectangles by inverting the axes mapping; \
                 they agree with the plotted values to floating-point round-off but are not the builder's own numbers."
                    .to_owned(),
            );
        }
        Some(data)
    }
}

fn build_panel(scene: &Scene, panel: &TracePanel, single_panel: bool) -> PlotPanel {
    let axes = &panel.axes;
    let right_of =
        |t: &TraceText| t.pos[0] >= axes.left - 12.0 && t.pos[0] <= axes.left + axes.width + 12.0;
    let upright = |t: &&TraceText| t.angle_deg.abs() < 1.0 && !t.text.trim().is_empty();
    let x_label = panel.x_label.clone().or_else(|| {
        let bottom = axes.top + axes.height;
        scene
            .data_trace
            .texts
            .iter()
            .filter(upright)
            .filter(|t| right_of(t) && t.pos[1] > bottom + 14.0 && t.pos[1] < bottom + 60.0)
            .min_by(|a, b| a.pos[1].total_cmp(&b.pos[1]))
            .map(|t| t.text.clone())
    });
    let y_label = panel.y_label.clone().or_else(|| {
        scene
            .data_trace
            .texts
            .iter()
            .filter(|t| (t.angle_deg.abs() - 90.0).abs() < 1.0)
            .filter(|t| t.pos[0] < axes.left && t.pos[0] > axes.left - 100.0)
            .filter(|t| t.pos[1] >= axes.top - 10.0 && t.pos[1] <= axes.top + axes.height + 10.0)
            .map(|t| t.text.clone())
            .next()
    });
    let title = scene
        .data_trace
        .texts
        .iter()
        .filter(upright)
        .filter(|t| right_of(t) && t.pos[1] <= axes.top + 2.0 && t.pos[1] >= axes.top - 40.0)
        .max_by(|a, b| a.font_size.total_cmp(&b.font_size))
        .map(|t| t.text.clone())
        .or_else(|| single_panel.then(|| scene.title.clone()).flatten());
    let mut out = PlotPanel::new(
        x_label.as_deref().unwrap_or(""),
        y_label.as_deref().unwrap_or(""),
    );
    out.x_label = x_label;
    out.y_label = y_label;
    out.title = title;
    out.x_scale = axes.x_scale;
    out.y_scale = axes.y_scale;
    out.x_range = axes.x_range;
    out.y_range = axes.y_range;
    out.series = panel
        .series
        .iter()
        .map(|s| PlotSeries {
            name: s.name.clone().unwrap_or_default(),
            kind: s.kind,
            color: s.color.clone(),
            x: s.x.clone(),
            y: s.y.clone(),
            z: s.z.clone(),
            x2: s.x2.clone(),
            y2: s.y2.clone(),
            x_edges: s.x_edges.clone(),
            y_edges: s.y_edges.clone(),
            labels: s.labels.clone(),
            exact: s.exact,
        })
        .collect();
    if axes.x_range[0].is_nan() {
        out.fit_ranges();
    }
    out
}

/// Name series from legend rows of the same colour; the rest get a stable
/// positional name that includes their colour.
fn name_series(data: &mut PlotData, trace: &DataTrace) {
    let mut used: Vec<String> = Vec::new();
    for (panel_index, panel) in data.panels.iter_mut().enumerate() {
        for (series_index, series) in panel.series.iter_mut().enumerate() {
            if !series.name.is_empty() {
                continue;
            }
            let legend = series.color.as_ref().and_then(|color| {
                trace
                    .legend
                    .iter()
                    .find(|row| &row.color == color && !used.contains(&row.label))
            });
            series.name = match legend {
                Some(row) => {
                    used.push(row.label.clone());
                    row.label.clone()
                }
                None => format!(
                    "{}{}_{}",
                    series.kind.as_str(),
                    if data_panel_count(&trace.panels) > 1 {
                        format!("_panel{}", panel_index + 1)
                    } else {
                        String::new()
                    },
                    series_index + 1
                ),
            };
        }
    }
}

fn data_panel_count(panels: &[TracePanel]) -> usize {
    panels.iter().filter(|p| !p.series.is_empty()).count()
}

#[cfg(test)]
mod tests;
