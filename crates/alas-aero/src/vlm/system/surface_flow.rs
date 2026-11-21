// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Surfaces flown at a dynamic-pressure ratio.
//!
//! A wing whose [`Wing::dynamic_pressure_ratio`] `r` is not one (the horizontal
//! tail in the wing wake and the fuselage flow) carries `r` of the lift,
//! moment and induced drag it would carry in the free stream at the same
//! angles: the textbook `q_t = eta q` with the downwash gradient unchanged.
//! The lattice is solved as it is (the angles, including the downwash the wing
//! induces at the surface, are those of the free stream) and the surface's
//! results are scaled afterwards: its panel forces by `r` and its circulation,
//! which the Trefftz-plane drag reads, by `sqrt(r)`, so that its own induced
//! drag scales with `r` too. A surface with ratio exactly one takes none of
//! these steps, so every other solve is bit-identical.

use super::*;

/// The per-panel dynamic-pressure ratio of one solve.
pub(super) struct SurfaceFlow {
    ratio: Vec<f64>,
}

impl SurfaceFlow {
    pub(super) fn new(airplane: &Airplane, panels: &[Panel]) -> Self {
        Self {
            ratio: panels
                .iter()
                .map(|panel| airplane.wings[panel.wing_index].dynamic_pressure_ratio)
                .collect(),
        }
    }

    /// The factor on panel `index`'s force.
    pub(super) fn force_scale(&self, index: usize) -> f64 {
        self.ratio[index]
    }

    /// The circulations the results report: the solved ones, with every
    /// reduced surface's scaled by the square root of its ratio.
    pub(super) fn reported_circulations(&self, solved: Vec<f64>) -> Vec<f64> {
        if self.ratio.iter().all(|&r| r == 1.0) {
            return solved;
        }
        solved
            .into_iter()
            .zip(&self.ratio)
            .map(|(gamma, &r)| if r == 1.0 { gamma } else { gamma * r.sqrt() })
            .collect()
    }
}
