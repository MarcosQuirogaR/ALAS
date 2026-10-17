// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! CSV import of component rows (one category per file).
//!
//! Header grammar, one column per member:
//!
//! - `id`, `mpn`, `manufacturer`, `status`, `valid_from`, `source_id`, `parts`
//!   (kits only, `;`-separated ids) and `notes` (ignored) are row metadata.
//! - A member column is its inventory stem or full name, optionally followed by
//!   a vendor unit in brackets: `kv [rpm/V]`, `mass [g]`, `rm [mOhm]`,
//!   `dims [mm]` (cell `100x33x31`). Without a bracket the SI unit is assumed.
//! - `<stem>.test_voltage [V]` and `<stem>.test_temp [degC]` attach test
//!   conditions to the member in the same row.
//! - Counts, text, booleans (`true/false/yes/no/1/0`) and `;`-separated text
//!   lists use plain columns.
//!
//! Curves and blade geometry are not importable from CSV (use JSON). Every
//! converted value keeps its raw cell text and unit in the provenance. A row
//! that fails is reported with its line number and skipped; a bad header fails
//! the whole file.

use serde_json::{json, Map, Value};

use super::csv;
use crate::component::{Category, Component, SCHEMA_VERSION};
use crate::date::IsoDate;
use crate::error::{DbError, DbResult};
use crate::field::Method;
use crate::ids::{slugify, SourceId};
use crate::inventory::{self, FieldInfo, FieldKind};
use crate::units::{quantity_of, to_si_as};

/// Parameters shared by every row of one import.
#[derive(Debug, Clone)]
pub struct ImportSpec {
    /// The category of every row in the file.
    pub category: Category,
    /// Source cited by rows without a `source_id` column.
    pub source_id: SourceId,
    /// Retrieval date of the source document, if known.
    pub retrieved_at: Option<IsoDate>,
    /// How the values were obtained.
    pub method: Method,
    /// Date stamped into `created_at`, `updated_at` and default `valid_from`.
    pub today: IsoDate,
    /// Field delimiter (`,` or `;`).
    pub delimiter: char,
}

/// A rejected row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowError {
    /// 1-based row number in the file (header is row 1).
    pub row: usize,
    /// Why the row was skipped.
    pub message: String,
}

/// Result of an import.
#[derive(Debug, Clone, Default)]
pub struct ImportOutput {
    /// Converted components.
    pub components: Vec<Component>,
    /// Skipped rows.
    pub rejected: Vec<RowError>,
}

