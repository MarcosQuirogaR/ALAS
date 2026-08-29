// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from MINPACK-1's dogleg.f.
// Upstream: MINPACK-1 (Argonne National Laboratory, 1980), public domain,
// as vendored in SciPy 1.11.4 and reached through scipy.optimize.fsolve.
// Reference: alas @ rust-port-baseline.

//! Powell's dogleg: the step `hybrd` takes inside a trust region of radius
//! `delta`.
//!
//! Newton's step solves the linearized system exactly and is what you want
//! when the linearization is trustworthy; the steepest-descent step is short
//! and reliable and is what you want when it is not. The dogleg picks between
//! them geometrically rather than by testing: if the Newton step fits inside
//! the trust region, take it; if even the descent step overshoots, take the
//! descent direction truncated to the boundary; otherwise take the point
//! where the segment joining the two crosses the boundary.
//!
//! That last case is the one that makes this a "dogleg" -- the path bends --
//! and it is the one whose closed form below looks arbitrary. It is the
//! positive root of a quadratic in the blending parameter, arranged by the
//! Fortran so that every intermediate is a ratio of comparable magnitudes,
//! which is why the expression is written in terms of `sgnorm / delta` and
//! `delta / qnorm` rather than being simplified.
//!
//! `r` is the packed upper triangle described in [`super::qr`], and `diag` is
//! `hybrd`'s scaling vector: the trust region is a ball in the scaled
//! variables `diag * x`, not in `x`, so that unknowns of different physical
//! magnitude -- a throttle near one and a body angle in radians near a
//! hundredth -- are stepped comparably.

use super::enorm::enorm;

