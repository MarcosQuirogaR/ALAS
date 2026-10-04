// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/cabin_layout.py (`build_passenger_layout`, the
// exit pass)

//! The emergency exits: one pair set per passenger deck, at the declared
//! door stations where the source prints them and on the monument bays
//! otherwise.

use super::*;

/// The emergency exits, and how many pairs were installed.
pub(in super::super) struct Exits {
    /// The door cutouts, in placement order.
    pub items: Vec<DeckItem>,
    /// The type every door was drawn at.
    pub exit_type: &'static str,
    /// Pairs installed, summed over the passenger decks.
    pub pairs: i64,
    /// Sum of the ratings of the installed complete exit pairs.
    pub capacity_total: i64,
}

/// Size and place one exit-pair set per passenger deck.
///
/// A double-decker's upper deck needs its own evacuation route, so this runs
/// per deck rather than once for the aircraft, and each deck is sized from the
/// passengers actually seated on it after the capacity cap, never from the
/// raw requested total, which is what a deck that could not seat them all would
/// otherwise be given doors for.
pub(in super::super) fn place_exits(
    g: &CabinGeometry,
    pax: &PassengerCabinConfig,
    seating: &Seating,
    product_exit_capacity: bool,
    source_exit_layout: Option<CertifiedExitLayout>,
) -> Exits {
    let default_spec = select_exit_type(g.diameter_m);
    let default_capacity_per_pair = if product_exit_capacity {
        effective_pair_capacity(default_spec, pax)
    } else {
        // Frozen Python compatibility treated the pair table as a side
        // quantity.  Keep that historical branch isolated from the product
        // path's corrected complete-pair unit.
        default_spec.capacity_per_pair
    };
    let mut items = Vec::new();
    let mut pairs = 0i64;
    let mut capacity_total = 0i64;

    for (deck_index, segment) in cabin_deck_segments(g).into_iter().enumerate() {
        let deck = segment.deck;
        let deck_pax = seating.on_deck(deck.name);
        if deck_pax <= 0 {
            continue;
        }
        if deck_index == 0 && !g.door_stations.is_empty() {
            let (door_items, rating) = declared_exit_items(g, deck, &g.door_stations);
            items.extend(door_items);
            pairs += g.door_stations.len() as i64;
            capacity_total += rating;
            continue;
        }
        let n_pairs = if let Some(source_exit_layout) = source_exit_layout {
            ceil_div(deck_pax, largest_pair_rating(&source_exit_layout))
                .max(min_exit_pairs(deck_pax))
                .clamp(1, source_exit_layout.pairs.len().max(1) as i64)
        } else {
            min_exit_pairs(deck_pax).max(ceil_div(deck_pax, default_capacity_per_pair.max(1)))
        };

        let mut deck_bays: Vec<&Bay> = seating
            .bays
            .iter()
            .filter(|bay| bay.deck == deck.name)
            .collect();
        deck_bays.sort_by(|a, b| a.x.total_cmp(&b.x));
        // A source arrangement has a fixed pair order.  Use the lower bay at
        // an exact midpoint instead of the generic banker's rounding: for
        // four model bays and three source pairs, rounding 1.5 upward would
        // select stations [0, 2, 3] and leave an avoidable over-spacing gap,
        // while [0, 1, 3] preserves the same physical bay candidates.
        let bay_indices = if source_exit_layout.is_some() {
            source_spread_bay_indices(n_pairs, deck_bays.len())
        } else {
            spread_bay_indices(n_pairs, deck_bays.len())
        };
        let mut exit_xs: Vec<f64> = bay_indices.into_iter().map(|i| deck_bays[i].x).collect();

        // More pairs than there are bays to hang them on: the remainder is
        // spread along the whole segment instead, a metre in from each end.
        if n_pairs > exit_xs.len() as i64 {
            let extra = n_pairs - exit_xs.len() as i64;
            let ex0 = segment.x0 - MONUMENT_LEN + 1.0;
            let ex1 = segment.x1 + MONUMENT_LEN - 1.0;
            exit_xs.extend((0..extra).map(|i| ex0 + (ex1 - ex0) * (i as f64 + 0.5) / extra as f64));
        }

        for (pair_index, xe) in exit_xs.into_iter().enumerate() {
            let pair_spec = source_exit_layout
                .and_then(|layout| layout.pairs.get(pair_index))
                .and_then(|pair| exit_spec(pair.exit_type))
                .unwrap_or(default_spec);
            let pair_capacity = source_exit_layout
                .and_then(|layout| layout.pairs.get(pair_index))
                .map_or(
                    if product_exit_capacity {
                        default_capacity_per_pair
                    } else {
                        // The frozen summary recorded two side ratings per
                        // pair; this conversion is deliberately confined to
                        // the compatibility branch.
                        default_capacity_per_pair * 2
                    },
                    |pair| pair_rating(pair.exit_type),
                );
            capacity_total += pair_capacity;
            items.extend(exit_pair_items(g, deck, xe, pair_spec));
        }
        pairs += n_pairs;
    }

    Exits {
        items,
        exit_type: source_exit_layout.map_or(default_spec.name, |layout| layout.label),
        pairs,
        capacity_total,
    }
}

