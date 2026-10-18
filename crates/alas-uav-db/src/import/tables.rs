// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! CSV import of offers and of propeller performance maps.
//!
//! Offers: columns `component_id`, `vendor`, `url`, `price`, `currency`, and
//! optionally `basis` (each, per_kg, per_metre, per_square_metre), `taxes_included`,
//! `stock`, `ship_from`, `observed_at`, `source_id`.
//!
//! Propeller maps: header names (case-insensitive) `rpm` or `omega_rad_s`,
//! `j`, `re`, `ct`, `cp`, `eta`. With delimiter `' '` the file is read as
//! whitespace-separated columns and lines starting with `#` are comments, which
//! covers the plain-text tables that measured and computed propeller databases
//! publish. A constant shaft speed for a whole file (one table per RPM) is given
//! with [`PerfImportSpec::rpm`] when the file has no speed column. Format
//! specific parsers (for example APC PER3 files) live next to their data in the
//! consuming crate; they end by building the same [`PropPerfSet`].

use super::csv;
use crate::date::IsoDate;
use crate::error::{DbError, DbResult};
use crate::ids::{ComponentId, SourceId};
use crate::offer::{Offer, PerfKind, PriceBasis, PropPerfRow, PropPerfSet};

fn lookup(header: &[String], name: &str) -> Option<usize> {
    header
        .iter()
        .position(|h| h.trim().eq_ignore_ascii_case(name))
}

fn cell(row: &[String], index: Option<usize>) -> Option<&str> {
    index
        .and_then(|i| row.get(i))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
}

fn parse_f64(text: &str, what: &str) -> DbResult<f64> {
    text.replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| DbError::new(format!("{what}: '{text}' is not a finite number")))
}

/// Parameters of an offers import.
#[derive(Debug, Clone)]
pub struct OfferImportSpec {
    /// Source cited by rows without a `source_id` column.
    pub source_id: SourceId,
    /// Observation date for rows without an `observed_at` column.
    pub observed_at: IsoDate,
    /// Field delimiter.
    pub delimiter: char,
}

/// Import offers; any bad row fails the whole file (prices are cheap to fix).
pub fn import_offers(text: &str, spec: &OfferImportSpec) -> DbResult<Vec<Offer>> {
    let rows = csv::parse(text, spec.delimiter)?;
    let (header, body) = rows
        .split_first()
        .ok_or_else(|| DbError::new("offers CSV has no header row"))?;
    let col = |name: &str| lookup(header, name);
    let mut offers = Vec::new();
    for (index, row) in body.iter().enumerate() {
        let what = format!("offers row {}", index + 2);
        let need = |name: &str| {
            cell(row, col(name)).ok_or_else(|| DbError::new(format!("{what}: missing '{name}'")))
        };
        let basis = match cell(row, col("basis")).unwrap_or("each") {
            "each" => PriceBasis::Each,
            "per_kg" => PriceBasis::PerKg,
            "per_metre" => PriceBasis::PerMetre,
            "per_square_metre" => PriceBasis::PerSquareMetre,
            other => return Err(DbError::new(format!("{what}: unknown basis '{other}'"))),
        };
        let taxes_included = match cell(row, col("taxes_included"))
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("true" | "yes" | "1") => true,
            Some("false" | "no" | "0") | None => false,
            Some(other) => return Err(DbError::new(format!("{what}: taxes_included '{other}'"))),
        };
        let stock = match cell(row, col("stock")) {
            Some(text) => Some(text.parse::<u32>().map_err(|_| {
                DbError::new(format!("{what}: stock '{text}' is not a whole number"))
            })?),
            None => None,
        };
        let observed_at = match cell(row, col("observed_at")) {
            Some(text) => IsoDate::parse(text)?,
            None => spec.observed_at.clone(),
        };
        let source_id = match cell(row, col("source_id")) {
            Some(text) => SourceId::new(text)?,
            None => spec.source_id.clone(),
        };
        let offer = Offer {
            component_id: ComponentId::new(need("component_id")?)?,
            vendor: need("vendor")?.to_owned(),
            url: need("url")?.to_owned(),
            price_amount: parse_f64(need("price")?, &what)?,
            basis,
            currency: need("currency")?.to_owned(),
            taxes_included,
            stock,
            ship_from: cell(row, col("ship_from")).map(str::to_owned),
            observed_at,
            source_id,
        };
        offer.validate()?;
        offers.push(offer);
    }
    Ok(offers)
}

/// Parameters of a propeller-map import.
#[derive(Debug, Clone)]
pub struct PerfImportSpec {
    /// The propeller the map belongs to.
    pub prop_id: ComponentId,
    /// Citing source.
    pub source_id: SourceId,
    /// Measured or computed.
    pub kind: PerfKind,
    /// Retrieval date, if known.
    pub retrieved_at: Option<IsoDate>,
    /// `,` or `;` for CSV, `' '` for whitespace-separated tables.
    pub delimiter: char,
    /// Constant shaft speed in rev/min for files without a speed column.
    pub rpm: Option<f64>,
}

/// Import a propeller map.
pub fn import_prop_perf(text: &str, spec: &PerfImportSpec) -> DbResult<PropPerfSet> {
    let rows = if spec.delimiter == ' ' {
        text.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| l.split_whitespace().map(str::to_owned).collect::<Vec<_>>())
            .collect()
    } else {
        csv::parse(text, spec.delimiter)?
    };
    let (header, body) = rows
        .split_first()
        .ok_or_else(|| DbError::new("propeller map has no header row"))?;
    let col = |name: &str| lookup(header, name);
    let (rpm_col, omega_col) = (col("rpm"), col("omega_rad_s"));
    if rpm_col.is_none() && omega_col.is_none() && spec.rpm.is_none() {
        return Err(DbError::new(
            "propeller map needs an rpm or omega_rad_s column, or a constant rpm",
        ));
    }
    let (j_col, ct_col, cp_col) = (col("j"), col("ct"), col("cp"));
    if j_col.is_none() || ct_col.is_none() || cp_col.is_none() {
        return Err(DbError::new("propeller map needs j, ct and cp columns"));
    }
    let to_omega = |rpm: f64| rpm * std::f64::consts::TAU / 60.0;
    let mut out = Vec::new();
    for (index, row) in body.iter().enumerate() {
        let what = format!("propeller map row {}", index + 2);
        let get = |c: Option<usize>, name: &str| -> DbResult<f64> {
            parse_f64(
                cell(row, c).ok_or_else(|| DbError::new(format!("{what}: missing '{name}'")))?,
                &what,
            )
        };
        let omega = match (omega_col, rpm_col, spec.rpm) {
            (Some(c), _, _) => get(Some(c), "omega_rad_s")?,
            (None, Some(c), _) => to_omega(get(Some(c), "rpm")?),
            (None, None, Some(rpm)) => to_omega(rpm),
            _ => return Err(DbError::new(format!("{what}: no shaft speed"))),
        };
        let optional = |name: &str| -> DbResult<Option<f64>> {
            cell(row, col(name))
                .map(|t| parse_f64(t, &what))
                .transpose()
        };
        out.push(PropPerfRow {
            omega_rad_s: omega,
            j: get(j_col, "j")?,
            re: optional("re")?,
            ct: get(ct_col, "ct")?,
            cp: get(cp_col, "cp")?,
            eta: optional("eta")?,
        });
    }
    let set = PropPerfSet {
        prop_id: spec.prop_id.clone(),
        source_id: spec.source_id.clone(),
        kind: spec.kind,
        retrieved_at: spec.retrieved_at.clone(),
        rows: out,
    };
    set.validate()?;
    Ok(set)
}
