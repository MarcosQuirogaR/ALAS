// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-struct::mesh` against `alas.geometry.wing_mesh_bdf`, via
//! `golden/generators/gen_struct_mesh.py`.
//!
//! **The comparison is between two written decks, not two object graphs.** The
//! generator writes the reference's `BDF` with `write_bdf(size=16)`: the same
//! call the solve orchestration makes, and records what `pyNastran` read back
//! out of that file; this test writes the port's own deck, reads it back with
//! `support::deck`, and compares the cards. What is being checked is therefore
//! the file a NASTRAN run would consume.
//!
//! Two tiers, and the split is the point of the row. Everything discrete,
//! which cards exist, what identifier each one got, which grids an element
//! names, the property and material each one points at, the constrained grid
//! list, the rivets' topology, the counts and the warning text, is compared
//! at [`Tier::Exact`], because an element that moved to a different grid is not
//! a tolerance question. Every value on those cards is compared at
//! [`Tier::Linalg`]: the grid coordinates come from
//! `alas-geom::wing_structure`'s rib surfaces, which are a spline evaluation
//! and are themselves a `linalg` quantity in their own green row, so a deck
//! that reproduced them bitwise would be asserting something the surfaces
//! underneath it do not claim. The closed-form numbers riding the same tier:
//! the material constants, the skin thickness: agree far tighter, exactly as
//! `alas-stab::trim`'s single tier does.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod support;

use alas_config::{DesignRequirements, EngineConfig, MassModelConfig};
use alas_struct::mesh::{build_wing_mesh_bdf, MeshHealthReport, MeshNodeIndex};
use alas_struct::sizing::size_wingbox_reference_compatibility;
use alas_testkit::{Comparison, Tier};
use serde_json::Value;
use support::deck::{parse, Card};
use support::{build_geometry, materials_for, structures_config_for, Case, Fixture};

/// Every card of one type, in the order the deck lists them.
fn named<'a>(cards: &'a [Card], name: &str) -> Vec<&'a Card> {
    cards.iter().filter(|card| card.name == name).collect()
}

/// Compare two integer rows, which is what a shell element is.
fn compare_rows(exact: &mut Comparison, label: &str, got: &[Vec<i64>], want: &[Vec<i64>]) {
    exact.exact(&format!("{label}: count"), &got.len(), &want.len());
    for (index, (row, reference)) in got.iter().zip(want).enumerate() {
        exact.exact(&format!("{label}[{index}]"), row, reference);
    }
}

fn compare_grids(exact: &mut Comparison, numeric: &mut Comparison, case: &Case, cards: &[Card]) {
    let mut got: Vec<&Card> = named(cards, "GRID");
    got.sort_by_key(|card| card.integer(0));
    exact.exact(
        &format!("{}: GRID count", case.name),
        &got.len(),
        &case.deck.grids.len(),
    );
    for (card, row) in got.iter().zip(&case.deck.grids) {
        let label = format!("{}: GRID {}", case.name, row.0);
        exact.exact(&format!("{label} id"), &card.integer(0), &row.0);
        numeric.scalar(&format!("{label}.x"), card.real(2), row.1);
        numeric.scalar(&format!("{label}.y"), card.real(3), row.2);
        numeric.scalar(&format!("{label}.z"), card.real(4), row.3);
    }
}

fn compare_shells(exact: &mut Comparison, case: &Case, cards: &[Card]) {
    for (name, want) in [("CQUAD4", &case.deck.cquad4), ("CTRIA3", &case.deck.ctria3)] {
        let mut got: Vec<Vec<i64>> = named(cards, name)
            .iter()
            .map(|card| card.integers_from(0))
            .collect();
        got.sort();
        compare_rows(exact, &format!("{}: {name}", case.name), &got, want);
    }
}

