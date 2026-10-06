// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Seat allocation between the main deck and the partial upper deck under an
//! upper-deck hump (747 type).
//!
//! A registered aircraft's source seat limit (its planning or certified
//! count) is the whole aircraft's. Allocated in deck order, the main deck's
//! exit capacity would take all of it and leave the upper deck empty, and
//! allocated by exit capacity the upper deck would reserve seats its short
//! floor cannot hold. So on a hump body the upper deck is laid out first,
//! against its own exit ceiling, and the main deck gets what the source
//! limit leaves of the seats actually placed upstairs. Bodies without an
//! upper deck keep the deck-order allocation unchanged.

use super::seating::CabinClass;
use super::{DeckCapacities, DeckSegment};
use crate::geometry::CabinGeometry;

impl DeckCapacities {
    /// [`Self::with_source_cap`] for the decks of `g`, the partial upper deck
    /// served first. This is the reported ceiling; the row packers use
    /// [`RemainingSourceCap`], which charges the seats actually placed.
    pub fn with_source_cap_for(&self, g: &CabinGeometry, source_capacity_cap: Option<i64>) -> Self {
        if g.upper_deck.is_none() {
            return self.with_source_cap(source_capacity_cap);
        }
        let reversed = Self {
            per_deck: self.per_deck.iter().rev().copied().collect(),
            total: self.total,
        }
        .with_source_cap(source_capacity_cap);
        Self {
            per_deck: reversed.per_deck.into_iter().rev().collect(),
            total: reversed.total,
        }
    }
}

/// The decks in layout order: the partial upper deck first on a hump body.
pub(crate) fn layout_order<'a>(
    g: &CabinGeometry,
    mut segments: Vec<DeckSegment<'a>>,
) -> Vec<DeckSegment<'a>> {
    if g.upper_deck.is_some() {
        segments.reverse();
    }
    segments
}

/// The class the partial upper deck starts with, given that the next class
/// to place is `next`: business before first, because a 747-type upper deck
/// is the business cabin while first class sits in the nose of the main
/// deck. Unchanged unless `next` is first class and business seats remain.
pub(crate) fn upper_deck_start_class(classes: &[CabinClass], next: usize) -> usize {
    match classes.get(next) {
        Some(class) if class.name == "First" => classes
            .iter()
            .position(|c| c.name == "Business" && c.remaining > 0)
            .unwrap_or(next),
        _ => next,
    }
}

/// The per-deck seat ceiling while the decks are laid out in
/// [`layout_order`].
pub(crate) struct RemainingSourceCap<'a> {
    hump: bool,
    geometric: &'a DeckCapacities,
    capped: &'a DeckCapacities,
    source: Option<i64>,
    seated: i64,
}

impl<'a> RemainingSourceCap<'a> {
    /// `geometric` are the exit ceilings without the source limit, `capped`
    /// the deck-order allocation every other body uses.
    pub(crate) fn new(
        g: &CabinGeometry,
        geometric: &'a DeckCapacities,
        capped: &'a DeckCapacities,
        source: Option<i64>,
    ) -> Self {
        Self {
            hump: g.upper_deck.is_some(),
            geometric,
            capped,
            source: source.filter(|cap| *cap >= 0),
            seated: 0,
        }
    }

    /// The ceiling of deck `name`, given the seats placed so far.
    pub(crate) fn cap(&self, name: &str) -> i64 {
        if !self.hump {
            return self.capped.for_deck(name);
        }
        let left = self
            .source
            .map_or(i64::MAX, |cap| (cap - self.seated).max(0));
        self.geometric.for_deck(name).min(left)
    }

    /// Charge `seats` placed on the deck just laid out.
    pub(crate) fn seat(&mut self, seats: i64) {
        self.seated += seats.max(0);
    }
}
