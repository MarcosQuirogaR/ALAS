// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Bracketing of a climb or descent rate between the engine's two limits.
//!
//! A configured vertical rate is a guidance target. The aircraft can fly it
//! only between the engine's two ends: a climb needs at most the maximum
//! thrust, a descent at least the flight-idle thrust. When the target is
//! refused the honest rate is the *boundary*: the fastest climb the engine
//! delivers, the steepest descent flight idle allows. A single fixed cut of
//! the target (the earlier halving) lands far inside that boundary and
//! produces a shallow glide or a crawling climb that no pilot flies.
//!
//! [`RateSearch`] keeps, per segment, the largest rate known to be flyable
//! and the smallest known to be refused, and proposes the next rate to try:
//! a modest cut while nothing is known to be flyable, then bisection until
//! the bracket is narrower than [`RELATIVE_TOLERANCE`].

/// First cut applied to a refused rate while no flyable rate is known.
pub(super) const FIRST_CUT_FACTOR: f64 = 0.85;

/// First cut applied to a refused climb rate. A climb is split where it first
/// saturates and its upper piece flown at this share of the refused rate, so
/// the climb is a staircase under the true maximum-thrust curve; the finer the
/// step the smaller the shortfall (about half a step on average: 2.5 % here,
/// 7.5 % at the descent factor).
pub(super) const CLIMB_CUT_FACTOR: f64 = 0.95;

/// Cumulative share of the refused rate the modest cuts may remove before the
/// search falls back to halving, so an aircraft that cannot fly the segment at
/// any useful rate is still given up on within a few revisions.
const MODEST_CUT_FLOOR: f64 = 0.6;

/// Cut applied once the modest cuts are spent.
const FALLBACK_CUT_FACTOR: f64 = 0.5;

/// The bracket is closed when its width is below this fraction of the
/// refused rate (a rate within 4 % of the engine boundary is the boundary
/// for a conceptual-design mission).
pub(super) const RELATIVE_TOLERANCE: f64 = 0.04;

/// Width, m/s, below which a descent's bracket between a rate refused for
/// lack of thrust and a larger rate refused below flight idle is closed: no
/// constant rate between them flies the leg. A physical resolution chosen up
/// front (0.01 m/s is 2 ft/min, below any rate the leg's time, distance or
/// fuel distinguishes), not a published value and not a share of the rate: a
/// relative width says nothing about whether a constant rate exists inside it.
pub(super) const WINDOW_CLOSED_TOLERANCE_M_S: f64 = 0.01;

/// Largest flyable and smallest refused rate of one segment, m/s.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct RateSearch {
    flyable: Option<f64>,
    refused: Option<f64>,
    cuts: u32,
    cut_factor: f64,
}

impl Default for RateSearch {
    fn default() -> Self {
        Self::with_cut_factor(FIRST_CUT_FACTOR)
    }
}

impl RateSearch {
    /// A search that cuts a refused rate by `cut_factor` while nothing is
    /// known to be flyable.
    pub(super) fn with_cut_factor(cut_factor: f64) -> Self {
        Self {
            flyable: None,
            refused: None,
            cuts: 0,
            cut_factor,
        }
    }

    /// The search of a leg: climbs cut finely, descents coarsely.
    pub(super) fn for_climb(climb: bool) -> Self {
        Self::with_cut_factor(if climb {
            CLIMB_CUT_FACTOR
        } else {
            FIRST_CUT_FACTOR
        })
    }

    /// A segment whose rate `refused_m_s` is already known to be refused.
    pub(super) fn refused_at(refused_m_s: f64, cut_factor: f64) -> Self {
        Self {
            refused: Some(refused_m_s),
            ..Self::with_cut_factor(cut_factor)
        }
    }

    /// Record that the segment was flown at `rate_m_s`.
    pub(super) fn record_flown(&mut self, rate_m_s: f64) {
        if self.refused.is_some_and(|refused| rate_m_s >= refused) {
            // The boundary moved (an earlier segment changed the mass or the
            // speed it starts from); the old refusal no longer brackets it.
            self.refused = None;
        }
        self.flyable = Some(self.flyable.map_or(rate_m_s, |known| known.max(rate_m_s)));
    }

    /// The smallest rate known to be refused, m/s.
    pub(super) fn refused_m_s(&self) -> Option<f64> {
        self.refused
    }

    /// Whether a flyable rate is known.
    pub(super) fn has_flyable(&self) -> bool {
        self.flyable.is_some()
    }

    /// Record that the segment was refused at `rate_m_s`.
    pub(super) fn record_refused(&mut self, rate_m_s: f64) {
        self.cuts += 1;
        if self.flyable.is_some_and(|flyable| rate_m_s <= flyable) {
            self.flyable = None;
        }
        self.refused = Some(self.refused.map_or(rate_m_s, |known| known.min(rate_m_s)));
    }

    /// The rate to fly next: a modest cut of the refused rate while nothing
    /// is known to be flyable, the midpoint of the bracket while it is wider
    /// than the tolerance, and otherwise the largest flyable rate (the
    /// engine boundary, to within the tolerance).
    pub(super) fn target(&self) -> Option<f64> {
        match (self.flyable, self.refused) {
            (Some(flyable), Some(refused)) => {
                Some(if refused - flyable > RELATIVE_TOLERANCE * refused {
                    0.5 * (flyable + refused)
                } else {
                    flyable
                })
            }
            (Some(flyable), None) => Some(flyable),
            (None, Some(refused)) => Some(refused * self.cut_factor()),
            (None, None) => None,
        }
    }

    fn cut_factor(&self) -> f64 {
        let modest = (MODEST_CUT_FLOOR.ln() / self.cut_factor.ln()).floor();
        if f64::from(self.cuts) < modest {
            self.cut_factor
        } else {
            FALLBACK_CUT_FACTOR
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_rate_is_cut_modestly_then_bisected_to_the_tolerance() {
        let mut search = RateSearch::default();
        search.record_refused(10.0);
        assert_eq!(search.target(), Some(8.5));
        search.record_flown(8.5);
        assert_eq!(search.target(), Some(9.25));
        search.record_refused(9.25);
        assert_eq!(search.target(), Some(8.875));
        search.record_flown(8.875);
        assert_eq!(search.target(), Some(9.0625));
        search.record_flown(9.0625);
        // 9.25 - 9.0625 is below 4 % of 9.25: the bracket is closed on the
        // flyable side.
        assert_eq!(search.target(), Some(9.0625));
    }

    #[test]
    fn a_flyable_rate_above_the_old_refusal_reopens_the_bracket() {
        let mut search = RateSearch::refused_at(5.0, FIRST_CUT_FACTOR);
        search.record_flown(6.0);
        assert_eq!(search.target(), Some(6.0));
    }

    #[test]
    fn a_refusal_at_or_below_the_flyable_rate_discards_it() {
        let mut search = RateSearch::default();
        search.record_flown(4.0);
        search.record_refused(3.0);
        assert_eq!(search.target(), Some(3.0 * FIRST_CUT_FACTOR));
    }

    #[test]
    fn repeated_refusals_fall_back_to_halving() {
        let mut search = RateSearch::default();
        for rate in [10.0, 8.5, 7.225] {
            search.record_refused(rate);
        }
        assert_eq!(search.target(), Some(7.225 * FALLBACK_CUT_FACTOR));
    }
}