fn compare_bars(exact: &mut Comparison, numeric: &mut Comparison, case: &Case, cards: &[Card]) {
    let mut got: Vec<&Card> = named(cards, "CBAR");
    got.sort_by_key(|card| card.integer(0));
    exact.exact(
        &format!("{}: CBAR count", case.name),
        &got.len(),
        &case.deck.cbar.len(),
    );
    for (card, row) in got.iter().zip(&case.deck.cbar) {
        let label = format!("{}: CBAR {}", case.name, row.0);
        exact.exact(
            &format!("{label} topology"),
            &vec![
                card.integer(0),
                card.integer(1),
                card.integer(2),
                card.integer(3),
            ],
            &vec![row.0, row.1, row.2, row.3],
        );
        exact.exact(&format!("{label}.offt"), &card.text(7), &row.5);
        for (axis, expected) in row.4.iter().enumerate() {
            numeric.scalar(
                &format!("{label}.x[{axis}]"),
                card.real(4 + axis),
                *expected,
            );
        }
    }
}

fn compare_masses(exact: &mut Comparison, numeric: &mut Comparison, case: &Case, cards: &[Card]) {
    let mut got: Vec<&Card> = named(cards, "CONM2");
    got.sort_by_key(|card| card.integer(0));
    exact.exact(
        &format!("{}: CONM2 count", case.name),
        &got.len(),
        &case.deck.conm2.len(),
    );
    for (card, row) in got.iter().zip(&case.deck.conm2) {
        let label = format!("{}: CONM2 {}", case.name, row.0);
        exact.exact(
            &format!("{label} attachment"),
            &vec![card.integer(0), card.integer(1), card.integer(2)],
            &vec![row.0, row.1, row.2],
        );
        numeric.scalar(&format!("{label}.mass"), card.real(3), row.3);
        for (axis, expected) in row.4.iter().enumerate() {
            numeric.scalar(
                &format!("{label}.offset[{axis}]"),
                card.real(4 + axis),
                *expected,
            );
        }
    }
}

fn compare_rivets(exact: &mut Comparison, numeric: &mut Comparison, case: &Case, cards: &[Card]) {
    let mut got: Vec<&Card> = named(cards, "RBE3");
    got.sort_by_key(|card| card.integer(0));
    exact.exact(
        &format!("{}: RBE3 count", case.name),
        &got.len(),
        &case.deck.rbe3.len(),
    );
    for (card, row) in got.iter().zip(&case.deck.rbe3) {
        let label = format!("{}: RBE3 {}", case.name, row.0);
        exact.exact(&format!("{label} id"), &card.integer(0), &row.0);
        exact.exact(&format!("{label}.refgrid"), &card.integer(2), &row.1);
        exact.exact(&format!("{label}.refc"), &card.text(3), &row.2);
        exact.exact(&format!("{label}.comp"), &card.text(5), &row.4);
        exact.exact(
            &format!("{label}.independents"),
            &card.integers_from(6),
            &row.5,
        );
        numeric.scalar(&format!("{label}.weight"), card.real(4), row.3);
    }
}

