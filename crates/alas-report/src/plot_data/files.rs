// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! CSV serialization, CSV parsing and the multi-plot file bundle.

use std::fmt::Write as _;

use super::{PlotData, PlotPanel, PlotSeries, SeriesKind, SCHEMA};

/// A plot-data file could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "plot data: {}", self.0)
    }
}

impl std::error::Error for ParseError {}

const COLUMNS: &str =
    "panel,series_index,series,kind,index,x,y,z,x2,y2,x_unit,y_unit,x_si,y_si,label";

/// Shortest decimal text that parses back to the same `f64`.
fn number(value: f64) -> String {
    format!("{value:?}")
}

fn scale_text(scale: super::AxisScale) -> String {
    match scale {
        super::AxisScale::Linear => "linear".to_owned(),
        super::AxisScale::Log10 => "log10".to_owned(),
        super::AxisScale::SymLog { linthresh } => format!("symlog(linthresh={linthresh:?})"),
    }
}

fn one_line(text: &str) -> String {
    text.replace(['\r', '\n'], " ")
}

fn quote(field: &str) -> String {
    let field = one_line(field);
    if field.contains([',', '"']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field
    }
}

/// Tidy long CSV: one row per data point, `#` header lines with metadata.
///
/// `x`, `y`, `z` hold the plotted values in the unit named by `x_unit` /
/// `y_unit`; `x_si` / `y_si` repeat them in SI when the unit is a known
/// display unit. Bars use `x2`/`y2` for the opposite corner; heatmaps put
/// cell centres in `x`/`y` and the cell value in `z`.
pub fn csv_string(data: &PlotData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# schema: {}", data.schema);
    let _ = writeln!(out, "# plot_id: {}", one_line(&data.plot_id));
    let _ = writeln!(out, "# title: {}", one_line(&data.title));
    if let Some(run) = &data.run_id {
        let _ = writeln!(out, "# run_id: {}", one_line(run));
    }
    let _ = writeln!(out, "# generator: {}", one_line(&data.generator));
    for note in &data.notes {
        let _ = writeln!(out, "# note: {}", one_line(note));
    }
    let _ = writeln!(
        out,
        "# values are the plotted numbers in the unit given by x_unit/y_unit; x_si/y_si are SI (empty when the unit is not a known display unit)"
    );
    for (index, panel) in data.panels.iter().enumerate() {
        let n = index + 1;
        let _ = writeln!(
            out,
            "# panel {n}: title={} | x_label={} | y_label={} | x_scale={} | y_scale={} | x_range={:?}..{:?} | y_range={:?}..{:?}",
            one_line(panel.title.as_deref().unwrap_or("")),
            one_line(panel.x_label.as_deref().unwrap_or("")),
            one_line(panel.y_label.as_deref().unwrap_or("")),
            scale_text(panel.x_scale),
            scale_text(panel.y_scale),
            panel.x_range[0],
            panel.x_range[1],
            panel.y_range[0],
            panel.y_range[1],
        );
    }
    let _ = writeln!(out, "{COLUMNS}");
    for (panel_index, panel) in data.panels.iter().enumerate() {
        let x_unit = panel.x_unit.clone().unwrap_or_default();
        let y_unit = panel.y_unit.clone().unwrap_or_default();
        for (series_index, series) in panel.series.iter().enumerate() {
            for i in 0..series.len() {
                let get = |values: &[f64]| values.get(i).map(|v| number(*v)).unwrap_or_default();
                let si = |value: f64, conv: &Option<super::SiConversion>| {
                    conv.as_ref()
                        .map(|c| number(value * c.factor))
                        .unwrap_or_default()
                };
                let _ = writeln!(
                    out,
                    "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                    panel_index + 1,
                    series_index + 1,
                    quote(&series.name),
                    series.kind.as_str(),
                    i,
                    number(series.x[i]),
                    series.y.get(i).map(|v| number(*v)).unwrap_or_default(),
                    get(&series.z),
                    get(&series.x2),
                    get(&series.y2),
                    quote(&x_unit),
                    quote(&y_unit),
                    si(series.x[i], &panel.x_si),
                    series
                        .y
                        .get(i)
                        .map_or(String::new(), |v| si(*v, &panel.y_si)),
                    quote(series.labels.get(i).map_or("", String::as_str)),
                );
            }
        }
    }
    out
}

fn split_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => fields.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    fields.push(current);
    fields
}

fn kind_from(text: &str) -> Result<SeriesKind, ParseError> {
    Ok(match text {
        "line" => SeriesKind::Line,
        "polygon" => SeriesKind::Polygon,
        "points" => SeriesKind::Points,
        "segments" => SeriesKind::Segments,
        "bars" => SeriesKind::Bars,
        "heatmap" => SeriesKind::Heatmap,
        other => return Err(ParseError(format!("unknown series kind '{other}'"))),
    })
}

fn float(text: &str, what: &str) -> Result<f64, ParseError> {
    text.parse::<f64>()
        .map_err(|_| ParseError(format!("bad {what} value '{text}'")))
}

