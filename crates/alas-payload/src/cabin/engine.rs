// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/cabin_layout.py (`build_passenger_layout`)
// Reference: alas @ rust-port-baseline.

//! The passenger layout engine: seats, monuments, exits and baggage assembled
//! into one interior.
//!
//! The four passes run in a fixed order because each depends on the last. The
//! seating carves out the bays; the monuments and exits fill them; the baggage
//! is trimmed toward the centre of gravity the seating produced. The order they
//! append their items in is part of the result -- a deck plan walking the list
//! differently would draw monuments over seats -- so it is reproduced exactly.

use alas_config::{DesignRequirements, PassengerCabinConfig};

use super::fittings::{place_baggage, place_exits, place_monuments};
use super::resolve_aisle_width;
use super::seating::{place_seats, resolve_classes};
use crate::geometry::CabinGeometry;
use crate::layout::{
    mass_properties, ItemKind, LayoutSummary, Mode, PassengerSummary, PayloadLayout,
};
use crate::numeric::round_to_digit;

/// Decimal places the deck utilization percentage is reported to.
const UTILIZATION_DIGITS: usize = 1;

/// Build the passenger interior for a cabin configuration on a fuselage.
///
/// The operating-empty mass and its balance are not arguments here, although
/// the dispatcher accepts them: only the freighter loader needs them, to solve
/// the payload centre of gravity backwards from a target *aircraft* centre of
/// gravity. A passenger cabin trims its bags toward the seating instead, which
/// it can see for itself.
pub fn build_passenger_layout(
    g: &CabinGeometry,
    pax: &PassengerCabinConfig,
    req: &DesignRequirements,
) -> PayloadLayout {
    let mut classes = resolve_classes(pax, req.num_passengers);
    let total_pax: i64 = classes.iter().map(|class| class.config.count).sum();
    let aisle_w = resolve_aisle_width(pax, total_pax);

    let mut seating = place_seats(g, pax, &mut classes, aisle_w);
    let seated: i64 = classes.iter().map(|class| class.seated).sum();

    let mut items = std::mem::take(&mut seating.items);
    let (monuments, monument_counts) = place_monuments(g, pax, &mut seating.bays, total_pax);
    items.extend(monuments);
    let exits = place_exits(g, &seating);
    items.extend(exits.items);

    // The bags follow the passengers, so the trim target is the seating's own
    // balance. An empty cabin has none, and the middle of it is the neutral
    // answer rather than the datum.
    let (seat_mass, seat_cg) = seat_mass_and_cg(&items, g);
    let bags = place_baggage(g, pax, req, seated, seat_mass, seat_cg);
    items.extend(bags.items);

    let (total_mass, cg_x, cg_y) = mass_properties(&items);
    let summary = PassengerSummary {
        total_pax,
        seated_pax: seated,
        classes: classes
            .iter()
            .map(|class| (class.name, class.seated))
            .collect(),
        lavatories: monument_counts.lavatories,
        galleys: monument_counts.galleys,
        exit_type: exits.exit_type,
        exit_pairs: exits.pairs,
        exit_capacity: exits.pairs * exits.capacity_per_side * 2,
        max_certifiable_capacity: seating.deck_caps.total,
        payload_t: total_mass / 1000.0,
        seat_mass_t: seat_mass / 1000.0,
        bag_mass_t: bags.bag_mass / 1000.0,
        belly_cargo_t: bags.belly_cargo / 1000.0,
        hold_capacity_t: bags.hold_capacity / 1000.0,
        hold_used_t: bags.hold_used / 1000.0,
        hold_ulds: bags.hold_ulds,
        aisle_width_m: aisle_w,
        max_abreast: seating.max_abreast,
        n_aisles: seating.max_aisles,
        deck_utilization: seating
            .deck_utilization
            .iter()
            .map(|&(deck, used)| (deck, round_to_digit(100.0 * used, UTILIZATION_DIGITS)))
            .collect(),
        cg_pct_mac: if total_mass > 0.0 {
            g.x_to_pct_mac(cg_x)
        } else {
            0.0
        },
        double_deck: g.is_double_deck,
    };

    PayloadLayout {
        mode: Mode::Passenger,
        items,
        total_mass,
        cg_x,
        cg_y,
        summary: LayoutSummary::Passenger(Box::new(summary)),
    }
}

/// The seated mass and where it balances, over the seat rows alone.
fn seat_mass_and_cg(items: &[crate::layout::DeckItem], g: &CabinGeometry) -> (f64, f64) {
    let mut mass = 0.0;
    let mut moment = 0.0;
    for item in items.iter().filter(|item| item.kind == ItemKind::SeatRow) {
        mass += item.mass;
        moment += item.mass * item.x;
    }
    if mass > 0.0 {
        (mass, moment / mass)
    } else {
        (mass, 0.5 * (g.cabin_start_x + g.cabin_end_x))
    }
}
