// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/cabin_layout.py (`build_passenger_layout`, the
// seating pass)
// Reference: alas @ rust-port-baseline.

//! Packing the seat rows into the decks, and carving out the bays that
//! everything else is hung on.
//!
//! # Why the deck is laid out twice
//!
//! The mid-cabin monument bays sit at the door stations, and how many doors a
//! deck needs depends on how many people end up seated on it, which depends
//! on how much floor the bays took. The pass below breaks that circle the way
//! upstream does: a simulation seats the deck with no mid-cabin bays to find
//! out how many door pairs it will want, the bays are charged against the
//! seating length, and the real pass then runs against what is left. The fast
//! auto-sizer in [`crate::build::simulate_passenger_counts`] budgets the same
//! bays for the same reason, so a preset's seat count and the detailed layout
//! agree.
//!
//! # Why the pitch is stretched
//!
//! Every deck's pitch is scaled up (never down) so the seated block spans
//! the whole available floor. A real high-density layout spreads its seats
//! across the entire cabin up to whichever limit binds; it does not bunch them
//! forward and leave bare floor at the back. Where the requested count already
//! needs the whole floor the multiplier is one and the truncation below is
//! what decides the count instead.

use std::collections::VecDeque;

use alas_config::{CertifiedExitLayout, PassengerCabinConfig, SeatClassConfig};

use super::{
    abreast, abreast_and_aisles, cabin_deck_segments, ceil_div, effective_pair_capacity,
    max_certifiable_capacity_reference_compatibility, max_certifiable_capacity_with_source_layout,
    min_exit_pairs, seat_blocks, select_exit_type, Bay, DeckCapacities, MIN_PITCH, MIN_SEAT_WIDTH,
    MONUMENT_LEN, SEAT_BOX_H,
};
use crate::geometry::CabinGeometry;
use crate::layout::{DeckItem, ItemKind, ItemMeta, SeatMeta};

/// One class of the cabin, and how much of it is left to place.
pub(super) struct CabinClass {
    /// The class name, which is also the seat row's label.
    pub name: &'static str,
    /// Its geometry and occupant mass.
    pub config: SeatClassConfig,
    /// Seats still to be placed.
    pub remaining: i64,
    /// Seats placed so far.
    pub seated: i64,
}