#[derive(Debug)]
enum Role {
    Meta(&'static str),
    Member(&'static FieldInfo),
    TestVoltage(&'static FieldInfo),
    TestTemp(&'static FieldInfo),
}

struct Column {
    role: Role,
    unit: Option<String>,
}

const META: [&str; 8] = [
    "id",
    "mpn",
    "manufacturer",
    "status",
    "valid_from",
    "source_id",
    "parts",
    "notes",
];

fn parse_header(cell: &str, category: Category) -> DbResult<Column> {
    let cell = cell.trim();
    let (name, unit) = match cell.split_once('[') {
        Some((name, rest)) => {
            let unit = rest.strip_suffix(']').ok_or_else(|| {
                DbError::new(format!("header '{cell}' has an unclosed unit bracket"))
            })?;
            (name.trim(), Some(unit.trim().to_owned()))
        }
        None => (cell, None),
    };
    if let Some(meta) = META.iter().find(|m| **m == name) {
        return Ok(Column {
            role: Role::Meta(meta),
            unit,
        });
    }
    let (base, suffix) = match name.rsplit_once('.') {
        Some((base, s)) if s == "test_voltage" || s == "test_temp" => (base, Some(s)),
        _ => (name, None),
    };
    let info = inventory::fields(category)
        .iter()
        .find(|i| i.stem == base || i.name == base)
        .ok_or_else(|| {
            DbError::new(format!(
                "column '{name}' is not a {} member",
                category.key()
            ))
        })?;
    let role = match suffix {
        Some("test_voltage") => Role::TestVoltage(info),
        Some(_) => Role::TestTemp(info),
        None => Role::Member(info),
    };
    Ok(Column { role, unit })
}

fn number(cell: &str, delimiter: char) -> Result<f64, String> {
    let text = cell.trim();
    let text = if delimiter == ';' && !text.contains('.') {
        text.replace(',', ".")
    } else {
        text.to_owned()
    };
    text.parse::<f64>()
        .map_err(|_| format!("'{cell}' is not a number"))
        .and_then(|v| {
            if v.is_finite() {
                Ok(v)
            } else {
                Err(format!("'{cell}' is not finite"))
            }
        })
}

fn convert(
    info: &FieldInfo,
    cell: &str,
    unit: Option<&str>,
    delimiter: char,
) -> Result<f64, String> {
    let value = number(cell, delimiter)?;
    let unit = unit.unwrap_or(info.unit_si);
    let expected = quantity_of(info.unit_si)
        .ok_or_else(|| format!("inventory unit '{}' is unknown", info.unit_si))?;
    to_si_as(value, unit, expected).map_err(|e| e.to_string())
}

fn member_value(
    info: &FieldInfo,
    cell: &str,
    unit: Option<&str>,
    delimiter: char,
) -> Result<Value, String> {
    let cell = cell.trim();
    match info.kind {
        FieldKind::Num => Ok(json!(convert(info, cell, unit, delimiter)?)),
        FieldKind::Count => cell
            .parse::<u32>()
            .map(|v| json!(v))
            .map_err(|_| format!("'{cell}' is not a whole number")),
        FieldKind::Text => Ok(json!(cell)),
        FieldKind::TextList => Ok(json!(cell
            .split([';', '|'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>())),
        FieldKind::Bool => match cell.to_ascii_lowercase().as_str() {
            "true" | "yes" | "1" => Ok(json!(true)),
            "false" | "no" | "0" => Ok(json!(false)),
            _ => Err(format!("'{cell}' is not a boolean")),
        },
        FieldKind::Dims3 => {
            let parts: Vec<&str> = cell.split(['x', 'X']).collect();
            if parts.len() != 3 {
                return Err(format!("'{cell}' is not LxWxH"));
            }
            let mut dims = Vec::new();
            for part in parts {
                dims.push(convert(info, part, unit, delimiter)?);
            }
            Ok(json!(dims))
        }
        FieldKind::Curve | FieldKind::CurveList | FieldKind::Geometry => {
            Err(format!("'{}' cannot be imported from CSV", info.name))
        }
    }
}

/// Convert a CSV file of one category into components.
pub fn import_components(text: &str, spec: &ImportSpec) -> DbResult<ImportOutput> {
    let rows = csv::parse(text, spec.delimiter)?;
    let (header, body) = rows
        .split_first()
        .ok_or_else(|| DbError::new("CSV file has no header row"))?;
    let columns = header
        .iter()
        .map(|cell| parse_header(cell, spec.category))
        .collect::<DbResult<Vec<_>>>()?;
    let mut output = ImportOutput::default();
    for (index, row) in body.iter().enumerate() {
        let number = index + 2;
        match build_row(row, &columns, spec) {
            Ok(component) => output.components.push(component),
            Err(message) => output.rejected.push(RowError {
                row: number,
                message,
            }),
        }
    }
    Ok(output)
}

fn build_row(row: &[String], columns: &[Column], spec: &ImportSpec) -> Result<Component, String> {
    let mut meta: Map<String, Value> = Map::new();
    let mut members: Map<String, Value> = Map::new();
    let mut tests: Vec<(&FieldInfo, &'static str, f64)> = Vec::new();
    let mut row_source = None;
    for (column, cell) in columns.iter().zip(row.iter()) {
        let cell = cell.trim();
        if cell.is_empty() {
            continue;
        }
        match column.role {
            Role::Meta("notes") => {}
            Role::Meta("source_id") => row_source = Some(cell.to_owned()),
            Role::Meta("parts") => {
                let ids: Vec<&str> = cell
                    .split(';')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect();
                members.insert("parts".into(), json!(ids));
            }
            Role::Meta(name) => {
                meta.insert(name.into(), json!(cell));
            }
            Role::Member(info) => {
                let value = member_value(info, cell, column.unit.as_deref(), spec.delimiter)?;
                let unit = column.unit.as_deref().unwrap_or(info.unit_si);
                let mut field = Map::new();
                field.insert("value".into(), value);
                field.insert("unit_si".into(), json!(info.unit_si));
                let raw = if unit == "1" {
                    cell.to_owned()
                } else {
                    format!("{cell} {unit}")
                };
                field.insert("raw".into(), json!(raw));
                members.insert(info.name.into(), Value::Object(field));
            }
            Role::TestVoltage(info) => {
                tests.push((
                    info,
                    "voltage_v",
                    convert_test(cell, column, "V", spec.delimiter)?,
                ));
            }
            Role::TestTemp(info) => {
                tests.push((
                    info,
                    "temp_k",
                    convert_test(cell, column, "K", spec.delimiter)?,
                ));
            }
        }
    }
    let source = row_source.unwrap_or_else(|| spec.source_id.as_str().to_owned());
    for (_, field) in members.iter_mut().filter(|(k, _)| k.as_str() != "parts") {
        if let Value::Object(map) = field {
            map.insert("source_id".into(), json!(source));
            map.insert("retrieved_at".into(), json!(spec.retrieved_at));
            map.insert("method".into(), json!(spec.method));
            let mut confidence = spec.method.default_confidence();
            if spec.retrieved_at.is_none() {
                confidence = confidence.min(0.5);
            }
            map.insert("confidence".into(), json!(confidence));
        }
    }
    for (info, key, value) in tests {
        let Some(Value::Object(map)) = members.get_mut(info.name) else {
            return Err(format!(
                "test condition for '{}' has no value in the row",
                info.name
            ));
        };
        let test = map.entry("test").or_insert_with(|| json!({}));
        if let Value::Object(test) = test {
            test.insert(key.into(), json!(value));
        }
    }
    let text = |key: &str| meta.get(key).and_then(Value::as_str).map(str::to_owned);
    let mpn = text("mpn").ok_or("missing mpn")?;
    let manufacturer = text("manufacturer").ok_or("missing manufacturer")?;
    let id = text("id").unwrap_or_else(|| slugify(&format!("{manufacturer} {mpn}")));
    let valid_from = text("valid_from").unwrap_or_else(|| spec.today.as_str().to_owned());
    let value = json!({
        "id": id,
        "mpn": mpn,
        "manufacturer": manufacturer,
        "status": text("status").unwrap_or_else(|| "active".to_owned()),
        "schema_version": SCHEMA_VERSION,
        "valid_from": valid_from,
        "valid_to": null,
        "created_at": spec.today,
        "updated_at": spec.today,
        "category": spec.category.key(),
        "spec": members,
    });
    serde_json::from_value::<Component>(value).map_err(|e| e.to_string())
}

fn convert_test(cell: &str, column: &Column, si: &str, delimiter: char) -> Result<f64, String> {
    let value = number(cell, delimiter)?;
    let unit = column.unit.as_deref().unwrap_or(si);
    let quantity = quantity_of(si).ok_or("internal: test unit")?;
    to_si_as(value, unit, quantity).map_err(|e| e.to_string())
}
