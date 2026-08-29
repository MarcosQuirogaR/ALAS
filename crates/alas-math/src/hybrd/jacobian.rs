// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from MINPACK-1's fdjac1.f.
// Upstream: MINPACK-1 (Argonne National Laboratory, 1980), public domain,
// as vendored in SciPy 1.11.4 and reached through scipy.optimize.fsolve.
// Reference: alas @ rust-port-baseline.

//! The forward-difference Jacobian, one column per unknown.
//!
//! # Scope
//!
//! `fdjac1` has two branches, dense and banded, chosen by whether
//! `ml + mu + 1` covers the whole system. `scipy.optimize.fsolve` selects the
//! banded one through its `band` argument, and mission analysis model's `converge_root` never
//! passes it -- SciPy then sends `ml = mu = -10`, which its C wrapper clips to
//! `n - 1`, making `ml + mu + 1 = 2n - 1` and taking the dense branch on every
//! call. So only the dense branch is translated. This was confirmed against
//! the reference rather than inferred: the fixture's logged evaluations show
//! `n` probes per Jacobian, each perturbing exactly one unknown.
//!
//! The step is relative -- `sqrt(eps) * |x[j]|`, falling back to `sqrt(eps)`
//! where the unknown is zero -- which is the standard compromise between the
//! truncation error of a one-sided difference and the cancellation error of
//! subtracting two nearly equal residuals.

/// Fill the column-major `n`-by-`n` `fjac` with forward differences of
/// `residual` about `x`, given the already-evaluated `fvec` there.
///
/// `x` is restored before returning; it is taken mutably only so that each
/// probe can perturb it in place rather than copying the whole vector `n`
/// times.
pub(super) fn fdjac1<F>(
    n: usize,
    residual: &mut F,
    x: &mut [f64],
    fvec: &[f64],
    fjac: &mut [f64],
    epsfcn: f64,
) where
    F: FnMut(&[f64], &mut [f64]),
{
    let eps = epsfcn.max(super::EPSMCH).sqrt();
    let mut probe = vec![0.0_f64; n];

    for j in 0..n {
        let saved = x[j];
        let mut h = eps * saved.abs();
        if h == 0.0 {
            h = eps;
        }
        x[j] = saved + h;
        residual(x, &mut probe);
        x[j] = saved;
        for i in 0..n {
            fjac[i + j * n] = (probe[i] - fvec[i]) / h;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_columns_approximate_the_partial_derivatives_of_a_known_system() {
        // f = [x0^2 + x1, 3*x0 - x1^3], so J = [[2*x0, 1], [3, -3*x1^2]].
        let n = 2;
        let mut f = |x: &[f64], out: &mut [f64]| {
            out[0] = x[0] * x[0] + x[1];
            out[1] = 3.0 * x[0] - x[1] * x[1] * x[1];
        };
        let mut x = vec![2.0, -1.5];
        let mut fvec = vec![0.0; n];
        f(&x, &mut fvec);

        let mut fjac = vec![0.0; n * n];
        fdjac1(n, &mut f, &mut x, &fvec, &mut fjac, f64::EPSILON);

        let expected = [4.0, 3.0, 1.0, -6.75];
        for (k, want) in expected.iter().enumerate() {
            assert!(
                (fjac[k] - want).abs() < 1e-6,
                "entry {k}: {} vs {want}",
                fjac[k]
            );
        }
    }

    #[test]
    fn the_point_is_restored_after_every_probe() {
        let n = 3;
        let mut f = |x: &[f64], out: &mut [f64]| out.copy_from_slice(x);
        let original = vec![1.0, -2.0, 0.0];
        let mut x = original.clone();
        let mut fvec = vec![0.0; n];
        f(&x, &mut fvec);
        let mut fjac = vec![0.0; n * n];
        fdjac1(n, &mut f, &mut x, &fvec, &mut fjac, f64::EPSILON);
        assert_eq!(x, original);
    }

    #[test]
    fn an_unknown_sitting_at_zero_is_probed_by_the_absolute_step() {
        // A relative step would be zero there and divide by it.
        let n = 1;
        let mut f = |x: &[f64], out: &mut [f64]| out[0] = 5.0 * x[0];
        let mut x = vec![0.0];
        let fvec = vec![0.0];
        let mut fjac = vec![0.0; 1];
        fdjac1(n, &mut f, &mut x, &fvec, &mut fjac, f64::EPSILON);
        assert!((fjac[0] - 5.0).abs() < 1e-9);
    }

    #[test]
    fn the_step_at_a_zero_unknown_is_minpacks_truncated_epsilon_and_not_the_exact_one() {
        // The one place the two epsilons are directly observable: everywhere
        // else the step is relative and `x[j] + h` rounds the difference away,
        // so a port that used `f64::EPSILON` agrees on every probe but this
        // one -- and then diverges through the Jacobian. Written as an
        // equality on the probe point because that is what the fixture's call
        // log disagreed about. See `super::EPSMCH`.
        let mut probed = 0.0_f64;
        let mut f = |x: &[f64], out: &mut [f64]| {
            probed = x[0];
            out[0] = x[0];
        };
        let mut x = vec![0.0];
        let fvec = vec![0.0];
        let mut fjac = vec![0.0; 1];
        fdjac1(1, &mut f, &mut x, &fvec, &mut fjac, f64::EPSILON);
        assert_eq!(probed, super::super::EPSMCH.sqrt());
        assert_ne!(probed, f64::EPSILON.sqrt());
    }

    #[test]
    fn one_evaluation_is_spent_per_unknown() {
        let n = 4;
        let mut calls = 0usize;
        {
            let mut f = |x: &[f64], out: &mut [f64]| {
                calls += 1;
                out.copy_from_slice(x);
            };
            let mut x = vec![1.0; n];
            let fvec = vec![1.0; n];
            let mut fjac = vec![0.0; n * n];
            fdjac1(n, &mut f, &mut x, &fvec, &mut fjac, f64::EPSILON);
        }
        assert_eq!(calls, n);
    }
}
