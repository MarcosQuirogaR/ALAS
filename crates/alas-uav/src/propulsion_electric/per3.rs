// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Parser for the fixed-width `PER3` propeller performance text layout.
//!
//! ALAS does not distribute any propeller performance table.  A user obtains
//! the files from the propeller maker (see `docs/propeller-data.md`) and this
//! parser turns each one into a bounded Ct/Cp map.  Layout read here:
//!
//! * the first non-blank line is `<D>x<P><suffix>   (<name>.dat)` with the
//!   diameter `D` and pitch `P` in inches;
//! * each `PROP RPM = <n>` line opens one shaft-speed block;
//! * a data row has at least five numeric columns: speed in mph, advance
//!   ratio, efficiency, Ct, Cp.  Only columns 1, 4 and 5 are used.
//!
//! Rows that do not parse as finite numbers (column headers, unit rows, `NaN`
//! entries) are skipped.  Speeds are converted from mph to m/s.

use super::fixture::{PropellerPerformanceMap, PropellerSample, RpmCurve};
use super::ElectricPropulsionError;

const MPH_TO_M_S: f64 = 0.447_04;
const INCH_TO_M: f64 = 0.0254;

/// Parse one `PER3` table.  `file_name` is the original file name; it keeps
/// the table traceable and supplies an optional `(variant)` suffix that makes
/// the catalogue identifier unique when two files print the same model.
pub fn parse_per3(
    text: &str,
    file_name: &str,
) -> Result<PropellerPerformanceMap, ElectricPropulsionError> {
    let fail = |detail: &str| {
        ElectricPropulsionError::InvalidInput(format!("propeller table {file_name}: {detail}"))
    };
    let heading_line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| fail("file is empty"))?;
    let model = heading_line
        .split('(')
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned();
    let (diameter_in, pitch_in) =
        diameter_and_pitch_inches(&model).ok_or_else(|| fail("no diameter and pitch in title"))?;

    let mut curves: Vec<RpmCurve> = Vec::new();
    let mut points: Vec<PropellerSample> = Vec::new();
    let mut rpm: Option<f64> = None;
    let mut finish = |rpm: Option<f64>, points: &mut Vec<PropellerSample>| {
        if let Some(rpm) = rpm {
            if !points.is_empty() {
                curves.push(RpmCurve {
                    rpm,
                    points: std::mem::take(points),
                });
            }
        }
        points.clear();
    };
    for line in text.lines() {
        if let Some(value) = rpm_marker(line) {
            finish(rpm, &mut points);
            rpm = Some(value);
            continue;
        }
        if rpm.is_none() {
            continue;
        }
        if let Some(sample) = data_row(line) {
            points.push(sample);
        }
    }
    finish(rpm, &mut points);

    let table = PropellerPerformanceMap::from_parts(
        table_identifier(&model, file_name),
        model,
        file_name.to_owned(),
        diameter_in * INCH_TO_M,
        pitch_in * INCH_TO_M,
        curves,
    );
    table.validate(file_name)?;
    Ok(table)
}

fn rpm_marker(line: &str) -> Option<f64> {
    let (_, tail) = line.split_once("PROP RPM")?;
    let value = tail.trim_start().strip_prefix('=')?.trim();
    value.split_whitespace().next()?.parse::<f64>().ok()
}

fn data_row(line: &str) -> Option<PropellerSample> {
    let mut fields = line.split_whitespace();
    let speed_mph: f64 = fields.next()?.parse().ok()?;
    let rest: Vec<&str> = fields.take(4).collect();
    if rest.len() < 4 {
        return None;
    }
    let thrust_coefficient: f64 = rest[2].parse().ok()?;
    let power_coefficient: f64 = rest[3].parse().ok()?;
    let sample = PropellerSample {
        speed_m_s: speed_mph * MPH_TO_M_S,
        thrust_coefficient,
        power_coefficient,
    };
    (speed_mph.is_finite() && thrust_coefficient.is_finite() && power_coefficient.is_finite())
        .then_some(sample)
}

/// Leading `<diameter>x<pitch>` of a model title, both in inches.
fn diameter_and_pitch_inches(model: &str) -> Option<(f64, f64)> {
    let (diameter, rest) = split_number(model.trim_start())?;
    let rest = rest.strip_prefix(['x', 'X'])?;
    let (pitch, _) = split_number(rest)?;
    (diameter > 0.0 && pitch > 0.0).then_some((diameter, pitch))
}

fn split_number(text: &str) -> Option<(f64, &str)> {
    let end = text
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(text.len());
    let value = text[..end].parse::<f64>().ok()?;
    Some((value, &text[end..]))
}

fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars().map(|c| c.to_ascii_lowercase()) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

/// Catalogue key: `apc-<model slug>[-<file variant slug>]`, for example
/// title `13x4.5EP` in `PER3_13x45EP(F2B).dat` gives `apc-13x4-5ep-f2b`.
fn table_identifier(model: &str, file_name: &str) -> String {
    let stem = file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem);
    let variant = stem
        .split_once('(')
        .and_then(|(_, tail)| tail.split_once(')'))
        .map(|(inner, _)| slug(inner))
        .filter(|inner| !inner.is_empty());
    match variant {
        Some(inner) => format!("apc-{}-{inner}", slug(model)),
        None => format!("apc-{}", slug(model)),
    }
}
