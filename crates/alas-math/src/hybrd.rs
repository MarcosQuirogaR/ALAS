// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from MINPACK-1's hybrd.f.
// Upstream: MINPACK-1 (Argonne National Laboratory, 1980), public domain,
// as vendored in SciPy 1.11.4 and reached through scipy.optimize.fsolve.
// Reference: alas @ rust-port-baseline.

//! MINPACK's modified Powell hybrid method: the root finder every mission
//! segment converges through.
//!
//! Without this, a mission segment has a residual and no way to zero it. mission analysis model
//! poses each segment as a system of nonlinear equations, for a cruise leg,
//! the throttle and body angle at each of sixteen control points against the
//! horizontal and vertical force balance there, and hands it to
//! `scipy.optimize.fsolve`, which is a thin wrapper over this routine.
//!
//! # Why this is a translation and not a call to a root finder
//!
//! The obvious alternative is any Newton-with-line-search of comparable
//! quality, and it would find the same roots. It would not find them along the
//! same path, and here the path is observable. `hybrd` stops when the trust
//! region shrinks below `xtol * ||x||`, not when the residual reaches zero, so
//! the answer it returns is wherever the iteration happened to be standing at
//! that moment. Two solvers that agree to their own tolerance still disagree
//! in the sixth or seventh digit of the throttle, and that throttle sets the
//! fuel flow, which integrates over the segment into the block fuel this whole
//! phase exists to compute. Reproducing the answer therefore means reproducing
//! the iteration.
//!
//! It also means reproducing the *failures*. `hybrd` gives up in four distinct
//! ways ([`Status`]), and mission analysis model reacts to the distinction: `converge_root`
//! prints the message and marks the segment unconverged, and the mission
//! carries that state forward. A solver that converged where the reference
//! gave up would report a different mission, not a better one.
//!
//! So the fixture behind this row records the full log of residual
//! evaluations, in call order, across eight systems, and the parity test
//! replays it. Matching the root is not the claim; matching every trial point
//! that led there is.
//!
//! # The method
//!
//! Each outer iteration builds a forward-difference Jacobian
//! ([`jacobian::fdjac1`]), factors it ([`qr::qrfac`]), and then takes as many
//! steps as it can on that one factorization. Each step is a dogleg
//! ([`dogleg::dogleg`]) inside a trust region whose radius grows when the
//! linear model predicted the observed reduction well and halves when it did
//! not. Rather than refactor after every step, the Jacobian is corrected by
//! Broyden's rank-one update applied directly to `Q` and `R`
//! ([`qr::r1updt`], [`qr::r1mpyq`]), and only after two consecutive
//! unsuccessful steps is a fresh Jacobian built. That is the "hybrid": Newton
//! where the model is good, steepest descent where it is not, and
//! quasi-Newton in between to keep residual evaluations down.
//!
//! Residual evaluations are the cost that shapes the whole design. One of them
//! is an entire segment analysis chain: atmosphere, propulsion, drag
//! buildup, weights, stability, and a Jacobian costs `n` of them.
//!
//! # Scope
//!
//! Translated: the dense path with `mode = 1` internal scaling, which is what
//! `converge_root` reaches. Left untranslated, each unreachable from this
//! program's inputs: the banded Jacobian (`fsolve`'s `band` argument, never
//! set), user-supplied scaling (`diag` with `mode = 2`, never set), and the
//! `nprint` iteration callback (SciPy does not expose it). `hybrj`, the
//! variant taking an analytic Jacobian, is a different entry point that
//! nothing here calls.
//!
//! The two settings mission analysis model leaves as sentinels are resolved here the way SciPy
//! resolves them, and [`Settings`] says so at each field rather than baking in
//! a number: `max_evaluations` of `None` means `200 * (n + 1)` and `step_size`
//! of `None` means the machine epsilon.

// The packed triangle `r` is walked by an index the Fortran advances by hand,
// often by a stride that is not one, and the loop that advances it is the same
// loop that indexes a second array. Both idioms clippy objects to here are that
// pattern, and rewriting either as a zipped iterator would put the Rust and the
// Fortran on different lines, which is the one thing that must not happen in
// a file whose correctness argument is a line-for-line reading of `hybrd.f`.
// Applies to the submodules below as well as to this file.
#![allow(clippy::explicit_counter_loop, clippy::needless_range_loop)]