/// Compute the dogleg step into `x`, for the trust-region radius `delta`.
///
/// `qtb` is `Q^T` times the residual, the right-hand side in the rotated
/// frame that `r` triangularizes.
pub(super) fn dogleg(n: usize, r: &[f64], diag: &[f64], qtb: &[f64], delta: f64, x: &mut [f64]) {
    let mut wa1 = vec![0.0_f64; n];
    let mut wa2 = vec![0.0_f64; n];

    // The Gauss-Newton direction, by back substitution through `r`.
    // Indices are one-based to keep the packed-triangle arithmetic legible.
    let mut jj = (n * (n + 1)) / 2 + 1;
    for k in 1..=n {
        let j = n - k + 1;
        jj -= k;
        let mut l = jj + 1;
        let mut sum = 0.0;
        for i in (j + 1)..=n {
            sum += r[l - 1] * x[i - 1];
            l += 1;
        }
        let mut temp = r[jj - 1];
        if temp == 0.0 {
            // A zero pivot: substitute a scaled measure of the column so the
            // division below is finite rather than infinite.
            let mut l = j;
            for i in 1..=j {
                temp = temp.max(r[l - 1].abs());
                l += n - i;
            }
            temp *= super::EPSMCH;
            if temp == 0.0 {
                temp = super::EPSMCH;
            }
        }
        x[j - 1] = (qtb[j - 1] - sum) / temp;
    }

    // If the Gauss-Newton step fits inside the trust region, it is the answer.
    for j in 0..n {
        wa1[j] = 0.0;
        wa2[j] = diag[j] * x[j];
    }
    let qnorm = enorm(&wa2);
    if qnorm <= delta {
        return;
    }

    // It does not fit. Form the scaled gradient direction `R^T qtb`.
    let mut l = 1;
    for j in 1..=n {
        let temp = qtb[j - 1];
        for i in j..=n {
            wa1[i - 1] += r[l - 1] * temp;
            l += 1;
        }
        wa1[j - 1] /= diag[j - 1];
    }

    let gnorm = enorm(&wa1);
    let mut sgnorm = 0.0;
    let mut alpha = delta / qnorm;

    if gnorm != 0.0 {
        // The point along the scaled gradient minimizing the quadratic.
        for j in 0..n {
            wa1[j] = (wa1[j] / gnorm) / diag[j];
        }
        let mut l = 1;
        for j in 1..=n {
            let mut sum = 0.0;
            for i in j..=n {
                sum += r[l - 1] * wa1[i - 1];
                l += 1;
            }
            wa2[j - 1] = sum;
        }
        let temp = enorm(&wa2);
        sgnorm = (gnorm / temp) / temp;

        alpha = 0.0;
        if sgnorm < delta {
            // Both legs are in play: blend them at the trust-region boundary.
            let bnorm = enorm(qtb);
            let dq = delta / qnorm;
            let sd = sgnorm / delta;
            let mut temp = (bnorm / gnorm) * (bnorm / qnorm) * sd;
            temp = temp - dq * sd * sd
                + ((temp - dq) * (temp - dq) + (1.0 - dq * dq) * (1.0 - sd * sd)).sqrt();
            alpha = (dq * (1.0 - sd * sd)) / temp;
        }
    }

    // The convex combination of the two directions.
    let temp = (1.0 - alpha) * sgnorm.min(delta);
    for j in 0..n {
        x[j] = temp * wa1[j] + alpha * x[j];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `R = I`, so the Gauss-Newton step is `qtb` itself.
    fn identity_triangle(n: usize) -> Vec<f64> {
        let mut r = Vec::with_capacity(n * (n + 1) / 2);
        for i in 0..n {
            for j in i..n {
                r.push(if i == j { 1.0 } else { 0.0 });
            }
        }
        r
    }

    #[test]
    fn a_newton_step_inside_the_trust_region_is_taken_whole() {
        let n = 3;
        let r = identity_triangle(n);
        let diag = vec![1.0; n];
        let qtb = vec![0.3, -0.4, 0.5];
        let mut x = vec![0.0; n];
        // The Newton step has norm sqrt(0.5) ~ 0.707, well inside.
        dogleg(n, &r, &diag, &qtb, 10.0, &mut x);
        for (got, want) in x.iter().zip(qtb.iter()) {
            assert!((got - want).abs() < 1e-14);
        }
    }

    #[test]
    fn a_step_is_never_longer_than_the_trust_region_allows() {
        let n = 3;
        let r = identity_triangle(n);
        let diag = vec![1.0; n];
        let qtb = vec![3.0, -4.0, 5.0];
        for &delta in &[0.05, 0.5, 2.0, 5.0] {
            let mut x = vec![0.0; n];
            dogleg(n, &r, &diag, &qtb, delta, &mut x);
            let scaled: Vec<f64> = x.iter().zip(diag.iter()).map(|(v, d)| d * v).collect();
            let length = enorm(&scaled);
            assert!(
                length <= delta * (1.0 + 1e-12),
                "step of {length} exceeds delta {delta}"
            );
        }
    }

    #[test]
    fn the_scaling_vector_measures_the_trust_region_in_scaled_variables() {
        // Doubling one unknown's scale halves how far the step may move it.
        let n = 2;
        let r = identity_triangle(n);
        let qtb = vec![1.0, 1.0];
        let delta = 0.5;

        let mut even = vec![0.0; n];
        dogleg(n, &r, &[1.0, 1.0], &qtb, delta, &mut even);
        let mut skewed = vec![0.0; n];
        dogleg(n, &r, &[1.0, 4.0], &qtb, delta, &mut skewed);

        assert!(skewed[1].abs() < even[1].abs());
    }

    #[test]
    fn a_zero_pivot_produces_a_finite_step_rather_than_an_infinite_one() {
        // The substitution guarding the back substitution's division is
        // unreachable from a well-conditioned Jacobian and is what keeps a
        // singular one from producing a step of infinities.
        let n = 2;
        let r = vec![0.0, 0.0, 0.0];
        let diag = vec![1.0; n];
        let qtb = vec![1.0, 1.0];
        let mut x = vec![0.0; n];
        dogleg(n, &r, &diag, &qtb, 1.0, &mut x);
        assert!(x.iter().all(|v| v.is_finite()));
    }
}
