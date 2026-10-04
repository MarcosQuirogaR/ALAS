// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Product SOL 101 curve extraction with explicit F06 subcase identity.
//! The historical positional print reader remains unchanged. This adapter
//! requires each displacement page's SUBCASE banner and rejects ambiguous
//! or repeated GRID rows rather than silently assigning positional cases.

use crate::loads::LoadCase;
use crate::mesh::{Deck, MeshNodeIndex};
use crate::nastran::static_spanwise::{extract, StaticSpanwiseResponse, StaticTables};
use std::collections::BTreeMap;

/// Extract complete front-spar translations and rotations from solver print.
/// Every displacement page must identify its subcase; absence is an error.
pub fn read_static_spanwise_print(
    print: &str,
    deck: &Deck,
    index: &MeshNodeIndex,
    cases: &[LoadCase],
) -> StaticSpanwiseResponse {
    match named_tables(print) {
        Ok(tables) => extract(deck, index, cases, &tables),
        Err(error) => StaticSpanwiseResponse::invalid(error),
    }
}

fn named_tables(print: &str) -> Result<StaticTables, String> {
    let mut sections = BTreeMap::<i64, String>::new();
    let mut pending = None;
    let mut active = None;
    for line in crate::nastran::text::splitlines(print) {
        if let Some((_, tail)) = line.rsplit_once("SUBCASE") {
            // Excludes summaries such as "FOR SUBCASE NUMBER 1". Real
            // result-page banners end in "SUBCASE 1" in both dialects.
            if let Some(id) = tail
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<i64>().ok())
            {
                pending = Some(id);
            }
        }
        if line.contains("D I S P L A C E M E N T   V E C T O R") {
            active = Some(
                pending
                    .take()
                    .ok_or("displacement page lacks explicit SUBCASE identity")?,
            );
        }
        if let Some(id) = active {
            let section = sections.entry(id).or_default();
            section.push_str(line);
            section.push('\n');
        }
    }
    let mut tables = BTreeMap::new();
    for (id, text) in sections {
        let mut parsed = super::read_displacement_tables(&text);
        if parsed.len() != 1 {
            return Err(format!(
                "SUBCASE {id} has missing or repeated displacement table rows"
            ));
        }
        tables.insert(id, parsed.remove(0));
    }
    Ok(tables)
}