mod dogleg;
mod enorm;
mod jacobian;
mod qr;

use dogleg::dogleg;
use enorm::enorm;
use jacobian::fdjac1;
use qr::{qform, qrfac, r1mpyq, r1updt};

/// MINPACK's own machine epsilon, `dpmpar(1)`, which is **not** `f64::EPSILON`.
///
/// `dpmpar.f` carries its machine constants as decimal literals, and the IEEE
/// entry it selects is `2.22044604926d-16`: eleven significant digits, where
/// `2^-52` is `2.220446049250313e-16`. The two differ by a relative `4.4e-12`,
/// which sounds like nothing until it reaches [`jacobian::fdjac1`]: the
/// finite-difference step is `sqrt(epsmch)`, so the difference lands in the
/// eleventh digit of every probe, and at an unknown sitting at zero (where
/// the step is absolute rather than relative) it is the whole of the probe
/// point. From there it is a different Jacobian, a different Newton step, and
/// within thirty evaluations a visibly different path.
///
/// The truncated constant is therefore load-bearing and reproduced rather than
/// corrected. `epsfcn`, which SciPy substitutes when the caller names no step
/// size, is a separate quantity and genuinely is `2^-52`; MINPACK takes the
/// larger of the two, which is this one.
const EPSMCH: f64 = 2.22044604926e-16;

/// The constants MINPACK names `p1`, `p5`, `p001` and `p0001`.
const RATIO_POOR: f64 = 0.1;
const RATIO_HALVE: f64 = 0.5;
const ACTRED_SLOW1: f64 = 1e-3;
const RATIO_ACCEPT: f64 = 1e-4;

/// How the iteration stopped.
///
/// Only [`Status::Converged`] is a success. mission analysis model's `converge_root` treats
/// every other value alike (it prints the message and sets
/// `segment.converged = False`) but the distinction is preserved because it
/// says whether to give the solver more budget, a better starting guess, or a
/// different problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The trust region fell below `xtol * ||diag * x||`, or the residual
    /// reached exactly zero. MINPACK's `info = 1`.
    Converged,
    /// The evaluation budget ran out. MINPACK's `info = 2`.
    MaxEvaluations,
    /// `xtol` is too small for any further progress to be representable.
    /// MINPACK's `info = 3`.
    ToleranceTooSmall,
    /// Five consecutive Jacobian rebuilds bought less than a tenth of a
    /// reduction between them. MINPACK's `info = 4`.
    NoProgressSinceJacobians,
    /// Ten consecutive iterations bought less than a thousandth. MINPACK's
    /// `info = 5`.
    NoProgressSinceIterations,
}

impl Status {
    /// Whether this is the one outcome that counts as a solved system.
    pub fn is_converged(self) -> bool {
        self == Self::Converged
    }
}

/// A system this routine declines to start on, which MINPACK reports as
/// `info = 0` and SciPy raises over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HybrdError {
    /// No unknowns to solve for.
    EmptySystem,
    /// `xtol` is negative.
    NegativeTolerance,
    /// `factor` is not strictly positive.
    NonPositiveFactor,
    /// The evaluation budget is zero, so not even the starting point could be
    /// evaluated.
    ZeroBudget,
}

/// The four knobs `converge_root` reaches, and nothing else.
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    /// Convergence is declared when the trust region falls below this times
    /// the scaled norm of the current point. mission analysis model passes
    /// `Numerics.tolerance_solution`, which defaults to `1e-8`.
    pub xtol: f64,
    /// Residual evaluations allowed. `None` is SciPy's substitution of
    /// `200 * (n + 1)`, which is what mission analysis model's `max_evaluations` of `0.`
    /// resolves to.
    pub max_evaluations: Option<usize>,
    /// Assumed relative error in the residual, setting the finite-difference
    /// step. `None` is SciPy's substitution of the machine epsilon, which is
    /// what mission analysis model's unset `step_size` resolves to.
    pub step_size: Option<f64>,
    /// Sets the initial trust region, as this times the scaled norm of the
    /// starting point. SciPy's default of `100` is the only value reached.
    pub factor: f64,
}

