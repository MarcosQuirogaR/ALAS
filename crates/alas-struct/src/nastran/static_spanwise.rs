// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Strict product static curves on the complete sampled front-spar upper line.
//! Basic-frame translations are metres and rotations radians. Secants can
//! bound a continuous slope from below, but these samples do not establish
//! whole-model geometric linearity, mesh convergence or laminate strength.

use std::collections::{BTreeMap, HashSet};

use crate::loads::LoadCase;
use crate::mesh::{Deck, MeshNodeIndex};
use crate::op2::Op2;

use super::{read_static, LabelledValues, ResultStatus};

/// What a SOL 101 static solve reported.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StaticResult {
    /// Whether a solve returned a usable result.
    pub status: ResultStatus,
    /// Failure detail, including incomplete product spanwise output.
    pub error: Option<String>,
    /// Basic-Z tip deflection by load case, metres.
    pub tip_deflection_m: LabelledValues,
    /// Peak shell von Mises by case, Pa: MSC reported CQUAD4 corners or
    /// NASTRAN-95 invariant derived from printed shell-centroid tensors.
    /// This historical field name does not restrict its scope to the root.
    pub root_von_mises_max_pa: LabelledValues,
    /// Complete product front-spar response; readers of the summary alone leave it absent.
    pub spanwise: Option<StaticSpanwiseResponse>,
    /// Attributed product shell stresses, when that solver adapter supports it.
    pub shell_stress: Option<super::StaticShellStressResponse>,
}

/// Provenance of the assignment of output tables to requested load cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticCaseIdentity {
    /// Explicit OP2 subcase keys or F06 SUBCASE page banners.
    ExplicitSubcaseIds,
    /// A complete, unambiguous mapping could not be recovered.
    Unresolved,
}

/// One complete sampled front-spar response in the mesh's basic frame.
#[derive(Debug, Clone, PartialEq)]
pub struct StaticSpanwiseCase {
    /// Explicit positive subcase identifier.
    pub subcase_id: i64,
    /// Corresponding requested load-case name.
    pub name: &'static str,
    /// Signed applied load factor; maneuver values are ultimate.
    pub load_factor: f64,
    /// Front-spar upper GRID identifiers, sorted by increasing basic Y.
    pub grid_ids: Vec<i64>,
    /// Undeformed basic XYZ coordinates, metres.
    pub xyz_m: Vec<[f64; 3]>,
    /// Projected undeformed span stations, metres.
    pub y_m: Vec<f64>,
    /// Basic XYZ translations at each grid, metres; Z preserves solver sign.
    pub translations_m: Vec<[f64; 3]>,
    /// Basic XYZ rotations at each grid, radians.
    pub rotations_rad: Vec<[f64; 3]>,
}

/// Complete product response or a specific reason completeness is unavailable.
#[derive(Debug, Clone, PartialEq)]
pub struct StaticSpanwiseResponse {
    /// Curves in requested load-case order. Empty whenever extraction fails.
    pub cases: Vec<StaticSpanwiseCase>,
    /// Finiteness, identity, row completeness or geometry error, if present.
    pub error: Option<String>,
    /// Output-to-load-case assignment provenance.
    pub case_identity: StaticCaseIdentity,
}

impl StaticSpanwiseResponse {
    pub(crate) fn invalid(error: impl Into<String>) -> Self {
        Self {
            cases: Vec::new(),
            error: Some(error.into()),
            case_identity: StaticCaseIdentity::Unresolved,
        }
    }
}

pub(crate) type StaticTables = BTreeMap<i64, Vec<(i64, [f64; 6])>>;

pub(crate) fn extract(
    deck: &Deck,
    index: &MeshNodeIndex,
    cases: &[LoadCase],
    tables: &StaticTables,
) -> StaticSpanwiseResponse {
    match extract_cases(deck, index, cases, tables) {
        Ok(cases) => StaticSpanwiseResponse {
            cases,
            error: None,
            case_identity: StaticCaseIdentity::ExplicitSubcaseIds,
        },
        Err(error) => StaticSpanwiseResponse::invalid(error),
    }
}

