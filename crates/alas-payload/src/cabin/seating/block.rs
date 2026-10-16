// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One deck block walked at a given pitch stretch, without touching the
//! classes: the pure row walk the seating pass runs, and the one it runs again
//! to back the stretch off when spreading the rows costs seats.

use std::collections::VecDeque;

use super::CabinClass;
use crate::cabin::{abreast_and_aisles, MIN_PITCH, MONUMENT_LEN};
use crate::geometry::CabinGeometry;

/// Bisection steps of the pitch back-off: 2^-20 of the stretch range is far
/// below a millimetre of row placement.
pub(super) const STRETCH_BACKOFF_ITERATIONS: usize = 20;

/// One row a block placed.
pub(super) struct BlockRow {
    pub(super) class: usize,
    pub(super) x0: f64,
    pub(super) pitch: f64,
    pub(super) seats: i64,
    pub(super) abreast: i64,
    pub(super) aisles: i64,
}

/// What one deck block holds at a given pitch stretch.
pub(super) struct PlacedBlock {
    pub(super) rows: Vec<BlockRow>,
    /// Starts of the mid-cabin and class-boundary bays, in placement order.
    pub(super) bay_stations: Vec<f64>,
    pub(super) seated: i64,
    /// First class with seats still to place.
    pub(super) class_end: usize,
    /// Station the walk stopped at.
    pub(super) x_end: f64,
}

/// A deck stretch to be seated front to back, and everything the pass fixes
/// before the pitch is chosen.
pub(super) struct BlockPlan<'a> {
    pub(super) g: &'a CabinGeometry,
    pub(super) deck: &'a crate::geometry::DeckSpec,
    pub(super) x0: f64,
    pub(super) x1: f64,
    pub(super) aisle_w: f64,
    pub(super) deck_cap: i64,
    pub(super) start_class: usize,
    /// Seat length of the unstretched simulation, m.
    pub(super) sim_len: f64,
    pub(super) n_charged_bays: i64,
    pub(super) n_pairs_est: i64,
    pub(super) n_mid: i64,
}

impl BlockPlan<'_> {
    /// Walk the rows aft at `stretch` times each class's pitch, with the
    /// mid-cabin bays at their even share of the block, without touching the
    /// classes.
    pub(super) fn run(&self, classes: &[CabinClass], stretch: f64) -> PlacedBlock {
        let (g, deck) = (self.g, self.deck);
        let block_len = self.sim_len * stretch + self.n_charged_bays as f64 * MONUMENT_LEN;
        let mut x = self.x0 + MONUMENT_LEN;
        let block_start = x;
        let mut mid_stations: VecDeque<f64> = (1..=self.n_mid)
            .map(|i| block_start + block_len * (i as f64 / self.n_pairs_est as f64))
            .collect();
        let mut remaining: Vec<i64> = classes.iter().map(|class| class.remaining).collect();
        let mut ci = self.start_class;
        let mut placed = PlacedBlock {
            rows: Vec::new(),
            bay_stations: Vec::new(),
            seated: 0,
            class_end: ci,
            x_end: x,
        };
        while ci < classes.len() && x < self.x1 - MONUMENT_LEN && placed.seated < self.deck_cap {
            if mid_stations
                .front()
                .is_some_and(|station| x >= station - 1e-6)
            {
                placed.bay_stations.push(x);
                x += MONUMENT_LEN;
                mid_stations.pop_front();
                continue;
            }
            if remaining[ci] <= 0 {
                ci += 1;
                if ci < classes.len() {
                    placed.bay_stations.push(x);
                    x += MONUMENT_LEN;
                }
                continue;
            }

            let class = &classes[ci];
            let (row_abreast, n_aisles) =
                abreast_and_aisles(&class.config, deck, g, self.aisle_w, x);
            let seats_row = row_abreast
                .min(remaining[ci])
                .min(self.deck_cap - placed.seated);
            let pitch = class.config.pitch_m.max(MIN_PITCH) * stretch;
            placed.rows.push(BlockRow {
                class: ci,
                x0: x,
                pitch,
                seats: seats_row,
                abreast: row_abreast,
                aisles: n_aisles,
            });
            remaining[ci] -= seats_row;
            placed.seated += seats_row;
            x += pitch;
        }
        placed.class_end = ci;
        placed.x_end = x;
        placed
    }
}