impl Default for Settings {
    /// The settings a mission segment is solved at: SciPy's `fsolve`
    /// defaults, with `xtol` at the value mission analysis model overrides it to.
    fn default() -> Self {
        Self {
            xtol: 1e-8,
            max_evaluations: None,
            step_size: None,
            factor: 100.0,
        }
    }
}

/// Where the iteration stopped, and what it cost.
#[derive(Debug, Clone)]
pub struct Solution {
    /// The final point. Meaningful whatever the status: MINPACK returns the
    /// best point it reached even when it gave up, and mission analysis model keeps it.
    pub x: Vec<f64>,
    /// The residual there.
    pub residual: Vec<f64>,
    /// Residual evaluations spent, MINPACK's `nfev`. This is the number the
    /// parity test checks hardest, because two implementations agreeing on
    /// the root while disagreeing here took different paths to it.
    pub evaluations: usize,
    /// How it stopped.
    pub status: Status,
}

/// Solve `residual(x) == 0` from the starting point `x0`.
///
/// `residual` writes `n` values into the slice it is given, for the `n`-vector
/// it is handed; it is `FnMut` because a segment residual mutates the segment
/// state it is computed from.
pub fn solve<F>(mut residual: F, x0: &[f64], settings: &Settings) -> Result<Solution, HybrdError>
where
    F: FnMut(&[f64], &mut [f64]),
{
    let n = x0.len();
    if n == 0 {
        return Err(HybrdError::EmptySystem);
    }
    if settings.xtol < 0.0 {
        return Err(HybrdError::NegativeTolerance);
    }
    if settings.factor <= 0.0 {
        return Err(HybrdError::NonPositiveFactor);
    }
    let maxfev = settings.max_evaluations.unwrap_or(200 * (n + 1));
    if maxfev == 0 {
        return Err(HybrdError::ZeroBudget);
    }
    let epsfcn = settings.step_size.unwrap_or(f64::EPSILON);

    let mut x = x0.to_vec();
    let mut fvec = vec![0.0_f64; n];
    residual(&x, &mut fvec);
    let mut nfev = 1_usize;
    let mut fnorm = enorm(&fvec);

    let mut diag = vec![1.0_f64; n];
    let mut fjac = vec![0.0_f64; n * n];
    let mut r = vec![0.0_f64; n * (n + 1) / 2];
    let mut qtf = vec![0.0_f64; n];
    // MINPACK's four scratch vectors, kept under their own names because the
    // roles they play change between phases of the iteration and renaming
    // them per use would obscure that the Fortran reuses one array.
    let mut wa1 = vec![0.0_f64; n];
    let mut wa2 = vec![0.0_f64; n];
    let mut wa3 = vec![0.0_f64; n];
    let mut wa4 = vec![0.0_f64; n];

    let mut xnorm = 0.0_f64;
    let mut delta = 0.0_f64;
    let mut iter = 1_usize;
    let (mut ncsuc, mut ncfail, mut nslow1, mut nslow2) = (0_usize, 0_usize, 0_usize, 0_usize);

    loop {
        let mut jeval = true;

        fdjac1(n, &mut residual, &mut x, &fvec, &mut fjac, epsfcn);
        nfev += n;

        qrfac(n, &mut fjac, &mut wa1, &mut wa2);

        // On the first iteration the column norms of the Jacobian set the
        // scaling, and the scaled norm of the starting point sets the initial
        // trust region.
        if iter == 1 {
            for j in 0..n {
                diag[j] = if wa2[j] == 0.0 { 1.0 } else { wa2[j] };
                wa3[j] = diag[j] * x[j];
            }
            xnorm = enorm(&wa3);
            delta = settings.factor * xnorm;
            if delta == 0.0 {
                delta = settings.factor;
            }
        }

        // Form `Q^T fvec` in `qtf`, by applying the stored reflections.
        qtf.copy_from_slice(&fvec);
        for j in 0..n {
            if fjac[j + j * n] != 0.0 {
                let mut sum = 0.0;
                for i in j..n {
                    sum += fjac[i + j * n] * qtf[i];
                }
                let temp = -sum / fjac[j + j * n];
                for i in j..n {
                    qtf[i] += fjac[i + j * n] * temp;
                }
            }
        }

        // Copy the upper triangle of the factorization into the packed `r`.
        let mut k = 0;
        for i in 0..n {
            for j in i..n {
                r[k] = if i == j { wa1[i] } else { fjac[i + j * n] };
                k += 1;
            }
        }

        qform(n, &mut fjac);

        for j in 0..n {
            diag[j] = diag[j].max(wa2[j]);
        }

        // The inner loop: steps taken against this one factorization.
        let status = loop {
            dogleg(n, &r, &diag, &qtf, delta, &mut wa1);

            for j in 0..n {
                wa1[j] = -wa1[j];
                wa2[j] = x[j] + wa1[j];
                wa3[j] = diag[j] * wa1[j];
            }
            let pnorm = enorm(&wa3);
            if iter == 1 {
                delta = delta.min(pnorm);
            }

            residual(&wa2, &mut wa4);
            nfev += 1;
            let fnorm1 = enorm(&wa4);

            // Reduction the step actually bought, as a fraction of the square
            // of the residual norm.
            let actred = if fnorm1 < fnorm {
                1.0 - (fnorm1 / fnorm).powi(2)
            } else {
                -1.0
            };

            // Reduction the linear model predicted.
            let mut l = 0;
            for i in 0..n {
                let mut sum = 0.0;
                for j in i..n {
                    sum += r[l] * wa1[j];
                    l += 1;
                }
                wa3[i] = qtf[i] + sum;
            }
            let temp = enorm(&wa3);
            let prered = if temp < fnorm {
                1.0 - (temp / fnorm).powi(2)
            } else {
                0.0
            };
            let ratio = if prered > 0.0 { actred / prered } else { 0.0 };

            // Grow or shrink the trust region on how well the model did.
            if ratio < RATIO_POOR {
                ncsuc = 0;
                ncfail += 1;
                delta *= RATIO_HALVE;
            } else {
                ncfail = 0;
                ncsuc += 1;
                if ratio >= RATIO_HALVE || ncsuc > 1 {
                    delta = delta.max(pnorm / RATIO_HALVE);
                }
                if (ratio - 1.0).abs() <= RATIO_POOR {
                    delta = pnorm / RATIO_HALVE;
                }
            }

            // Accept the step if it bought anything at all.
            if ratio >= RATIO_ACCEPT {
                for j in 0..n {
                    x[j] = wa2[j];
                    wa2[j] = diag[j] * x[j];
                    fvec[j] = wa4[j];
                }
                xnorm = enorm(&wa2);
                fnorm = fnorm1;
                iter += 1;
            }

            nslow1 += 1;
            if actred >= ACTRED_SLOW1 {
                nslow1 = 0;
            }
            if jeval {
                nslow2 += 1;
            }
            if actred >= RATIO_POOR {
                nslow2 = 0;
            }

            // Convergence is tested alone and exits at once. The four failure
            // tests below are then all evaluated, and the *last* one that
            // matches is the one reported: the Fortran assigns `info` in
            // sequence without branching out, so a run that has both
            // exhausted its budget and stalled reports the stall. Taking the
            // first match instead would report a different reason for the
            // same stopping point, and `converge_root` prints that reason.
            if delta <= settings.xtol * xnorm || fnorm == 0.0 {
                break Some(Status::Converged);
            }
            let mut info = None;
            if nfev >= maxfev {
                info = Some(Status::MaxEvaluations);
            }
            if RATIO_POOR * (RATIO_POOR * delta).max(pnorm) <= EPSMCH * xnorm {
                info = Some(Status::ToleranceTooSmall);
            }
            if nslow2 == 5 {
                info = Some(Status::NoProgressSinceJacobians);
            }
            if nslow1 == 10 {
                info = Some(Status::NoProgressSinceIterations);
            }
            if info.is_some() {
                break info;
            }

            // Two unsuccessful steps in a row means the carried Jacobian has
            // gone stale; rebuild it rather than update it again.
            if ncfail == 2 {
                break None;
            }

            // Broyden's rank-one correction, applied to the factors directly.
            for j in 0..n {
                let mut sum = 0.0;
                for i in 0..n {
                    sum += fjac[i + j * n] * wa4[i];
                }
                wa2[j] = (sum - wa3[j]) / pnorm;
                wa1[j] = diag[j] * ((diag[j] * wa1[j]) / pnorm);
                if ratio >= RATIO_ACCEPT {
                    qtf[j] = sum;
                }
            }
            r1updt(n, &mut r, &wa1, &mut wa2, &mut wa3);
            r1mpyq(n, n, &mut fjac, &wa2, &wa3);
            r1mpyq(1, n, &mut qtf, &wa2, &wa3);

            jeval = false;
        };

        if let Some(status) = status {
            return Ok(Solution {
                x,
                residual: fvec,
                evaluations: nfev,
                status,
            });
        }
    }
}

