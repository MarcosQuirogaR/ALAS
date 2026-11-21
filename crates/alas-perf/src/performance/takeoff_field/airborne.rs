// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The air leg of the take-off: from lift-off to the 35 ft screen at no less
//! than `V2`, climbing from the lift-off position of the wing.

use super::{Model, G, SCREEN_HEIGHT_M, TRANSITION_LOAD_FACTOR};

/// Smallest net gradient of the air leg: below it the leg has no screen.
const MIN_GRADIENT: f64 = 0.002;
/// Simpson cells of the energy integral of the air leg (even).
const AIR_STEPS: usize = 8;

impl Model<'_> {
    /// Distance from lift-off at `vlof` to 35 ft at no less than `V2`, m.
    ///
    /// The energy height `E = h + V^2/2g` rises from lift-off to the screen
    /// at the net gradient `gamma = (T - D)/W`, `dE/ds = gamma`; speed and
    /// height are taken to rise together in proportion to the energy, so
    /// the distance is `Delta E` times the mean of `1 / gamma(f)` over the
    /// energy fraction `f` (Simpson, [`AIR_STEPS`] cells) with the drag at
    /// each point's own height (ground effect) and speed. Without ground
    /// effect and a constant gradient this is Torenbeek Eq. 5-85/5-86. The
    /// wing climbs from its lift-off position ([`Self::liftoff_rise`]).
    pub(super) fn air(&self, vlof: f64, one_engine_out: bool) -> f64 {
        let v2 = self.inputs.speeds.v2_ms.max(vlof);
        let base = self.liftoff_rise(vlof);
        let gamma_c = self.gradient(v2, one_engine_out, base.raised(SCREEN_HEIGHT_M));
        let mut inverse_mean = 0.0;
        for k in 0..=AIR_STEPS {
            let f = k as f64 / AIR_STEPS as f64;
            let v = (vlof * vlof + f * (v2 * v2 - vlof * vlof)).sqrt();
            let gamma = self.gradient(v, one_engine_out, base.raised(f * SCREEN_HEIGHT_M));
            if !gamma.is_finite() || gamma <= MIN_GRADIENT {
                return f64::INFINITY;
            }
            let weight = if k == 0 || k == AIR_STEPS {
                1.0
            } else if k % 2 == 1 {
                4.0
            } else {
                2.0
            };
            inverse_mean += weight / gamma;
        }
        inverse_mean /= 3.0 * AIR_STEPS as f64;
        let energy = ((v2 * v2 - vlof * vlof) / (2.0 * G) + SCREEN_HEIGHT_M) * inverse_mean;
        let angle = gamma_c.min(0.99).asin();
        let radius = (0.5 * (vlof + v2)).powi(2) / ((TRANSITION_LOAD_FACTOR - 1.0) * G);
        let arc_height = radius * (1.0 - angle.cos());
        let kinematic = if arc_height >= SCREEN_HEIGHT_M {
            (2.0 * radius * SCREEN_HEIGHT_M - SCREEN_HEIGHT_M.powi(2)).sqrt()
        } else {
            radius * angle.sin() + (SCREEN_HEIGHT_M - arc_height) / angle.tan()
        };
        energy.max(kinematic)
    }
}
