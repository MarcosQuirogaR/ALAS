// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from MINPACK-1's enorm.f.
// Upstream: MINPACK-1 (Argonne National Laboratory, 1980), public domain,
// as vendored in SciPy 1.11.4 and reached through scipy.optimize.fsolve.
// Reference: alas @ rust-port-baseline.

//! MINPACK's Euclidean norm, which is not `x.iter().map(sq).sum().sqrt()`.
//!
//! It splits the vector into three magnitude classes -- below `3.834e-20`,
//! above `1.304e19 / n`, and the ordinary range between -- and accumulates
//! each with its own running maximum, so that a vector containing one huge or
//! one tiny component neither overflows nor loses the rest to underflow.
//!
//! The reason to reproduce the split rather than call a straightforward norm
//! is not robustness, which the mission's residuals never need: it is that
//! the three-accumulator form and the naive sum differ in the last ulp on
//! ordinary data, because they add the same terms in a different order and
//! with a different scaling. `hybrd` compares norms against `xtol` and
//! against each other to decide whether to accept a step, shrink the trust
//! region or rebuild the Jacobian, so a last-ulp difference in a norm is a
//! branch that can go the other way, and from there the two implementations
//! are running different iterations. That is the difference this row exists
//! to rule out.

/// Components at or below this are accumulated against their own running
/// maximum, so that squaring them cannot underflow to zero.
const RDWARF: f64 = 3.834e-20;
/// Divided by the length, the threshold above which squaring could overflow.
const RGIANT: f64 = 1.304e19;

/// The Euclidean norm of `x`, by MINPACK's three-accumulator method.
pub(super) fn enorm(x: &[f64]) -> f64 {
    let mut s1 = 0.0_f64;
    let mut s2 = 0.0_f64;
    let mut s3 = 0.0_f64;
    let mut x1max = 0.0_f64;
    let mut x3max = 0.0_f64;
    let agiant = RGIANT / x.len() as f64;

    for &value in x {
        let xabs = value.abs();
        if xabs > RDWARF && xabs < agiant {
            // Intermediate components: the ordinary running sum of squares.
            s2 += xabs * xabs;
        } else if xabs <= RDWARF {
            // Small components, scaled by the largest small one seen so far.
            if xabs <= x3max {
                if xabs != 0.0 {
                    s3 += (xabs / x3max) * (xabs / x3max);
                }
            } else {
                s3 = 1.0 + s3 * (x3max / xabs) * (x3max / xabs);
                x3max = xabs;
            }
        } else if xabs <= x1max {
            // Large components, scaled by the largest one seen so far.
            s1 += (xabs / x1max) * (xabs / x1max);
        } else {
            s1 = 1.0 + s1 * (x1max / xabs) * (x1max / xabs);
            x1max = xabs;
        }
    }

    if s1 != 0.0 {
        x1max * (s1 + (s2 / x1max) / x1max).sqrt()
    } else if s2 != 0.0 {
        if s2 >= x3max {
            (s2 * (1.0 + (x3max / s2) * (x3max * s3))).sqrt()
        } else {
            (x3max * ((s2 / x3max) + (x3max * s3))).sqrt()
        }
    } else {
        x3max * s3.sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_norm_of_an_ordinary_vector_agrees_with_the_naive_sum_of_squares() {
        let x = [3.0, 4.0];
        assert!((enorm(&x) - 5.0).abs() < 1e-15);
    }

    #[test]
    fn the_norm_of_the_zero_vector_is_zero() {
        assert_eq!(enorm(&[0.0, 0.0, 0.0]), 0.0);
    }

    #[test]
    fn a_vector_of_tiny_components_keeps_a_norm_the_naive_sum_would_underflow_to_zero() {
        // Squaring 1e-200 underflows to zero in `f64`, so the naive sum
        // reports a norm of zero for a vector that plainly has one.
        let x = [1e-200, 1e-200];
        let naive: f64 = x.iter().map(|v| v * v).sum::<f64>().sqrt();
        assert_eq!(naive, 0.0);
        assert!((enorm(&x) - 1e-200 * std::f64::consts::SQRT_2).abs() < 1e-215);
    }

    #[test]
    fn a_vector_of_huge_components_keeps_a_norm_the_naive_sum_would_overflow_to_infinity() {
        let x = [1e200, 1e200];
        let naive: f64 = x.iter().map(|v| v * v).sum::<f64>().sqrt();
        assert!(naive.is_infinite());
        assert!((enorm(&x) / (1e200 * std::f64::consts::SQRT_2) - 1.0).abs() < 1e-14);
    }

    #[test]
    fn a_mixture_of_magnitude_classes_reaches_every_accumulator() {
        // One component from each class, so all three of `s1`, `s2` and `s3`
        // are nonzero and the final assembly takes its first branch.
        let x = [1e200, 1.0, 1e-200];
        assert!((enorm(&x) / 1e200 - 1.0).abs() < 1e-14);
    }
}
