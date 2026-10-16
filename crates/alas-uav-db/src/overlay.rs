// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! User overlay: private or measured data layered over the shipped database.
//!
//! The overlay uses the same schema as the shipped bundle (`method` is
//! typically `user` or `measured`). Applying it never edits the base in place:
//! the result is a new [`Database`] plus an [`OverlayReport`] saying what was
//! added, which base revisions were closed by a newer user revision, and which
//! were replaced outright (same id and `valid_from`).

use std::path::Path;

use crate::component::Component;
use crate::database::Database;
use crate::date::IsoDate;
use crate::error::{DbError, DbResult};
use crate::ids::ComponentId;

/// What an overlay changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OverlayReport {
    /// Component ids that did not exist in the base.
    pub added: Vec<ComponentId>,
    /// Base revisions closed (`valid_to` set) by a newer user revision.
    pub closed: Vec<(ComponentId, IsoDate)>,
    /// Base revisions replaced by a user revision with the same `valid_from`.
    pub replaced: Vec<(ComponentId, IsoDate)>,
}

/// Layer `user` over `base`, returning the combined, validated database.
pub fn apply(base: &Database, user: Database) -> DbResult<(Database, OverlayReport)> {
    let mut out = base.clone();
    let mut report = OverlayReport::default();
    for source in user.sources {
        match out.sources.iter().find(|s| s.id == source.id) {
            Some(existing) if existing == &source => {}
            Some(_) => {
                return Err(DbError::new(format!(
                    "overlay source '{}' differs from the base source with the same id",
                    source.id.as_str()
                )))
            }
            None => out.sources.push(source),
        }
    }
    let mut rows = user.components;
    rows.sort_by(|a, b| (&a.id, &a.valid_from).cmp(&(&b.id, &b.valid_from)));
    for row in rows {
        insert_revision(&mut out, row, &mut report)?;
    }
    out.offers.extend(user.offers);
    out.prop_perf.extend(user.prop_perf);
    let findings = out.validate();
    if findings.is_ok() {
        Ok((out, report))
    } else {
        Err(DbError::new(format!(
            "overlay result is invalid: {}",
            findings.error_text()
        )))
    }
}

fn insert_revision(db: &mut Database, row: Component, report: &mut OverlayReport) -> DbResult<()> {
    let same_id: Vec<usize> = (0..db.components.len())
        .filter(|&i| db.components[i].id == row.id)
        .collect();
    if same_id.is_empty() {
        report.added.push(row.id.clone());
        db.components.push(row);
        return Ok(());
    }
    if db.components[same_id[0]].category() != row.category() {
        return Err(DbError::new(format!(
            "overlay changes the category of '{}'",
            row.id.as_str()
        )));
    }
    if let Some(&i) = same_id
        .iter()
        .find(|&&i| db.components[i].valid_from == row.valid_from)
    {
        report
            .replaced
            .push((row.id.clone(), row.valid_from.clone()));
        db.components[i] = row;
        return Ok(());
    }
    for &i in &same_id {
        let existing = &mut db.components[i];
        let overlaps = existing.valid_from < row.valid_from
            && existing
                .valid_to
                .as_ref()
                .is_none_or(|end| end > &row.valid_from);
        if overlaps {
            report
                .closed
                .push((existing.id.clone(), existing.valid_from.clone()));
            existing.valid_to = Some(row.valid_from.clone());
        }
    }
    db.components.push(row);
    Ok(())
}

/// Read every `*.json` file of `dir` (sorted by name) as an overlay shard,
/// merge the shards and apply them over `base`.
pub fn load_dir(base: &Database, dir: &Path) -> DbResult<(Database, OverlayReport)> {
    let mut names = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| {
        DbError::new(format!(
            "cannot read overlay directory {}: {e}",
            dir.display()
        ))
    })?;
    for entry in entries {
        let path = entry
            .map_err(|e| DbError::new(format!("cannot list {}: {e}", dir.display())))?
            .path();
        if path.extension().is_some_and(|ext| ext == "json") {
            names.push(path);
        }
    }
    names.sort();
    let mut merged = Database::new();
    for path in names {
        let text = std::fs::read_to_string(&path)
            .map_err(|e| DbError::new(format!("cannot read {}: {e}", path.display())))?;
        let shard: Database = serde_json::from_str(&text).map_err(|e| {
            DbError::new(format!("{} is not a database shard: {e}", path.display()))
        })?;
        merged.merge(shard)?;
    }
    apply(base, merged)
}
