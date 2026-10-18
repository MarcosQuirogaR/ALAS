// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lever A2 (`speedups::Lever::WarmStart`): spend closure work in proportion
//! to the distance from the fixed point.
//!
//! While the outer takeoff-mass iteration is far from its fixed point, a
//! re-trim at the current centre of gravity is thrown away by the next pass
//! (the mass keeps moving, and the centre of gravity with it), and the
//! dispatch trips are flown to a tolerance far tighter than the iterate's own
//! error. A *far* pass therefore (i) defers the re-trim to the first pass
//! that is no longer far and (ii) solves its dispatch to a tolerance of
//! [`INNER_FRACTION`] of the previous change instead of the configured one
//! (an inexact fixed-point iteration: the inner error stays well below the
//! outer step). No far pass can declare convergence, and every pass that can
//! (change below the configured tolerance, dispatch at the configured
//! tolerance, polar within `retrim_cg_tolerance_pct_mac` of the final centre
//! of gravity) is evaluated exactly as before, so the closed state satisfies
//! the same criteria as without the lever; only the path to it is shorter.

use crate::mdo::speedups::{enabled, Lever};

/// A pass is far from the fixed point while the previous outer change exceeds
/// this multiple of the configured tolerance (kg per kg of tolerance). At
/// 50 kg of mass change the centre of gravity moves by roughly 0.05 % MAC,
/// half of the default re-trim tolerance.
const FAR_FACTOR: f64 = 50.0;

/// Dispatch tolerance of a far pass, as a fraction of the previous outer
/// change; never below the configured tolerance.
const INNER_FRACTION: f64 = 0.05;

/// The outer loop's view of its own convergence.
pub(super) struct PassSchedule {
    tolerance_kg: f64,
    previous_change_kg: Option<f64>,
    active: bool,
}

impl PassSchedule {
    pub(super) fn new(tolerance_kg: f64) -> Self {
        Self {
            tolerance_kg,
            previous_change_kg: None,
            active: enabled(Lever::WarmStart),
        }
    }

    /// Whether the next pass is far from the fixed point.
    pub(super) fn is_far(&self) -> bool {
        self.active
            && self
                .previous_change_kg
                .is_some_and(|change| change > FAR_FACTOR * self.tolerance_kg)
    }

    /// The dispatch tolerance of the next pass, kg.
    pub(super) fn dispatch_tolerance_kg(&self) -> f64 {
        match self.previous_change_kg {
            Some(change) if self.is_far() => (INNER_FRACTION * change).max(self.tolerance_kg),
            _ => self.tolerance_kg,
        }
    }

    /// Record the outer change of the pass just finished, kg.
    pub(super) fn record(&mut self, change_kg: f64) {
        self.previous_change_kg = Some(change_kg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn far_passes_defer_and_relax_and_near_passes_are_exact() {
        let mut schedule = PassSchedule::new(1.0);
        schedule.active = true;
        assert!(!schedule.is_far());
        assert!((schedule.dispatch_tolerance_kg() - 1.0).abs() < 1e-12);
        schedule.record(1_000.0);
        assert!(schedule.is_far());
        assert!((schedule.dispatch_tolerance_kg() - 50.0).abs() < 1e-12);
        schedule.record(51.0);
        assert!(schedule.is_far());
        assert!((schedule.dispatch_tolerance_kg() - 2.55).abs() < 1e-12);
        schedule.record(50.0);
        assert!(!schedule.is_far());
        assert!((schedule.dispatch_tolerance_kg() - 1.0).abs() < 1e-12);
        schedule.record(f64::NAN);
        assert!(!schedule.is_far());
    }
}
