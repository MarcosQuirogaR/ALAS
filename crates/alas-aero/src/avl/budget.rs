// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Horseshoe-vortex and strip budget of a native AVL deck against the solver's compiled array bounds.
//!
//! Reference: AVL 3.52 source, `ADIMEN.INC` (`NVMAX = 5000`, `NSMAX = 500`) and
//! `amake.f` (`MAKESURF`, spanwise spacing per section interval).

use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::Wing;

/// AVL's compiled horseshoe-vortex array bound (`NVMAX` in `ADIMEN.INC` of the
/// distributed 3.x source). A deck above it makes the solver stop with
/// "SDUPL: Vortex array overflow" before any output exists.
pub const AVL_VORTEX_LIMIT: usize = 5000;

/// Planning target kept under [`AVL_VORTEX_LIMIT`]: AVL rounds the spanwise
/// panels of each SECTION interval, so the realised count can exceed the
/// nominal product by a few strips.
pub const AVL_VORTEX_TARGET: usize = AVL_VORTEX_LIMIT * 95 / 100;

/// AVL's compiled strip array bound (`NSMAX` in `ADIMEN.INC`). Every spanwise
/// strip of every surface, `YDUPLICATE` images included, counts against it;
/// a deck above it stops with "SDUPL: Strip array overflow".
pub const AVL_STRIP_LIMIT: usize = 500;

/// Planning target kept under [`AVL_STRIP_LIMIT`].
pub const AVL_STRIP_TARGET: usize = AVL_STRIP_LIMIT * 95 / 100;

/// Arc length of each SECTION interval in the surface's y-z trace, the
/// quantity `MAKESURF` uses to place section stations (`YZLEN`).
fn interval_lengths(wing: &Wing) -> Vec<f64> {
    wing.xsecs
        .windows(2)
        .map(|pair| {
            let dy = pair[1].xyz_le[1] - pair[0].xyz_le[1];
            let dz = pair[1].xyz_le[2] - pair[0].xyz_le[2];
            dy.hypot(dz)
        })
        .collect()
}

/// Spanwise vortices per SECTION interval for one surface when about `span`
/// vortices are spread over the surface in proportion to interval arc length,
/// with at least one per interval.
///
/// The deck gives AVL these counts section by section instead of one
/// surface-wide `Nspan`. With a surface-wide count `MAKESURF` snaps the
/// spacing nodes to the sections and stops ("Cannot adjust spanwise spacing
/// at section", `amake.f`) as soon as two adjacent sections share the same
/// nearest node, which the closely spaced strip-edge sections of the refined
/// ALAS loft do at any practical `Nspan`.
pub fn interval_vortex_counts(wing: &Wing, span: usize) -> Vec<usize> {
    let lengths = interval_lengths(wing);
    let total: f64 = lengths.iter().sum();
    lengths
        .iter()
        .map(|&length| {
            if total > 0.0 && total.is_finite() {
                let share = span as f64 * length / total;
                (share.round() as usize).max(1)
            } else {
                1
            }
        })
        .collect()
}

/// Spanwise strips AVL builds for a deck, images included.
pub fn avl_strip_count(airplane: &Airplane, spanwise: usize) -> usize {
    airplane
        .wings
        .iter()
        .filter(|wing| !wing.is_geometry_only())
        .map(|wing| {
            let strips: usize = interval_vortex_counts(wing, spanwise).iter().sum();
            strips.saturating_mul(if wing.symmetric { 2 } else { 1 })
        })
        .fold(0_usize, usize::saturating_add)
}

/// Horseshoe vortices AVL builds for a deck: per surface, chordwise x spanwise
/// panels (spanwise as allocated per SECTION interval), twice for a
/// `YDUPLICATE` image surface.
pub fn avl_vortex_count(airplane: &Airplane, chordwise: usize, spanwise: usize) -> usize {
    chordwise.saturating_mul(avl_strip_count(airplane, spanwise))
}

/// Reduce the requested lattice until it fits [`AVL_VORTEX_TARGET`] and
/// [`AVL_STRIP_TARGET`].
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
    let fits = |chord: usize, span: usize| {
        avl_vortex_count(airplane, chord, span) <= AVL_VORTEX_TARGET
            && avl_strip_count(airplane, span) <= AVL_STRIP_TARGET
    };
    // The strip bound does not depend on the chordwise count, so reducing
    // chordwise panels cannot satisfy it; only the vortex bound is walked first.
    while chord > floor && avl_vortex_count(airplane, chord, span) > AVL_VORTEX_TARGET {
        chord -= 1;
    }
    while span > 1 && !fits(chord, span) {
        span -= 1;
    }
    while chord > 1 && !fits(chord, span) {
        chord -= 1;
    }
    (chord, span)
}
