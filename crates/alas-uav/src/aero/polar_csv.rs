// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Imported 2-D section polars (decision D7: NeuralFoil-native polars plus
//! CSV import; no XFOIL/MSES integration).
//!
//! # Accepted text
//!
//! * Delimiters: comma, semicolon, tab or runs of blanks (detected per file).
//! * Lines starting with `#` or `%` and blank lines are ignored.  A line made
//!   only of dashes (the XFOIL/XFLR5 table rule) is ignored.
//! * The header is the first line whose cells include an angle column
//!   (`alpha`, `alpha_deg`, `aoa`, `a`) and a lift column (`cl`).  Drag
//!   (`cd`) is required; moment (`cm`) and Reynolds number (`re`,
//!   `reynolds`) are optional.  Other columns (`CDp`, `Top_Xtr`, ...) are
//!   skipped.  Column names are case-insensitive.
//! * XFLR5/XFOIL polar exports carry the Reynolds number in a preamble line
//!   such as `Mach = 0.000  Re = 0.200 e 6  Ncrit = 9.000`; it is read when
//!   the table has no `re` column.  Otherwise the caller supplies
//!   `default_re`.
//!
//! # Units
//!
//! Angles are degrees in the file and radians inside [`PolarSet`]; `cl`,
//! `cd`, `cm` are dimensionless (`cm` about the quarter chord, nose-up
//! positive, as in the XFOIL convention; it is not re-referenced).
//!
//! # Lookup
//!
//! [`PolarTable::eval`] interpolates linearly in angle and in `ln Re`
//! between sets.  It never extrapolates: a query outside the table is
//! clamped to its edge and reported through [`Validity`] (alpha outside the
//! table, or Re outside the tabulated range by more than a factor 1.5).

use super::validity::{Flag, Severity, Validity};
use super::AeroError;

/// One polar at a single Reynolds number.
#[derive(Debug, Clone, PartialEq)]
pub struct PolarSet {
    /// Chord Reynolds number.
    pub re: f64,
    /// Angles of attack, rad, strictly increasing.
    pub alpha_rad: Vec<f64>,
    /// Lift coefficients.
    pub cl: Vec<f64>,
    /// Drag coefficients (> 0).
    pub cd: Vec<f64>,
    /// Quarter-chord moment coefficients (zeros when the file has none).
    pub cm: Vec<f64>,
    /// Whether `cm` came from the file.
    pub has_cm: bool,
}

/// A section polar table: one or more [`PolarSet`]s ordered by Reynolds number.
#[derive(Debug, Clone, PartialEq)]
pub struct PolarTable {
    /// Display name (file name or airfoil).
    pub name: String,
    /// Sets, strictly increasing in `re`.
    pub sets: Vec<PolarSet>,
}

/// Interpolated section coefficients.
#[derive(Debug, Clone, PartialEq)]
pub struct TablePoint {
    /// Lift coefficient.
    pub cl: f64,
    /// Drag coefficient.
    pub cd: f64,
    /// Quarter-chord moment coefficient.
    pub cm: f64,
    /// Domain findings.
    pub validity: Validity,
}

/// Re ratio beyond the tabulated range at which a query is flagged.
const RE_EXTRAPOLATION_FACTOR: f64 = 1.5;

fn csv_err(line: usize, message: impl Into<String>) -> AeroError {
    AeroError::Csv {
        line,
        message: message.into(),
    }
}

fn split_cells(line: &str, delimiter: Option<char>) -> Vec<String> {
    match delimiter {
        Some(d) => line.split(d).map(|c| c.trim().to_owned()).collect(),
        None => line.split_whitespace().map(str::to_owned).collect(),
    }
}

fn detect_delimiter(text: &str) -> Option<char> {
    for candidate in [',', ';', '\t'] {
        let hits = text
            .lines()
            .filter(|l| {
                let t = l.trim();
                !t.is_empty() && !t.starts_with('#') && !t.starts_with('%')
            })
            .filter(|l| l.contains(candidate))
            .count();
        if hits >= 2 {
            return Some(candidate);
        }
    }
    None
}

fn is_rule(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty() && t.chars().all(|c| c == '-' || c.is_whitespace())
}

fn column_kind(name: &str) -> Option<&'static str> {
    match name.trim().to_ascii_lowercase().as_str() {
        "alpha" | "alpha_deg" | "alpha(deg)" | "aoa" | "aoa_deg" | "a" => Some("alpha"),
        "cl" => Some("cl"),
        "cd" => Some("cd"),
        "cm" => Some("cm"),
        "re" | "reynolds" | "reynolds_number" => Some("re"),
        _ => None,
    }
}

