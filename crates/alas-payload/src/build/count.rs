// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/payload.py (`simulate_passenger_counts`, the
// per-deck row count)

//! The generic counting pass of the auto-sizer: rows per class on one deck
//! stretch whose doors are unknown.

use alas_config::{PassengerCabinConfig, SeatClassConfig};

use super::PassengerCounts;
use crate::cabin::{abreast, service_reserve_len, MIN_PITCH, MONUMENT_LEN};
use crate::geometry::CabinGeometry;
use crate::numeric::round_half_even;

/// Rounding slack on the row-length budget, so a class whose rows divide its
/// share exactly is not denied its last row by a floating-point remainder.
const BUDGET_EPSILON: f64 = 1e-9;

/// One deck stretch, as the counting pass reads it.
pub(super) struct Deck<'a> {
    pub geometry: &'a CabinGeometry,
    pub spec: &'a crate::geometry::DeckSpec,
    pub x0: f64,
    pub total_length: f64,
    pub cap: i64,
}

/// Seat one deck with `n_mid_bays` extra monument bays charged against it.
///
/// The row-length budget is shared across the whole deck rather than split
/// into per-class sections. Splitting it and truncating each class
/// independently throws away up to one row's worth of floor in *every* class,
/// which compounds to several rows across a four-class cabin. Earlier classes
/// take their share rounded to the nearest whole row, so the rounding averages
/// out instead of always undershooting, and the last class absorbs whatever is
/// left, which is how an airline actually sets an exact business row count
/// and lets economy fill the rest.
pub(super) fn count_deck(
    deck: &Deck<'_>,
    pax: &PassengerCabinConfig,
    classes: &[(&str, f64)],
    mix: &[(&str, f64)],
    aisle_w: f64,
    n_mid_bays: i64,
) -> (PassengerCounts, i64) {
    let mut local = PassengerCounts::default();
    let mut seated = 0i64;

    let bays = classes.len() as i64 + 1 + n_mid_bays;
    let l_seating = deck.total_length
        - bays as f64 * MONUMENT_LEN
        - service_reserve_len(deck.total_length, mix);
    if l_seating <= 0.0 {
        return (local, seated);
    }

    let mut x = deck.x0 + MONUMENT_LEN;
    let mut remaining = l_seating;
    for (i, &(name, share)) in classes.iter().enumerate() {
        let class = class_config(pax, name);
        let pitch = class.pitch_m.max(MIN_PITCH);
        let is_last = i == classes.len() - 1;
        let budget = if is_last {
            remaining
        } else {
            let rows = (round_half_even(share * l_seating / pitch) as i64).max(0);
            remaining.min(rows as f64 * pitch)
        };

        let mut n_rows = 0i64;
        while n_rows as f64 * pitch + pitch <= budget + BUDGET_EPSILON && seated < deck.cap {
            let row = abreast(class, deck.spec, deck.geometry, aisle_w, x).min(deck.cap - seated);
            local.add(name, row);
            seated += row;
            x += pitch;
            n_rows += 1;
        }
        remaining -= n_rows as f64 * pitch;
        if !is_last {
            x += MONUMENT_LEN;
        }
    }
    (local, seated)
}

/// The class slot a mix name refers to.
pub(super) fn class_config<'a>(pax: &'a PassengerCabinConfig, name: &str) -> &'a SeatClassConfig {
    match name {
        "First" => &pax.first,
        "Business" => &pax.business,
        "Premium" => &pax.premium,
        _ => &pax.economy,
    }
}
