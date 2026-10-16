// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The curated kit: facts hand-entered from manufacturer pages and datasheets,
//! each field cited. Licence of the data: CC BY 4.0 (see `LICENSE-DATA`).
//!
//! Only sources whose `redistribution` is `facts_with_citation` or
//! `permission_granted` may be cited; `Database::validate_for_shipping` and a
//! test enforce it. Raw tables, datasheets, images and price histories are
//! never copied into the bundle.

use std::sync::OnceLock;

use crate::database::Database;
use crate::error::{DbError, DbResult};

const SHARDS: [&str; 3] = [
    include_str!("../data/kit/sources.json"),
    include_str!("../data/kit/motors.json"),
    include_str!("../data/kit/avionics.json"),
];

/// Build the kit from its embedded shards (validated, shippable).
pub fn load() -> DbResult<Database> {
    let mut db = Database::new();
    for shard in SHARDS {
        db.merge(crate::import::parse_shard(shard)?)?;
    }
    let report = db.validate_for_shipping();
    if report.is_ok() {
        Ok(db)
    } else {
        Err(DbError::new(report.error_text()))
    }
}

/// The kit, built once.
pub fn kit() -> Result<&'static Database, &'static DbError> {
    static KIT: OnceLock<DbResult<Database>> = OnceLock::new();
    KIT.get_or_init(load).as_ref()
}
