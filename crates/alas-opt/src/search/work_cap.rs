// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The per-candidate work cap of the native product search.
//!
//! A generation of the refinement waits for its slowest candidate, so one
//! candidate whose sizing closure keeps re-flying its missions stalls every
//! lane. The search therefore sizes each candidate under a
//! [`SizingBudget`] of [`WORK_CAP_MULTIPLE`] times the work the nominal
//! design's own closure spent, measured once before the search starts. A
//! candidate that exhausts it is rejected as `sizing_budget_exhausted`, which
//! ranks it in the not-closed tier: its residual table would describe an
//! unconverged aircraft.
//!
//! The counters are deterministic, so the cap, and which candidates it
//! rejects, are the same at any worker count and on any machine.

use alas_config::design_variables::DesignVector;

use crate::mdo::mission_model::SizingBudget;
use crate::mdo::{assess_candidate_with_controls, SizingControls, SizingWork};
use crate::DesignObjective;

/// Multiple of the nominal design's closure work one candidate may spend
/// (engineering choice). Over 60 seeded screening candidates per preset
/// (A320-200, B787-9, A380-800, ATR72-600, `screening_rank_correlation`),
/// the most expensive candidate spent at most 1.6 times the nominal's trip
/// flights and 2.1 times its deck evaluations (both B787-9), so five times
/// leaves every ordinary candidate untouched and stops only a closure that
/// keeps re-planning.
pub const WORK_CAP_MULTIPLE: u32 = 5;

/// The budget [`WORK_CAP_MULTIPLE`] times `nominal`. The outer-pass count
/// stays with the configured `sizing_max_iterations`.
#[must_use]
pub fn work_cap(nominal: SizingWork) -> SizingBudget {
    let multiple = u64::from(WORK_CAP_MULTIPLE);
    SizingBudget {
        max_trip_flights: u32::try_from(nominal.trip_flights.max(1).saturating_mul(multiple))
            .unwrap_or(u32::MAX),
        max_deck_evals: nominal.deck_evals.max(1).saturating_mul(multiple),
        max_outer_passes: u32::MAX,
    }
}

/// The cap for a search under `objective` around `nominal`, or `None` when
/// the nominal itself does not size, in which case there is no reference
/// work and the search runs uncapped.
pub(crate) fn nominal_work_cap(
    objective: &DesignObjective,
    nominal: &DesignVector,
) -> Option<SizingBudget> {
    assess_candidate_with_controls(objective, &nominal.to_array(), SizingControls::default())
        .ok()
        .map(|assessment| work_cap(assessment.sized.work))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cap_is_the_multiple_of_the_nominal_and_never_zero() {
        let cap = work_cap(SizingWork {
            plan_freezes: 3,
            trip_flights: 40,
            deck_evals: 90_000,
        });
        assert_eq!(cap.max_trip_flights, 40 * WORK_CAP_MULTIPLE);
        assert_eq!(cap.max_deck_evals, 90_000 * u64::from(WORK_CAP_MULTIPLE));
        assert_eq!(cap.max_outer_passes, u32::MAX);
        let idle = work_cap(SizingWork::default());
        assert!(idle.max_trip_flights > 0 && idle.max_deck_evals > 0);
        let huge = work_cap(SizingWork {
            trip_flights: u64::MAX,
            ..SizingWork::default()
        });
        assert_eq!(huge.max_trip_flights, u32::MAX);
    }
}
