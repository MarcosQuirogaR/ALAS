// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Structural validation of a [`Database`] against the field inventory.
//!
//! Errors make a database unloadable; warnings (cross-field sanity, unknown
//! retrieval dates) are reported but do not block use. The walk reads the
//! serialized form of each row, so the inventory table is the only place that
//! knows a member's unit and range.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::component::{Category, Component, SCHEMA_VERSION};
use crate::database::Database;
use crate::field::Curve;
use crate::inventory::{self, FieldInfo, FieldKind};
use crate::parts::PropGeometry;

/// Severity of a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// The database is not usable.
    Error,
    /// Suspicious but usable.
    Warning,
}

/// One validation finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Error or warning.
    pub severity: Severity,
    /// Where: component id, `offer[i]`, ...
    pub path: String,
    /// What is wrong.
    pub message: String,
}

/// All findings of one validation pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Findings in discovery order.
    pub findings: Vec<Finding>,
}

impl Report {
    pub(crate) fn error(&mut self, path: &str, message: impl Into<String>) {
        self.push(Severity::Error, path, message);
    }

    pub(crate) fn warn(&mut self, path: &str, message: impl Into<String>) {
        self.push(Severity::Warning, path, message);
    }

    fn push(&mut self, severity: Severity, path: &str, message: impl Into<String>) {
        self.findings.push(Finding {
            severity,
            path: path.to_owned(),
            message: message.into(),
        });
    }

    /// The error findings.
    pub fn errors(&self) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
    }

    /// The warning findings.
    pub fn warnings(&self) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Warning)
    }

    /// True when there is no error.
    pub fn is_ok(&self) -> bool {
        self.errors().next().is_none()
    }

    /// All error messages joined for display.
    pub fn error_text(&self) -> String {
        self.errors()
            .map(|f| format!("{}: {}", f.path, f.message))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// Validate the whole database.
pub fn validate(db: &Database) -> Report {
    let mut report = Report::default();
    let mut source_ids = BTreeSet::new();
    for source in &db.sources {
        if !source_ids.insert(source.id.as_str()) {
            report.error(source.id.as_str(), "duplicate source id");
        }
        if let Err(error) = source.validate() {
            report.error(source.id.as_str(), error.message());
        }
    }
    check_revisions(db, &mut report);
    for component in &db.components {
        check_component(db, component, &source_ids, &mut report);
    }
    for (i, offer) in db.offers.iter().enumerate() {
        let path = format!("offer[{i}]");
        if let Err(error) = offer.validate() {
            report.error(&path, error.message());
        }
        if db.revisions(&offer.component_id).next().is_none() {
            report.error(&path, "unknown component_id");
        }
        if !source_ids.contains(offer.source_id.as_str()) {
            report.error(&path, "unknown source_id");
        }
    }
    for (i, set) in db.prop_perf.iter().enumerate() {
        let path = format!("prop_perf[{i}]");
        if let Err(error) = set.validate() {
            report.error(&path, error.message());
        }
        match db.revisions(&set.prop_id).next() {
            Some(c) if c.category() == Category::Propeller => {}
            _ => report.error(&path, "prop_id is not a known propeller"),
        }
        if !source_ids.contains(set.source_id.as_str()) {
            report.error(&path, "unknown source_id");
        }
    }
    crate::sanity::check(db, &mut report);
    report
}

fn check_revisions(db: &Database, report: &mut Report) {
    let mut by_id: BTreeMap<&str, Vec<&Component>> = BTreeMap::new();
    for component in &db.components {
        by_id
            .entry(component.id.as_str())
            .or_default()
            .push(component);
    }
    for (id, mut rows) in by_id {
        rows.sort_by(|a, b| a.valid_from.cmp(&b.valid_from));
        if rows.iter().any(|c| c.category() != rows[0].category()) {
            report.error(id, "revisions of one id must share a category");
        }
        for pair in rows.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if a.valid_from == b.valid_from {
                report.error(id, "two revisions share valid_from");
            } else if a.valid_to.as_ref().is_none_or(|end| end > &b.valid_from) {
                report.error(
                    id,
                    "revision intervals overlap; close the earlier row with valid_to",
                );
            }
        }
    }
}

