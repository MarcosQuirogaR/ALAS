// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::{
    build_modes_deck, build_modes_deck_banded, build_modes_deck_for_nodes, time_limit, Dialect,
    ModalBand, UnitScale,
};
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

fn unit_fixture() -> Deck {
    use crate::mesh::{Mat1, Pshell};
    let mut deck = Deck::new();
    deck.add_grid(1, [25.0, 21.6, 1.18]);
    deck.materials.push(Mat1 {
        mid: 1,
        e: 7.1e10,
        g: 2.669e10,
        nu: 0.33,
        rho: 2810.0,
    });
    deck.shell_properties.push(Pshell {
        pid: 1,
        mid1: 1,
        t: 0.002,
        mid2: 1,
    });
    deck
}

/// The eight-column real at fixed field `index` (0 is the card name) of the
/// first line of `deck` starting with `card`, with the NASTRAN exponent
/// shorthand (`7.1+8`) expanded.
fn field(deck: &str, card: &str, index: usize) -> f64 {
    let line = deck
        .lines()
        .find(|line| line.starts_with(card))
        .expect("card present");
    let text = line[8 * index..(8 * (index + 1)).min(line.len())].trim();
    // `7.1+8` is 7.1e8: an exponent sign after the first character.
    let split = text
        .char_indices()
        .skip(1)
        .find(|&(_, c)| c == '+' || c == '-')
        .map(|(i, _)| i);
    match split {
        Some(i) => format!("{}e{}", &text[..i], &text[i..]),
        None => text.to_owned(),
    }
    .parse()
    .expect("a real")
}

#[test]
fn decimetre_decks_scale_geometry_stiffness_and_density_coherently() {
    let band = ModalBand {
        upper_hz: 8.0,
        estimate: 12,
        extract: 12,
    };
    let write = |units| {
        build_modes_deck_banded(
            &unit_fixture(),
            &StructuresConfig::default(),
            Dialect::Nastran95,
            &[],
            units,
            band,
        )
    };
    let (si, dm) = (write(UnitScale::SI), write(UnitScale::DECIMETRE));
    let close = |a: f64, b: f64| (a - b).abs() <= 1.0e-5 * b.abs();
    // Metres: coordinates and thickness as stored; E in pascals.
    assert!(close(field(&si, "GRID", 3), 25.0) && close(field(&si, "GRID", 5), 1.18));
    assert!(close(field(&si, "MAT1", 2), 7.1e10));
    // Decimetres: 25 m is 250 dm; E is 7.1e8 (100 Pa); density is
    // (10 kg)/dm^3 (2810 kg/m^3 = 0.281); thickness 2 mm is 0.02 dm.
    assert!(close(field(&dm, "GRID", 3), 250.0), "{dm}");
    assert!(close(field(&dm, "GRID", 5), 11.8), "{dm}");
    assert!(close(field(&dm, "MAT1", 2), 7.1e8), "{dm}");
    assert!(close(field(&dm, "MAT1", 3), 2.669e8), "{dm}");
    assert!(close(field(&dm, "MAT1", 5), 0.281), "{dm}");
    assert!(close(field(&dm, "PSHELL", 3), 0.02), "{dm}");
    // Both carry the explicit narrow search band, not the 0-100 Hz default.
    for deck in [&si, &dm] {
        assert!(deck.contains("EIGR    1       INV     0.      8.      12      12"));
    }
}

#[test]
fn the_default_modal_band_is_unchanged_for_the_public_builders() {
    assert_eq!(
        ModalBand::legacy(6),
        ModalBand {
            upper_hz: 100.0,
            estimate: 64,
            extract: 16
        }
    );
}