fn compare_properties(
    exact: &mut Comparison,
    numeric: &mut Comparison,
    case: &Case,
    cards: &[Card],
) {
    let mut shells: Vec<&Card> = named(cards, "PSHELL");
    shells.sort_by_key(|card| card.integer(0));
    exact.exact(
        &format!("{}: PSHELL count", case.name),
        &shells.len(),
        &case.deck.pshell.len(),
    );
    for (card, row) in shells.iter().zip(&case.deck.pshell) {
        let label = format!("{}: PSHELL {}", case.name, row.0);
        exact.exact(
            &format!("{label} materials"),
            &vec![card.integer(0), card.integer(1), card.integer(3)],
            &vec![row.0, row.1, row.3],
        );
        numeric.scalar(&format!("{label}.t"), card.real(2), row.2);
    }

    let mut bars: Vec<&Card> = named(cards, "PBARL");
    bars.sort_by_key(|card| card.integer(0));
    exact.exact(
        &format!("{}: PBARL count", case.name),
        &bars.len(),
        &case.deck.pbarl.len(),
    );
    for (card, row) in bars.iter().zip(&case.deck.pbarl) {
        let label = format!("{}: PBARL {}", case.name, row.0);
        exact.exact(
            &format!("{label} identity"),
            &vec![card.integer(0), card.integer(1)],
            &vec![row.0, row.1],
        );
        exact.exact(&format!("{label}.section"), &card.text(3), &row.2);
        // The dimensions start after the card's four reserved columns.
        let dimensions = card.reals_from(8);
        exact.exact(
            &format!("{label}.dimension count"),
            &dimensions.len(),
            &row.3.len(),
        );
        numeric.slice(&format!("{label}.dim"), &dimensions, &row.3);
    }

    let mut materials: Vec<&Card> = named(cards, "MAT1");
    materials.sort_by_key(|card| card.integer(0));
    exact.exact(
        &format!("{}: MAT1 count", case.name),
        &materials.len(),
        &case.deck.mat1.len(),
    );
    for (card, row) in materials.iter().zip(&case.deck.mat1) {
        let label = format!("{}: MAT1 {}", case.name, row.0);
        exact.exact(&format!("{label} id"), &card.integer(0), &row.0);
        numeric.scalar(&format!("{label}.e"), card.real(1), row.1);
        numeric.scalar(&format!("{label}.g"), card.real(2), row.2);
        numeric.scalar(&format!("{label}.nu"), card.real(3), row.3);
        numeric.scalar(&format!("{label}.rho"), card.real(4), row.4);
    }
}

fn compare_constraints_and_params(exact: &mut Comparison, case: &Case, cards: &[Card]) {
    let mut constraints: Vec<&Card> = named(cards, "SPC1");
    constraints.sort_by_key(|card| card.integer(0));
    exact.exact(
        &format!("{}: SPC1 count", case.name),
        &constraints.len(),
        &case.deck.spc1.len(),
    );
    for (card, row) in constraints.iter().zip(&case.deck.spc1) {
        let label = format!("{}: SPC1 {}", case.name, row.0);
        exact.exact(&format!("{label} id"), &card.integer(0), &row.0);
        exact.exact(&format!("{label}.components"), &card.text(1), &row.1);
        exact.exact(&format!("{label}.nodes"), &card.integers_from(2), &row.2);
    }

    let mut params: Vec<&Card> = named(cards, "PARAM");
    params.sort_by_key(|card| card.text(0));
    exact.exact(
        &format!("{}: PARAM count", case.name),
        &params.len(),
        &case.deck.params.len(),
    );
    for (card, (key, values)) in params.iter().zip(&case.deck.params) {
        let label = format!("{}: PARAM {key}", case.name);
        exact.exact(&format!("{label} name"), &card.text(0), key);
        for (index, value) in values.iter().enumerate() {
            match value {
                Value::String(text) => {
                    exact.exact(&format!("{label}[{index}]"), &card.text(1 + index), text);
                }
                other => {
                    let expected = other.as_f64().expect("a PARAM value is text or a number");
                    exact.scalar(&format!("{label}[{index}]"), card.real(1 + index), expected);
                }
            }
        }
    }
}

