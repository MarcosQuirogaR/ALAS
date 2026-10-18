// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Importers: CSV (components, offers, propeller maps) and JSON bundles.
//!
//! Network access is deliberately absent. Fetching happens outside this crate
//! (the collector and the user-side data tools); what arrives here is text the
//! caller already holds, so importing is deterministic and testable offline.

pub mod components;
pub mod csv;
pub mod tables;

use crate::database::Database;
use crate::error::{DbError, DbResult};

pub use components::{import_components, ImportOutput, ImportSpec, RowError};
pub use tables::{import_offers, import_prop_perf, OfferImportSpec, PerfImportSpec};

/// Parse a JSON bundle shard without running validation, for merging several
/// shards before the whole is validated.
pub fn parse_shard(text: &str) -> DbResult<Database> {
    serde_json::from_str(text).map_err(|e| DbError::new(format!("not a database shard: {e}")))
}
