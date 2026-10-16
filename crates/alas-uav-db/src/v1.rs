// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Converter from the v1 catalogue (`alas-uav/data/*.json`, per-record
//! provenance) to schema v2 (per-field provenance).
//!
//! Every v2 field inherits the record's source, is flagged `confidence = 0.5`
//! until re-verified, and keeps the v1 value in `raw` (`v1 mass_kg=0.045`).
//! The v1 files carry no retrieval date, so `retrieved_at` is the caller's
//! best knowledge (or `None`). Sources are created as `Unreviewed`, which the
//! shipping gate refuses: converted retailer data is for local analysis, not
//! for the public bundle. Interpretation decisions:
//!
//! - Motor `max_current_a` is mapped to `i_max_burst_a`, not to the continuous
//!   rating, because v1 states it as "a published peak limit" without duration.
//! - Motor `dimensions` are a bounding cylinder: `diameter = width_m`,
//!   `length = height_m` (axial); a length/width mismatch is a warning.
//! - Electronics `max_current_a` becomes `i_max_a` (a rating, not a draw).
//! - Servo operating points become torque, speed and stall-current curves
//!   over supply voltage.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

mod spec;
use spec::convert_spec;

use crate::component::{Component, SCHEMA_VERSION};
use crate::database::Database;
use crate::date::IsoDate;
use crate::error::{DbError, DbResult};
use crate::field::Method;
use crate::ids::{slugify, SourceId};
use crate::source::{Redistribution, Source, SourceKind};

/// Conversion parameters.
#[derive(Debug, Clone)]
pub struct V1Options {
    /// Retrieval date to stamp on every field, if known.
    pub retrieved_at: Option<IsoDate>,
    /// Date for `created_at`, `updated_at` and `valid_from`.
    pub today: IsoDate,
}

/// Converted database and the conversion notes.
#[derive(Debug, Clone)]
pub struct V1Conversion {
    /// The v2 database (sources and components).
    pub database: Database,
    /// Fields or records that could not be represented losslessly.
    pub warnings: Vec<String>,
}

struct Ctx {
    source: SourceId,
    retrieved_at: Option<IsoDate>,
    method: Method,
}

impl Ctx {
    fn prov(&self, raw: String) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert("source_id".into(), json!(self.source));
        map.insert("retrieved_at".into(), json!(self.retrieved_at));
        map.insert("method".into(), json!(self.method));
        map.insert("raw".into(), json!(raw));
        map.insert("confidence".into(), json!(0.5));
        map
    }

    fn field(&self, value: Value, unit_si: &str, raw: String) -> Value {
        let mut map = self.prov(raw);
        map.insert("value".into(), value);
        map.insert("unit_si".into(), json!(unit_si));
        Value::Object(map)
    }

    fn curve(
        &self,
        x_name: &str,
        x_unit: &str,
        y_unit: &str,
        x: Vec<f64>,
        y: Vec<f64>,
        raw: String,
    ) -> Value {
        let mut map = self.prov(raw);
        map.insert("x_name".into(), json!(x_name));
        map.insert("x_unit_si".into(), json!(x_unit));
        map.insert("y_unit_si".into(), json!(y_unit));
        map.insert("x".into(), json!(x));
        map.insert("y".into(), json!(y));
        Value::Object(map)
    }
}

fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn host_of(url: &str) -> &str {
    url.trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or("unknown")
}

/// Convert a v1 catalogue JSON text.
pub fn convert_v1(json_text: &str, options: &V1Options) -> DbResult<V1Conversion> {
    let root: Value = serde_json::from_str(json_text)
        .map_err(|e| DbError::new(format!("v1 catalogue is not valid JSON: {e}")))?;
    if root.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err(DbError::new(
            "not a v1 catalogue (schema_version must be 1)",
        ));
    }
    let records = root
        .get("records")
        .and_then(Value::as_array)
        .ok_or_else(|| DbError::new("v1 catalogue has no records array"))?;
    let mut database = Database::new();
    let mut warnings = Vec::new();
    let mut sources: BTreeMap<String, SourceId> = BTreeMap::new();
    for record in records {
        let id = record
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_owned();
        match convert_record(record, options, &mut sources, &mut database, &mut warnings) {
            Ok(()) => {}
            Err(error) => warnings.push(format!("{id}: record skipped: {}", error.message())),
        }
    }
    Ok(V1Conversion { database, warnings })
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn convert_record(
    record: &Value,
    options: &V1Options,
    sources: &mut BTreeMap<String, SourceId>,
    database: &mut Database,
    warnings: &mut Vec<String>,
) -> DbResult<()> {
    let missing = |what: &str| DbError::new(format!("v1 record has no {what}"));
    let id = text(record, "id").ok_or_else(|| missing("id"))?;
    let manufacturer = text(record, "manufacturer").ok_or_else(|| missing("manufacturer"))?;
    let model = text(record, "model").ok_or_else(|| missing("model"))?;
    let kind = text(record, "kind").ok_or_else(|| missing("kind"))?;
    let spec = record.get("spec").ok_or_else(|| missing("spec"))?;
    let prov = record
        .get("provenance")
        .ok_or_else(|| missing("provenance"))?;
    let url = text(prov, "source_url").ok_or_else(|| missing("provenance.source_url"))?;
    let publisher = text(prov, "publisher").unwrap_or("unknown");

    let is_manufacturer = publisher.eq_ignore_ascii_case(manufacturer);
    let source_id = match sources.get(url) {
        Some(existing) => existing.clone(),
        None => {
            let slug = format!(
                "v1-{}-{:08x}",
                slugify(host_of(url)),
                fnv1a(url) & 0xffff_ffff
            );
            let source_id = SourceId::new(&slug)?;
            let notes = prov
                .get("transformations")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" | ")
                })
                .unwrap_or_default();
            database.sources.push(Source {
                id: source_id.clone(),
                url: url.to_owned(),
                kind: if is_manufacturer {
                    SourceKind::Manufacturer
                } else {
                    SourceKind::Retailer
                },
                licence: "none stated".into(),
                redistribution: Redistribution::Unreviewed,
                tos_checked_at: None,
                robots_checked_at: None,
                content_hash: None,
                citation: format!(
                    "{publisher}: {} [v1 notes: {notes}]",
                    text(prov, "source_title").unwrap_or("untitled")
                ),
            });
            sources.insert(url.to_owned(), source_id.clone());
            source_id
        }
    };
    let ctx = Ctx {
        source: source_id,
        retrieved_at: options.retrieved_at.clone(),
        method: if is_manufacturer {
            Method::Datasheet
        } else {
            Method::Retailer
        },
    };
    let (category, members) = convert_spec(id, kind, spec, &ctx, warnings)?;
    let row = json!({
        "id": id,
        "mpn": model,
        "manufacturer": manufacturer,
        "status": "active",
        "schema_version": SCHEMA_VERSION,
        "valid_from": options.today,
        "valid_to": null,
        "created_at": options.today,
        "updated_at": options.today,
        "category": category,
        "spec": members,
    });
    let component: Component = serde_json::from_value(row)
        .map_err(|e| DbError::new(format!("converted row does not fit schema v2: {e}")))?;
    database.components.push(component);
    Ok(())
}
