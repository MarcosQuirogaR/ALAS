// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! How tall and how wide a monument can stand against the lining.

use super::*;

/// How tall a monument of `length` at `x` whose outboard edge stands `y_out`
/// from the centre line can be, m: the deck's headroom, or the height at which
/// the lined section closes over that edge where that is lower (a box cannot
/// follow the sidewall's curve, so a monument at the wall is lower than one
/// beside the aisle). At least the seat-box height the bay was sized for.
pub(super) fn monument_height(
    g: &CabinGeometry,
    deck: &crate::geometry::DeckSpec,
    x: f64,
    length: f64,
    y_out: f64,
) -> f64 {
    let headroom = g.clamp_height(deck, x, SEAT_BOX_H);
    if !g.enforces_physical_envelope() {
        return headroom;
    }
    let mut height = g.deck_height(deck, x);
    for dx in [-0.5, 0.0, 0.5] {
        let xi = x + dx * length;
        let top = g.lining_top_z(xi, y_out).unwrap_or(f64::NEG_INFINITY);
        height = height.min(top - g.floor_z(deck, xi));
        height = height.min(g.ceil_z(deck, xi) - g.floor_z(deck, xi));
    }
    height.max(headroom)
}

/// The width a monument of `length` at `x` can stand in at the height the bay
/// is sized for (the seat box's, which `monument_height` never goes under): the
/// narrowest wall-inset chord at the floor and at its top, at both ends.
/// Unlimited on the frozen frame.
pub(super) fn contained_width(
    g: &CabinGeometry,
    deck: &crate::geometry::DeckSpec,
    x: f64,
    length: f64,
) -> f64 {
    contained_width_to(g, deck, x, length, SEAT_BOX_H)
}

/// [`contained_width`] for an item `top` high (clamped to the headroom).
pub(super) fn contained_width_to(
    g: &CabinGeometry,
    deck: &crate::geometry::DeckSpec,
    x: f64,
    length: f64,
    top: f64,
) -> f64 {
    if !g.enforces_physical_envelope() {
        return f64::INFINITY;
    }
    let height = g.clamp_height(deck, x, top);
    let mut width = f64::INFINITY;
    for dx in [-0.5, 0.0, 0.5] {
        let xi = x + dx * length;
        for z in [g.floor_z(deck, xi), g.floor_z(deck, xi) + height] {
            width = width.min(g.lining_chord_at_z(xi, z));
        }
    }
    width
}
