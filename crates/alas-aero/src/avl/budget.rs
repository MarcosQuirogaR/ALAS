// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Horseshoe-vortex budget of a native AVL deck against the solver's compiled array bound.

use alas_geom::aircraft::airplane::Airplane;

/// AVL's compiled horseshoe-vortex array bound (`NVMAX` in `AVL.INC` of the
/// distributed 3.x source). A deck above it makes the solver stop with
/// "SDUPL: Vortex array overflow" before any output exists.
pub const AVL_VORTEX_LIMIT: usize = 5000;

/// Planning target kept under [`AVL_VORTEX_LIMIT`]: AVL rounds the spanwise
/// panels of each SECTION interval, so the realised count can exceed the
/// nominal product by a few strips.
pub const AVL_VORTEX_TARGET: usize = AVL_VORTEX_LIMIT * 95 / 100;

/// Horseshoe vortices AVL builds for a deck: per surface, chordwise x spanwise
/// panels (spanwise raised to four per section station as in the deck), twice
/// for a `YDUPLICATE` image surface.
pub fn avl_vortex_count(airplane: &Airplane, chordwise: usize, spanwise: usize) -> usize {
    airplane
        .wings
        .iter()
        .map(|wing| {
            let span = spanwise.max(wing.xsecs.len().saturating_mul(4));
            chordwise
                .saturating_mul(span)
                .saturating_mul(if wing.symmetric { 2 } else { 1 })
        })
        .fold(0_usize, usize::saturating_add)
}

/// Reduce the requested lattice until it fits [`AVL_VORTEX_TARGET`].
///
/// Chordwise panels are reduced first, down to `chordwise_floor` (the
/// converged 8-chord audit mesh); only then are spanwise panels reduced, and
/// finally chordwise panels below the floor. The result may still exceed the
/// target for pathological layouts; `render_geometry` rejects those.
pub fn fit_vortex_budget(
    airplane: &Airplane,
    chordwise: usize,
    spanwise: usize,
    chordwise_floor: usize,
) -> (usize, usize) {
    let (mut chord, mut span) = (chordwise.max(1), spanwise.max(1));
    let floor = chordwise_floor.clamp(1, chord);
    while chord > floor && avl_vortex_count(airplane, chord, span) > AVL_VORTEX_TARGET {
        chord -= 1;
    }
    while span > 1 && avl_vortex_count(airplane, chord, span) > AVL_VORTEX_TARGET {
        span -= 1;
    }
    while chord > 1 && avl_vortex_count(airplane, chord, span) > AVL_VORTEX_TARGET {
        chord -= 1;
    }
    (chord, span)
}
