// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Product static displacement identity, completeness and basic-frame units.
// Fixture assertions deliberately fail immediately on malformed setup.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::loads::{load_cases, LoadCase};
use crate::mesh::{Deck, MeshNodeIndex};
use crate::nastran::{read_static, read_static_product, ResultStatus, StaticCaseIdentity};
use crate::nastran95::read_static_spanwise_print;
use crate::op2::{Op2, VectorTable};
use alas_config::DesignRequirements;

fn fixture() -> (Deck, MeshNodeIndex, Vec<LoadCase>, Op2) {
    let mut deck = Deck::new();
    for (id, y) in [(1, 0.0), (2, 5.0), (3, 10.0)] {
        deck.add_grid(id, [0.6 * y, y, 0.1 * y]);
    }
    let index = MeshNodeIndex {
        root_nid: 1,
        tip_nid: 3,
        kink_nid: 2,
        spar_upper_nids: vec![vec![3, 1, 2]],
        spar_lower_nids: vec![],
        engine_nids: vec![],
    };
    let cases = load_cases(&DesignRequirements::default(), 1.0);
    let mut op2 = Op2::default();
    for sid in 1..=3 {
        op2.displacements.insert(
            sid,
            VectorTable {
                node_ids: vec![1, 2, 3],
                data: (0..3)
                    .map(|j| {
                        [
                            0.01 * j as f64,
                            0.02 * j as f64,
                            0.3 * j as f64 * sid as f64,
                            0.001,
                            0.002,
                            0.003,
                        ]
                    })
                    .collect(),
            },
        );
    }
    (deck, index, cases.to_vec(), op2)
}

#[test]
fn product_msc_extracts_every_dof_in_span_order_without_changing_the_summary() {
    let (deck, index, cases, op2) = fixture();
    let summary_only = read_static(&op2, &index, &cases);
    assert!(summary_only.spanwise.is_none());
    let product = read_static_product(&op2, &deck, &index, &cases);
    assert_eq!(product.status, ResultStatus::Ok);
    assert_eq!(product.tip_deflection_m, summary_only.tip_deflection_m);
    let response = product.spanwise.unwrap();
    assert!(response.error.is_none());
    assert_eq!(
        response.case_identity,
        StaticCaseIdentity::ExplicitSubcaseIds
    );
    assert_eq!(response.cases.len(), 3);
    let case = &response.cases[0];
    assert_eq!(case.name, "pull-up");
    assert_eq!(case.grid_ids, [1, 2, 3]);
    assert_eq!(case.y_m, [0.0, 5.0, 10.0]);
    assert_eq!(case.xyz_m[2], [6.0, 10.0, 1.0]);
    assert_eq!(case.translations_m[2], [0.02, 0.04, 0.6]);
    assert_eq!(case.rotations_rad[2], [0.001, 0.002, 0.003]);
}

#[test]
fn incomplete_duplicate_or_nonfinite_msc_output_cannot_become_a_zero_curve() {
    let (deck, index, cases, original) = fixture();
    let reject = |op2: &Op2| {
        let result = read_static_product(op2, &deck, &index, &cases);
        assert_eq!(result.status, ResultStatus::Error);
        let response = result.spanwise.unwrap();
        assert!(response.error.is_some());
        assert!(response.cases.is_empty());
    };
    let mut input = original.clone();
    input.displacements.remove(&2);
    reject(&input);
    let mut input = original.clone();
    input
        .displacements
        .insert(4, input.displacements[&1].clone());
    reject(&input);
    let mut input = original.clone();
    input.displacements.get_mut(&1).unwrap().node_ids[1] = 1;
    reject(&input);
    let mut input = original.clone();
    input.displacements.get_mut(&1).unwrap().data[1][4] = f64::NAN;
    reject(&input);
    let mut input = original.clone();
    input.displacements.get_mut(&1).unwrap().node_ids[1] = 99;
    reject(&input);
    let mut input = original.clone();
    input.displacements.get_mut(&1).unwrap().data.pop();
    reject(&input);
    let mut input = original;
    input.duplicate_static_subcases.push(1);
    reject(&input);
}

#[test]
fn missing_or_repeated_line_coordinates_are_not_valid_spanwise_evidence() {
    let (mut deck, index, cases, op2) = fixture();
    deck.add_grid(2, [3.0, 0.0, 0.5]);
    let result = read_static_product(&op2, &deck, &index, &cases);
    assert_eq!(result.status, ResultStatus::Error);
}

fn page(sid: i64, ids: &[i64]) -> String {
    let mut text = format!("0 WINGBOX SUBCASE {sid}\n D I S P L A C E M E N T   V E C T O R\n");
    for id in ids {
        text.push_str(&format!(
            " {id} G 0.0 0.0 {} 0.01 0.02 0.03\n",
            sid as f64 * (*id as f64 - 1.0)
        ));
    }
    text
}

#[test]
fn product_nastran95_uses_explicit_subcases_across_pages_and_reordered_output() {
    let (deck, index, cases, _) = fixture();
    let print =
        page(3, &[1, 2, 3]) + &page(1, &[1]) + "\x0c" + &page(1, &[2, 3]) + &page(2, &[1, 2, 3]);
    let response = read_static_spanwise_print(&print, &deck, &index, &cases);
    assert!(response.error.is_none(), "{:?}", response.error);
    assert_eq!(
        response.case_identity,
        StaticCaseIdentity::ExplicitSubcaseIds
    );
    assert_eq!(response.cases[0].translations_m[2][2], 2.0);
    assert_eq!(response.cases[1].translations_m[2][2], 4.0);
    assert_eq!(response.cases[2].translations_m[2][2], 6.0);
    assert_eq!(response.cases[2].rotations_rad[1], [0.01, 0.02, 0.03]);
}

#[test]
fn product_nastran95_rejects_missing_identity_cases_and_repeated_tables() {
    let (deck, index, cases, _) = fixture();
    let good = page(1, &[1, 2, 3]) + &page(2, &[1, 2, 3]) + &page(3, &[1, 2, 3]);
    for bad in [
        good.replace("SUBCASE", "UNKNOWN"),
        page(1, &[1, 2, 3]) + &page(3, &[1, 2, 3]),
        good.clone() + &page(3, &[1, 2, 3]),
        good.replace("0.02", "NaN"),
    ] {
        let result = read_static_spanwise_print(&bad, &deck, &index, &cases);
        assert!(result.error.is_some());
        assert!(result.cases.is_empty());
    }
}
