// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::{build_modes_deck, build_modes_deck_for_nodes, time_limit, Dialect};
use crate::mesh::Deck;
use alas_config::StructuresConfig;

#[test]
fn legacy_time_card_tracks_the_per_solution_timeout_in_whole_minutes() {
    let mut card = String::new();
    time_limit(&mut card, 121.0);
    assert_eq!(card, "TIME 3\n");
}

#[test]
fn local_modes_use_a_bounded_search_with_a_sufficient_root_estimate() {
    let config = StructuresConfig {
        n_modes: 6,
        ..StructuresConfig::default()
    };
    let deck = build_modes_deck(&Deck::default(), &config, Dialect::Nastran95);
    assert!(deck.contains("EIGR    1       INV     0.      100.    64      16"));
}

#[test]
fn local_modes_preserve_a_higher_explicit_request() {
    let config = StructuresConfig {
        n_modes: 30,
        ..StructuresConfig::default()
    };
    let deck = build_modes_deck(&Deck::default(), &config, Dialect::Nastran95);
    assert!(deck.contains("EIGR    1       INV     0.      100.    120     30"));
}

#[test]
fn local_modes_can_limit_eigenvector_printing_to_required_nodes() {
    let nodes: Vec<i64> = (1..=40).map(|value| value * 10_000).collect();
    let deck = build_modes_deck_for_nodes(
        &Deck::default(),
        &StructuresConfig::default(),
        Dialect::Nastran95,
        &nodes,
    );
    assert!(deck.contains("  SET 9500 = "));
    assert!(deck.contains("  DISPLACEMENT = 9500\n"));
    assert!(!deck.contains("  DISPLACEMENT = ALL\n"));
    for line in deck
        .lines()
        .skip_while(|line| !line.starts_with("  SET 9500"))
    {
        if line == "  DISPLACEMENT = 9500" {
            break;
        }
        assert!(
            line.len() <= 72,
            "case-control line exceeds 72 columns: {line}"
        );
    }
}
