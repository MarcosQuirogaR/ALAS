// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Placing grids, and remembering which rib point each one came from.
//!
//! Two lookups are needed while a mesh is built, and they are not the same
//! lookup. Elements are written from `(rib, chordwise point, surface)`, so
//! [`NodeMap`] answers that. Grids themselves are deduplicated by *coordinate*,
//! because a rib's leading-edge point and its pinched trailing edge are one
//! point on both surfaces, and writing them twice would leave the skin sewn to
//! itself through a pair of coincident, unconnected grids.
//!
//! The rivets in [`super::rivets`] iterate the map in the order grids were
//! placed, and that order decides which of two equally distant skin grids a
//! rivet reaches for. It is therefore recorded rather than left to a hash.

use std::collections::HashMap;

use super::cards::Deck;

/// Which surface of a rib a point sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Surface {
    /// The upper surface.
    Ext,
    /// The lower surface.
    Int,
}

/// A rib's chordwise point on one of its two surfaces.
pub(super) type PointKey = (usize, usize, Surface);

/// Where every rib point ended up, and in what order.
#[derive(Debug, Default)]
pub(super) struct NodeMap {
    by_point: HashMap<PointKey, i64>,
    order: Vec<(PointKey, i64)>,
    by_coordinate: HashMap<[u64; 3], i64>,
    next_id: i64,
}

impl NodeMap {
    /// An empty map, numbering grids from one.
    pub(super) fn new() -> Self {
        Self {
            next_id: 1,
            ..Default::default()
        }
    }

    /// Place `xyz` for rib point `key`, reusing an existing grid when one is
    /// already at that coordinate.
    pub(super) fn place(&mut self, deck: &mut Deck, key: PointKey, xyz: [f64; 3]) {
        let coordinate = coordinate_key(xyz);
        let nid = match self.by_coordinate.get(&coordinate) {
            Some(&existing) => existing,
            None => {
                let nid = deck.add_grid(self.next_id, xyz);
                self.next_id += 1;
                self.by_coordinate.insert(coordinate, nid);
                nid
            }
        };
        self.by_point.insert(key, nid);
        self.order.push((key, nid));
    }

    /// The grid at rib point `key`, if that point was placed.
    pub(super) fn get(&self, key: PointKey) -> Option<i64> {
        self.by_point.get(&key).copied()
    }

    /// The grid at rib point `key`.
    ///
    /// Returns zero -- never a valid identifier, since numbering starts at one
    /// -- for a point that was not placed. Every call site indexes a point it
    /// has just counted, so the fallback exists to keep a mesh defect from
    /// becoming a panic rather than because it can be reached.
    pub(super) fn at(&self, rib: usize, point: usize, surface: Surface) -> i64 {
        self.get((rib, point, surface)).unwrap_or(0)
    }

    /// Every placement, in the order it was made. A grid shared by two rib
    /// points appears once per point.
    pub(super) fn placements(&self) -> &[(PointKey, i64)] {
        &self.order
    }
}

/// The key two coincident rib points have to agree on.
///
/// Rounded to a micrometre before hashing, matching the reference's own
/// six-decimal rounding: two points that a spline evaluated separately arrive a
/// few ulps apart and have to be recognized as the same point anyway. Negative
/// zero is folded onto zero so a coordinate that rounded down to it from either
/// side hashes the same.
fn coordinate_key(xyz: [f64; 3]) -> [u64; 3] {
    [
        (round6(xyz[0]) + 0.0).to_bits(),
        (round6(xyz[1]) + 0.0).to_bits(),
        (round6(xyz[2]) + 0.0).to_bits(),
    ]
}

/// `value` rounded to six decimal places -- Python's `round(value, 6)`.
///
/// Python rounds a halfway case to even and this rounds it away from zero.
/// These are node coordinates in metres, produced by a spline evaluation, and
/// the two rules can only disagree about a value that lands exactly on a
/// micrometre-and-a-half boundary; the parity fixture would show it as a grid
/// count that differs by one.
fn round6(value: f64) -> f64 {
    (value * 1e6).round() / 1e6
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_rib_points_a_few_ulps_apart_share_one_grid() {
        let mut deck = Deck::new();
        let mut nodes = NodeMap::new();
        nodes.place(&mut deck, (0, 0, Surface::Ext), [1.0, 2.0, 3.0]);
        nodes.place(&mut deck, (0, 0, Surface::Int), [1.0, 2.0, 3.0 + 1e-12]);

        assert_eq!(deck.grids().len(), 1);
        assert_eq!(nodes.at(0, 0, Surface::Ext), 1);
        assert_eq!(nodes.at(0, 0, Surface::Int), 1);
        assert_eq!(nodes.placements().len(), 2);
    }

    #[test]
    fn points_a_millimetre_apart_stay_separate_grids() {
        let mut deck = Deck::new();
        let mut nodes = NodeMap::new();
        nodes.place(&mut deck, (0, 0, Surface::Ext), [0.0, 0.0, 0.0]);
        nodes.place(&mut deck, (0, 1, Surface::Ext), [0.0, 0.0, 0.001]);

        assert_eq!(deck.grids().len(), 2);
        assert_eq!(nodes.at(0, 1, Surface::Ext), 2);
    }

    #[test]
    fn a_coordinate_that_rounded_to_negative_zero_hashes_as_zero() {
        assert_eq!(coordinate_key([-1e-9, 0.0, 0.0]), coordinate_key([0.0; 3]));
    }

    #[test]
    fn an_unplaced_point_reads_back_as_an_identifier_no_grid_can_have() {
        let nodes = NodeMap::new();
        assert_eq!(nodes.get((3, 1, Surface::Int)), None);
        assert_eq!(nodes.at(3, 1, Surface::Int), 0);
    }
}