fn compare_health(
    exact: &mut Comparison,
    numeric: &mut Comparison,
    case: &Case,
    report: &MeshHealthReport,
) {
    let want = &case.health;
    let label = format!("{}: health", case.name);
    for (field, got, expected) in [
        ("n_nodes", report.n_nodes, want.n_nodes),
        ("n_elements", report.n_elements, want.n_elements),
        (
            "n_perp_warnings",
            report.n_perp_warnings,
            want.n_perp_warnings,
        ),
        ("n_warping_bad", report.n_warping_bad, want.n_warping_bad),
        ("n_cquad4", report.n_cquad4, want.n_cquad4),
        ("n_ctria3", report.n_ctria3, want.n_ctria3),
        (
            "n_spar_straightness_warnings",
            report.n_spar_straightness_warnings,
            want.n_spar_straightness_warnings,
        ),
        ("rbe3_count", report.rbe3_count, want.rbe3_count),
    ] {
        exact.exact(&format!("{label}.{field}"), &got, &expected);
    }
    exact.exact(&format!("{label}.ok"), &report.ok(), &want.ok);
    exact.exact(
        &format!("{label}.warnings"),
        &report.warnings,
        &want.warnings,
    );

    numeric.scalar(
        &format!("{label}.warping_max"),
        report.warping_max,
        want.warping_max,
    );
    numeric.scalar(
        &format!("{label}.warping_mean"),
        report.warping_mean,
        want.warping_mean,
    );
    numeric.scalar(
        &format!("{label}.triangle_ratio"),
        report.triangle_ratio,
        want.triangle_ratio,
    );
    exact.exact(
        &format!("{label}.straightness count"),
        &report.spar_straightness_max_dev_m.len(),
        &want.spar_straightness_max_dev_m.len(),
    );
    for (index, (&(fraction, deviation), &(want_fraction, want_deviation))) in report
        .spar_straightness_max_dev_m
        .iter()
        .zip(&want.spar_straightness_max_dev_m)
        .enumerate()
    {
        exact.scalar(
            &format!("{label}.straightness[{index}].x/c"),
            fraction,
            want_fraction,
        );
        numeric.scalar(
            &format!("{label}.straightness[{index}].dev"),
            deviation,
            want_deviation,
        );
    }
}

fn compare_node_index(exact: &mut Comparison, case: &Case, index: &MeshNodeIndex) {
    let want = &case.node_index;
    let label = format!("{}: node index", case.name);
    exact.exact(&format!("{label}.root"), &index.root_nid, &want.root_nid);
    exact.exact(&format!("{label}.tip"), &index.tip_nid, &want.tip_nid);
    exact.exact(&format!("{label}.kink"), &index.kink_nid, &want.kink_nid);
    exact.exact(
        &format!("{label}.spar_upper"),
        &index.spar_upper_nids,
        &want.spar_upper_nids,
    );
    exact.exact(
        &format!("{label}.spar_lower"),
        &index.spar_lower_nids,
        &want.spar_lower_nids,
    );
    exact.exact(
        &format!("{label}.engines"),
        &index.engine_nids,
        &want.engine_nids,
    );
}

#[test]
fn build_wing_mesh_bdf_writes_the_deck_python_writes() {
    let fixture: Fixture = alas_testkit::load("struct", "mesh");
    let req = DesignRequirements::default();
    let engine_cfg = EngineConfig::default();
    let mass_cfg = MassModelConfig::default();

    let mut exact = Comparison::new("alas-struct::mesh (deck structure)", Tier::Exact);
    let mut numeric = Comparison::new("alas-struct::mesh (deck values)", Tier::Linalg);

    for case in &fixture.cases {
        let wsg = build_geometry(&case.spar_chord_fractions, &case.spar_full_span);
        let cfg = structures_config_for(&case.config);
        let [skin, web, cap, rib] = materials_for(&case.materials);

        let sizing = size_wingbox_reference_compatibility(&wsg, &cfg, &req, skin, web, cap, rib);
        exact.exact(
            &format!("{}: num_ribs", case.name),
            &sizing.num_ribs,
            &case.num_ribs,
        );

        let (deck, report, node_index) = build_wing_mesh_bdf(
            &wsg,
            &sizing,
            &cfg,
            &engine_cfg,
            &mass_cfg,
            &req,
            skin,
            web,
            cap,
            rib,
        )
        .expect("the nominal wingbox meshes without a fatal health finding");

        let cards = parse(&deck.write_bulk());
        compare_grids(&mut exact, &mut numeric, case, &cards);
        compare_shells(&mut exact, case, &cards);
        compare_bars(&mut exact, &mut numeric, case, &cards);
        compare_masses(&mut exact, &mut numeric, case, &cards);
        compare_rivets(&mut exact, &mut numeric, case, &cards);
        compare_properties(&mut exact, &mut numeric, case, &cards);
        compare_constraints_and_params(&mut exact, case, &cards);
        compare_health(&mut exact, &mut numeric, case, &report);
        compare_node_index(&mut exact, case, &node_index);
    }

    exact.finish();
    numeric.finish();
}