fn extract_cases(
    deck: &Deck,
    index: &MeshNodeIndex,
    cases: &[LoadCase],
    tables: &StaticTables,
) -> Result<Vec<StaticSpanwiseCase>, String> {
    if cases.is_empty()
        || cases
            .iter()
            .map(|case| case.name)
            .collect::<HashSet<_>>()
            .len()
            != cases.len()
        || cases.iter().any(|case| !case.load_factor.is_finite())
        || tables.len() != cases.len()
        || tables.keys().copied().ne(1..=cases.len() as i64)
    {
        return Err("missing, duplicate or unexpected static load-case identity".into());
    }
    let ids = index
        .spar_upper_nids
        .first()
        .ok_or("missing front-spar node line")?;
    let unique: HashSet<_> = ids.iter().copied().collect();
    if unique.len() < 2
        || unique.len() != ids.len()
        || !unique.contains(&index.root_nid)
        || !unique.contains(&index.tip_nid)
    {
        return Err("front-spar line is duplicated, incomplete or has fewer than two grids".into());
    }
    let mut ordered = Vec::with_capacity(ids.len());
    for &id in ids {
        let xyz = deck
            .grid_xyz(id)
            .filter(|xyz| xyz.iter().all(|v| v.is_finite()))
            .ok_or_else(|| format!("front-spar GRID {id} coordinates unavailable/nonfinite"))?;
        ordered.push((id, xyz));
    }
    ordered.sort_by(|a, b| a.1[1].total_cmp(&b.1[1]));
    if ordered.first().map(|row| row.0) != Some(index.root_nid)
        || ordered.last().map(|row| row.0) != Some(index.tip_nid)
        || ordered.windows(2).any(|pair| pair[1].1[1] <= pair[0].1[1])
    {
        return Err("front-spar line does not run strictly from root to tip in basic Y".into());
    }
    cases
        .iter()
        .enumerate()
        .map(|(i, case)| {
            let sid = i as i64 + 1;
            let rows = &tables[&sid];
            let values: BTreeMap<_, _> = rows.iter().map(|(id, dof)| (*id, *dof)).collect();
            if values.len() != rows.len() {
                return Err(format!("duplicate static GRID rows in subcase {sid}"));
            }
            let mut translations = Vec::with_capacity(ordered.len());
            let mut rotations = Vec::with_capacity(ordered.len());
            for &(id, _) in &ordered {
                let dof = values
                    .get(&id)
                    .filter(|row| row.iter().all(|v| v.is_finite()))
                    .ok_or_else(|| {
                        format!("missing/nonfinite GRID {id} response in subcase {sid}")
                    })?;
                translations.push([dof[0], dof[1], dof[2]]);
                rotations.push([dof[3], dof[4], dof[5]]);
            }
            Ok(StaticSpanwiseCase {
                subcase_id: sid,
                name: case.name,
                load_factor: case.load_factor,
                grid_ids: ordered.iter().map(|row| row.0).collect(),
                xyz_m: ordered.iter().map(|row| row.1).collect(),
                y_m: ordered.iter().map(|row| row.1[1]).collect(),
                translations_m: translations,
                rotations_rad: rotations,
            })
        })
        .collect()
}

/// Preserve the summary while requiring complete product span curves.
pub fn read_static_product(
    op2: &Op2,
    deck: &Deck,
    index: &MeshNodeIndex,
    cases: &[LoadCase],
) -> StaticResult {
    let mut result = read_static(op2, index, cases);
    let response = if !op2.duplicate_static_subcases.is_empty() {
        StaticSpanwiseResponse::invalid(format!(
            "duplicate OP2 static subcase tables: {:?}",
            op2.duplicate_static_subcases
        ))
    } else if op2
        .displacements
        .values()
        .any(|table| table.node_ids.len() != table.data.len())
    {
        StaticSpanwiseResponse::invalid("static OP2 GRID/data row counts differ")
    } else {
        let tables = op2
            .displacements
            .iter()
            .map(|(id, table)| {
                (
                    *id,
                    table
                        .node_ids
                        .iter()
                        .copied()
                        .zip(table.data.iter().copied())
                        .collect(),
                )
            })
            .collect();
        extract(deck, index, cases, &tables)
    };
    attach(&mut result, response);
    result
}

pub(crate) fn attach(result: &mut StaticResult, response: StaticSpanwiseResponse) {
    if let Some(error) = &response.error {
        result.status = ResultStatus::Error;
        result.error = Some(format!("static spanwise response incomplete: {error}"));
    }
    result.spanwise = Some(response);
}

#[cfg(test)]
#[path = "static_spanwise_tests.rs"]
mod tests;