/// What the seating pass produced.
pub(super) struct Seating {
    /// The seat rows, in placement order.
    pub items: Vec<DeckItem>,
    /// The monument anchors, in the order they were carved out.
    pub bays: Vec<Bay>,
    /// Fraction of each deck's floor the seated block spans.
    pub deck_utilization: Vec<(&'static str, f64)>,
    /// Seats placed on each passenger deck.
    pub deck_seated: Vec<(&'static str, i64)>,
    /// The widest row placed.
    pub max_abreast: i64,
    /// The most aisles any row needed.
    pub max_aisles: i64,
    /// The exit-derived ceiling the seating was truncated against.
    pub deck_caps: DeckCapacities,
    /// The exit and available-floor ceiling before an aircraft-source cap.
    pub geometric_deck_caps: DeckCapacities,
}

impl Seating {
    /// Seats placed on one deck.
    pub fn on_deck(&self, name: &str) -> i64 {
        self.deck_seated
            .iter()
            .find(|(deck, _)| *deck == name)
            .map_or(0, |&(_, seated)| seated)
    }
}

/// The classes to lay out, in cabin order.
///
/// A cabin where no class carries a seat count is not an empty aeroplane: it
/// is one whose class mix has not been decided yet, and it falls back to a
/// single economy cabin sized to the requested passenger count, keeping
/// economy's own geometry.
pub(super) fn resolve_classes(
    pax: &PassengerCabinConfig,
    num_passengers: i64,
    product_interior: bool,
) -> Vec<CabinClass> {
    // The product allocates the three supported classes. The frozen reference
    // layouts were generated with the premium-economy slot still active, so
    // the compatibility interior resolves it too, in its historical position.
    let declared: Vec<(&'static str, &SeatClassConfig)> = if product_interior {
        pax.classes()
    } else {
        [
            ("First", &pax.first),
            ("Business", &pax.business),
            ("Premium", &pax.premium),
            ("Economy", &pax.economy),
        ]
        .into_iter()
        .filter(|(_, class)| class.is_present())
        .collect()
    };
    if declared.is_empty() {
        let single = SeatClassConfig {
            count: num_passengers,
            ..pax.economy.clone()
        };
        return vec![CabinClass {
            name: "Economy",
            remaining: single.count,
            seated: 0,
            config: single,
        }];
    }
    declared
        .into_iter()
        .map(|(name, class)| CabinClass {
            name,
            remaining: class.count,
            seated: 0,
            config: class.clone(),
        })
        .collect()
}

/// Pack `classes` into the passenger decks, front to back.
pub(super) fn place_seats(
    g: &CabinGeometry,
    pax: &PassengerCabinConfig,
    classes: &mut [CabinClass],
    aisle_w: f64,
    product_exit_capacity: bool,
    source_capacity_cap: Option<i64>,
    source_exit_layout: Option<CertifiedExitLayout>,
) -> Seating {
    let geometric_deck_caps = if product_exit_capacity {
        max_certifiable_capacity_with_source_layout(g, pax, source_exit_layout, None)
    } else {
        max_certifiable_capacity_reference_compatibility(g, pax)
    };
    let deck_caps = geometric_deck_caps.with_source_cap(source_capacity_cap);
    let segments = cabin_deck_segments(g);
    let exit_spec = select_exit_type(g.diameter_m);
    let est_cap = if let Some(source_exit_layout) = source_exit_layout {
        source_exit_layout
            .pairs
            .iter()
            .map(|pair| pair.capacity_per_pair)
            .max()
            .unwrap_or(1)
    } else if product_exit_capacity {
        effective_pair_capacity(exit_spec, pax)
    } else {
        exit_spec.capacity_per_pair
    };

    let mut items = Vec::new();
    let mut bays = Vec::new();
    let mut deck_utilization = Vec::new();
    let mut deck_seated = Vec::new();
    let mut max_abreast = 0;
    let mut max_aisles = 1;
    let mut ci = 0usize;

    for segment in &segments {
        let deck = segment.deck;
        let deck_cap = deck_caps.for_deck(deck.name);

        let (sim_len, sim_seated, sim_class_bays) = simulate_segment(
            g, classes, ci, aisle_w, deck_cap, segment.x0, segment.x1, deck,
        );

        // One bay between each adjacent door pair, charged against the seating
        // length before the seats are placed rather than discovered afterwards.
        let n_pairs_est = if sim_seated == 0 {
            1
        } else {
            min_exit_pairs(sim_seated).max(ceil_div(sim_seated, est_cap))
        };
        let n_mid = (n_pairs_est - 1).max(0);

        // The loop below also carves a bay at every class boundary it crosses,
        // and the simulation reports how many that is. Charging only the
        // door bays here would stretch the pitch over floor the class bays
        // then take, so the block would run past the aft bay and the last row
        // would be truncated: the fast auto-sizer
        // (`crate::build::count_deck`) budgets `classes + 1 + n_mid` bays for
        // exactly this reason, and the two product passes have to agree.
        // The frozen Python replay charged only the door bays, so the
        // compatibility interior keeps that budget and its item-for-item
        // parity fixture with it.
        let n_charged_bays = if product_exit_capacity {
            n_mid + sim_class_bays
        } else {
            n_mid
        };

        let l_avail = segment.x1 - segment.x0 - 2.0 * MONUMENT_LEN;
        let seating_room = (l_avail - n_charged_bays as f64 * MONUMENT_LEN).max(0.0);
        let pitch_stretch = if sim_len > 0.0 {
            (seating_room / sim_len).max(1.0)
        } else {
            1.0
        };
        let block_len = sim_len * pitch_stretch + n_charged_bays as f64 * MONUMENT_LEN;
        deck_utilization.push((
            deck.name,
            if l_avail > 0.0 {
                (block_len / l_avail).min(1.0)
            } else {
                0.0
            },
        ));

        // Seating starts immediately behind the forward bay. Any floor the
        // block does not need is left aft, near the rear bulkhead, which is
        // where a real aircraft's spare cabin length is; centring the block
        // would put half of it forward and drag the payload CG with it.
        let mut x = segment.x0 + MONUMENT_LEN;
        let block_start = x;
        let mut mid_stations: VecDeque<f64> = (1..=n_mid)
            .map(|i| block_start + block_len * (i as f64 / n_pairs_est as f64))
            .collect();

        bays.push(Bay::new(
            segment.x0 + MONUMENT_LEN / 2.0,
            deck.name,
            g.usable_width(deck, segment.x0),
        ));

        let mut seated_here = 0i64;
        while ci < classes.len() && x < segment.x1 - MONUMENT_LEN && seated_here < deck_cap {
            if mid_stations
                .front()
                .is_some_and(|station| x >= station - 1e-6)
            {
                bays.push(Bay::new(
                    x + MONUMENT_LEN / 2.0,
                    deck.name,
                    g.usable_width(deck, x),
                ));
                x += MONUMENT_LEN;
                mid_stations.pop_front();
                continue;
            }
            if classes[ci].remaining <= 0 {
                ci += 1;
                if ci < classes.len() {
                    bays.push(Bay::new(
                        x + MONUMENT_LEN / 2.0,
                        deck.name,
                        g.usable_width(deck, x),
                    ));
                    x += MONUMENT_LEN;
                }
                continue;
            }

            let class = &classes[ci];
            let (row_abreast, n_aisles) = abreast_and_aisles(&class.config, deck, g, aisle_w, x);
            let seats_row = row_abreast.min(class.remaining).min(deck_cap - seated_here);
            let usable = g.usable_width(deck, x);
            let pitch = class.config.pitch_m.max(MIN_PITCH) * pitch_stretch;
            let seat_w = class.config.width_m.max(MIN_SEAT_WIDTH);
            let blocks = seat_blocks(row_abreast, n_aisles);
            max_abreast = max_abreast.max(row_abreast);
            max_aisles = max_aisles.max(n_aisles);

            items.push(DeckItem {
                kind: ItemKind::SeatRow,
                deck: deck.name,
                x: x + pitch / 2.0,
                y: 0.0,
                z: g.item_z(deck, x, SEAT_BOX_H),
                length: pitch,
                width: usable,
                mass: seats_row as f64 * class.config.mass_per_pax_kg,
                height: g.clamp_height(deck, x, SEAT_BOX_H),
                label: class.name.to_owned(),
                meta: ItemMeta::Seat(SeatMeta {
                    cls: class.name,
                    abreast: row_abreast,
                    filled: seats_row,
                    deck: deck.name,
                    aisles: n_aisles,
                    blocks,
                    seat_w,
                    aisle_w,
                }),
            });

            classes[ci].remaining -= seats_row;
            classes[ci].seated += seats_row;
            seated_here += seats_row;
            x += pitch;
        }

        bays.push(Bay::new(
            segment.x1 - MONUMENT_LEN / 2.0,
            deck.name,
            g.usable_width(deck, segment.x1 - MONUMENT_LEN),
        ));
        deck_seated.push((deck.name, seated_here));
    }

    Seating {
        items,
        bays,
        deck_utilization,
        deck_seated,
        max_abreast,
        max_aisles,
        deck_caps,
        geometric_deck_caps,
    }
}

/// Seat one deck stretch without placing anything, to find the length the
/// block will want, the count the door sizing keys off, and the number of
/// class-boundary bays the real pass will carve out of the same floor.
///
/// This runs at the configured pitch: the stretch factor it feeds is derived
/// from the length it reports, so applying it here would be circular. The
/// reported length is seats only; the bays are returned separately because
/// they are charged at full size whatever the pitch does.
#[allow(clippy::too_many_arguments)] // The simulation reads the same seven
                                     // quantities the real pass does; bundling them into a struct used once would
                                     // name the pass's own locals twice.
fn simulate_segment(
    g: &CabinGeometry,
    classes: &[CabinClass],
    start_class: usize,
    aisle_w: f64,
    deck_cap: i64,
    x0: f64,
    x1: f64,
    deck: &crate::geometry::DeckSpec,
) -> (f64, i64, i64) {
    let mut remaining: Vec<i64> = classes.iter().map(|class| class.remaining).collect();
    let mut ci = start_class;
    let mut x = x0 + MONUMENT_LEN;
    let mut length = 0.0;
    let mut seated = 0i64;
    let mut class_bays = 0i64;

    while ci < classes.len() && x < x1 - MONUMENT_LEN && seated < deck_cap {
        if remaining[ci] <= 0 {
            ci += 1;
            if ci < classes.len() {
                x += MONUMENT_LEN;
                class_bays += 1;
            }
            continue;
        }
        let class = &classes[ci].config;
        let row = abreast(class, deck, g, aisle_w, x);
        let seats = row.min(remaining[ci]).min(deck_cap - seated);
        let pitch = class.pitch_m.max(MIN_PITCH);
        length += pitch;
        remaining[ci] -= seats;
        seated += seats;
        x += pitch;
    }
    (length, seated, class_bays)
}