/// One exit pair drawn as its two door cut-outs at station `xe`.
fn exit_pair_items(
    g: &CabinGeometry,
    deck: &crate::geometry::DeckSpec,
    xe: f64,
    spec: &ExitSpec,
) -> [DeckItem; 2] {
    let half = g.usable_width(deck, xe) / 2.0 + g.wall;
    [-1.0, 1.0].map(|side| DeckItem {
        kind: ItemKind::Exit,
        deck: deck.name,
        x: xe,
        y: side * half,
        z: g.item_z(deck, xe, spec.height_m),
        length: spec.width_m,
        width: 0.25,
        mass: 0.0,
        height: g.clamp_height(deck, xe, spec.height_m),
        label: format!("Type {}", spec.name),
        meta: ItemMeta::Exit(ExitMeta {
            exit_type: spec.name,
            door_w: spec.width_m,
            door_h: spec.height_m,
        }),
    })
}

/// The declared doors of a cabin bounded by them: every pair the structure
/// has is installed, at its station, rated by its type.
fn declared_exit_items(
    g: &CabinGeometry,
    deck: &crate::geometry::DeckSpec,
    doors: &[DoorStation],
) -> (Vec<DeckItem>, i64) {
    let mut items = Vec::with_capacity(2 * doors.len());
    let mut rating = 0;
    for door in doors {
        items.extend(exit_pair_items(g, deck, door.x, door.spec));
        rating += pair_rating(door.spec.name);
    }
    (items, rating)
}

/// Spread a registered source arrangement across existing bay stations.
///
/// The source sequence fixes the number and order of exit pairs, while the
/// model's monument/seat pass supplies the available stations.  Choosing the
/// lower integer at an exact midpoint keeps a source pair away from the aft
/// endpoint when the candidate count is even; the registered A220/A320
/// layouts then satisfy CS-25's 18.3 m adjacent-exit spacing check without
/// inventing a longitudinal station.  The final spacing assertion belongs in
/// the source-specific acceptance test because this helper has no fuselage
/// frame or regulatory applicability context.
fn source_spread_bay_indices(n_items: i64, n_bays: usize) -> Vec<usize> {
    if n_bays == 0 || n_items <= 0 {
        return Vec::new();
    }
    let n_items = (n_items as usize).min(n_bays);
    if n_items == 1 {
        return vec![0];
    }
    let mut out: Vec<usize> = Vec::with_capacity(n_items);
    for i in 0..n_items {
        let exact = (i * (n_bays - 1)) as f64 / (n_items - 1) as f64;
        let mut idx = exact.floor() as usize;
        while out.contains(&idx) && idx < n_bays - 1 {
            idx += 1;
        }
        out.push(idx);
    }
    out
}