fn check_component(
    db: &Database,
    component: &Component,
    sources: &BTreeSet<&str>,
    report: &mut Report,
) {
    let id = component.id.as_str();
    if component.schema_version != SCHEMA_VERSION {
        report.error(id, format!("schema_version must be {SCHEMA_VERSION}"));
    }
    if component.mpn.trim().is_empty() || component.manufacturer.trim().is_empty() {
        report.error(id, "mpn and manufacturer are required");
    }
    if component
        .valid_to
        .as_ref()
        .is_some_and(|end| end <= &component.valid_from)
    {
        report.error(id, "valid_to must be after valid_from");
    }
    if component.updated_at < component.created_at {
        report.error(id, "updated_at precedes created_at");
    }
    let Ok(Value::Object(row)) = serde_json::to_value(component) else {
        report.error(id, "row cannot be serialized");
        return;
    };
    let empty = Value::Object(serde_json::Map::new());
    let spec = row.get("spec").unwrap_or(&empty);
    let category = component.category();
    if let Value::Object(members) = spec {
        for name in members.keys() {
            if inventory::field(category, name).is_none()
                && !(category == Category::Kit && name == "parts")
            {
                report.error(
                    id,
                    format!("member '{name}' is not in the {} inventory", category.key()),
                );
            }
        }
    }
    for info in inventory::fields(category) {
        let value = spec.get(info.name).unwrap_or(&Value::Null);
        check_member(id, info, value, sources, report);
    }
    if let (Some(Value::Array(parts)), true) = (spec.get("parts"), category == Category::Kit) {
        for part in parts.iter().filter_map(Value::as_str) {
            if !db.components.iter().any(|c| c.id.as_str() == part) {
                report.error(id, format!("kit part '{part}' is not in the database"));
            }
        }
    }
}

fn check_member(
    id: &str,
    info: &FieldInfo,
    value: &Value,
    sources: &BTreeSet<&str>,
    report: &mut Report,
) {
    let path = format!("{id}.{}", info.name);
    match (info.kind, value) {
        (_, Value::Null) => {}
        (FieldKind::CurveList, Value::Array(items)) => {
            for (i, item) in items.iter().enumerate() {
                check_curve(&format!("{path}[{i}]"), info, item, sources, report);
            }
        }
        (FieldKind::Curve, _) => check_curve(&path, info, value, sources, report),
        (FieldKind::Geometry, _) => match serde_json::from_value::<PropGeometry>(value.clone()) {
            Ok(geometry) => {
                if let Err(error) = geometry.validate(&path) {
                    report.error(&path, error.message());
                }
                check_prov(&path, value, sources, report);
            }
            Err(error) => report.error(&path, format!("invalid geometry: {error}")),
        },
        _ => {
            if value.get("unit_si").and_then(Value::as_str) != Some(info.unit_si) {
                report.error(&path, format!("unit_si must be '{}'", info.unit_si));
            }
            check_value(&path, info, value.get("value"), report);
            check_prov(&path, value, sources, report);
        }
    }
}

fn check_value(path: &str, info: &FieldInfo, value: Option<&Value>, report: &mut Report) {
    let in_range = |x: f64| x.is_finite() && x >= info.min && x <= info.max;
    let ok = match (info.kind, value) {
        (FieldKind::Num | FieldKind::Count, Some(v)) => {
            v.as_f64().is_some_and(in_range)
                && (info.kind != FieldKind::Count || v.as_u64().is_some())
        }
        (FieldKind::Dims3, Some(Value::Array(xs))) => {
            xs.len() == 3 && xs.iter().all(|x| x.as_f64().is_some_and(in_range))
        }
        (FieldKind::Text, Some(Value::String(s))) => !s.trim().is_empty(),
        (FieldKind::TextList, Some(Value::Array(xs))) => {
            !xs.is_empty()
                && xs
                    .iter()
                    .all(|x| x.as_str().is_some_and(|s| !s.trim().is_empty()))
        }
        (FieldKind::Bool, Some(Value::Bool(_))) => true,
        _ => false,
    };
    if !ok {
        report.error(
            path,
            format!(
                "value is missing, of the wrong type or outside [{}, {}] {}",
                info.min, info.max, info.unit_si
            ),
        );
    }
}

fn check_curve(
    path: &str,
    info: &FieldInfo,
    value: &Value,
    sources: &BTreeSet<&str>,
    report: &mut Report,
) {
    match serde_json::from_value::<Curve>(value.clone()) {
        Ok(curve) => {
            if let Err(error) = curve.validate(path) {
                report.error(path, error.message());
            }
            if curve.y_unit_si != info.unit_si || curve.x_unit_si != info.x_unit_si {
                report.error(
                    path,
                    format!(
                        "curve units must be x '{}' and y '{}'",
                        info.x_unit_si, info.unit_si
                    ),
                );
            }
            check_prov(path, value, sources, report);
        }
        Err(error) => report.error(path, format!("invalid curve: {error}")),
    }
}

fn check_prov(path: &str, value: &Value, sources: &BTreeSet<&str>, report: &mut Report) {
    match value.get("source_id").and_then(Value::as_str) {
        Some(source) if sources.contains(source) => {}
        Some(source) => report.error(path, format!("unknown source_id '{source}'")),
        None => report.error(path, "source_id is missing"),
    }
    match value.get("confidence").and_then(Value::as_f64) {
        Some(c) if (0.0..=1.0).contains(&c) => {}
        _ => report.error(path, "confidence must be in [0, 1]"),
    }
    if value.get("retrieved_at").is_none_or(Value::is_null) {
        report.warn(path, "retrieved_at is unknown");
    }
}