// A test states the system it solves inline, so a failed unwrap or expect is
// the assertion failing rather than a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn rosenbrock(x: &[f64], out: &mut [f64]) {
        out[0] = 10.0 * (x[1] - x[0] * x[0]);
        out[1] = 1.0 - x[0];
    }

    #[test]
    fn a_well_posed_system_converges_to_its_root() {
        let solution =
            solve(rosenbrock, &[-1.2, 1.0], &Settings::default()).expect("well-posed system");
        assert_eq!(solution.status, Status::Converged);
        assert!((solution.x[0] - 1.0).abs() < 1e-9);
        assert!((solution.x[1] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_starting_point_already_at_the_root_still_reports_convergence() {
        let solution =
            solve(rosenbrock, &[1.0, 1.0], &Settings::default()).expect("well-posed system");
        assert_eq!(solution.status, Status::Converged);
    }

    #[test]
    fn an_exhausted_budget_reports_itself_rather_than_a_root() {
        let settings = Settings {
            max_evaluations: Some(5),
            ..Settings::default()
        };
        let solution = solve(rosenbrock, &[-1.2, 1.0], &settings).expect("well-posed system");
        assert_eq!(solution.status, Status::MaxEvaluations);
        assert!(!solution.status.is_converged());
    }

    #[test]
    fn the_budget_defaults_to_scipys_substitution_rather_than_to_zero() {
        // mission analysis model passes `max_evaluations = 0.`, which SciPy turns into
        // `200 * (n + 1)`; a port that read the zero literally would refuse
        // to run every segment.
        assert!(Settings::default().max_evaluations.is_none());
        let mut calls = 0usize;
        let settings = Settings {
            xtol: 0.0,
            ..Settings::default()
        };
        let solution = solve(
            |x: &[f64], out: &mut [f64]| {
                calls += 1;
                // A system with no root, so only the budget can stop it.
                out[0] = x[0] * x[0] + 1.0;
                out[1] = x[1] * x[1] + 1.0;
            },
            &[1.0, 1.0],
            &settings,
        )
        .expect("well-posed system");
        assert!(!solution.status.is_converged());
        assert!(solution.evaluations <= 200 * 3);
        assert!(calls > 2);
    }

    #[test]
    fn a_system_with_no_unknowns_is_refused_rather_than_solved() {
        let result = solve(rosenbrock, &[], &Settings::default());
        assert_eq!(result.unwrap_err(), HybrdError::EmptySystem);
    }

    #[test]
    fn a_non_positive_factor_is_refused() {
        let settings = Settings {
            factor: 0.0,
            ..Settings::default()
        };
        assert_eq!(
            solve(rosenbrock, &[1.0, 2.0], &settings).unwrap_err(),
            HybrdError::NonPositiveFactor
        );
    }

    #[test]
    fn unknowns_of_very_different_magnitude_are_both_converged() {
        // The internal scaling is what makes this work: without it the trust
        // region is a ball in the raw variables and the small unknown is
        // stepped as coarsely as the large one.
        let solution = solve(
            |x: &[f64], out: &mut [f64]| {
                out[0] = x[0] - 1.0e6;
                out[1] = x[1] - 1.0e-6;
            },
            &[0.0, 0.0],
            &Settings::default(),
        )
        .expect("well-posed system");
        assert_eq!(solution.status, Status::Converged);
        assert!((solution.x[0] / 1.0e6 - 1.0).abs() < 1e-9);
        assert!((solution.x[1] / 1.0e-6 - 1.0).abs() < 1e-6);
    }
}
