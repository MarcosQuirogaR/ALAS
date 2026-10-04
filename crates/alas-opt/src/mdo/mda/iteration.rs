// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Admissible extrapolation of the takeoff-mass fixed point.

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
