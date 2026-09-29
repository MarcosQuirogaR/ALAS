// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

fn seat_meta(filled: i64) -> SeatMeta {
    SeatMeta {
        cls: "Economy",
        abreast: 10,
        filled,
        deck: "main",
        aisles: 2,
        blocks: vec![3, 4, 3],
        seat_w: 0.46,
        aisle_w: 0.51,
    }
}

fn row(deck: &'static str) -> DeckItem {
    DeckItem {
        kind: ItemKind::SeatRow,
        deck,
        x: 10.0,
        y: 0.0,
        z: 0.0,
        length: 0.8,
        width: 6.0,
        mass: 900.0,
        height: 1.0,
        label: "Economy".to_owned(),
        meta: ItemMeta::Seat(seat_meta(10)),
    }
}

#[test]
fn twin_aisle_letters_and_partial_rows_follow_the_placed_blocks() {
    let full = filled_seat_positions(&seat_meta(10));
    assert_eq!(full.len(), 10);
    assert_eq!(full[0].1, "A");
    assert_eq!(full[7].1, "H");
    assert_eq!(full[8].1, "J");
    assert_eq!(filled_seat_positions(&seat_meta(7)).len(), 7);
    assert!(full[3].0 - full[2].0 > 0.9);
    assert!(full[7].0 - full[6].0 > 0.9);
}

#[test]
fn upper_deck_row_numbers_continue_after_main_deck_rows() {
    let mut items = vec![row("main"); 12];
    items.push(row("upper"));
    assert_eq!(first_row_number_for_deck(&items, "main"), 1);
    assert_eq!(first_row_number_for_deck(&items, "upper"), 14);
}