/// Read `Re = 0.200 e 6` / `Re = 2.0e5` / `Re=200000` from a preamble line.
fn preamble_reynolds(line: &str) -> Option<f64> {
    // Normalise "re=0.2" and "re = 0.2" to separate tokens, then take the
    // number after a whole-word "re".
    let lower = line.to_ascii_lowercase().replace('=', " = ");
    let tokens: Vec<&str> = lower.split_whitespace().collect();
    let at = tokens.iter().position(|t| *t == "re")?;
    let mut rest = &tokens[at + 1..];
    if rest.first() == Some(&"=") {
        rest = &rest[1..];
    }
    let mut value: f64 = rest.first()?.parse().ok()?;
    if let Some(second) = rest.get(1) {
        if let Some(exp) = second.strip_prefix('e') {
            let exponent: i32 = if exp.is_empty() {
                rest.get(2)?.parse().ok()?
            } else {
                exp.parse().ok()?
            };
            value *= 10f64.powi(exponent);
        }
    }
    (value.is_finite() && value > 0.0).then_some(value)
}

impl PolarTable {
    /// Parse polar text.  `default_re` is used when neither a `re` column nor
    /// a preamble `Re =` is present.
    ///
    /// # Errors
    ///
    /// [`AeroError::Csv`] with the 1-based line for malformed content, and
    /// for tables without a Reynolds number, with fewer than three points
    /// per set, non-increasing angles, non-finite values or `cd <= 0`.
    pub fn parse(name: &str, text: &str, default_re: Option<f64>) -> Result<Self, AeroError> {
        let delimiter = detect_delimiter(text);
        let mut header: Option<Vec<Option<&'static str>>> = None;
        let mut preamble_re: Option<f64> = None;
        let mut rows: Vec<(usize, Vec<f64>)> = Vec::new();
        let mut kinds: Vec<Option<&'static str>> = Vec::new();
        for (index, raw) in text.lines().enumerate() {
            let line_no = index + 1;
            let line = raw.trim_start_matches('\u{feff}').trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('%') || is_rule(line) {
                continue;
            }
            if header.is_none() {
                let cells = split_cells(line, delimiter);
                let found: Vec<Option<&'static str>> =
                    cells.iter().map(|c| column_kind(c)).collect();
                if found.contains(&Some("alpha")) && found.contains(&Some("cl")) {
                    // Trailing unnamed columns (XFLR5 'Top Xtr' is two tokens)
                    // are dropped so short data rows are not rejected.
                    let used = found.iter().rposition(Option::is_some).map_or(0, |i| i + 1);
                    kinds = found[..used].to_vec();
                    header = Some(kinds.clone());
                } else if let Some(re) = preamble_reynolds(line) {
                    preamble_re = Some(re);
                }
                continue;
            }
            let cells = split_cells(line, delimiter);
            if cells.len() < kinds.len() {
                return Err(csv_err(line_no, "fewer cells than the header has columns"));
            }
            let mut values = Vec::with_capacity(kinds.len());
            for cell in cells.iter().take(kinds.len()) {
                let value: f64 = cell
                    .parse()
                    .map_err(|_| csv_err(line_no, format!("'{cell}' is not a number")))?;
                if !value.is_finite() {
                    return Err(csv_err(line_no, "non-finite value"));
                }
                values.push(value);
            }
            rows.push((line_no, values));
        }
        let header = header.ok_or_else(|| csv_err(1, "no header with alpha and cl columns"))?;
        let col = |kind: &str| header.iter().position(|k| *k == Some(kind));
        let alpha_col = col("alpha").ok_or_else(|| csv_err(1, "no alpha column"))?;
        let cl_col = col("cl").ok_or_else(|| csv_err(1, "no cl column"))?;
        let cd_col = col("cd").ok_or_else(|| csv_err(1, "no cd column"))?;
        let cm_col = col("cm");
        let re_col = col("re");
        if rows.is_empty() {
            return Err(csv_err(1, "no data rows"));
        }

        // Group by Reynolds number, preserving first-seen order.
        type Group<'a> = (f64, Vec<(usize, &'a Vec<f64>)>);
        let mut groups: Vec<Group<'_>> = Vec::new();
        for (line_no, values) in &rows {
            let re = match re_col {
                Some(c) => values[c],
                None => preamble_re.or(default_re).ok_or_else(|| {
                    csv_err(
                        *line_no,
                        "no Reynolds number: add a re column, a 'Re =' preamble or pass default_re",
                    )
                })?,
            };
            if !(re.is_finite() && re > 0.0) {
                return Err(csv_err(*line_no, "Reynolds number must be positive"));
            }
            match groups
                .iter_mut()
                .find(|(r, _)| (*r - re).abs() <= 1e-9 * re)
            {
                Some((_, members)) => members.push((*line_no, values)),
                None => groups.push((re, vec![(*line_no, values)])),
            }
        }

        let mut sets = Vec::with_capacity(groups.len());
        for (re, mut members) in groups {
            members.sort_by(|a, b| a.1[alpha_col].total_cmp(&b.1[alpha_col]));
            if members.len() < 3 {
                return Err(csv_err(
                    members[0].0,
                    format!("Re {re:.3e}: a polar needs at least three points"),
                ));
            }
            let mut set = PolarSet {
                re,
                alpha_rad: Vec::new(),
                cl: Vec::new(),
                cd: Vec::new(),
                cm: Vec::new(),
                has_cm: cm_col.is_some(),
            };
            for (line_no, values) in members {
                let alpha = values[alpha_col].to_radians();
                if set.alpha_rad.last().is_some_and(|last| alpha <= *last) {
                    return Err(csv_err(
                        line_no,
                        "duplicate or non-increasing angle of attack",
                    ));
                }
                let cd = values[cd_col];
                if cd <= 0.0 {
                    return Err(csv_err(line_no, "cd must be positive"));
                }
                if values[cl_col].abs() > 5.0 {
                    return Err(csv_err(line_no, "implausible |cl| > 5"));
                }
                set.alpha_rad.push(alpha);
                set.cl.push(values[cl_col]);
                set.cd.push(cd);
                set.cm.push(cm_col.map_or(0.0, |c| values[c]));
            }
            sets.push(set);
        }
        sets.sort_by(|a, b| a.re.total_cmp(&b.re));
        Ok(Self {
            name: name.to_owned(),
            sets,
        })
    }

    /// Smallest and largest tabulated Reynolds number.
    pub fn re_range(&self) -> (f64, f64) {
        (
            self.sets.first().map_or(f64::NAN, |s| s.re),
            self.sets.last().map_or(f64::NAN, |s| s.re),
        )
    }

    /// Alpha range common to every set, rad.
    pub fn alpha_range_rad(&self) -> (f64, f64) {
        let lo = self.sets.iter().map(|s| s.alpha_rad[0]);
        let hi = self.sets.iter().map(|s| s.alpha_rad[s.alpha_rad.len() - 1]);
        (
            lo.fold(f64::NEG_INFINITY, f64::max),
            hi.fold(f64::INFINITY, f64::min),
        )
    }

    fn interp_set(set: &PolarSet, alpha: f64, validity: &mut Validity) -> (f64, f64, f64) {
        let n = set.alpha_rad.len();
        let lo = set.alpha_rad[0];
        let hi = set.alpha_rad[n - 1];
        let a = if alpha < lo || alpha > hi {
            validity.push(Severity::Warn, Flag::OutsideTable(alpha.to_degrees()));
            alpha.clamp(lo, hi)
        } else {
            alpha
        };
        let k = set.alpha_rad.partition_point(|x| *x <= a).clamp(1, n - 1);
        let t = (a - set.alpha_rad[k - 1]) / (set.alpha_rad[k] - set.alpha_rad[k - 1]);
        let mix = |v: &[f64]| v[k - 1] + t * (v[k] - v[k - 1]);
        (mix(&set.cl), mix(&set.cd), mix(&set.cm))
    }

    /// Coefficients at `alpha_rad` and chord Reynolds number `re`.
    pub fn eval(&self, alpha_rad: f64, re: f64) -> TablePoint {
        let mut validity = Validity::default();
        let (re_lo, re_hi) = self.re_range();
        if re < re_lo / RE_EXTRAPOLATION_FACTOR || re > re_hi * RE_EXTRAPOLATION_FACTOR {
            validity.push(Severity::Warn, Flag::OutsideTable(re));
        }
        let n = self.sets.len();
        let (cl, cd, cm) = if n == 1 || re <= re_lo {
            Self::interp_set(&self.sets[0], alpha_rad, &mut validity)
        } else if re >= re_hi {
            Self::interp_set(&self.sets[n - 1], alpha_rad, &mut validity)
        } else {
            let k = self.sets.partition_point(|s| s.re <= re).clamp(1, n - 1);
            let (a, b) = (&self.sets[k - 1], &self.sets[k]);
            let t = (re.ln() - a.re.ln()) / (b.re.ln() - a.re.ln());
            let pa = Self::interp_set(a, alpha_rad, &mut validity);
            let mut sink = Validity::default();
            let pb = Self::interp_set(b, alpha_rad, &mut sink);
            (
                pa.0 + t * (pb.0 - pa.0),
                pa.1 + t * (pb.1 - pa.1),
                pa.2 + t * (pb.2 - pa.2),
            )
        };
        TablePoint {
            cl,
            cd,
            cm,
            validity,
        }
    }
}
