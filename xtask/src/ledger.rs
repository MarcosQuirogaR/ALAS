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
        if !mentions_crate(&ledger, &name) {
            findings.push(format!(
                "docs/PORTING.md: no row mentions the crate `{name}`"
            ));
        }
    }
    findings.sort();
    Ok(findings)
}

/// Whether `ledger` names `crate_name` as a whole identifier. A plain substring
/// test would let `alas-geometry` stand in for a crate called `alas-geo`.
fn mentions_crate(ledger: &str, crate_name: &str) -> bool {
    let is_name_char = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    ledger.match_indices(crate_name).any(|(start, matched)| {
        let before = ledger[..start].chars().next_back();
        let after = ledger[start + matched.len()..].chars().next();
        !before.is_some_and(is_name_char) && !after.is_some_and(is_name_char)
    })
}

#[cfg(test)]
mod tests {
    use super::mentions_crate;

    #[test]
    fn a_crate_is_mentioned_only_as_a_whole_name() {
        let ledger = "| `alas-geometry` | green |\n| x | `alas-mass::fuel` | todo |";
        assert!(mentions_crate(ledger, "alas-geometry"));
        assert!(mentions_crate(ledger, "alas-mass"));
        assert!(!mentions_crate(ledger, "alas-geo"));
        assert!(!mentions_crate(ledger, "alas"));
        assert!(!mentions_crate(ledger, "geometry"));
    }
}
