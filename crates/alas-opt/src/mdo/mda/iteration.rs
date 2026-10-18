// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Admissible extrapolation of the takeoff-mass fixed point.

use crate::mdo::types::CandidateFailure;

/// Consecutive passes whose dispatch stalled at the evaluable edge, the takeoff mass
/// moving by at most [`EDGE_STALL_MASS_FRACTION`] of itself, after which the sizing loop
/// stops (engineering choice: the third pass repeats the first two's status; the cycle of
/// the AVE and A380 wide-body probes was period 3 and never closed in 30 passes).
pub(super) const EDGE_STALL_PASSES: u32 = 3;

/// Largest change of the takeoff mass between passes, as a fraction of the pass mass, that
/// still counts as the same stalled edge (the mass-model feedback moved the edge 0.04 to
/// 0.16 % per pass in the probes).
pub(super) const EDGE_STALL_MASS_FRACTION: f64 = 0.01;

/// Aitken delta-squared extrapolation of the fixed-point iterates
/// `x0 -> x1 -> x2`, or `None` when the denominator vanishes or the
/// estimate leaves `(0, ceiling]`. Iterates and ceiling are masses, kg.
pub(super) fn aitken(x0: f64, x1: f64, x2: f64, ceiling: f64) -> Option<f64> {
    let denominator = (x2 - x1) - (x1 - x0);
    if denominator.abs() < 1e-9 {
        return None;
    }
    let estimate = x2 - (x2 - x1) * (x2 - x1) / denominator;
    (estimate.is_finite() && estimate > 0.0 && estimate <= ceiling).then_some(estimate)
}

/// A failure with the `mass_coordinates` reason, for the one arm of
/// [`converge`] that keeps it panic-free.
///
/// This is deliberately **not** the seam for a station-placement failure.
/// Every mass and coordinate result this loop consumes comes from
/// `build::mass_analysis_with_structural_feedback`, which already classifies
/// a missing main-gear station as `main_gear_station_not_measured` and
/// propagates it with `?`; the candidate is rejected there, before the loop
/// body runs. The arm below is reached only if `max_passes` were zero, which
/// the caller's own `max(1)` prevents, so it names no physical cause at all.
pub(super) fn mass_coordinates_failure() -> CandidateFailure {
    CandidateFailure {
        reason: "mass_coordinates",
    }
}

#[cfg(test)]
mod tests {
    use super::aitken;

    #[test]
    fn aitken_reaches_the_fixed_point_of_a_linear_contraction_in_one_step() {
        // x -> 0.25 x + 75 has the fixed point 100; three iterates from 0
        // extrapolate to it exactly.
        let g = |x: f64| 0.25 * x + 75.0;
        let x0 = 0.0;
        let x1 = g(x0);
        let x2 = g(x1);
        let estimate = aitken(x0, x1, x2, 1_000.0).unwrap_or_else(|| panic!("admissible"));
        assert!((estimate - 100.0).abs() < 1e-9);
    }

    #[test]
    fn a_stalled_or_out_of_range_extrapolation_is_declined() {
        assert!(aitken(1.0, 1.0, 1.0, 10.0).is_none());
        assert!(aitken(0.0, 10.0, 15.0, 12.0).is_none());
    }
}
