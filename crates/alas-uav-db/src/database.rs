// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The database container: sources, component revisions, offers and
//! propeller maps, with lookup, merge and the shipping gate.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::component::{Category, Component, SCHEMA_VERSION};
use crate::date::IsoDate;
use crate::error::{DbError, DbResult};
use crate::ids::{ComponentId, SourceId};
use crate::offer::{Offer, PropPerfSet};
use crate::source::{Redistribution, Source, SourceKind};
use crate::validate::{validate, Report};

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

/// A complete database (also the on-disk bundle shape; shards share it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Database {
    /// Bundle schema version ([`SCHEMA_VERSION`]).
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// Cited sources.
    #[serde(default)]
    pub sources: Vec<Source>,
    /// Component revisions.
    #[serde(default)]
    pub components: Vec<Component>,
    /// Price and stock observations (never shipped in the public bundle).
    #[serde(default)]
    pub offers: Vec<Offer>,
    /// Propeller maps.
    #[serde(default)]
    pub prop_perf: Vec<PropPerfSet>,
}

impl Default for Database {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            sources: Vec::new(),
            components: Vec::new(),
            offers: Vec::new(),
            prop_perf: Vec::new(),
        }
    }
}

impl Database {
    /// An empty database.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse a bundle and reject it when validation reports any error.
    pub fn from_json(text: &str) -> DbResult<Self> {
        let db: Self = serde_json::from_str(text)
            .map_err(|e| DbError::new(format!("database JSON is invalid: {e}")))?;
        if db.schema_version != SCHEMA_VERSION {
            return Err(DbError::new(format!(
                "unsupported database schema version {} (expected {SCHEMA_VERSION}); convert v1 with the v1 module",
                db.schema_version
            )));
        }
        let report = db.validate();
        if report.is_ok() {
            Ok(db)
        } else {
            Err(DbError::new(report.error_text()))
        }
    }

    /// Pretty JSON with stable member order.
    pub fn to_json_pretty(&self) -> DbResult<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| DbError::new(format!("database cannot be serialized: {e}")))
    }

    /// Run all structural checks and cross-field sanity checks.
    pub fn validate(&self) -> Report {
        validate(self)
    }

    /// The source with `id`.
    pub fn source(&self, id: &SourceId) -> Option<&Source> {
        self.sources.iter().find(|s| &s.id == id)
    }

    /// Every revision of a component, in storage order.
    pub fn revisions<'a>(&'a self, id: &'a ComponentId) -> impl Iterator<Item = &'a Component> {
        self.components.iter().filter(move |c| &c.id == id)
    }

    /// The revision of `id` that applies on `date`.
    pub fn component_at<'a>(
        &'a self,
        id: &'a ComponentId,
        date: &IsoDate,
    ) -> Option<&'a Component> {
        self.revisions(id).find(|c| c.valid_on(date))
    }

    /// The open-ended revision of `id`, or the latest one if all are closed.
    pub fn current<'a>(&'a self, id: &'a ComponentId) -> Option<&'a Component> {
        let open = self.revisions(id).find(|c| c.valid_to.is_none());
        open.or_else(|| {
            self.revisions(id)
                .max_by(|a, b| a.valid_from.cmp(&b.valid_from))
        })
    }

    /// Components of a category that apply on `date`.
    pub fn by_category<'a>(
        &'a self,
        category: Category,
        date: &'a IsoDate,
    ) -> impl Iterator<Item = &'a Component> {
        self.components
            .iter()
            .filter(move |c| c.category() == category && c.valid_on(date))
    }

    /// Offers for a component, oldest first.
    pub fn offers_for(&self, id: &ComponentId) -> Vec<&Offer> {
        let mut offers: Vec<&Offer> = self
            .offers
            .iter()
            .filter(|o| &o.component_id == id)
            .collect();
        offers.sort_by(|a, b| a.observed_at.cmp(&b.observed_at));
        offers
    }

    /// Propeller maps for a component.
    pub fn prop_maps_for(&self, id: &ComponentId) -> Vec<&PropPerfSet> {
        self.prop_perf.iter().filter(|p| &p.prop_id == id).collect()
    }

    /// Append another database's rows. A source id present in both must be
    /// identical; a component revision present in both is a conflict.
    pub fn merge(&mut self, other: Database) -> DbResult<()> {
        for source in other.sources {
            match self.sources.iter().find(|s| s.id == source.id) {
                Some(existing) if existing == &source => {}
                Some(_) => {
                    return Err(DbError::new(format!(
                        "source '{}' is defined differently in two files",
                        source.id.as_str()
                    )))
                }
                None => self.sources.push(source),
            }
        }
        for component in other.components {
            let clash = self
                .components
                .iter()
                .any(|c| c.id == component.id && c.valid_from == component.valid_from);
            if clash {
                return Err(DbError::new(format!(
                    "component '{}' revision {} exists in both inputs",
                    component.id.as_str(),
                    component.valid_from.as_str()
                )));
            }
            self.components.push(component);
        }
        self.offers.extend(other.offers);
        self.prop_perf.extend(other.prop_perf);
        Ok(())
    }

    /// The gate for a bundle that ships in the public repository: every cited
    /// source must permit redistribution, no user-owned data, and no offers.
    pub fn validate_for_shipping(&self) -> Report {
        let mut report = self.validate();
        if !self.offers.is_empty() {
            report.error(
                "offers",
                "price observations are never shipped in the public bundle",
            );
        }
        let mut cited = BTreeSet::new();
        for component in &self.components {
            if let Ok(value) = serde_json::to_value(component) {
                collect_source_ids(&value, &mut cited);
            }
        }
        cited.extend(
            self.prop_perf
                .iter()
                .map(|p| p.source_id.as_str().to_owned()),
        );
        for id in cited {
            let Some(source) = self.sources.iter().find(|s| s.id.as_str() == id) else {
                continue;
            };
            if !source.redistribution.allows_shipping()
                || source.kind == SourceKind::User
                || source.redistribution == Redistribution::UserOwned
            {
                report.error(
                    &id,
                    format!(
                        "source is {:?} and may not be cited by a shipped bundle",
                        source.redistribution
                    ),
                );
            }
        }
        report
    }
}

fn collect_source_ids(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                match (key.as_str(), inner) {
                    ("source_id", Value::String(id)) => {
                        out.insert(id.clone());
                    }
                    _ => collect_source_ids(inner, out),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| collect_source_ids(item, out)),
        _ => {}
    }
}
