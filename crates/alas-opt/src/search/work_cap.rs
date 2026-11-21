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
/// the nominal's own closure does not close, in which case there is no
/// reference work and the search runs uncapped.
pub(crate) fn nominal_work_cap(
    objective: &DesignObjective,
    nominal: &DesignVector,
) -> Option<SizingBudget> {
    nominal_reference(objective, nominal).cap
}

/// What the search takes from the nominal design's own closure, measured
/// once before the search with the default controls, so it is the same at
/// any worker count and on any machine.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct NominalReference {
    /// The per-candidate work cap ([`nominal_work_cap`]).
    pub cap: Option<SizingBudget>,
    /// The engine a clean-sheet sizing scaled to the nominal's thrust
    /// requirements, when the nominal closed.
    pub sized_engine: Option<alas_config::SizedEngine>,
}

/// The nominal's closure under `objective` ([`NominalReference`]).
pub(crate) fn nominal_reference(
    objective: &DesignObjective,
    nominal: &DesignVector,
) -> NominalReference {
    let Ok(assessment) =
        assess_candidate_with_controls(objective, &nominal.to_array(), SizingControls::default())
    else {
        return NominalReference::default();
    };
    let closed = assessment.sized.sizing_closed;
    NominalReference {
        cap: reference_work(closed, assessment.sized.work).map(work_cap),
        sized_engine: assessment.resolved.sized_engine.filter(|_| closed),
    }
}

/// The closure work a cap is a multiple of: the nominal's, only when its
/// closure closed. A nominal that stops short (a dispatch stalled at the edge
/// of what the aircraft can fly, a failed burn model, a mass that does not
/// settle) spends a fraction of an ordinary closure: a clean-sheet 72.5 t
/// twin's nominal stopped after 5 trip flights, its closed candidates took
/// 15-24, and a cap of 25 rejected 27 of 32 candidates as
/// `sizing_budget_exhausted` before their closure was judged.
fn reference_work(closed: bool, work: SizingWork) -> Option<SizingWork> {
    closed.then_some(work)
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

    #[test]
    fn only_a_closed_nominal_sets_the_cap() {
        let work = SizingWork {
            plan_freezes: 1,
            trip_flights: 5,
            deck_evals: 140_000,
        };
        assert_eq!(reference_work(true, work), Some(work));
        assert_eq!(reference_work(false, work), None);
    }
}