/// Parse the identifiers, units and series arrays from [`csv_string`] output.
pub(super) fn parse_csv(text: &str) -> Result<PlotData, ParseError> {
    let mut data = PlotData::new("", "", None);
    data.schema.clear();
    let mut in_rows = false;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# ") {
            for (key, target) in [
                ("schema: ", 0),
                ("plot_id: ", 1),
                ("title: ", 2),
                ("run_id: ", 3),
                ("generator: ", 4),
                ("note: ", 5),
            ] {
                if let Some(value) = rest.strip_prefix(key) {
                    match target {
                        0 => data.schema = value.to_owned(),
                        1 => data.plot_id = value.to_owned(),
                        2 => data.title = value.to_owned(),
                        3 => data.run_id = Some(value.to_owned()),
                        4 => data.generator = value.to_owned(),
                        _ => data.notes.push(value.to_owned()),
                    }
                }
            }
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        if !in_rows {
            if line != COLUMNS {
                return Err(ParseError(format!("unexpected header '{line}'")));
            }
            in_rows = true;
            continue;
        }
        let f = split_fields(line);
        if f.len() != 15 {
            return Err(ParseError(format!("expected 15 fields, got {}", f.len())));
        }
        let panel_n: usize = f[0].parse().map_err(|_| ParseError("bad panel".into()))?;
        let series_n: usize = f[1].parse().map_err(|_| ParseError("bad series".into()))?;
        while data.panels.len() < panel_n {
            data.panels.push(PlotPanel::new("", ""));
        }
        let panel = &mut data.panels[panel_n - 1];
        if panel.x_unit.is_none() && !f[10].is_empty() {
            panel.x_unit = Some(f[10].clone());
        }
        if panel.y_unit.is_none() && !f[11].is_empty() {
            panel.y_unit = Some(f[11].clone());
        }
        while panel.series.len() < series_n {
            panel.series.push(PlotSeries::from_xy(
                f[2].clone(),
                kind_from(&f[3])?,
                Vec::new(),
                Vec::new(),
            ));
        }
        let series = &mut panel.series[series_n - 1];
        series.x.push(float(&f[5], "x")?);
        if !f[14].is_empty() {
            series.labels.push(f[14].clone());
        }
        if !f[6].is_empty() {
            series.y.push(float(&f[6], "y")?);
        }
        for (column, target) in [(7, 0), (8, 1), (9, 2)] {
            if !f[column].is_empty() {
                let value = float(&f[column], "extra")?;
                match target {
                    0 => series.z.push(value),
                    1 => series.x2.push(value),
                    _ => series.y2.push(value),
                }
            }
        }
    }
    if data.schema != SCHEMA {
        return Err(ParseError(format!("unsupported schema '{}'", data.schema)));
    }
    Ok(data)
}

/// File stem safe on every platform: ASCII letters, digits, `-` and `_`.
pub fn safe_file_stem(id: &str) -> String {
    let stem: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if stem.is_empty() {
        "plot".to_owned()
    } else {
        stem
    }
}

/// Files for a set of plots: `<stem>.csv` and `<stem>.json` per plot, plus
/// `index.csv`, `index.json` and `README.txt`. Returns `(file name, bytes)`.
pub fn bundle_files(plots: &[PlotData]) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut stems: Vec<String> = Vec::new();
    let mut index_csv = String::from("plot_id,title,panels,series,rows,csv_file,json_file\n");
    let mut index_json: Vec<serde_json::Value> = Vec::new();
    for plot in plots {
        let base = safe_file_stem(crate::export_file_stem(&plot.plot_id));
        let mut stem = base.clone();
        let mut suffix = 2;
        while stems.contains(&stem) {
            stem = format!("{base}_{suffix}");
            suffix += 1;
        }
        stems.push(stem.clone());
        let (csv_name, json_name) = (format!("{stem}.csv"), format!("{stem}.json"));
        let _ = writeln!(
            index_csv,
            "{},{},{},{},{},{},{}",
            quote(&plot.plot_id),
            quote(&plot.title),
            plot.panels.len(),
            plot.series_count(),
            plot.row_count(),
            csv_name,
            json_name
        );
        index_json.push(serde_json::json!({
            "plot_id": plot.plot_id,
            "title": plot.title,
            "run_id": plot.run_id,
            "panels": plot.panels.len(),
            "series": plot.series_count(),
            "rows": plot.row_count(),
            "csv_file": csv_name,
            "json_file": json_name,
        }));
        files.push((csv_name, csv_string(plot).into_bytes()));
        files.push((json_name, plot.to_json().into_bytes()));
    }
    files.push(("index.csv".to_owned(), index_csv.into_bytes()));
    files.push((
        "index.json".to_owned(),
        serde_json::to_string_pretty(&index_json)
            .unwrap_or_default()
            .into_bytes(),
    ));
    files.push(("README.txt".to_owned(), README.as_bytes().to_vec()));
    files
}

const README: &str = "ALAS plot data export\n\
\n\
One CSV and one JSON file per plot; index.csv / index.json list them.\n\
CSV: tidy long format, one row per data point; '#' lines carry metadata.\n\
  x, y, z are the plotted values in the unit named by x_unit / y_unit.\n\
  x_si, y_si repeat them in SI when the unit is a known display unit (empty otherwise).\n\
JSON: full fidelity (labels, units, SI factors, axis scales and ranges, series colours,\n\
  heatmap cell edges). Non-finite numbers are written as the strings NaN / inf / -inf.\n\
Series kinds: line, polygon (vertices), points (markers), segments (consecutive point pairs),\n\
  bars (x,y one corner and x2,y2 the opposite corner), heatmap (cell centre in x,y; value in z).\n";
