// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The cabin crew a seated layout needs, deck by deck (EASA Air Operations
//! ORO.CC.100(b)): the type-certificate number of a registered aircraft for
//! the seats each deck installs, and never fewer than one per 50 seats on
//! that deck.

use alas_config::presets::CrewDeck;
use alas_config::AlasConfig;

use crate::geometry::CabinGeometry;
use crate::layout::{DeckItem, ItemMeta, LayoutSummary, PayloadLayout, UPPER};

/// Seats placed on the main deck and on the upper deck.
fn deck_seats(items: &[DeckItem]) -> (usize, usize) {
    let (mut main, mut upper) = (0_i64, 0_i64);
    for item in items {
        if let ItemMeta::Seat(seat) = &item.meta {
            if item.deck == UPPER {
                upper += seat.filled;
            } else {
                main += seat.filled;
            }
        }
    }
    let count = |seats: i64| usize::try_from(seats.max(0)).unwrap_or(usize::MAX);
    (count(main), count(upper))
}

/// Seats placed on `deck` in rows whose centre is forward of `x`.
fn seats_forward_of(items: &[DeckItem], deck: CrewDeck, x: f64) -> usize {
    let seats: i64 = items
        .iter()
        .filter(|item| (item.deck == UPPER) == (deck == CrewDeck::Upper) && item.x < x)
        .filter_map(|item| match &item.meta {
            ItemMeta::Seat(seat) => Some(seat.filled),
            _ => None,
        })
        .sum();
    usize::try_from(seats.max(0)).unwrap_or(usize::MAX)
}

/// The minimum cabin crew of the seats in `items`, with the type-certificate
/// rows of `preset` (none for an unregistered name); none on a cabin whose
/// capacity ceiling `max_seats` is at most 19 (ORO.CC.100(a)). A row's door
/// station, measured from the nose tip, is placed from the nose tip of the
/// built body at `nose_x`: a forward door does not move with the tail.
pub(crate) fn minimum_cabin_crew(
    preset: &str,
    items: &[DeckItem],
    max_seats: i64,
    nose_x: f64,
) -> i64 {
    let max_seats = usize::try_from(max_seats.max(0)).unwrap_or(usize::MAX);
    if max_seats <= alas_config::presets::MAX_SEATS_WITHOUT_CABIN_CREW {
        return 0;
    }
    let (main, upper) = deck_seats(items);
    let forward =
        |deck: CrewDeck, station_m: f64| seats_forward_of(items, deck, nose_x + station_m);
    let crew = alas_config::presets::minimum_cabin_crew_with_doors(preset, main, upper, &forward);
    i64::try_from(crew).unwrap_or(i64::MAX)
}

/// Re-derive `layout`'s minimum cabin crew with the type-certificate rows of
/// the registered aircraft `config` names; a clean-sheet brief keeps the
/// one-per-50-seats rule.
pub(crate) fn apply_registered_cabin_crew(
    layout: &mut PayloadLayout,
    config: &AlasConfig,
    g: &CabinGeometry,
) {
    let preset = if config.derives_clean_sheet_start() {
        ""
    } else {
        config.preset.as_str()
    };
    if let LayoutSummary::Passenger(summary) = &mut layout.summary {
        let max_seats = summary.max_certifiable_capacity;
        summary.min_cabin_crew = minimum_cabin_crew(preset, &layout.items, max_seats, g.x_min);
    }
}
