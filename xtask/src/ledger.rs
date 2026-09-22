// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The check that keeps `docs/PORTING.md` honest.
//!
//! The ledger is only useful if it describes the tree as it is. The direction
//! checked here is that no crate exists without a row: code may not appear
//! without a statement of where it came from and what it must agree with.
//!
//! The opposite direction is not checked, because a row naming a crate that
//! does not exist yet is the normal state of a plan: most rows are `todo`,
//! and they are the schedule.

use std::path::Path;

pub fn check(root: &Path) -> Result<Vec<String>, String> {
    let ledger_path = root.join("docs").join("PORTING.md");
    let ledger = std::fs::read_to_string(&ledger_path)
        .map_err(|e| format!("cannot read {}: {e}", ledger_path.display()))?;

    let crates_dir = root.join("crates");
    if !crates_dir.is_dir() {
        return Ok(Vec::new());
    }

    let entries = std::fs::read_dir(&crates_dir)
        .map_err(|e| format!("cannot read {}: {e}", crates_dir.display()))?;

    let mut findings = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot read {}: {e}", crates_dir.display()))?;
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if !ledger.contains(&name) {
            findings.push(format!(
                "docs/PORTING.md: no row mentions the crate `{name}`"
            ));
        }
    }
    findings.sort();
    Ok(findings)
}
